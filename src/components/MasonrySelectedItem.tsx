import { ImageItem } from "../types";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";

interface MasonrySelectedItemProps {
  item: ImageItem;
}

export function MasonrySelectedItem(props: MasonrySelectedItemProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{props.item.name}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="rounded-xl overflow-hidden drop-shadow-lg hover:cursor-pointer duration-700 transition-all ease-out">
          <img className="w-full" src={props.item.url} />
        </div>
      </CardContent>
    </Card>
  );
}
