import { useState, useRef, useEffect, useMemo } from "react";
import { SearchIcon, XCircleIcon, PlusCircleIcon } from "lucide-react";
import { Badge } from "./ui/badge";
import { RxCrossCircled } from "react-icons/rx";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "./ui/command";
import { Popover, PopoverContent, PopoverAnchor } from "./ui/popover";
import { Tag } from "@/types";

interface SearchBarProps {
  tags?: Tag[];
  onSearchChange: (selectedTags: Tag[], searchText: string) => void;
  placeholder?: string;
  onCreateTag?: (name: string, color: string) => Promise<Tag>;
}

export function SearchBar(props: SearchBarProps) {
  const [selectedTags, setSelectedTags] = useState<Tag[]>([]);
  const [inputText, setInputText] = useState("");
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [suggestionsFilter, setSuggestionsFilter] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Check if we should show tag suggestions (# detected)
  const hashIndex = inputText.lastIndexOf("#");
  const isTypingTag = hashIndex !== -1;

  useEffect(() => {
    if (isTypingTag) {
      const textAfterHash = inputText.slice(hashIndex + 1);
      setSuggestionsFilter(textAfterHash);
      setShowSuggestions(true);
    } else {
      setShowSuggestions(false);
      setSuggestionsFilter("");
    }
  }, [inputText, isTypingTag, hashIndex]);

  // Filter tags based on current input after #
  // Also exclude tags that are already added
  const selectedTagIds = selectedTags.map((tag) => tag.id);

  const filteredTags = props.tags
    ? props.tags.filter(
      (tag) =>
        tag.name.toLowerCase().includes(suggestionsFilter.toLowerCase()) &&
        !selectedTagIds.includes(tag.id)
    )
    : [];

  // Check if the filter text exactly matches an existing tag
  const exactMatch = useMemo(() => {
    if (!props.tags || !suggestionsFilter.trim()) return null;
    return props.tags.find(
      (t) => t.name.toLowerCase() === suggestionsFilter.trim().toLowerCase()
    );
  }, [props.tags, suggestionsFilter]);

  // Show create option when there's input after # and no exact match
  const showCreateOption =
    suggestionsFilter.trim() && !exactMatch && props.onCreateTag;

  // Extract state for parent
  useEffect(() => {
    props.onSearchChange(selectedTags, inputText.trim());
  }, [selectedTags, inputText]);

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setInputText(e.target.value);
  };

  const handleTagSelect = (tag: Tag) => {
    // Find the # and remove only the #substring
    const hashIdx = inputText.lastIndexOf("#");
    if (hashIdx !== -1) {
      // Remove from # to the end of the input
      const newText = inputText.slice(0, hashIdx);
      setInputText(newText);
    }

    // Add tag to selected tags
    setSelectedTags((prev) => [...prev, tag]);

    setShowSuggestions(false);
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  const handleCreateTag = async () => {
    if (!props.onCreateTag || !suggestionsFilter.trim()) return;

    const newTag = await props.onCreateTag(suggestionsFilter.trim(), "#3B82F6");
    if (newTag) {
      handleTagSelect(newTag);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    // Handle Enter when suggestions are showing
    if (e.key === "Enter" && showSuggestions) {
      e.preventDefault();
      if (filteredTags.length > 0) {
        handleTagSelect(filteredTags[0]);
      } else if (showCreateOption) {
        handleCreateTag();
      }
      return;
    }

    // Handle backspace at the start of input to remove rightmost tag
    if (
      e.key === "Backspace" &&
      inputText === "" &&
      selectedTags.length > 0 &&
      inputRef.current?.selectionStart === 0
    ) {
      e.preventDefault();
      setSelectedTags((prev) => prev.slice(0, -1));
    }
  };

  const handleClear = () => {
    setSelectedTags([]);
    setInputText("");
    inputRef.current?.focus();
  };

  const hasContent = selectedTags.length > 0 || inputText !== "";

  const removeTag = (tagId: number) => {
    setSelectedTags((prev) => prev.filter((tag) => tag.id !== tagId));
  };

  return (
    <div className="relative w-full">
      <div
        ref={containerRef}
        className="flex items-center gap-3 rounded-full border border-transparent bg-secondary px-5 py-3 text-base transition-all duration-200 focus-within:border-border focus-within:shadow-lg focus-within:ring-1 focus-within:ring-primary/30"
      >
        <SearchIcon className="size-5 shrink-0 text-muted-foreground" />

        <div className="flex flex-1 flex-wrap items-center gap-1.5">
          {/* Tags on the left */}
          {selectedTags.map((tag) => (
            <Badge key={tag.id} className="px-2 py-0.5">
              <span className="text-xs">{tag.name}</span>
              <div
                className="ml-0.5 hover:cursor-pointer"
                onClick={() => removeTag(tag.id)}
              >
                <RxCrossCircled className="size-3" />
              </div>
            </Badge>
          ))}

          {/* Single input on the right */}
          <input
            ref={inputRef}
            type="text"
            value={inputText}
            onChange={handleInputChange}
            onKeyDown={handleKeyDown}
            placeholder={props.placeholder || "Search..."}
            className="flex-1 min-w-[120px] bg-transparent outline-none text-foreground placeholder:text-muted-foreground"
          />
        </div>

        {hasContent && (
          <button
            type="button"
            onClick={handleClear}
            className="shrink-0 opacity-50 hover:opacity-100 transition-opacity"
          >
            <XCircleIcon className="size-4" />
          </button>
        )}
      </div>

      <Popover open={showSuggestions} onOpenChange={setShowSuggestions}>
        <PopoverAnchor asChild>
          <div className="absolute top-0 left-0 w-full h-full pointer-events-none" />
        </PopoverAnchor>
        <PopoverContent
          className="w-[300px] p-0"
          align="start"
          onOpenAutoFocus={(e) => e.preventDefault()}
        >
          <Command>
            <CommandList>
              {/* Create new tag option */}
              {showCreateOption && (
                <>
                  <CommandGroup>
                    <CommandItem
                      value={`create-${suggestionsFilter}`}
                      onSelect={handleCreateTag}
                      className="cursor-pointer"
                    >
                      <PlusCircleIcon className="mr-2 h-4 w-4 text-blue-500" />
                      <span>
                        Create "
                        <span className="font-medium">
                          {suggestionsFilter.trim()}
                        </span>
                        "
                      </span>
                    </CommandItem>
                  </CommandGroup>
                  {filteredTags.length > 0 && <CommandSeparator />}
                </>
              )}

              {/* Existing tags */}
              {filteredTags.length > 0 ? (
                <CommandGroup
                  heading={showCreateOption ? "Existing tags" : undefined}
                >
                  {filteredTags.map((tag) => (
                    <CommandItem
                      key={tag.id}
                      value={tag.id.toString()}
                      onSelect={() => handleTagSelect(tag)}
                      className="cursor-pointer"
                    >
                      <Badge variant="outline" className="pointer-events-none">
                        {tag.name}
                      </Badge>
                    </CommandItem>
                  ))}
                </CommandGroup>
              ) : (
                !showCreateOption && (
                  <CommandEmpty>
                    {props.tags && props.tags.length > 0
                      ? "No tags found"
                      : "Type to create a new tag"}
                  </CommandEmpty>
                )
              )}
            </CommandList>
          </Command>
        </PopoverContent>
      </Popover>
    </div>
  );
}
