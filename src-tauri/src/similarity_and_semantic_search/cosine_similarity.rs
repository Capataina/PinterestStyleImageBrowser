use crate::db;
use crate::paths;
use ndarray::Array1;
use rand::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Comparator for `(usize, f32)` similarity tuples — sorts by score
/// descending, NaN-tolerant. Pulled out as a free function so the
/// three retrieval methods + `select_nth_unstable_by` and `sort_by`
/// both reference one definition (was previously triplicated inline).
///
/// NaN is treated as smaller than every real number — a NaN score is
/// never preferred over a real score. Two NaNs compare equal.
fn score_cmp_desc(a: &(usize, f32), b: &(usize, f32)) -> std::cmp::Ordering {
    match (b.1.is_nan(), a.1.is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater, // b is NaN, a wins
        (false, true) => std::cmp::Ordering::Less,    // a is NaN, b wins
        (false, false) => b.1.partial_cmp(&a.1).unwrap(),
    }
}

pub struct CosineIndex {
    pub cached_images: Vec<(PathBuf, Array1<f32>)>,
    /// Reusable scratch buffer for per-query similarity calculations.
    /// Holds `(index_into_cached_images, similarity)` tuples — keyed
    /// by index so the inner loop never clones a `PathBuf`. Cleared
    /// on entry to each retrieval method, capacity is preserved.
    ///
    /// Composes with `select_nth_unstable_by` (used by
    /// `get_similar_images_sorted` and the diversity-pool prefix in
    /// `get_similar_images`) to make warm queries effectively
    /// allocation-free in the inner loop.
    scratch: Vec<(usize, f32)>,
}

impl CosineIndex {
    pub fn new() -> Self {
        CosineIndex {
            cached_images: Vec::new(),
            scratch: Vec::new(),
        }
    }

    pub fn add_image(&mut self, path: PathBuf, embedding: Array1<f32>) {
        self.cached_images.push((path, embedding));
    }

    /// Populate the in-memory index by SELECTing every embedding in one
    /// query.
    ///
    /// Replaces the previous N+1 implementation (one SELECT per image)
    /// which was ~30x slower for libraries of 1000+ images. Also takes
    /// `&ImageDatabase` rather than a `db_path: &str` so the cosine
    /// module no longer opens its own second SQLite connection.
    #[tracing::instrument(name = "cosine.populate_from_db", skip(self, db))]
    pub fn populate_from_db(&mut self, db: &db::ImageDatabase) {
        let start = Instant::now();
        info!("populate_from_db called");
        let rows = match db.get_all_embeddings() {
            Ok(r) => r,
            Err(e) => {
                warn!("populate_from_db: get_all_embeddings failed: {e}");
                return;
            }
        };
        let total = rows.len();
        self.cached_images.clear();
        self.cached_images.reserve(total);
        for (_id, path, embedding) in rows {
            if embedding.is_empty() {
                continue;
            }
            self.cached_images
                .push((PathBuf::from(path), Array1::from_vec(embedding)));
        }
        info!(
            "Population complete: {} embeddings loaded in {:?}",
            self.cached_images.len(),
            start.elapsed()
        );
    }

    /// Persist the in-memory index to disk for fast next-launch load.
    /// The cache is keyed by file mtime — a future startup will only
    /// trust it if the SQLite DB hasn't been modified since.
    ///
    /// Failure is non-fatal — we just log; the next launch will
    /// repopulate from the DB.
    pub fn save_to_disk(&self) {
        self.save_to_path(&paths::cosine_cache_path());
    }

    /// Path-explicit variant of `save_to_disk`. Used by tests that
    /// want to write into a tempdir rather than the live app data
    /// directory; production callers should use `save_to_disk`.
    pub fn save_to_path(&self, path: &std::path::Path) {
        // Convert to a (String, Vec<f32>) shape so bincode can serialise
        // without needing PathBuf serde support.
        let serialisable: Vec<(String, Vec<f32>)> = self
            .cached_images
            .iter()
            .map(|(p, e)| (p.to_string_lossy().into_owned(), e.to_vec()))
            .collect();
        match bincode::serialize(&serialisable) {
            Ok(bytes) => match fs::write(path, bytes) {
                Ok(_) => info!(
                    "cosine cache saved to {} ({} entries)",
                    path.display(),
                    self.cached_images.len()
                ),
                Err(e) => warn!("cosine cache write failed: {e}"),
            },
            Err(e) => warn!("cosine cache serialise failed: {e}"),
        }
    }

    /// Try to load the cache from disk. Returns true if the cache was
    /// loaded successfully and is fresher than the SQLite DB; false
    /// otherwise (caller should fall back to populate_from_db).
    pub fn load_from_disk_if_fresh(&mut self, db_path: &std::path::Path) -> bool {
        self.load_from_path_if_fresh(&paths::cosine_cache_path(), db_path)
    }

    /// Path-explicit variant of `load_from_disk_if_fresh`. The two
    /// arguments are the cache file and the DB file whose mtime is
    /// the freshness benchmark.
    pub fn load_from_path_if_fresh(
        &mut self,
        cache_path: &std::path::Path,
        db_path: &std::path::Path,
    ) -> bool {
        if !cache_path.exists() {
            return false;
        }

        let cache_mtime = match fs::metadata(cache_path).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(e) => {
                warn!("could not stat cosine cache: {e}");
                return false;
            }
        };
        let db_mtime = match fs::metadata(db_path).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => {
                // If we can't stat the DB, we can't trust the cache
                // either. Fall through to repopulate.
                return false;
            }
        };
        if cache_mtime < db_mtime {
            debug!("cosine cache stale (DB modified since save); refusing");
            return false;
        }

        let bytes = match fs::read(cache_path) {
            Ok(b) => b,
            Err(e) => {
                warn!("cosine cache read failed: {e}");
                return false;
            }
        };
        let parsed: Vec<(String, Vec<f32>)> = match bincode::deserialize(&bytes) {
            Ok(p) => p,
            Err(e) => {
                warn!("cosine cache deserialise failed: {e}; will repopulate");
                return false;
            }
        };

        self.cached_images.clear();
        self.cached_images.reserve(parsed.len());
        for (p, e) in parsed {
            self.cached_images
                .push((PathBuf::from(p), Array1::from_vec(e)));
        }
        info!(
            "cosine cache loaded from disk ({} entries)",
            self.cached_images.len()
        );
        true
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
    // exclude_path: optional path to exclude from results (e.g., the query image itself)
    #[tracing::instrument(name = "cosine.get_similar_images", skip(self, embedding, exclude_path), fields(cached = self.cached_images.len(), top_n))]
    pub fn get_similar_images(
        &mut self,
        embedding: &Array1<f32>,
        top_n: usize,
        exclude_path: Option<&PathBuf>,
    ) -> Vec<(PathBuf, f32)> {
        debug!(
            "get_similar_images called - cached_images: {}, top_n: {}, exclude_path: {:?}",
            self.cached_images.len(),
            top_n,
            exclude_path
        );

        // Step 1: compute similarities for every cached image (except the
        // optionally-excluded query image) into the reusable scratch
        // buffer. Indices into cached_images, NOT cloned PathBufs — we
        // only clone the paths that actually survive into the final result.
        self.scratch.clear();
        let mut excluded_count = 0;
        for (idx, (path, emb)) in self.cached_images.iter().enumerate() {
            if let Some(exclude) = exclude_path {
                if path == exclude {
                    excluded_count += 1;
                    continue;
                }
            }
            let sim = Self::cosine_similarity(embedding, emb);
            self.scratch.push((idx, sim));
        }

        debug!(
            "Calculated similarities for {} images (excluded {}), query embedding length: {}",
            self.scratch.len(),
            excluded_count,
            embedding.len()
        );

        if self.scratch.is_empty() {
            warn!("No similarities calculated! Returning empty result.");
            return Vec::new();
        }

        // Diversity pool: top 20% by similarity (or top_n, whichever is
        // larger). Random sampling within the pool produces diversity
        // without sacrificing relevance.
        //
        // We use `select_nth_unstable_by` to partition around the
        // (select_count - 1)th best score in O(n) average — only the
        // pool needs to end up at the front of the buffer; ordering
        // *within* the pool doesn't matter because we random-sample.
        // This is the algorithm pinned by
        // tests/cosine_topk_partial_sort_diagnostic.rs.
        let base_pool = (self.scratch.len() as f32 * 0.2).ceil() as usize;
        let select_count = base_pool.max(top_n).min(self.scratch.len());
        if select_count > 0 && select_count < self.scratch.len() {
            self.scratch
                .select_nth_unstable_by(select_count - 1, score_cmp_desc);
            self.scratch.truncate(select_count);
        }

        debug!(
            "Diversity pool - base_pool: {}, select_count: {}, pool size: {}",
            base_pool,
            select_count,
            self.scratch.len()
        );

        // Random sample top_n from the pool. We sample indices into the
        // (now trimmed) scratch, then materialise the surviving paths
        // exactly once each.
        let mut rng = rand::rng();
        let take = top_n.min(self.scratch.len());
        let sampled: Vec<&(usize, f32)> =
            self.scratch.choose_multiple(&mut rng, take).collect();
        let selected: Vec<(PathBuf, f32)> = sampled
            .iter()
            .map(|(cache_idx, sim)| (self.cached_images[*cache_idx].0.clone(), *sim))
            .collect();

        debug!("Final selected results: {} images", selected.len());
        for (i, (path, sim)) in selected.iter().enumerate() {
            debug!(
                "  {}. {:?} - score: {:.4}",
                i + 1,
                path.file_name().unwrap_or_default(),
                sim
            );
        }

        selected
    }

    /// Get the top N most similar images sorted by similarity score (descending).
    /// Unlike get_similar_images, this does NOT randomly sample - it returns
    /// results in exact order of similarity. Best for semantic search where
    /// ranking accuracy matters.
    #[tracing::instrument(name = "cosine.get_similar_sorted", skip(self, embedding, exclude_path), fields(cached = self.cached_images.len(), top_n))]
    pub fn get_similar_images_sorted(
        &mut self,
        embedding: &Array1<f32>,
        top_n: usize,
        exclude_path: Option<&PathBuf>,
    ) -> Vec<(PathBuf, f32)> {
        debug!(
            "get_similar_images_sorted called - cached_images: {}, top_n: {}, exclude_path: {:?}",
            self.cached_images.len(),
            top_n,
            exclude_path
        );

        // Step 1: scratch buffer of (cache_idx, similarity) for every
        // non-excluded image. No PathBuf clones in the inner loop.
        self.scratch.clear();
        for (idx, (path, emb)) in self.cached_images.iter().enumerate() {
            if let Some(exclude) = exclude_path {
                if path == exclude {
                    continue;
                }
            }
            let sim = Self::cosine_similarity(embedding, emb);
            self.scratch.push((idx, sim));
        }

        if self.scratch.is_empty() {
            warn!("No similarities calculated! Returning empty result.");
            return Vec::new();
        }

        // Step 2: top-N selection. Partition around the (top_n - 1)th
        // best score using `select_nth_unstable_by` (O(n) average) so we
        // only pay the O(n log n) sort cost on the surviving top_n
        // elements. For top_n=50 over 1500 images this is the 2.53×
        // speedup measured in tests/cosine_topk_partial_sort_diagnostic.rs.
        //
        // The sort *after* the partial-select preserves the
        // descending-order contract that semantic-search and the modal
        // navigation rely on.
        let want = top_n.min(self.scratch.len());
        if want == 0 {
            return Vec::new();
        }
        if want < self.scratch.len() {
            self.scratch.select_nth_unstable_by(want - 1, score_cmp_desc);
            self.scratch.truncate(want);
        }
        self.scratch.sort_unstable_by(score_cmp_desc);

        // Step 3: materialise the surviving top_n into the return shape.
        // This is the only PathBuf clone — `want` clones, not `n`.
        let result: Vec<(PathBuf, f32)> = self
            .scratch
            .iter()
            .map(|(cache_idx, sim)| (self.cached_images[*cache_idx].0.clone(), *sim))
            .collect();

        debug!(
            "Returning {} results sorted by similarity",
            result.len()
        );

        if !result.is_empty() {
            debug!("Top 5 results:");
            for (i, (path, sim)) in result.iter().take(5).enumerate() {
                debug!(
                    "  {}. {:?} - score: {:.4}",
                    i + 1,
                    path.file_name().unwrap_or_default(),
                    sim
                );
            }
            if result.len() > 1 {
                debug!(
                    "Score range: {:.4} (best) to {:.4} (worst in top N)",
                    result.first().map(|(_, s)| *s).unwrap_or(0.0),
                    result.last().map(|(_, s)| *s).unwrap_or(0.0)
                );
            }
        }

        result
    }

    /// Get tiered similar images - Pinterest style
    /// Samples images from progressively less similar tiers:
    /// - 5 random from top 5%
    /// - 5 random from top 5-10%
    /// - 5 random from top 10-15%
    /// ... and so on until top 50%
    #[tracing::instrument(name = "cosine.get_tiered_similar", skip(self, embedding, exclude_path), fields(cached = self.cached_images.len()))]
    pub fn get_tiered_similar_images(
        &mut self,
        embedding: &Array1<f32>,
        exclude_path: Option<&PathBuf>,
    ) -> Vec<(PathBuf, f32)> {
        debug!(
            "get_tiered_similar_images called - cached_images: {}, exclude_path: {:?}",
            self.cached_images.len(),
            exclude_path
        );

        // Step 1: similarities into the scratch buffer (index-keyed,
        // no PathBuf clones in the inner loop).
        self.scratch.clear();
        for (idx, (path, emb)) in self.cached_images.iter().enumerate() {
            if let Some(exclude) = exclude_path {
                if path == exclude {
                    continue;
                }
            }
            let sim = Self::cosine_similarity(embedding, emb);
            self.scratch.push((idx, sim));
        }

        if self.scratch.is_empty() {
            warn!("No similarities calculated! Returning empty result.");
            return Vec::new();
        }

        // Tiered sampling needs a fully-sorted list because tiers span
        // 0-50% in 5% buckets. Partial-sort can't help here without
        // restructuring the tier definitions, so we keep the full sort
        // — but the scratch buffer still wins by skipping the per-item
        // PathBuf clone.
        self.scratch.sort_unstable_by(score_cmp_desc);

        let total = self.scratch.len();
        let mut result: Vec<(PathBuf, f32)> = Vec::new();
        let mut rng = rand::rng();
        let mut used_indices: std::collections::HashSet<usize> =
            std::collections::HashSet::new();

        // Sample from each tier: 0-5%, 5-10%, 10-15%, ..., 45-50%
        let tiers = [
            (0.0, 0.05, 5),  // top 5%: pick 5
            (0.05, 0.10, 5), // 5-10%: pick 5
            (0.10, 0.15, 5), // 10-15%: pick 5
            (0.15, 0.20, 5), // 15-20%: pick 5
            (0.20, 0.30, 5), // 20-30%: pick 5
            (0.30, 0.40, 5), // 30-40%: pick 5
            (0.40, 0.50, 5), // 40-50%: pick 5
        ];

        for (start_pct, end_pct, count) in tiers {
            let start_idx = (total as f32 * start_pct).floor() as usize;
            let end_idx = (total as f32 * end_pct).ceil() as usize;
            let end_idx = end_idx.min(total);

            if start_idx >= total {
                break;
            }

            // Get scratch-indices in this tier that haven't been used
            let available: Vec<usize> = (start_idx..end_idx)
                .filter(|i| !used_indices.contains(i))
                .collect();

            let to_take = count.min(available.len());
            let sampled: Vec<usize> = available
                .choose_multiple(&mut rng, to_take)
                .cloned()
                .collect();

            // Resolve scratch index → cache index → (PathBuf, f32) here.
            // Only sampled items pay the clone cost.
            for scratch_idx in sampled {
                used_indices.insert(scratch_idx);
                let (cache_idx, sim) = self.scratch[scratch_idx];
                result.push((self.cached_images[cache_idx].0.clone(), sim));
            }
        }

        debug!(
            "Tiered sampling complete - returned {} images from {} total",
            result.len(),
            total
        );

        // Log score ranges
        if !result.is_empty() {
            let scores: Vec<f32> = result.iter().map(|(_, s)| *s).collect();
            let min_score = scores.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_score = scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            debug!(
                "Score range: {:.4} to {:.4}",
                min_score, max_score
            );
        }

        result
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

        assert!(
            (similarity - 1.0).abs() < 1e-6,
            "Identical vectors should have similarity ~1.0, got {}",
            similarity
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        // Orthogonal vectors (perpendicular) should have similarity of 0.0
        let a = array![1.0, 0.0, 0.0];
        let b = array![0.0, 1.0, 0.0];

        let similarity = CosineIndex::cosine_similarity(&a, &b);

        assert!(
            similarity.abs() < 1e-6,
            "Orthogonal vectors should have similarity ~0.0, got {}",
            similarity
        );
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        // Opposite vectors should have similarity of -1.0
        let a = array![1.0, 2.0, 3.0];
        let b = array![-1.0, -2.0, -3.0];

        let similarity = CosineIndex::cosine_similarity(&a, &b);

        assert!(
            (similarity - (-1.0)).abs() < 1e-6,
            "Opposite vectors should have similarity ~-1.0, got {}",
            similarity
        );
    }

    #[test]
    fn test_cosine_similarity_parallel_vectors_different_magnitude() {
        // Parallel vectors with different magnitudes should still have similarity ~1.0
        let a = array![1.0, 2.0, 3.0];
        let b = array![2.0, 4.0, 6.0]; // Same direction, double magnitude

        let similarity = CosineIndex::cosine_similarity(&a, &b);

        assert!(
            (similarity - 1.0).abs() < 1e-6,
            "Parallel vectors should have similarity ~1.0 regardless of magnitude, got {}",
            similarity
        );
    }

    #[test]
    fn test_cosine_similarity_45_degree_vectors() {
        // Two vectors at 45 degrees should have similarity of ~0.707 (cos(45°) = √2/2)
        let a = array![1.0, 0.0];
        let b = array![1.0, 1.0]; // 45 degrees from x-axis

        let similarity = CosineIndex::cosine_similarity(&a, &b);
        let expected = 1.0 / 2.0_f32.sqrt(); // cos(45°) = 1/√2 ≈ 0.707

        assert!(
            (similarity - expected).abs() < 1e-6,
            "45-degree vectors should have similarity ~0.707, got {}",
            similarity
        );
    }

    #[test]
    fn test_cosine_similarity_high_dimensional() {
        // Test with 512-dimensional vectors (same as CLIP embeddings)
        let a = Array1::from_vec(vec![1.0; 512]);
        let b = Array1::from_vec(vec![1.0; 512]);

        let similarity = CosineIndex::cosine_similarity(&a, &b);

        assert!(
            (similarity - 1.0).abs() < 1e-6,
            "High-dimensional identical vectors should have similarity ~1.0, got {}",
            similarity
        );
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

        assert!(
            similarity.abs() < 1e-6,
            "High-dimensional orthogonal vectors should have similarity ~0.0, got {}",
            similarity
        );
    }

    #[test]
    fn test_cosine_similarity_normalized_vectors() {
        // Test with pre-normalized vectors (unit vectors)
        let a = array![0.6, 0.8]; // length = 1
        let b = array![0.8, 0.6]; // length = 1

        let similarity = CosineIndex::cosine_similarity(&a, &b);

        // For normalized vectors, cosine similarity equals dot product
        let expected_dot = 0.6 * 0.8 + 0.8 * 0.6;
        assert!(
            (similarity - expected_dot).abs() < 1e-6,
            "Normalized vectors: similarity should equal dot product, got {}",
            similarity
        );
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
        index.add_image(
            PathBuf::from("/images/somewhat_similar.jpg"),
            somewhat_similar,
        );
        index.add_image(PathBuf::from("/images/dissimilar.jpg"), dissimilar);

        // Search for images similar to query
        let results = index.get_similar_images(&query_embedding, 2, None);

        // Should return 2 results
        assert_eq!(results.len(), 2);

        // The very_similar image should have higher similarity than somewhat_similar
        // (though order might vary due to random sampling from top 20%)
        let similarities: Vec<f32> = results.iter().map(|(_, sim)| *sim).collect();

        // All returned similarities should be reasonable (between -1 and 1)
        for sim in &similarities {
            assert!(
                *sim >= -1.0 && *sim <= 1.0,
                "Similarity out of bounds: {}",
                sim
            );
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
        let results = index.get_similar_images(&query, 10, None);

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
        let results = index.get_similar_images(&query, 10, None);

        // Should return only what's available (3 or fewer due to 20% sampling)
        assert!(results.len() <= 3, "Returned more images than available");
    }

    #[test]
    fn test_empty_index() {
        // The retrieval methods take &mut self for the scratch buffer.
        let mut index = CosineIndex::new();
        let query = array![1.0, 2.0, 3.0];

        let results = index.get_similar_images(&query, 5, None);

        assert_eq!(results.len(), 0, "Empty index should return no results");
    }

    // ============================================================
    //  Phase 5: cosine cache disk persistence
    // ============================================================

    #[test]
    fn cache_save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("cosine.bin");
        let db_path = dir.path().join("fake.db");
        // Touch a "DB" file so freshness check has something to compare against.
        std::fs::write(&db_path, b"").unwrap();

        // Wait briefly so the cache mtime can land >= the DB mtime.
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Build an index, save it, load into a fresh index, compare.
        let mut original = CosineIndex::new();
        original.add_image(PathBuf::from("/a.jpg"), array![1.0, 2.0, 3.0]);
        original.add_image(PathBuf::from("/b.jpg"), array![0.5, 0.5, 0.5]);
        original.save_to_path(&cache_path);

        let mut loaded = CosineIndex::new();
        let ok = loaded.load_from_path_if_fresh(&cache_path, &db_path);
        assert!(ok, "load should succeed when cache is fresher than DB");
        assert_eq!(loaded.cached_images.len(), 2);
        assert_eq!(loaded.cached_images[0].0, PathBuf::from("/a.jpg"));
        assert_eq!(loaded.cached_images[1].0, PathBuf::from("/b.jpg"));
        // Embeddings should round-trip exactly (bincode is bit-for-bit).
        assert_eq!(loaded.cached_images[0].1.to_vec(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn cache_refuses_stale_cache_when_db_is_newer() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("cosine.bin");
        let db_path = dir.path().join("fake.db");

        let mut idx = CosineIndex::new();
        idx.add_image(PathBuf::from("/x.jpg"), array![1.0, 2.0]);
        idx.save_to_path(&cache_path);

        // Sleep, then touch the DB so its mtime is now newer than the
        // cache. Subsequent load should refuse.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&db_path, b"changed").unwrap();

        let mut loaded = CosineIndex::new();
        let ok = loaded.load_from_path_if_fresh(&cache_path, &db_path);
        assert!(!ok, "load should refuse when DB is newer than cache");
        assert!(loaded.cached_images.is_empty());
    }

    #[test]
    fn cache_returns_false_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("does-not-exist.bin");
        let db_path = dir.path().join("fake.db");
        std::fs::write(&db_path, b"").unwrap();

        let mut loaded = CosineIndex::new();
        let ok = loaded.load_from_path_if_fresh(&cache_path, &db_path);
        assert!(!ok);
    }

    #[test]
    fn cache_handles_corrupt_file_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("corrupt.bin");
        let db_path = dir.path().join("fake.db");
        std::fs::write(&db_path, b"").unwrap();
        // Junk bytes that bincode can't parse.
        std::fs::write(&cache_path, b"NOT VALID BINCODE").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut loaded = CosineIndex::new();
        let ok = loaded.load_from_path_if_fresh(&cache_path, &db_path);
        assert!(!ok, "corrupt cache should fall through, not panic");
        assert!(loaded.cached_images.is_empty());
    }

    #[test]
    fn cache_overwrites_on_resave() {
        // save -> load -> save with different data -> load -> verifies
        // we get the new data, not stale-cached state.
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("cosine.bin");
        let db_path = dir.path().join("fake.db");
        std::fs::write(&db_path, b"").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut idx = CosineIndex::new();
        idx.add_image(PathBuf::from("/a.jpg"), array![1.0, 1.0]);
        idx.save_to_path(&cache_path);

        let mut idx2 = CosineIndex::new();
        idx2.add_image(PathBuf::from("/b.jpg"), array![2.0, 2.0]);
        idx2.add_image(PathBuf::from("/c.jpg"), array![3.0, 3.0]);
        idx2.save_to_path(&cache_path);

        let mut loaded = CosineIndex::new();
        loaded.load_from_path_if_fresh(&cache_path, &db_path);
        assert_eq!(loaded.cached_images.len(), 2);
        assert_eq!(loaded.cached_images[0].0, PathBuf::from("/b.jpg"));
    }
}
