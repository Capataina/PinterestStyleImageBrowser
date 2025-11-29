import { useEffect, useState } from "react";
import { ImageItem, Tag } from "../types";
import { Card, CardContent, CardFooter, CardHeader } from "./ui/card";
import { TagDropdown } from "./TagDropdown";
import { Button } from "./ui/button";
import { FaChevronLeft } from "react-icons/fa";
import { Badge } from "./ui/badge";
import { RxCrossCircled } from "react-icons/rx";
import { AnimatePresence, motion } from "framer-motion";

const container = {
  hidden: {},
  show: {
    transition: {
      staggerChildren: 0.06,
    },
  },
};

const item = {
  hidden: { opacity: 0, y: 8 },
  show: { opacity: 1, y: 0, transition: { duration: 0.18 } },
  exit: (i: number) => ({
    opacity: 0,
    y: 8,
    transition: { duration: 0.16, delay: i * 0.04 },
  }),
};

interface MasonrySelectedItemProps {
  item?: ImageItem | null;
  height?: number;
  navigateBack: () => void;
  tags?: Tag[] | null;
  onCreateTag: (name: string, color: string) => Promise<Tag>;
  onAssignTag: (imageId: number, tagId: number) => void;
  onRemoveTag: (imageId: number, tagId: number) => void;
}

export function MasonrySelectedFrame(props: MasonrySelectedItemProps) {
  const [comboboxOpen, setComboboxOpen] = useState(false);
  const [selectedTags, setSelectedTags] = useState<number[]>([]);

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
            instruction="Select tags to add"
            onCreateTag={props.onCreateTag}
            imageId={props.item?.id}
            onAssignTag={props.onAssignTag}
            onRemoveTag={props.onRemoveTag}
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
        <motion.div
          variants={container}
          initial="hidden"
          animate="show"
          className="flex flex-row gap-2"
        >
          <AnimatePresence mode="popLayout">
            {props.item.tags.map((tag, i) => (
              <motion.div
                key={tag.id}
                layout
                variants={item}
                custom={i}
                initial="hidden"
                animate="show"
                exit="exit"
                transition={{ layout: { duration: 0.2 } }}
              >
                <Badge className="px-3 py-1">
                  <span className="text-sm">{tag.name}</span>
                  <div
                    className="ml-0.5 hover:cursor-pointer"
                    onClick={() => props.onRemoveTag(props.item!.id, tag.id)}
                  >
                    <RxCrossCircled className="size-[15px]!" />
                  </div>
                </Badge>
              </motion.div>
            ))}
          </AnimatePresence>
        </motion.div>
      </CardFooter>
    </Card>
  );
}
