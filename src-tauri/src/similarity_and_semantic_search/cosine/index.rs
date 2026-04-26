use super::math::{cosine_similarity, score_cmp_desc};
use crate::db;
use ndarray::Array1;
use rand::prelude::*;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info, warn};

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
    pub(super) scratch: Vec<(usize, f32)>,
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

    /// Populate the in-memory index from the per-encoder embeddings
    /// table, picking only rows for the given encoder_id.
    ///
    /// Used by the encoder-picker dispatch: when the user switches
    /// the chosen image encoder, the cache is wiped and repopulated
    /// from this method. The on-disk embeddings stay intact (one row
    /// per (image_id, encoder_id)) so swapping back to a previously-
    /// used encoder is instant — the embeddings are already there.
    #[tracing::instrument(name = "cosine.populate_for_encoder", skip(self, db))]
    pub fn populate_from_db_for_encoder(
        &mut self,
        db: &db::ImageDatabase,
        encoder_id: &str,
    ) {
        let start = Instant::now();
        info!("populate_from_db_for_encoder({encoder_id})");
        let rows = match db.get_all_embeddings_for(encoder_id) {
            Ok(r) => r,
            Err(e) => {
                warn!("populate_from_db_for_encoder({encoder_id}) failed: {e}");
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
            "populate_for_encoder({encoder_id}) done: {} embeddings in {:?}",
            self.cached_images.len(),
            start.elapsed()
        );
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

    // Function to compute cosine similarity between two embeddings
    //
    // Thin delegate to `super::math::cosine_similarity` so the existing
    // `CosineIndex::cosine_similarity(&a, &b)` call path stays valid
    // for callers (notably `tests/similarity_integration_test.rs`).
    pub fn cosine_similarity(a: &Array1<f32>, b: &Array1<f32>) -> f32 {
        cosine_similarity(a, b)
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
}
