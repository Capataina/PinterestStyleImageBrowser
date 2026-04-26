use super::index::CosineIndex;
use crate::paths;
use ndarray::Array1;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

impl CosineIndex {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

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
