"use client";

import { CheckIcon, ChevronsUpDownIcon } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Tag } from "@/types";
import { useEffect, useMemo, useState } from "react";

interface TagDropdownProps {
  tags?: Tag[] | null;
  open: boolean;
  setOpen: (open: boolean) => void;
  selected: number[];
  setSelected: (vals: number[]) => void;
  placeholder: string;
  instruction: string;
  onCreateTag: (name: string, color: string) => Promise<Tag>;
  imageId?: number;
  onAssignTag: (imageId: number, tagId: number) => void;
  onRemoveTag: (imageId: number, tagId: number) => void;
}

export function TagDropdown(props: TagDropdownProps) {
  const [input, setInput] = useState("");

  useEffect(() => {
    if (props.open === false) {
      setInput("");
    }
  }, [props.open]);

  const filtered = useMemo(() => {
    if (props.tags) {
      if (props.tags.length === 0) {
        if (input === "") {
          return null;
        } else {
          return [];
        }
      } else {
        return props.tags.filter((t) =>
          t.name.toLowerCase().includes(input.toLowerCase())
        );
      }
    }
  }, [props.tags, input]);

  return (
    <Popover open={props.open} onOpenChange={props.setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={props.open}
          className="w-[200px] justify-between"
        >
          Add Tags
          <ChevronsUpDownIcon className="ml-2 h-4 w-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[200px] p-0">
        <Command shouldFilter={false}>
          <CommandInput
            value={input}
            onValueChange={setInput}
            placeholder={props.instruction}
          />
          <CommandList>
            <CommandGroup>
              {filtered ? (
                filtered.length > 0 ? (
                  filtered.map((tag) => (
                    <CommandItem
                      key={tag.id}
                      value={tag.id.toString()}
                      onSelect={(id) => {
                        const numId = parseInt(id);
                        if (!props.imageId) return;
                        const wasSelected = props.selected.includes(numId);
                        console.log(props.selected);
                        if (wasSelected) {
                          props.onRemoveTag(props.imageId, numId);
                        } else {
                          props.onAssignTag(props.imageId, numId);
                        }
                        setInput("");
                      }}
                    >
                      <CheckIcon
                        className={cn(
                          "mr-2 h-4 w-4",
                          props.selected.includes(tag.id)
                            ? "opacity-100"
                            : "opacity-0"
                        )}
                      />
                      {tag.name}
                    </CommandItem>
                  ))
                ) : (
                  <CommandItem
                    value={input.trimEnd()}
                    onSelect={async (val) => {
                      const newTag = await props.onCreateTag(val, "#3B82F6");
                      if (props.imageId && newTag) {
                        props.onAssignTag(props.imageId, newTag.id);
                      }
                      setInput("");
                    }}
                    className="text-center p-4"
                  >
                    Create "{input}"
                  </CommandItem>
                )
              ) : (
                <CommandEmpty>Type to create tags</CommandEmpty>
              )}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
