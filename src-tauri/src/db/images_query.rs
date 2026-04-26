//! Image-grid SELECTs + path/root lookups.
//!
//! The four "fetch a grid of images" queries (`get_images`,
//! `get_images_with_thumbnails`, `get_images_without_embeddings`,
//! `get_images_without_thumbnails`) all share the same
//! "images LEFT JOIN images_tags LEFT JOIN tags" row shape. The
//! `aggregate_image_rows` helper at the top of the file performs the
//! HashMap-keyed-by-image-id roll-up so each caller stays focused on
//! its WHERE clause.

use std::collections::HashMap;

use rusqlite::params_from_iter;

use super::{ID, ImageDatabase};
use crate::{image_struct::ImageData, tag_struct::Tag};

/// Aggregate the standard "images LEFT JOIN images_tags LEFT JOIN tags"
/// row shape into a Vec of `(id, path, tags, thumbnail_path, width, height)`.
///
/// Audit finding: this aggregation pattern (HashMap-keyed-by-image-id,
/// each entry collecting Tags as the LEFT JOIN unrolls) was duplicated
/// across `get_images`, `get_images_without_embeddings`,
/// `get_images_without_thumbnails`, and `get_images_with_thumbnails`.
/// Four near-identical 25-line blocks → one helper. The next change to
/// tag-aggregation logic happens in one place; ditto the thumbnail-
/// column shape.
///
/// Expected column names (the SELECT must alias accordingly):
///   img_id, img_path,
///   thumbnail_path, width, height — nullable, OK to be absent in the
///     SELECT (treated as NULL in that case via the COALESCE pattern
///     each caller uses),
///   tag_id, tag_name, tag_color — nullable LEFT JOIN columns.
///
/// Callers that don't need the thumbnail columns simply discard them
/// from the returned tuples; callers that don't include the columns in
/// their SELECT must alias `NULL AS thumbnail_path`, etc., so the
/// helper's `row.get("thumbnail_path")` resolves to `None`.
fn aggregate_image_rows(
    rows: &mut rusqlite::Rows<'_>,
) -> rusqlite::Result<Vec<(ID, String, Vec<Tag>, Option<String>, Option<i64>, Option<i64>)>> {
    let mut map: HashMap<ID, (String, Vec<Tag>, Option<String>, Option<i64>, Option<i64>)> =
        HashMap::new();
    while let Some(row) = rows.next()? {
        let img_id: ID = row.get("img_id")?;
        let img_path: String = row.get("img_path")?;
        let thumbnail_path: Option<String> = row.get("thumbnail_path")?;
        let width: Option<i64> = row.get("width")?;
        let height: Option<i64> = row.get("height")?;
        let tag_id_opt: Option<ID> = row.get("tag_id")?;

        let entry = map.entry(img_id).or_insert((
            img_path,
            Vec::new(),
            thumbnail_path,
            width,
            height,
        ));
        if let Some(tag_id) = tag_id_opt {
            entry.1.push(Tag {
                id: tag_id,
                name: row.get("tag_name")?,
                color: row.get("tag_color")?,
            });
        }
    }
    Ok(map
        .into_iter()
        .map(|(id, (path, tags, tp, w, h))| (id, path, tags, tp, w, h))
        .collect())
}

impl ImageDatabase {
    pub fn get_images(
        &self,
        filter_tag_ids: Vec<ID>,
        _filter_string: String,
    ) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();

        // Always SELECT the thumbnail columns as NULL aliases so the
        // shared `aggregate_image_rows` helper can read by name. This
        // function discards them — the legacy "no thumbnail data"
        // shape — but the helper's contract is uniform across all
        // four callers.
        let sql = if !filter_tag_ids.is_empty() {
            let placeholders = vec!["?"; filter_tag_ids.len()].join(", ");
            format!(
                "SELECT images.id AS img_id, images.path AS img_path,
                NULL AS thumbnail_path, NULL AS width, NULL AS height,
                tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
                FROM images
                LEFT JOIN images_tags ON images.id = images_tags.image_id
                LEFT JOIN tags ON tags.id = images_tags.tag_id
                WHERE EXISTS (
                    SELECT 1
                    FROM images_tags it2
                    WHERE it2.image_id = images.id
                    AND it2.tag_id IN ({})
                );",
                placeholders
            )
        } else {
            "SELECT images.id AS img_id, images.path AS img_path,
            NULL AS thumbnail_path, NULL AS width, NULL AS height,
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id;"
                .to_string()
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(filter_tag_ids))?;
        let aggregated = aggregate_image_rows(&mut rows)?;

        let mut images: Vec<ImageData> = aggregated
            .into_iter()
            .map(|(id, path, tags, _tp, _w, _h)| {
                ImageData::new(id, std::path::Path::new(&path), tags)
            })
            .collect();
        images.sort_by_key(|img| img.id);

        Ok(images)
    }

    pub fn get_all_images(&self) -> rusqlite::Result<Vec<ImageData>> {
        self.get_images(Vec::new(), "".to_string())
    }

    // Get images that don't have embeddings yet
    pub fn get_images_without_embeddings(&self) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT images.id AS img_id, images.path AS img_path,
            NULL AS thumbnail_path, NULL AS width, NULL AS height,
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id
            WHERE images.embedding IS NULL;",
        )?;
        let mut rows = stmt.query([])?;
        let aggregated = aggregate_image_rows(&mut rows)?;

        let mut images: Vec<ImageData> = aggregated
            .into_iter()
            .map(|(id, path, tags, _tp, _w, _h)| {
                ImageData::new(id, std::path::Path::new(&path), tags)
            })
            .collect();
        images.sort_by_key(|img| img.id);

        Ok(images)
    }

    /// Get images that don't have thumbnails yet
    pub fn get_images_without_thumbnails(&self) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT images.id AS img_id, images.path AS img_path,
            images.thumbnail_path, images.width, images.height,
            tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
            FROM images
            LEFT JOIN images_tags ON images.id = images_tags.image_id
            LEFT JOIN tags ON tags.id = images_tags.tag_id
            WHERE images.thumbnail_path IS NULL OR images.thumbnail_path = '';",
        )?;
        let mut rows = stmt.query([])?;
        let aggregated = aggregate_image_rows(&mut rows)?;

        // This function returns images that need thumbnails — by
        // definition the thumbnail columns are NULL/empty, so we
        // discard them when materialising into ImageData. The helper
        // returns them anyway because its contract is uniform.
        let mut images: Vec<ImageData> = aggregated
            .into_iter()
            .map(|(id, path, tags, _tp, _w, _h)| {
                ImageData::new(id, std::path::Path::new(&path), tags)
            })
            .collect();
        images.sort_by_key(|img| img.id);

        Ok(images)
    }

    /// Get images with their thumbnail info included.
    ///
    /// Filters out:
    /// - rows whose root is disabled (multi-folder, Phase 6)
    /// - rows marked orphaned (file removed from disk, Phase 7)
    ///
    /// Rows with NULL root_id are kept — those are legacy un-migrated
    /// rows from before multi-folder support and should still display.
    ///
    /// `match_all_tags` controls multi-tag semantics: false (default)
    /// matches images with ANY of the selected tags (OR), true requires
    /// ALL of them (AND). Threaded through from the user's tagFilterMode
    /// preference via the get_images Tauri command.
    pub fn get_images_with_thumbnails(
        &self,
        filter_tag_ids: Vec<ID>,
        _filter_string: String,
        match_all_tags: bool,
    ) -> rusqlite::Result<Vec<ImageData>> {
        let conn = self.connection.lock().unwrap();

        // Common WHERE for root-and-orphan filtering. Used both with
        // and without tag-filter SQL.
        let root_filter = "(
            images.orphaned = 0
            AND (
                images.root_id IS NULL
                OR images.root_id IN (SELECT id FROM roots WHERE enabled = 1)
            )
        )";

        let sql = if !filter_tag_ids.is_empty() {
            let placeholders = vec!["?"; filter_tag_ids.len()].join(", ");
            if match_all_tags {
                // AND semantic: image must have EVERY selected tag.
                // GROUP BY image_id with HAVING COUNT = number of distinct
                // selected tags. Note we COUNT(DISTINCT tag_id) so a tag
                // appearing twice for the same image (impossible given
                // the PK but defensive) doesn't satisfy the constraint
                // for two different selected tags.
                let n = filter_tag_ids.len();
                format!(
                    "SELECT images.id AS img_id, images.path AS img_path,
                    images.thumbnail_path, images.width, images.height,
                    tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
                    FROM images
                    LEFT JOIN images_tags ON images.id = images_tags.image_id
                    LEFT JOIN tags ON tags.id = images_tags.tag_id
                    WHERE {root_filter}
                    AND images.id IN (
                        SELECT it2.image_id
                        FROM images_tags it2
                        WHERE it2.tag_id IN ({placeholders})
                        GROUP BY it2.image_id
                        HAVING COUNT(DISTINCT it2.tag_id) = {n}
                    );"
                )
            } else {
                // OR semantic: image must have ANY selected tag.
                format!(
                    "SELECT images.id AS img_id, images.path AS img_path,
                    images.thumbnail_path, images.width, images.height,
                    tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
                    FROM images
                    LEFT JOIN images_tags ON images.id = images_tags.image_id
                    LEFT JOIN tags ON tags.id = images_tags.tag_id
                    WHERE {root_filter}
                    AND EXISTS (
                        SELECT 1
                        FROM images_tags it2
                        WHERE it2.image_id = images.id
                        AND it2.tag_id IN ({placeholders})
                    );"
                )
            }
        } else {
            format!(
                "SELECT images.id AS img_id, images.path AS img_path,
                images.thumbnail_path, images.width, images.height,
                tags.id AS tag_id, tags.name AS tag_name, tags.color AS tag_color
                FROM images
                LEFT JOIN images_tags ON images.id = images_tags.image_id
                LEFT JOIN tags ON tags.id = images_tags.tag_id
                WHERE {root_filter};"
            )
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(filter_tag_ids))?;
        let aggregated = aggregate_image_rows(&mut rows)?;

        // This function is the only one that USES the thumbnail
        // columns — the helper provides them uniformly; we read them
        // here when materialising into ImageData.
        let mut images: Vec<ImageData> = aggregated
            .into_iter()
            .map(|(id, path, tags, thumbnail_path, width, height)| {
                let mut img = ImageData::new(id, std::path::Path::new(&path), tags);
                img.thumbnail_path = thumbnail_path;
                img.width = width.map(|w| w as u32);
                img.height = height.map(|h| h as u32);
                img
            })
            .collect();

        // Stable order by id (oldest first). The previous "shuffle on
        // every read" caused the visible "entire app refreshes"
        // behaviour during indexing — every refetch (every ~2s while
        // thumbnails were generating) reordered the grid, making
        // tiles jump around. Sort modes are now controlled via the
        // user's `sortMode` preference and applied frontend-side
        // when needed (the frontend can apply a deterministic
        // shuffle with a session seed if the user picks "shuffle").
        images.sort_by_key(|i| i.id);

        Ok(images)
    }

    /// Return a map from every image's path to its root_id (or None
    /// for legacy un-migrated rows) in a single SELECT.
    ///
    /// Replaces the indexing pipeline's previous N+1 pattern (one
    /// `get_root_id_by_path` per image-needing-thumbnail, holding the
    /// DB Mutex 1500 times in rapid succession on a typical first
    /// run). Aligned with the existing `get_all_embeddings` shape —
    /// "fetch the whole table in one SELECT, the caller filters in
    /// memory" is the established pattern in this module.
    ///
    /// Used by `indexing::run_pipeline_inner` to route each generated
    /// thumbnail into its per-root subfolder.
    pub fn get_paths_to_root_ids(&self) -> rusqlite::Result<HashMap<String, Option<ID>>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path, root_id FROM images")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<ID>>(1)?))
        })?;
        rows.collect()
    }

    pub fn get_image_id_by_path(&self, path: &str) -> rusqlite::Result<ID> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id FROM images WHERE path = ?1 LIMIT 1")?;
        let mut rows = stmt.query([path])?;
        if let Some(row) = rows.next()? {
            Ok(row.get("id")?)
        } else {
            Err(rusqlite::Error::QueryReturnedNoRows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::fresh_db;
    use super::*;

    #[test]
    fn test_database_operations() {
        let db = fresh_db();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned(), None).unwrap();

        let images = db.get_images(vec![], "".to_string()).unwrap();
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn test_prevent_duplicate_images() {
        let db = fresh_db();

        let test_image_path = "/path/to/image.jpg";
        db.add_image(test_image_path.to_owned(), None).unwrap();
        db.add_image(test_image_path.to_owned(), None).unwrap();

        let images = db.get_images(vec![], "".to_string()).unwrap();
        assert_eq!(images.len(), 1); // Should still be only one image
    }

    #[test]
    fn test_empty_database() {
        let db = fresh_db();

        let images = db.get_images(vec![], "".to_string()).unwrap();
        assert_eq!(images.len(), 0); // No images should be present
    }

    // ============================================================
    //  Phase 6: get_images_with_thumbnails — multi-folder filter
    // ============================================================

    #[test]
    fn grid_query_excludes_disabled_root_images() {
        let db = fresh_db();
        let a = db.add_root("/a".into()).unwrap();
        let b = db.add_root("/b".into()).unwrap();
        db.add_image("/a/x.jpg".into(), Some(a.id)).unwrap();
        db.add_image("/b/y.jpg".into(), Some(b.id)).unwrap();

        // Both enabled → both in the grid.
        let imgs = db
            .get_images_with_thumbnails(vec![], "".into(), false)
            .unwrap();
        assert_eq!(imgs.len(), 2);

        // Disable root b.
        db.set_root_enabled(b.id, false).unwrap();
        let imgs = db
            .get_images_with_thumbnails(vec![], "".into(), false)
            .unwrap();
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].path, "/a/x.jpg");
    }

    #[test]
    fn grid_query_includes_null_root_id_images() {
        let db = fresh_db();
        // Legacy un-migrated rows (root_id = NULL) should still appear.
        db.add_image("/legacy.jpg".into(), None).unwrap();
        let imgs = db
            .get_images_with_thumbnails(vec![], "".into(), false)
            .unwrap();
        assert_eq!(imgs.len(), 1);
    }

    // ============================================================
    //  Phase 6: get_images_with_thumbnails — AND vs OR tag filter
    // ============================================================

    fn setup_tagged_images(db: &ImageDatabase) -> (i64, i64, i64, i64) {
        let r = db.add_root("/r".into()).unwrap();
        // 3 images: A has tag-1, B has tag-2, C has both.
        db.add_image("/r/a.jpg".into(), Some(r.id)).unwrap();
        db.add_image("/r/b.jpg".into(), Some(r.id)).unwrap();
        db.add_image("/r/c.jpg".into(), Some(r.id)).unwrap();
        let imgs = db.get_all_images().unwrap();
        let id_a = imgs.iter().find(|i| i.path == "/r/a.jpg").unwrap().id;
        let id_b = imgs.iter().find(|i| i.path == "/r/b.jpg").unwrap().id;
        let id_c = imgs.iter().find(|i| i.path == "/r/c.jpg").unwrap().id;
        let t1 = db.create_tag("one".into(), "#fff".into()).unwrap().id;
        let t2 = db.create_tag("two".into(), "#000".into()).unwrap().id;
        db.add_tag_to_image(id_a, t1).unwrap();
        db.add_tag_to_image(id_b, t2).unwrap();
        db.add_tag_to_image(id_c, t1).unwrap();
        db.add_tag_to_image(id_c, t2).unwrap();
        (id_a, id_b, id_c, t1)
    }

    #[test]
    fn or_filter_matches_any_selected_tag() {
        let db = fresh_db();
        let (id_a, id_b, id_c, t1) = setup_tagged_images(&db);
        let _ = id_b;

        // Filter by t1 alone — should return A and C (both have t1).
        let imgs = db
            .get_images_with_thumbnails(vec![t1], "".into(), false)
            .unwrap();
        let ids: Vec<i64> = imgs.iter().map(|i| i.id).collect();
        assert!(ids.contains(&id_a));
        assert!(ids.contains(&id_c));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn and_filter_requires_all_selected_tags() {
        let db = fresh_db();
        let (id_a, id_b, id_c, t1) = setup_tagged_images(&db);
        let _ = id_a;
        let _ = id_b;
        // Re-fetch t2 since setup returns only t1
        let tags = db.get_tags().unwrap();
        let t2 = tags.iter().find(|t| t.name == "two").unwrap().id;

        // OR semantic: t1 + t2 → A, B, C all match.
        let or_match = db
            .get_images_with_thumbnails(vec![t1, t2], "".into(), false)
            .unwrap();
        assert_eq!(or_match.len(), 3);

        // AND semantic: only C has BOTH tags.
        let and_match = db
            .get_images_with_thumbnails(vec![t1, t2], "".into(), true)
            .unwrap();
        assert_eq!(and_match.len(), 1);
        assert_eq!(and_match[0].id, id_c);
    }
}
