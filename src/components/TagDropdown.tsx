"use client";

import { CheckIcon, ChevronsUpDownIcon, PlusCircleIcon } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
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

  // Filter tags based on input
  const filtered = useMemo(() => {
    if (!props.tags) return [];
    return props.tags.filter((t) =>
      t.name.toLowerCase().includes(input.toLowerCase())
    );
  }, [props.tags, input]);

  // Check if input exactly matches an existing tag
  const exactMatch = useMemo(() => {
    if (!props.tags || !input.trim()) return null;
    return props.tags.find(
      (t) => t.name.toLowerCase() === input.trim().toLowerCase()
    );
  }, [props.tags, input]);

  // Show create option when there's input and no exact match
  const showCreateOption = input.trim() && !exactMatch;

  const handleCreateTag = async () => {
    if (!input.trim() || !props.imageId) return;
    const newTag = await props.onCreateTag(input.trim(), "#3B82F6");
    if (newTag) {
      props.onAssignTag(props.imageId, newTag.id);
      props.setSelected([...props.selected, newTag.id]);
    }
    setInput("");
  };

  const handleSelectTag = (tag: Tag) => {
    if (!props.imageId) return;
    const wasSelected = props.selected.includes(tag.id);
    if (wasSelected) {
      props.onRemoveTag(props.imageId, tag.id);
      props.setSelected(props.selected.filter((id) => id !== tag.id));
    } else {
      props.onAssignTag(props.imageId, tag.id);
      props.setSelected([...props.selected, tag.id]);
    }
    setInput("");
  };

  return (
    <Popover open={props.open} onOpenChange={props.setOpen} modal={true}>
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
      <PopoverContent className="w-[220px] p-0 z-[200]" align="start">
        <Command shouldFilter={false}>
          <CommandInput
            value={input}
            onValueChange={setInput}
            placeholder="Search or create tag..."
          />
          <CommandList>
            {/* Create new tag option */}
            {showCreateOption && (
              <>
                <CommandGroup>
                  <CommandItem
                    value={`create-${input}`}
                    onSelect={handleCreateTag}
                    className="cursor-pointer"
                  >
                    <PlusCircleIcon className="mr-2 h-4 w-4 text-blue-500" />
                    <span>
                      Create "<span className="font-medium">{input.trim()}</span>"
                    </span>
                  </CommandItem>
                </CommandGroup>
                {filtered.length > 0 && <CommandSeparator />}
              </>
            )}

            {/* Existing tags */}
            {filtered.length > 0 ? (
              <CommandGroup heading={showCreateOption ? "Existing tags" : undefined}>
                {filtered.map((tag) => (
                  <CommandItem
                    key={tag.id}
                    value={tag.id.toString()}
                    onSelect={() => handleSelectTag(tag)}
                    className="cursor-pointer"
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
                ))}
              </CommandGroup>
            ) : (
              !showCreateOption && (
                <CommandEmpty>
                  {props.tags && props.tags.length > 0
                    ? "No tags found"
                    : "Type to create your first tag"}
                </CommandEmpty>
              )
            )}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
