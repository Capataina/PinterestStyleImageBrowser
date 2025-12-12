export type ImageData = {
  path: string;
  name: string;
  tags: Tag[];
  id: number;
};

export type ImageItem = {
  id: number;
  url: string;
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
  url: string;
  width: number;
  height: number;
  score: number;
  name?: string;
};