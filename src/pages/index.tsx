import { useEffect, useState } from "react";
import Masonry from "../components/Masonry";
import { useImages } from "../queries/useImages";
import { ImageItem } from "../types";

export default function Home() {
  const [selectedItem, setSelectedItem] = useState<ImageItem | null>(null);
  const { data, isFetching, refetch } = useImages();

  useEffect(() => {
    console.log(data);
  }, [data]);

  return (
    <main className="w-screen h-screen overflow-x-hidden overflow-y-auto">
      <div className="px-10 py-6 w-full h-full relative box-border">
        {data && (
          <Masonry
            items={data}
            columnGap={25}
            verticalGap={25}
            minItemWidth={300}
            selectedItem={selectedItem}
            onItemClick={(item) => {
              console.log(item);
              setSelectedItem(item);
            }}
          />
        )}
      </div>
    </main>
  );
}
