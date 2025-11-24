import { useState } from "react";
import { ImageItem } from "../types";
import { Card, CardContent, CardFooter, CardHeader } from "./ui/card";
import { Combobox } from "./ui/combobox";

interface MasonrySelectedItemProps {
  item: ImageItem | undefined | null;
  height?: number;
}

export function MasonrySelectedFrame(props: MasonrySelectedItemProps) {
  const [comboboxOpen, setComboboxOpen] = useState(false);
  const [comboboxValue, setComboboxValue] = useState("");

  if (!props.item) return;

  return (
    <Card
      className="overflow-hidden"
      style={{
        height: props.height ? props.height : "auto",
        transition: "height 0.3s ease-in-out",
      }}
    >
      <CardHeader>
        <div className="w-full flex flex-row justify-end">
          <Combobox
            items={[
              {
                value: "test-1",
                label: "Test 1",
              },
              {
                value: "test-2",
                label: "Test 2",
              },
            ]}
            open={comboboxOpen}
            setOpen={setComboboxOpen}
            value={comboboxValue}
            setValue={setComboboxValue}
            placeholder="Tags"
            emptyMessage="Create tag"
            instruction="Select tags to add"
          />
        </div>
      </CardHeader>
      <CardContent>
        <div className="rounded-xl overflow-hidden">
          <img id="img" className="w-full invisible" src={props.item.url} />
        </div>
      </CardContent>
      <div className="grow" />
      <CardFooter>
        <hr />
        <h1>Test</h1>
      </CardFooter>
    </Card>
  );
}
