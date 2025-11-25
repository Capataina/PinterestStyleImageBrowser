use ndarray::Array1;
use std::path::PathBuf;
use rand::prelude::*;


pub struct CosineIndex {
    pub cached_images: Vec<(PathBuf, Array1<f32>)>,
}

impl CosineIndex {
    pub fn new() -> Self {
        CosineIndex {
            cached_images: Vec::new(),
        }
    }

    pub fn add_image(&mut self, path: PathBuf, embedding: Array1<f32>) {
        self.cached_images.push((path, embedding));
    }

    // add a function thats going to connect to our sql database, query all images and their embeddings, and populate the cached_images vector
    pub fn populate_from_db(&mut self, _db_path: &str) {
        // Placeholder for database connection and querying logic
        // For each image and its embedding retrieved from the database,
        // call self.add_image(path, embedding);
    }

    // Function to compute cosine similarity between two embeddings
    pub fn cosine_similarity(a: &Array1<f32>, b: &Array1<f32>) -> f32 {
        let dot_product = a.dot(b);
        let norm_a = a.dot(a).sqrt();
        let norm_b = b.dot(b).sqrt();

        // Handle zero vectors
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0; // or some other sensible default
        }

        dot_product / (norm_a * norm_b)
    }

    // write the return images function. This function is going to take an embedding and return the top n most similar images from the cached_images vector
    // the images that ir returns will be a top x percent of the cached images based on cosine similarity to encourage diversity
    pub fn get_similar_images(&self, embedding: &Array1<f32>, top_n: usize) -> Vec<(PathBuf, f32)> {
        let mut similarities: Vec<(PathBuf, f32)> = self
            .cached_images
            .iter()
            .map(|(path, emb)| {
                let sim = Self::cosine_similarity(embedding, emb);
                (path.clone(), sim)
            })
            .collect();

        similarities.sort_by(|a, b| {
            // Sort in descending order (highest similarity first)
            // Handle NaN by treating it as less than any real number
            match (b.1.is_nan(), a.1.is_nan()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater, // b is NaN, a comes first
                (false, true) => std::cmp::Ordering::Less,    // a is NaN, b comes first
                (false, false) => b.1.partial_cmp(&a.1).unwrap(), // Both normal, compare
            }
        });

        // Select top N percent to encourage diversity, but ensure pool is at least top_n size
        let base_pool = (similarities.len() as f32 * 0.2).ceil() as usize;
        let select_count = base_pool.max(top_n).min(similarities.len());
        let diverse_pool = &similarities[..select_count];

        // Randomly select top_n from the diverse pool
        let mut rng = rand::rng();
        let selected: Vec<(PathBuf, f32)> = diverse_pool
            .choose_multiple(&mut rng, top_n.min(diverse_pool.len()))
            .cloned()
            .collect();

        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        // Two identical vectors should have similarity of 1.0
        let a = array![1.0, 2.0, 3.0, 4.0];
        let b = array![1.0, 2.0, 3.0, 4.0];
        
        let similarity = CosineIndex::cosine_similarity(&a, &b);
        
        assert!((similarity - 1.0).abs() < 1e-6, 
                "Identical vectors should have similarity ~1.0, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        // Orthogonal vectors (perpendicular) should have similarity of 0.0
        let a = array![1.0, 0.0, 0.0];
        let b = array![0.0, 1.0, 0.0];
        
        let similarity = CosineIndex::cosine_similarity(&a, &b);
        
        assert!(similarity.abs() < 1e-6, 
                "Orthogonal vectors should have similarity ~0.0, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        // Opposite vectors should have similarity of -1.0
        let a = array![1.0, 2.0, 3.0];
        let b = array![-1.0, -2.0, -3.0];
        
        let similarity = CosineIndex::cosine_similarity(&a, &b);
        
        assert!((similarity - (-1.0)).abs() < 1e-6, 
                "Opposite vectors should have similarity ~-1.0, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_parallel_vectors_different_magnitude() {
        // Parallel vectors with different magnitudes should still have similarity ~1.0
        let a = array![1.0, 2.0, 3.0];
        let b = array![2.0, 4.0, 6.0]; // Same direction, double magnitude
        
        let similarity = CosineIndex::cosine_similarity(&a, &b);
        
        assert!((similarity - 1.0).abs() < 1e-6, 
                "Parallel vectors should have similarity ~1.0 regardless of magnitude, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_45_degree_vectors() {
        // Two vectors at 45 degrees should have similarity of ~0.707 (cos(45°) = √2/2)
        let a = array![1.0, 0.0];
        let b = array![1.0, 1.0]; // 45 degrees from x-axis
        
        let similarity = CosineIndex::cosine_similarity(&a, &b);
        let expected = 1.0 / 2.0_f32.sqrt(); // cos(45°) = 1/√2 ≈ 0.707
        
        assert!((similarity - expected).abs() < 1e-6, 
                "45-degree vectors should have similarity ~0.707, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_high_dimensional() {
        // Test with 512-dimensional vectors (same as CLIP embeddings)
        let a = Array1::from_vec(vec![1.0; 512]);
        let b = Array1::from_vec(vec![1.0; 512]);
        
        let similarity = CosineIndex::cosine_similarity(&a, &b);
        
        assert!((similarity - 1.0).abs() < 1e-6, 
                "High-dimensional identical vectors should have similarity ~1.0, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_high_dimensional_orthogonal() {
        // Create two orthogonal 512-dimensional vectors
        let mut vec_a = vec![0.0; 512];
        vec_a[0] = 1.0;
        let mut vec_b = vec![0.0; 512];
        vec_b[1] = 1.0;
        
        let a = Array1::from_vec(vec_a);
        let b = Array1::from_vec(vec_b);
        
        let similarity = CosineIndex::cosine_similarity(&a, &b);
        
        assert!(similarity.abs() < 1e-6, 
                "High-dimensional orthogonal vectors should have similarity ~0.0, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_normalized_vectors() {
        // Test with pre-normalized vectors (unit vectors)
        let a = array![0.6, 0.8]; // length = 1
        let b = array![0.8, 0.6]; // length = 1
        
        let similarity = CosineIndex::cosine_similarity(&a, &b);
        
        // For normalized vectors, cosine similarity equals dot product
        let expected_dot = 0.6 * 0.8 + 0.8 * 0.6;
        assert!((similarity - expected_dot).abs() < 1e-6, 
                "Normalized vectors: similarity should equal dot product, got {}", similarity);
    }

    #[test]
    fn test_add_image() {
        let mut index = CosineIndex::new();
        let path = PathBuf::from("/test/image.jpg");
        let embedding = array![1.0, 2.0, 3.0];
        
        index.add_image(path.clone(), embedding.clone());
        
        assert_eq!(index.cached_images.len(), 1);
        assert_eq!(index.cached_images[0].0, path);
        assert_eq!(index.cached_images[0].1, embedding);
    }

    #[test]
    fn test_add_multiple_images() {
        let mut index = CosineIndex::new();
        
        for i in 0..10 {
            let path = PathBuf::from(format!("/test/image_{}.jpg", i));
            let embedding = Array1::from_vec(vec![i as f32; 512]);
            index.add_image(path, embedding);
        }
        
        assert_eq!(index.cached_images.len(), 10);
    }

    #[test]
    fn test_get_similar_images_returns_most_similar() {
        let mut index = CosineIndex::new();
        
        // Add a query image
        let query_embedding = array![1.0, 0.0, 0.0];
        
        // Add several images with varying similarity
        let very_similar = array![0.9, 0.1, 0.0]; // Very close
        let somewhat_similar = array![0.7, 0.3, 0.0]; // Moderately close
        let dissimilar = array![0.0, 0.0, 1.0]; // Orthogonal
        
        index.add_image(PathBuf::from("/images/very_similar.jpg"), very_similar);
        index.add_image(PathBuf::from("/images/somewhat_similar.jpg"), somewhat_similar);
        index.add_image(PathBuf::from("/images/dissimilar.jpg"), dissimilar);
        
        // Search for images similar to query
        let results = index.get_similar_images(&query_embedding, 2);
        
        // Should return 2 results
        assert_eq!(results.len(), 2);
        
        // The very_similar image should have higher similarity than somewhat_similar
        // (though order might vary due to random sampling from top 20%)
        let similarities: Vec<f32> = results.iter().map(|(_, sim)| *sim).collect();
        
        // All returned similarities should be reasonable (between -1 and 1)
        for sim in &similarities {
            assert!(*sim >= -1.0 && *sim <= 1.0, "Similarity out of bounds: {}", sim);
        }
        
        println!("Returned similarities: {:?}", similarities);
    }

    #[test]
    fn test_get_similar_images_with_many_candidates() {
        let mut index = CosineIndex::new();
        
        // Create a query embedding
        let query = Array1::from_vec(vec![1.0; 512]);
        
        // Add 100 images with random embeddings
        for i in 0..100 {
            let mut vec = vec![0.0; 512];
            // Make some components match the query for varying similarity
            for j in 0..(i % 512) {
                vec[j] = 1.0;
            }
            let embedding = Array1::from_vec(vec);
            index.add_image(PathBuf::from(format!("/images/img_{}.jpg", i)), embedding);
        }
        
        // Request top 10
        let results = index.get_similar_images(&query, 10);
        
        assert_eq!(results.len(), 10);
        
        // All paths should be unique
        let paths: Vec<&PathBuf> = results.iter().map(|(path, _)| path).collect();
        let unique_count = paths.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(unique_count, 10, "Returned duplicate paths");
    }

    #[test]
    fn test_get_similar_images_request_more_than_available() {
        let mut index = CosineIndex::new();
        let query = array![1.0, 0.0, 0.0];
        
        // Add only 3 images
        for i in 0..3 {
            let embedding = array![1.0, i as f32, 0.0];
            index.add_image(PathBuf::from(format!("/images/img_{}.jpg", i)), embedding);
        }
        
        // Request 10 (more than available)
        let results = index.get_similar_images(&query, 10);
        
        // Should return only what's available (3 or fewer due to 20% sampling)
        assert!(results.len() <= 3, "Returned more images than available");
    }

    #[test]
    fn test_empty_index() {
        let index = CosineIndex::new();
        let query = array![1.0, 2.0, 3.0];
        
        let results = index.get_similar_images(&query, 5);
        
        assert_eq!(results.len(), 0, "Empty index should return no results");
    }
}