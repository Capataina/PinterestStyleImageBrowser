export type ImageData = {
  path: string;
  name: string;
  tags: Tag[];
  id: string;
};

export type ImageItem = {
  id: string;
  url: string;
  width: number;
  height: number;
  name: string;
  tags: Tag[];
};

export type Tag = {
  id: string;
  name: string;
  color: string;
};
