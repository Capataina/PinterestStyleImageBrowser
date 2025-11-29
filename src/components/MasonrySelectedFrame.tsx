import { useEffect, useState } from "react";
import { ImageItem, Tag } from "../types";
import { Card, CardContent, CardFooter, CardHeader } from "./ui/card";
import { TagDropdown } from "./TagDropdown";
import { Button } from "./ui/button";
import { FaChevronLeft } from "react-icons/fa";
import { Badge } from "./ui/badge";

interface MasonrySelectedItemProps {
  item?: ImageItem | null;
  height?: number;
  navigateBack: () => void;
  tags?: Tag[] | null;
  onCreateTag: (name: string, color: string) => Promise<Tag>;
  onAssignTag: (imageId: string, tagId: string) => void;
}

export function MasonrySelectedFrame(props: MasonrySelectedItemProps) {
  const [comboboxOpen, setComboboxOpen] = useState(false);
  const [selectedTags, setSelectedTags] = useState<string[]>([]);

  useEffect(() => {
    if (props.item) setSelectedTags(props.item.tags.map((t) => t.id));
  }, [props.item]);

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
        <div className="w-full flex flex-row justify-between">
          <Button
            variant="outline"
            size="icon"
            className="hover:cursor-pointer"
            onClick={props.navigateBack}
          >
            <FaChevronLeft />
          </Button>
          <TagDropdown
            tags={props.tags}
            open={comboboxOpen}
            setOpen={setComboboxOpen}
            selected={selectedTags}
            setSelected={setSelectedTags}
            placeholder="Tags"
            emptyMessage="Create tag"
            instruction="Select tags to add"
            onCreateTag={props.onCreateTag}
            imageId={props.item?.id}
            onAssignTag={props.onAssignTag}
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
        {props.item.tags.length > 0 ? (
          props.item.tags.map((tag) => <Badge>{tag.name}</Badge>)
        ) : (
          <p>Untaged</p>
        )}
      </CardFooter>
    </Card>
  );
}
