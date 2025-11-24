import { useState } from "react";
import { ImageItem } from "../types";
import { Card, CardContent, CardFooter, CardHeader } from "./ui/card";
import { Combobox } from "./ui/combobox";

interface MasonrySelectedItemProps {
  item: ImageItem;
}

export function MasonrySelectedItem(props: MasonrySelectedItemProps) {
  const [comboboxOpen, setComboboxOpen] = useState(false);
  const [comboboxValue, setComboboxValue] = useState("");

  return (
    <Card>
      <CardHeader>
        <div className="w-full flex flex-row items-end">
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
        <div className="rounded-xl overflow-hidden hover:shadow-lg/50 hover:scale-[1.02] hover:cursor-pointer duration-400 transition-all ease-out">
          <img className="w-full" src={props.item.url} />
        </div>
      </CardContent>
      <CardFooter>
        <hr />
        <h1>Test</h1>
      </CardFooter>
    </Card>
  );
}
