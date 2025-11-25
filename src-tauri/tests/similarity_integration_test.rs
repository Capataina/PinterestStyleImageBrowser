use image_browser_lib::similarity_and_semantic_search::cosine_similarity::CosineIndex;
use image_browser_lib::similarity_and_semantic_search::encoder::Encoder;
use ndarray::Array1;
use std::path::PathBuf;
use std::fs;

#[test]
#[ignore] // Mark as ignored so it doesn't run by default (requires reference images)
fn test_real_image_similarity_search() {
    // Setup: Point to your references folder
    let references_dir = PathBuf::from("references");
    
    // Skip test if references folder doesn't exist
    if !references_dir.exists() {
        println!("Skipping test: 'references' folder not found");
        println!("Create a 'references' folder and add test images to run this test");
        return;
    }

    println!("Loading encoder...");
    // Use relative path to model
    let model_path = PathBuf::from("models/model.onnx");
    
    if !model_path.exists() {
        println!("Skipping test: model file not found at {:?}", model_path);
        println!("Make sure the ONNX model is available");
        return;
    }
    
    let mut encoder = Encoder::new(&model_path).expect("Failed to create encoder");

    println!("Scanning references directory...");
    let mut image_paths = Vec::new();
    
    // Recursively find all image files
    for entry in fs::read_dir(&references_dir).expect("Failed to read references directory") {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();
        
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if matches!(ext_str.as_str(), "jpg" | "jpeg" | "png" | "webp" | "bmp") {
                    image_paths.push(path);
                }
            }
        }
    }

    println!("Found {} images", image_paths.len());
    assert!(image_paths.len() > 0, "No images found in references directory");

    // Encode all images using batch processing
    println!("Encoding images in batches...");
    let mut index = CosineIndex::new();
    let mut embeddings_map = std::collections::HashMap::new();
    
    const BATCH_SIZE: usize = 32;
    let total_images = image_paths.len();
    
    for (batch_idx, chunk) in image_paths.chunks(BATCH_SIZE).enumerate() {
        let start_idx = batch_idx * BATCH_SIZE;
        let end_idx = (start_idx + chunk.len()).min(total_images);
        
        println!("Processing batch {}/{} (images {}-{})...", 
                 batch_idx + 1, 
                 (total_images + BATCH_SIZE - 1) / BATCH_SIZE,
                 start_idx + 1,
                 end_idx);
        
        // Prepare batch of path references
        let batch_paths: Vec<&std::path::Path> = chunk.iter().map(|p| p.as_path()).collect();
        
        match encoder.encode_batch(&batch_paths) {
            Ok(embeddings) => {
                // Match embeddings back to their paths
                for (path, embedding) in chunk.iter().zip(embeddings.iter()) {
                    embeddings_map.insert(path.clone(), embedding.clone());
                    index.add_image(path.clone(), Array1::from_vec(embedding.clone()));
                }
                println!("  Successfully encoded {} images", embeddings.len());
            }
            Err(e) => {
                eprintln!("Batch encoding failed: {}", e);
                eprintln!("Falling back to individual encoding for this batch...");
                
                // Fallback: encode individually for this batch
                for path in chunk {
                    match encoder.encode(path) {
                        Ok(embedding) => {
                            embeddings_map.insert(path.clone(), embedding.clone());
                            index.add_image(path.clone(), Array1::from_vec(embedding.clone()));
                        }
                        Err(e) => {
                            eprintln!("Failed to encode {:?}: {}", path.file_name().unwrap(), e);
                        }
                    }
                }
            }
        }
    }

    println!("\nSuccessfully encoded {} images", index.cached_images.len());
    assert!(index.cached_images.len() > 0, "No images were successfully encoded");

    // Test 1: Search for similar images to the first image
    println!("\n=== Test 1: Similarity search for first image ===");
    let query_path = &image_paths[0];
    let query_embedding = embeddings_map.get(query_path).expect("Query image not encoded");
    
    let similar_images = index.get_similar_images(&Array1::from_vec(query_embedding.clone()), 5);
    
    println!("Query image: {:?}", query_path.file_name().unwrap());
    println!("Top 5 similar images:");
    for (i, (path, similarity)) in similar_images.iter().enumerate() {
        println!("  {}. {:?} - similarity: {:.4}", i + 1, path.file_name().unwrap(), similarity);
    }
    
    assert!(similar_images.len() > 0, "Should return at least one similar image");
    
    // Check that similarities are in valid range
    for (_, similarity) in &similar_images {
        assert!(*similarity >= -1.0 && *similarity <= 1.0, 
                "Similarity score out of range: {}", similarity);
    }

    // Test 2: Search for multiple different query images
    println!("\n=== Test 2: Testing multiple queries ===");
    let num_queries = image_paths.len().min(3); // Test with first 3 images
    
    for query_idx in 0..num_queries {
        let query_path = &image_paths[query_idx];
        let query_embedding = embeddings_map.get(query_path).unwrap();
        
        println!("\nQuery {}: {:?}", query_idx + 1, query_path.file_name().unwrap());
        
        let results = index.get_similar_images(&Array1::from_vec(query_embedding.clone()), 3);
        
        println!("  Top 3 matches:");
        for (i, (path, sim)) in results.iter().enumerate() {
            println!("    {}. {:?} (sim: {:.4})", i + 1, path.file_name().unwrap(), sim);
        }
        
        // Verify each query returns results
        assert!(results.len() > 0, "Query {} returned no results", query_idx);
    }

    // Test 3: Check embedding dimensions
    println!("\n=== Test 3: Verifying embedding properties ===");
    for (path, embedding) in embeddings_map.iter().take(5) {
        println!("Image: {:?}", path.file_name().unwrap());
        println!("  Embedding shape: {}", embedding.len());
        println!("  Embedding range: [{:.4}, {:.4}]", 
                 embedding.iter().cloned().fold(f32::INFINITY, f32::min),
                 embedding.iter().cloned().fold(f32::NEG_INFINITY, f32::max));
        
        // Verify embedding dimension matches CLIP output (512 for ViT-B/32)
        assert_eq!(embedding.len(), 512, "Embedding dimension should be 512");
    }

    // Test 4: Verify similarity scores make sense
    println!("\n=== Test 4: Sanity check on similarity scores ===");
    
    // Self-similarity should be very high (close to 1.0)
    let self_path = &image_paths[0];
    let self_embedding = embeddings_map.get(self_path).unwrap();
    let self_similarity = CosineIndex::cosine_similarity(&Array1::from_vec(self_embedding.clone()), &Array1::from_vec(self_embedding.clone()));
    
    println!("Self-similarity (image compared to itself): {:.6}", self_similarity);
    assert!((self_similarity - 1.0).abs() < 1e-4, 
            "Self-similarity should be ~1.0, got {}", self_similarity);
    
    // Compare first and second image
    if image_paths.len() > 1 {
        let embedding_a = embeddings_map.get(&image_paths[0]).unwrap();
        let embedding_b = embeddings_map.get(&image_paths[1]).unwrap();
        let cross_similarity = CosineIndex::cosine_similarity(&Array1::from_vec(embedding_a.clone()), &Array1::from_vec(embedding_b.clone()));
        
        println!("Cross-similarity (first vs second image): {:.6}", cross_similarity);
        println!("  Image A: {:?}", image_paths[0].file_name().unwrap());
        println!("  Image B: {:?}", image_paths[1].file_name().unwrap());
        
        // Cross-similarity should be less than self-similarity (unless images are identical)
        assert!(cross_similarity < 1.0, 
                "Cross-similarity between different images should be < 1.0");
    }

    println!("\n=== All tests passed! ===");
}

#[test]
#[ignore]
fn test_similarity_distribution() {
    // This test analyzes the distribution of similarity scores
    let references_dir = PathBuf::from("references");
    
    if !references_dir.exists() {
        println!("Skipping test: 'references' folder not found");
        return;
    }

    println!("Loading encoder...");
    let model_path = PathBuf::from("models/model.onnx");
    
    if !model_path.exists() {
        println!("Skipping test: model file not found at {:?}", model_path);
        return;
    }
    
    let mut encoder = Encoder::new(&model_path).expect("Failed to create encoder");

    // Find all images
    let mut image_paths = Vec::new();
    for entry in fs::read_dir(&references_dir).expect("Failed to read references directory") {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();
        
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if matches!(ext_str.as_str(), "jpg" | "jpeg" | "png" | "webp" | "bmp") {
                    image_paths.push(path);
                }
            }
        }
    }

    // Encode subset of images using batch processing (limit to 20 for speed)
    let sample_size = image_paths.len().min(20);
    println!("Encoding {} images in batch...", sample_size);
    
    let sample_paths: Vec<&std::path::Path> = image_paths.iter()
        .take(sample_size)
        .map(|p| p.as_path())
        .collect();
    
    let embeddings_vec = encoder.encode_batch(&sample_paths)
        .expect("Failed to encode batch");
    
    // Pair paths with embeddings
    let embeddings: Vec<(PathBuf, Vec<f32>)> = image_paths.iter()
        .take(sample_size)
        .cloned()
        .zip(embeddings_vec.into_iter())
        .collect();

    println!("Computing pairwise similarities...");
    let mut similarities = Vec::new();
    
    for i in 0..embeddings.len() {
        for j in (i + 1)..embeddings.len() {
            let sim = CosineIndex::cosine_similarity(
                &Array1::from_vec(embeddings[i].1.clone()), 
                &Array1::from_vec(embeddings[j].1.clone())
            );
            similarities.push(sim);
        }
    }

    // Compute statistics
    let mean = similarities.iter().sum::<f32>() / similarities.len() as f32;
    let min = similarities.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = similarities.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    
    println!("\nSimilarity distribution:");
    println!("  Samples: {}", similarities.len());
    println!("  Mean: {:.4}", mean);
    println!("  Min: {:.4}", min);
    println!("  Max: {:.4}", max);
    
    // Create histogram
    let buckets = 10;
    let mut histogram = vec![0; buckets];
    
    for sim in &similarities {
        let bucket = ((sim + 1.0) / 2.0 * buckets as f32).floor() as usize;
        let bucket = bucket.min(buckets - 1);
        histogram[bucket] += 1;
    }
    
    println!("\nHistogram (similarity range -1.0 to 1.0):");
    for (i, count) in histogram.iter().enumerate() {
        let range_start = -1.0 + (i as f32 * 2.0 / buckets as f32);
        let range_end = -1.0 + ((i + 1) as f32 * 2.0 / buckets as f32);
        let bar = "█".repeat((*count as f32 / similarities.len() as f32 * 50.0) as usize);
        println!("  [{:5.2}, {:5.2}): {} ({})", range_start, range_end, bar, count);
    }
}
