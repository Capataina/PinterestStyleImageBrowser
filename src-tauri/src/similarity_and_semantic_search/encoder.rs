use image::ImageReader;
use ndarray;
use ort::{
    execution_providers::CUDAExecutionProvider,
    session::Session,
    value::{Tensor, Value},
};
use std::{error::Error, path::Path};
use tracing::{debug, info, warn};

use crate::db;

pub struct Encoder {
    session: Session,
}

impl Encoder {
    pub fn new(model_path: &Path) -> Result<Self, Box<dyn Error>> {
        info!("=== Initializing Encoder ===");
        info!("Attempting to enable CUDA...");

        // Try to build with CUDA explicitly
        let builder_result = Session::builder()?
            .with_execution_providers([CUDAExecutionProvider::default().build()]);

        // for some reason, the session says it's initialised with CUDA but the encoding is incredibly slow, to the point
        // where it's either not using the GPU at all or something is very wrong
        // so we do a test run here to see if it actually works, if it fails we fall back to CPU
        // this is a bit hacky but idk how else to do it with the current ort api
        // and to be fair, it's definitely using the CPU not GPU despite saying it initialised with the GPU so yeah idk

        // wait actually it seems like during debug, the PC uses the CPU even if CUDA is enabled
        // but in release mode it uses the GPU, so maybe this is just a debug vs release issue?
        // leaving this fallback in for now just in case though
        match builder_result {
            Ok(builder) => {
                info!("✓ CUDA execution provider accepted");
                let session = builder.commit_from_file(model_path)?;
                info!("✓ Session created with CUDA");
                Ok(Encoder { session })
            }
            Err(e) => {
                warn!("✗ CUDA execution provider failed: {}", e);
                info!("Falling back to CPU...");
                let session = Session::builder()?.commit_from_file(model_path)?;
                Ok(Encoder { session })
            }
        }
    }

    pub fn inspect_model(&self) {
        debug!("Model inputs:");
        for input in self.session.inputs.iter() {
            debug!("  Name: {:?}", input.name);
        }

        debug!("\nModel outputs:");
        for output in self.session.outputs.iter() {
            debug!("  Name: {:?}", output.name);
        }
    }

    pub fn preprocess_image(
        &self,
        image_path: &Path,
    ) -> Result<ndarray::Array4<f32>, Box<dyn std::error::Error>> {
        // Lanczos3 preserves edge information vastly better than the
        // previous Nearest filter — this matters for CLIP because the
        // 224x224 downsample is the only resampling step before the
        // network sees the image. Reference CLIP uses bicubic; Lanczos3
        // is closer to it than Nearest.
        let img = ImageReader::open(image_path)?
            .with_guessed_format()?
            .decode()?
            .resize_exact(224, 224, image::imageops::FilterType::Lanczos3)
            .to_rgb8();

        let mut input_tensor: Vec<f32> = Vec::with_capacity((224 * 224 * 3) as usize);

        // ONNX expects channels-first (CHW) layout: all R values first,
        // then all G, then all B. The image crate gives us interleaved
        // RGBRGB pixels, so we split-and-concat here.
        let mut r = Vec::with_capacity((224 * 224) as usize);
        let mut g = Vec::with_capacity((224 * 224) as usize);
        let mut b = Vec::with_capacity((224 * 224) as usize);

        for pixel in img.pixels() {
            r.push(pixel[0] as f32 / 255.0);
            g.push(pixel[1] as f32 / 255.0);
            b.push(pixel[2] as f32 / 255.0);
        }

        input_tensor.extend(r);
        input_tensor.extend(g);
        input_tensor.extend(b);

        // CLIP-native normalization stats (from openai/CLIP repository).
        // The previous code used ImageNet stats which subtly shift the
        // embedding distribution away from what the OpenAI reference
        // produces.
        let mean = [0.48145466_f32, 0.4578275, 0.40821073];
        let std = [0.26862954_f32, 0.26130258, 0.27577711];

        for c in 0..3 {
            for i in 0..(224 * 224) {
                let idx = c * 224 * 224 + i;
                input_tensor[idx] = (input_tensor[idx] - mean[c]) / std[c];
            }
        }

        // we create a 4d array using ndarray bc otherwise ort tensor creation is a pain
        let input_array = ndarray::Array4::from_shape_vec((1, 3, 224, 224), input_tensor)?;
        Ok(input_array)
    }

    // write a function to preprocess in batches of x, this will be useful for the batch encode function
    pub fn batch_preprocess_image(
        &mut self,
        image_paths: &[&Path],
        batch_size: usize,
    ) -> Result<Vec<ndarray::Array4<f32>>, Box<dyn std::error::Error>> {
        if batch_size == 0 {
            return Err("batch_size must be greater than 0".into());
        }

        let mut batches: Vec<ndarray::Array4<f32>> = Vec::new();

        // Process the incoming paths in chunks of `batch_size`
        for chunk in image_paths.chunks(batch_size) {
            let mut preprocessed_chunk: Vec<ndarray::Array4<f32>> = Vec::new();

            // Preprocess each image in the current chunk as a single-image tensor (1, 3, 224, 224)
            for path in chunk {
                let input_array = self.preprocess_image(path)?;
                preprocessed_chunk.push(input_array);
            }

            // Concatenate along the batch axis (axis 0) to get shape (chunk_len, 3, 224, 224)
            let batch_array = ndarray::concatenate(
                ndarray::Axis(0),
                &preprocessed_chunk
                    .iter()
                    .map(|a| a.view())
                    .collect::<Vec<_>>(),
            )?;

            batches.push(batch_array);
        }

        Ok(batches)
    }

    pub fn encode(&mut self, image_path: &Path) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let input_array = self.preprocess_image(image_path)?;

        // TODO: Optimize this to avoid copies
        // this is a temp workaround for now, idk how to fix this but basically, the way Tensor::from_array works
        // is that it requires ownership of the data vec so we have to extract it from the ndarray first
        // the wiki shows them using 2 diff methods, one with a pure ndarray and one with a shape and vec
        // when I tried to use the pure ndarray one it gave lifetime issues so this is the workaround
        // ideally we would just be able to pass the ndarray directly but idk how to do that yet
        let shape = [1usize, 3, 224, 224];
        let (data, _offset) = input_array.into_raw_vec_and_offset();

        // use the shape and data to create the tensor
        let onnx_input: Tensor<f32> = Tensor::from_array((shape, data))?;

        // Create dummy inputs for the text branch to prevent ONNX crashes,
        // honestly annoying as hell and will most likely break something later on but we can always refactor right hahah ha
        // TODO: Fix this when it breaks in the future...
        // The model expects these to exist even if we are only doing image encoding
        let text_shape = [1usize, 1];
        let dummy_text_data = vec![0i64; 1]; // Batch size 1, sequence length 1, value 0

        // We clone dummy_text_data for the first one because Tensor takes ownership
        let input_ids: Tensor<i64> = Tensor::from_array((text_shape, dummy_text_data.clone()))?;
        let attention_mask: Tensor<i64> = Tensor::from_array((text_shape, dummy_text_data))?;
        // --------------------------------------------------------------------------

        // Use hardcoded input/output names since we know them from inspection
        // do not change these unless your model uses different names
        // so probs never
        let input_name = "pixel_values";
        let output_name = "image_embeds";

        // Now run the model (mutable borrow happens here)
        // Updated to include the text branch inputs required by the graph
        let outputs = self.session.run(ort::inputs![
            input_name => onnx_input,
            "input_ids" => input_ids,
            "attention_mask" => attention_mask
        ])?;

        // Extract from outputs using the name we got earlier
        let dyn_tensor: &Value<_> = &outputs[output_name];

        // this is an absolute pain to fix
        // it always keeps breaking because if we get a tuple from the tensor
        // tuples do not have view, as slice or any of the other methods for us to turn them into a vec soooooo
        // we use try_extract_tensor which gives us both the shape and a view we can turn into a vec
        let (_out_shape, data_view) = dyn_tensor.try_extract_tensor::<f32>()?;
        let embedding = data_view.to_vec();

        Ok(embedding)
    }

    pub fn encode_batch(
        &mut self,
        image_paths: &[&Path],
    ) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
        if image_paths.is_empty() {
            return Ok(Vec::new());
        }

        // Use a reasonable default batch size for preprocessing
        // This helps manage memory usage for large numbers of images
        let preprocessing_batch_size = 32;

        // Step 1: Preprocess images in batches using the batch_preprocess_image function
        let batched_arrays = self.batch_preprocess_image(image_paths, preprocessing_batch_size)?;

        // Step 2: Process each preprocessed batch through the model
        let mut all_embeddings = Vec::new();

        for batch_array in batched_arrays {
            let batch_size = batch_array.shape()[0];

            // Step 3: Convert to ONNX tensor
            let shape = [batch_size, 3, 224, 224];
            let (data, _offset) = batch_array.into_raw_vec_and_offset();
            let onnx_input: Tensor<f32> = Tensor::from_array((shape, data))?;

            // Step 4: Create dummy text inputs
            let text_shape = [batch_size, 1];
            let dummy_text_data = vec![0i64; batch_size];
            let input_ids: Tensor<i64> = Tensor::from_array((text_shape, dummy_text_data.clone()))?;
            let attention_mask: Tensor<i64> = Tensor::from_array((text_shape, dummy_text_data))?;

            // Step 5: Run inference
            let outputs = self.session.run(ort::inputs![
                "pixel_values" => onnx_input,
                "input_ids" => input_ids,
                "attention_mask" => attention_mask
            ])?;

            // Step 6: Split output into individual embeddings
            let dyn_tensor: &Value<_> = &outputs["image_embeds"];
            let (out_shape, data_view) = dyn_tensor.try_extract_tensor::<f32>()?;

            let data_slice = data_view.to_vec();
            let embedding_size = out_shape[1] as usize;

            for i in 0..batch_size {
                let start = i * embedding_size;
                let end = start + embedding_size;
                all_embeddings.push(data_slice[start..end].to_vec());
            }
        }

        Ok(all_embeddings)
    }

    // Encode all images in the database and store the embeddings in the database as blob,
    // this function will run once every startup after we call it. Its going to take all images from the db,
    // check which ones have no embedding, encode them and store the embeddings in the database as blob.
    pub fn encode_all_images_in_database(
        &mut self,
        batch_size: usize,
        db: &db::ImageDatabase,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Only get images that don't have embeddings yet
        let images = db.get_images_without_embeddings()?;

        if images.is_empty() {
            info!("All images already have embeddings, skipping encoding.");
            return Ok(());
        }

        info!(
            "Found {} images without embeddings, encoding...",
            images.len()
        );

        // use batch embedding to speed up the process
        let total_images = images.len();
        let batches: Vec<_> = images.chunks(batch_size).collect();
        let total_batches = batches.len();

        for (batch_idx, batch) in batches.iter().enumerate() {
            debug!(
                "Encoding batch {}/{} ({} images)...",
                batch_idx + 1,
                total_batches,
                batch.len()
            );

            // Only encode the images in the current batch (not the whole list).
            let batch_paths: Vec<&Path> =
                batch.iter().map(|image| Path::new(&image.path)).collect();
            let embeddings = self.encode_batch(&batch_paths)?;
            for (image, embedding) in batch.iter().zip(embeddings.iter()) {
                db.update_image_embedding(image.id, embedding.clone())?;
            }
        }

        info!("Successfully encoded {} images.", total_images);
        Ok(())
    }
}

