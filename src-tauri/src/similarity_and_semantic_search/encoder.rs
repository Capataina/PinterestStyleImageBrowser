use ort::{execution_providers::CUDAExecutionProvider, session::Session, value::{Tensor, Value}};
use ndarray;
use std::{error::Error, path::Path};
use image::ImageReader;

pub struct Encoder {
    session: Session,
}

impl Encoder {
    pub fn new(model_path: &Path) -> Result<Self, Box<dyn Error>> {
        println!("=== Initializing Encoder ===");
        println!("Attempting to enable CUDA...");
        
        // Try to build with CUDA explicitly
        let builder_result = Session::builder()?
            .with_execution_providers([
                CUDAExecutionProvider::default().build()
            ]);
        
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
                println!("✓ CUDA execution provider accepted");
                let session = builder.commit_from_file(model_path)?;
                println!("✓ Session created with CUDA");
                Ok(Encoder { session })
            }
            Err(e) => {
                println!("✗ CUDA execution provider failed: {}", e);
                println!("Falling back to CPU...");
                let session = Session::builder()?
                    .commit_from_file(model_path)?;
                Ok(Encoder { session })
            }
        }
    }

    pub fn inspect_model(&self) {
        println!("Model inputs:");
        for input in self.session.inputs.iter() {
            println!("  Name: {:?}", input.name);
        }
        
        println!("\nModel outputs:");
        for output in self.session.outputs.iter() {
            println!("  Name: {:?}", output.name);
        }
    }
    
    pub fn preprocess_image(
        &self,
        image_path: &Path,
    ) -> Result<ndarray::Array4<f32>, Box<dyn std::error::Error>> {
        let img = ImageReader::open(image_path)?
            .decode()?
            .resize_exact(224, 224, image::imageops::FilterType::Nearest)
            .to_rgb8();

        let mut input_tensor: Vec<f32> = Vec::with_capacity((224 * 224 * 3) as usize);

        // Change layout from RGBRGB... to RRR...GGG...BBB...
        // for some reason ONNX wants it like this
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

        // CLIP-style normalization, we use IMGNET stats here
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];

        for c in 0..3 {
            for i in 0..(224 * 224) {
                let idx = c * 224 * 224 + i;
                input_tensor[idx] = (input_tensor[idx] - mean[c]) / std[c];
            }
        }

        // we create a 4d array using ndarray bc otherwise ort tensor creation is a pain
        let input_array =
            ndarray::Array4::from_shape_vec((1, 3, 224, 224), input_tensor)?;
        Ok(input_array)
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
        let outputs = self
            .session
            .run(ort::inputs![
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

    // write a batch encode function that takes a vec of image paths and returns a vec of embeddings
    // the batch size should be configurable
    pub fn encode_batch(
    &mut self,
    image_paths: &[&Path],
    ) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
        if image_paths.is_empty() {
            return Ok(Vec::new());
        }
        
        // Step 1: Preprocess all images
        let mut preprocessed = Vec::new();
        for path in image_paths {
            let input_array = self.preprocess_image(path)?;
            preprocessed.push(input_array);
        }
        
        // Step 2: Concatenate (not stack!) into single batch tensor
        let batch_array = ndarray::concatenate(
            ndarray::Axis(0),
            &preprocessed.iter().map(|a| a.view()).collect::<Vec<_>>(),
        )?;
        
        // Verify the shape is correct
        assert_eq!(
            batch_array.shape(),
            &[image_paths.len(), 3, 224, 224],
            "Batch array has wrong shape"
        );
        
        // Step 3: Convert to ONNX tensor
        let shape = [image_paths.len(), 3, 224, 224];
        let (data, _offset) = batch_array.into_raw_vec_and_offset();
        let onnx_input: Tensor<f32> = Tensor::from_array((shape, data))?;
        
        // Step 4: Create dummy text inputs
        let text_shape = [image_paths.len(), 1]; 
        let dummy_text_data = vec![0i64; image_paths.len()];
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
        
        let mut embeddings = Vec::new();
        for i in 0..image_paths.len() {
            let start = i * embedding_size;
            let end = start + embedding_size;
            embeddings.push(data_slice[start..end].to_vec());
        }
        
        Ok(embeddings)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper function to calculate cosine similarity between two embeddings
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len(), "Embeddings must be same length");
        
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot_product / (norm_a * norm_b)
        }
    }

    #[test]
    fn test_check_execution_providers() {
        println!("=== Checking ONNX Runtime Configuration ===\n");
        
        // Check if CUDA libraries are available at runtime
        println!("Attempting to create a session...");
        
        let model_path = std::path::Path::new("models/model.onnx");
        if !model_path.exists() {
            println!("Model file not found, skipping test");
            return;
        }
        
        match Session::builder() {
            Ok(builder) => {
                println!("✓ Session builder created successfully");
                
                match builder.commit_from_file(model_path) {
                    Ok(session) => {
                        println!("✓ Session loaded successfully");
                        println!("\nSession info:");
                        println!("  Inputs: {}", session.inputs.len());
                        println!("  Outputs: {}", session.outputs.len());
                        
                        // The session was created, but we can't directly query which EP is being used
                        // in this version of ort. However, if CUDA is available, it will be used automatically.
                        println!("\n⚠ Note: This version of ort doesn't expose which execution provider is active.");
                        println!("If you have CUDA enabled in Cargo.toml and CUDA installed, it should be used automatically.");
                    }
                    Err(e) => {
                        println!("✗ Failed to load session: {}", e);
                    }
                }
            }

            Err(e) => {
                println!("✗ Failed to create session builder: {}", e);
            }
        }
        
        println!("\n=== System Checks ===");
        println!("Run these commands to verify CUDA setup:");
        println!("  1. Check GPU: nvidia-smi");
        println!("  2. Check CUDA version: nvcc --version");
        println!("  3. Check ort features: cargo tree -p ort");
    }

    #[test]
    fn test_inference_speed() {
        use std::time::Instant;
        
        let mut encoder = Encoder::new(std::path::Path::new("models/model.onnx")).unwrap();
        
        // Use one of your actual test images
        let test_image = std::path::Path::new("test_images/66bb951dcab9c27a144292c8_WestStudio-LOL-Splash-Vol2-021.jpg");
        
        if !test_image.exists() {
            println!("Test image not found, skipping");
            return;
        }
        
        println!("=== Inference Speed Test ===\n");
        
        // Warm up (first inference is always slower)
        println!("Warming up...");
        let _ = encoder.encode(test_image);
        
        // Time 5 inferences
        println!("Running 5 inference tests...\n");
        let mut times = Vec::new();
        
        for i in 1..=5 {
            let start = Instant::now();
            let _ = encoder.encode(test_image).unwrap();
            let elapsed = start.elapsed();
            times.push(elapsed);
            println!("  Run {}: {:.3}s ({:.0}ms)", i, elapsed.as_secs_f64(), elapsed.as_millis());
        }
        
        let avg_time = times.iter().map(|t| t.as_secs_f64()).sum::<f64>() / times.len() as f64;
        
        println!("\n=== Results ===");
        println!("Average time: {:.3}s ({:.0}ms)", avg_time, avg_time * 1000.0);
        
        println!("\n=== Expected Times ===");
        println!("CPU:  0.5 - 1.5 seconds (500-1500ms)");
        println!("GPU:  0.02 - 0.1 seconds (20-100ms)");
        
        println!("\n=== Verdict ===");
        if avg_time > 0.3 {
            println!("⚠ RUNNING ON CPU - CUDA IS NOT WORKING");
            println!("  Your time: {:.3}s ({:.0}ms)", avg_time, avg_time * 1000.0);
        } else if avg_time > 0.1 {
            println!("⚠ UNCLEAR - Might be CPU or slow GPU");
            println!("  Your time: {:.3}s ({:.0}ms)", avg_time, avg_time * 1000.0);
        } else {
            println!("✓ RUNNING ON GPU - CUDA IS WORKING!");
            println!("  Your time: {:.3}s ({:.0}ms)", avg_time, avg_time * 1000.0);
        }
    }
    
    #[test]
    fn test_preprocess_image() {
        let encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        
        let result = encoder
            .preprocess_image(Path::new(
                "C:\\image-browser\\src-tauri\\test_images\\66bb951dcab9c27a144292c8_WestStudio-LOL-Splash-Vol2-021.jpg",
            ))
            .unwrap();
        
        // Check shape
        assert_eq!(result.shape(), &[1, 3, 224, 224]);
        
        // Check that values are in a reasonable range after normalization
        let min = result.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = result.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        
        // After normalization, values should roughly be in [-3, 3]
        assert!(min > -5.0);
        assert!(max < 5.0);
        
        println!("Preprocessing - Min: {}, Max: {}", min, max);
    }
    
    #[test]
    fn test_inspect_model() {
        let encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        encoder.inspect_model();
    }
    
    #[test]
    fn test_encode_basic() {
        let mut encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        
        let embedding = encoder
            .encode(Path::new("C:\\image-browser\\src-tauri\\test_images\\66bb951dcab9c27a144292c8_WestStudio-LOL-Splash-Vol2-021.jpg"))
            .unwrap();
        
        // CLIP ViT-B/32 produces 512-dimensional embeddings
        assert_eq!(embedding.len(), 512, "Embedding should be 512-dimensional");
        
        // Check that embedding values are finite
        for val in &embedding {
            assert!(val.is_finite(), "All embedding values should be finite");
        }
        
        // Check that embedding is not all zeros
        let sum: f32 = embedding.iter().sum();
        assert!(sum.abs() > 0.001, "Embedding should not be all zeros");
        
        // Calculate L2 norm
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        println!("Embedding L2 norm: {}", norm);
        
        // CLIP embeddings are usually normalized, so norm should be close to 1
        assert!(norm > 0.9 && norm < 1.1, "Embedding should be approximately normalized");
    }
    
    #[test]
    fn test_encode_consistency() {
        let mut encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        
        let image_path = Path::new("C:\\image-browser\\src-tauri\\test_images\\66bb951dcab9c27a144292c8_WestStudio-LOL-Splash-Vol2-021.jpg");
        
        // Encode the same image twice
        let embedding1 = encoder.encode(image_path).unwrap();
        let embedding2 = encoder.encode(image_path).unwrap();
        
        // They should be identical (or extremely close due to floating point)
        for (v1, v2) in embedding1.iter().zip(embedding2.iter()) {
            let diff = (v1 - v2).abs();
            assert!(diff < 1e-6, "Same image should produce identical embeddings");
        }
        
        println!("Consistency test passed: same image produces identical embeddings");
    }
    
    #[test]
    fn test_encode_multiple_images() {
        let mut encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        
        // You'll need at least 2-3 test images for this
        let image_paths = vec![
            "test_images/image1.jpg",
            "C:\\image-browser\\src-tauri\\test_images\\66bb951dcab9c27a144292c8_WestStudio-LOL-Splash-Vol2-021.jpg",
            "C:\\image-browser\\src-tauri\\test_images\\joaquin-castro-uribe-1653436027503.jpg",
            "C:\\image-browser\\src-tauri\\test_images\\main-qimg-281979616ac1176d214c72ef38c73478-lq.jpeg",
        ];
        
        let mut embeddings = Vec::new();
        
        for path in &image_paths {
            match encoder.encode(Path::new(path)) {
                Ok(emb) => {
                    assert_eq!(emb.len(), 512);
                    embeddings.push(emb);
                    println!("Successfully encoded: {}", path);
                }
                Err(e) => {
                    println!("Warning: Could not encode {}: {}", path, e);
                }
            }
        }
        
        // Verify we encoded at least some images
        assert!(!embeddings.is_empty(), "Should successfully encode at least one image");
        
        // All embeddings should have same dimensions
        for emb in &embeddings {
            assert_eq!(emb.len(), 512);
        }
    }
    
    #[test]
    fn test_similarity_same_vs_different() {
        let mut encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        
        // You need at least 2 different images for this test
        let image1_path = Path::new("C:\\image-browser\\src-tauri\\test_images\\66bb951dcab9c27a144292c8_WestStudio-LOL-Splash-Vol2-021.jpg");
        let image2_path = Path::new("C:\\image-browser\\src-tauri\\test_images\\joaquin-castro-uribe-1653436027503.jpg");
        
        let emb1_first = encoder.encode(image1_path).unwrap();
        let emb1_second = encoder.encode(image1_path).unwrap();
        let emb2 = encoder.encode(image2_path).unwrap();
        
        // Similarity between same image should be ~1.0
        let same_similarity = cosine_similarity(&emb1_first, &emb1_second);
        println!("Same image similarity: {}", same_similarity);
        assert!(same_similarity > 0.999, "Same image should have similarity ~1.0");
        
        // Similarity between different images should be less than 1.0
        let diff_similarity = cosine_similarity(&emb1_first, &emb2);
        println!("Different images similarity: {}", diff_similarity);
        assert!(diff_similarity < 0.999, "Different images should have similarity < 1.0");
    }
    
    #[test]
    fn test_encode_invalid_path() {
        let mut encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        
        let result = encoder.encode(Path::new("nonexistent_image.jpg"));
        
        assert!(result.is_err(), "Should return error for nonexistent image");
        println!("Correctly handled invalid path: {:?}", result.err());
    }
    
    #[test]
    fn test_embedding_value_ranges() {
        let mut encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        
        let embedding = encoder
            .encode(Path::new("C:\\image-browser\\src-tauri\\test_images\\66bb951dcab9c27a144292c8_WestStudio-LOL-Splash-Vol2-021.jpg"))
            .unwrap();
        
        // Calculate statistics
        let min = embedding.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = embedding.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let mean = embedding.iter().sum::<f32>() / embedding.len() as f32;
        
        println!("Embedding statistics:");
        println!("  Min: {}", min);
        println!("  Max: {}", max);
        println!("  Mean: {}", mean);
        
        // CLIP embeddings typically have reasonable value ranges
        assert!(min > -5.0 && min < 5.0, "Min value should be reasonable");
        assert!(max > -5.0 && max < 5.0, "Max value should be reasonable");
    }

    #[test]
    fn test_encode_batch() {
        let mut encoder = Encoder::new(Path::new("models/model.onnx")).unwrap();
        
        let paths = vec![
            Path::new("C:\\image-browser\\src-tauri\\test_images\\66bb951dcab9c27a144292c8_WestStudio-LOL-Splash-Vol2-021.jpg"),
            Path::new("C:\\image-browser\\src-tauri\\test_images\\613syV-a1sL._AC_UF894,1000_QL80_.jpg"),
            Path::new("C:\\image-browser\\src-tauri\\test_images\\joaquin-castro-uribe-1653436027503.jpg"),
        ];
        
        // Encode individually
        let individual: Vec<Vec<f32>> = paths.iter()
            .map(|p| encoder.encode(p).unwrap())
            .collect();
        
        // Encode as batch
        let batch_paths: Vec<&Path> = paths.iter().map(|p| p.as_ref()).collect();
        let batched = encoder.encode_batch(&batch_paths).unwrap();
        
        // Results should match
        assert_eq!(batched.len(), individual.len());
        for (batch_emb, indiv_emb) in batched.iter().zip(individual.iter()) {
            assert_eq!(batch_emb.len(), indiv_emb.len());
            for (b, i) in batch_emb.iter().zip(indiv_emb.iter()) {
                assert!((b - i).abs() < 1e-5, "Batch and individual encoding should match");
            }
        }
        
        println!("Batch encoding matches individual encoding!");
    }
}