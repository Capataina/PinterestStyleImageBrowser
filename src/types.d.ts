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