export type ImageData = {
  path: string;
  name: string;
  tags: Tag[];
  id: number;
  /** Path to the thumbnail image (from backend) */
  thumbnail_path?: string;
  /** Original image width in pixels (from backend) */
  width?: number;
  /** Original image height in pixels (from backend) */
  height?: number;
  /** Free-text annotation (Phase 11) */
  notes?: string | null;
};

export type ImageItem = {
  id: number;
  /** Full resolution image URL */
  url: string;
  /** Thumbnail image URL (for grid display) */
  thumbnailUrl?: string;
  width: number;
  height: number;
  name: string;
  tags: Tag[];
  /** Free-text annotation (Phase 11) */
  notes?: string | null;
};

export type Tag = {
  id: number;
  name: string;
  color: string;
};

export type SimilarImageItem = {
  id: number;
  path: string;
  /** Full resolution image URL */
  url: string;
  /** Thumbnail image URL (for grid display) */
  thumbnailUrl?: string;
  width: number;
  height: number;
  score: number;
  name?: string;
};

/** A configured scan root (multi-folder support, Phase 6). */
export type Root = {
  id: number;
  path: string;
  enabled: boolean;
  /** Unix epoch seconds */
  added_at: number;
};
