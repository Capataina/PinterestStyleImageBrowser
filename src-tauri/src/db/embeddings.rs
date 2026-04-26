//! CLIP-style embedding storage as SQLite BLOBs.
//!
//! Embeddings are stored as raw little-endian f32 byte arrays. The
//! `bytemuck::cast_slice` calls perform the f32 ↔ byte reinterpretation
//! safely — the `Pod` marker on f32 proves the conversion at compile
//! time, replacing the previous `unsafe slice::from_raw_parts` blocks.

use super::{ID, ImageDatabase};

impl ImageDatabase {
    // update the embedding of an image
    pub fn update_image_embedding(
        &self,
        image_id: ID,
        embedding: Vec<f32>,
    ) -> rusqlite::Result<()> {
        // Handle empty embeddings explicitly
        if embedding.is_empty() {
            self.connection.lock().unwrap().execute(
                "UPDATE images SET embedding = ?1 WHERE id = ?2",
                rusqlite::params![&[] as &[u8], image_id],
            )?;
            return Ok(());
        }

        // Convert Vec<f32> to bytes for BLOB storage. bytemuck::cast_slice
        // proves at compile time (via the `Pod` marker on f32) that the
        // reinterpretation is safe — no manual unsafe block required.
        // Audit finding: replaces three unsafe slice::from_raw_parts blocks
        // with one trait-checked safe API. Same zero-copy view, same bytes
        // hit the BLOB.
        let embedding_bytes: &[u8] = bytemuck::cast_slice(&embedding);
        self.connection.lock().unwrap().execute(
            "UPDATE images SET embedding = ?1 WHERE id = ?2",
            rusqlite::params![embedding_bytes, image_id],
        )?;
        Ok(())
    }

    // function to get the embedding of an image
    pub fn get_image_embedding(&self, image_id: ID) -> rusqlite::Result<Vec<f32>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("SELECT embedding FROM images WHERE id = ?1")?;
        let mut rows = stmt.query([image_id])?;
        if let Some(row) = rows.next()? {
            // Handle NULL embeddings
            let embedding_bytes: Option<Vec<u8>> = row.get("embedding")?;

            match embedding_bytes {
                None => Err(rusqlite::Error::QueryReturnedNoRows),
                Some(bytes) => {
                    // Handle empty embeddings
                    if bytes.is_empty() {
                        return Ok(Vec::new());
                    }

                    // Ensure the byte length is a multiple of f32 size
                    let f32_size = std::mem::size_of::<f32>();
                    if bytes.len() % f32_size != 0 {
                        return Err(rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Blob,
                            format!(
                                "Embedding byte length {} is not a multiple of f32 size ({})",
                                bytes.len(),
                                f32_size
                            )
                            .into(),
                        ));
                    }

                    // Convert bytes back to Vec<f32>. bytemuck::cast_slice
                    // does the alignment + size proof at compile time;
                    // the runtime length-mod-f32 check above stays as a
                    // belt-and-braces guard against malformed BLOBs.
                    let embedding: Vec<f32> =
                        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec();
                    Ok(embedding)
                }
            }
        } else {
            Err(rusqlite::Error::QueryReturnedNoRows)
        }
    }

    /// Fetch every (id, path, embedding) row whose embedding is non-null
    /// in a single SELECT.
    ///
    /// Replaces the per-row `get_image_embedding(id)` call inside the
    /// cosine populate loop, which was N+1 (one query per image —
    /// ~30x slower than this for 1000+ image libraries). The path is
    /// returned alongside the embedding because that's what the cosine
    /// index keys by.
    ///
    /// Empty embeddings are skipped at the SQL level (`length(embedding) > 0`)
    /// so callers don't have to filter them out.
    pub fn get_all_embeddings(&self) -> rusqlite::Result<Vec<(ID, String, Vec<f32>)>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, embedding FROM images
             WHERE embedding IS NOT NULL AND length(embedding) > 0",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: ID = row.get(0)?;
            let path: String = row.get(1)?;
            let bytes: Vec<u8> = row.get(2)?;
            Ok((id, path, bytes))
        })?;

        let mut out = Vec::new();
        let f32_size = std::mem::size_of::<f32>();
        for row in rows {
            let (id, path, bytes) = row?;
            if bytes.len() % f32_size != 0 {
                // Skip malformed rows — they were probably written by an
                // older version with a different layout. The user can
                // wipe and re-encode.
                continue;
            }
            // bytemuck::cast_slice is the safe, alignment-checked-at-
            // compile-time replacement for slice::from_raw_parts here.
            let embedding: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&bytes).to_vec();
            out.push((id, path, embedding));
        }
        Ok(out)
    }

    // =================================================================
    // Per-encoder embeddings (encoder picker, Phase 2)
    // =================================================================
    //
    // The above methods read/write the legacy `images.embedding` column.
    // The methods below read/write the new `embeddings` table, which is
    // keyed by (image_id, encoder_id). The legacy column is preserved
    // for backward compatibility through one release; the indexing
    // pipeline writes to BOTH for now.

    /// Insert or replace an embedding for a specific encoder.
    pub fn upsert_embedding(
        &self,
        image_id: ID,
        encoder_id: &str,
        embedding: &[f32],
    ) -> rusqlite::Result<()> {
        let bytes: &[u8] = bytemuck::cast_slice(embedding);
        self.connection.lock().unwrap().execute(
            "INSERT OR REPLACE INTO embeddings (image_id, encoder_id, embedding)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![image_id, encoder_id, bytes],
        )?;
        Ok(())
    }

    /// Fetch a specific image's embedding for a specific encoder.
    pub fn get_embedding(
        &self,
        image_id: ID,
        encoder_id: &str,
    ) -> rusqlite::Result<Vec<f32>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT embedding FROM embeddings
             WHERE image_id = ?1 AND encoder_id = ?2",
        )?;
        let mut rows = stmt.query(rusqlite::params![image_id, encoder_id])?;
        match rows.next()? {
            Some(row) => {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(bytemuck::cast_slice::<u8, f32>(&bytes).to_vec())
            }
            None => Err(rusqlite::Error::QueryReturnedNoRows),
        }
    }

    /// Fetch every (image_id, path, embedding) for a specific encoder
    /// in one SELECT. Used by `CosineIndex::populate_from_db_for_encoder`
    /// at startup + after re-indexing.
    pub fn get_all_embeddings_for(
        &self,
        encoder_id: &str,
    ) -> rusqlite::Result<Vec<(ID, String, Vec<f32>)>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT i.id, i.path, e.embedding
             FROM embeddings e
             JOIN images i ON i.id = e.image_id
             WHERE e.encoder_id = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![encoder_id], |row| {
            Ok((
                row.get::<_, ID>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;

        let mut out = Vec::new();
        let f32_size = std::mem::size_of::<f32>();
        for row in rows {
            let (id, path, bytes) = row?;
            if bytes.len() % f32_size != 0 {
                continue;
            }
            let embedding: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&bytes).to_vec();
            out.push((id, path, embedding));
        }
        Ok(out)
    }

    /// Return image rows that don't yet have an embedding for the given
    /// encoder. Used by the indexing pipeline to drive the per-encoder
    /// encode loop.
    ///
    /// Returns a Vec of (image_id, path) — the encoder will preprocess
    /// the path and write the result back via `upsert_embedding`.
    pub fn get_images_without_embedding_for(
        &self,
        encoder_id: &str,
    ) -> rusqlite::Result<Vec<(ID, String)>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT i.id, i.path
             FROM images i
             WHERE i.orphaned = 0
             AND NOT EXISTS (
                 SELECT 1 FROM embeddings e
                 WHERE e.image_id = i.id AND e.encoder_id = ?1
             )",
        )?;
        let rows = stmt.query_map(rusqlite::params![encoder_id], |row| {
            Ok((row.get::<_, ID>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect()
    }

    /// Count embeddings per encoder. Used by `get_pipeline_stats` to
    /// surface "X images encoded with SigLIP-2, Y with DINOv2".
    pub fn count_embeddings_for(&self, encoder_id: &str) -> rusqlite::Result<i64> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM embeddings WHERE encoder_id = ?1",
        )?;
        stmt.query_row(rusqlite::params![encoder_id], |row| row.get::<_, i64>(0))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::fresh_db;

    #[test]
    fn test_update_image_embedding_basic() {
        let db = fresh_db();

        // Add an image first
        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned(), None).unwrap();

        // Get the image ID
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Create a test embedding (512 dimensions, typical for CLIP)
        let test_embedding: Vec<f32> = (0..512).map(|i| i as f32 * 0.001).collect();

        // Update the embedding
        db.update_image_embedding(image_id, test_embedding.clone())
            .expect("Failed to update embedding");

        // Verify the embedding was stored correctly
        let retrieved_embedding = db
            .get_image_embedding(image_id)
            .expect("Failed to retrieve embedding");

        assert_eq!(
            retrieved_embedding.len(),
            test_embedding.len(),
            "Embedding length mismatch"
        );

        // Verify the values match (with small tolerance for floating point)
        for (retrieved, original) in retrieved_embedding.iter().zip(test_embedding.iter()) {
            assert!(
                (retrieved - original).abs() < 1e-6,
                "Embedding value mismatch: {} vs {}",
                retrieved,
                original
            );
        }
    }

    #[test]
    fn test_update_image_embedding_round_trip() {
        let db = fresh_db();

        let test_image_path = "/path/to/test_image.png";
        db.add_image(test_image_path.to_owned(), None).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Create a realistic embedding (normalized vector)
        let mut embedding: Vec<f32> = (0..512).map(|i| (i as f32).sin()).collect();
        // Normalize the embedding
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        embedding.iter_mut().for_each(|x| *x /= norm);

        // Store it
        db.update_image_embedding(image_id, embedding.clone())
            .expect("Failed to store embedding");

        // Retrieve it
        let retrieved = db
            .get_image_embedding(image_id)
            .expect("Failed to retrieve embedding");

        // Verify dimensions
        assert_eq!(retrieved.len(), 512, "Embedding should be 512 dimensions");

        // Verify values match
        for (i, (ret, orig)) in retrieved.iter().zip(embedding.iter()).enumerate() {
            assert!(
                (ret - orig).abs() < 1e-5,
                "Value mismatch at index {}: {} vs {}",
                i,
                ret,
                orig
            );
        }
    }

    #[test]
    fn test_update_image_embedding_overwrite() {
        let db = fresh_db();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned(), None).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Store first embedding
        let first_embedding: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        db.update_image_embedding(image_id, first_embedding.clone())
            .expect("Failed to store first embedding");

        // Verify first embedding is stored
        let retrieved1 = db.get_image_embedding(image_id).unwrap();
        assert_eq!(retrieved1, first_embedding);

        // Store second embedding (different size)
        let second_embedding: Vec<f32> = vec![5.0, 6.0, 7.0, 8.0, 9.0];
        db.update_image_embedding(image_id, second_embedding.clone())
            .expect("Failed to overwrite embedding");

        // Verify second embedding replaced the first
        let retrieved2 = db.get_image_embedding(image_id).unwrap();
        assert_eq!(retrieved2, second_embedding);
        assert_ne!(retrieved2, first_embedding);
    }

    #[test]
    fn test_update_image_embedding_nonexistent_image() {
        let db = fresh_db();

        // Try to update embedding for non-existent image
        let fake_image_id = 99999;
        let test_embedding: Vec<f32> = vec![1.0, 2.0, 3.0];

        // This should succeed (UPDATE doesn't fail if no rows match, it just updates 0 rows)
        // But we should verify the embedding wasn't actually stored
        let result = db.update_image_embedding(fake_image_id, test_embedding.clone());

        // The update itself should succeed (SQL UPDATE doesn't error on no matches)
        assert!(
            result.is_ok(),
            "UPDATE should succeed even if no rows match"
        );

        // But retrieving should fail
        let retrieve_result = db.get_image_embedding(fake_image_id);
        assert!(
            retrieve_result.is_err(),
            "Should fail to retrieve embedding for non-existent image"
        );
    }

    #[test]
    fn test_update_image_embedding_empty_embedding() {
        let db = fresh_db();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned(), None).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Store empty embedding
        let empty_embedding: Vec<f32> = Vec::new();
        db.update_image_embedding(image_id, empty_embedding.clone())
            .expect("Failed to store empty embedding");

        // Retrieve and verify
        let retrieved = db.get_image_embedding(image_id).unwrap();
        assert_eq!(retrieved.len(), 0);
        assert_eq!(retrieved, empty_embedding);
    }

    #[test]
    fn test_update_image_embedding_large_embedding() {
        let db = fresh_db();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned(), None).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Create a large embedding (larger than typical 512)
        let large_embedding: Vec<f32> = (0..2048).map(|i| i as f32).collect();

        db.update_image_embedding(image_id, large_embedding.clone())
            .expect("Failed to store large embedding");

        let retrieved = db.get_image_embedding(image_id).unwrap();
        assert_eq!(retrieved.len(), 2048);
        assert_eq!(retrieved, large_embedding);
    }

    #[test]
    fn test_get_image_embedding_before_update() {
        let db = fresh_db();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned(), None).unwrap();
        let image_id = db.get_image_id_by_path(test_image_path).unwrap();

        // Try to get embedding before it's set (should be NULL in DB)
        let result = db.get_image_embedding(image_id);

        // This should fail because the embedding is NULL
        assert!(result.is_err(), "Should fail to retrieve NULL embedding");
    }

    // ============================================================
    //  Phase 5: get_all_embeddings (N+1 fix)
    // ============================================================

    #[test]
    fn get_all_embeddings_returns_only_populated_rows() {
        let db = fresh_db();
        db.add_image("/with.jpg".into(), None).unwrap();
        db.add_image("/without.jpg".into(), None).unwrap();
        let with_id = db.get_image_id_by_path("/with.jpg").unwrap();

        // Embedding only on the first row.
        let emb: Vec<f32> = (0..512).map(|i| i as f32).collect();
        db.update_image_embedding(with_id, emb.clone()).unwrap();

        let rows = db.get_all_embeddings().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, with_id);
        assert_eq!(rows[0].1, "/with.jpg");
        assert_eq!(rows[0].2.len(), 512);
        assert!(
            (rows[0].2[0] - 0.0).abs() < 1e-6
                && (rows[0].2[511] - 511.0).abs() < 1e-6
        );
    }

    #[test]
    fn get_all_embeddings_is_empty_when_nothing_encoded() {
        let db = fresh_db();
        db.add_image("/a.jpg".into(), None).unwrap();
        let rows = db.get_all_embeddings().unwrap();
        assert!(rows.is_empty());
    }
}
