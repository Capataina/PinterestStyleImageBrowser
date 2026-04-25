import { useEffect, useState } from "react";
import { ImageItem, Tag } from "../types";
import { motion, AnimatePresence } from "framer-motion";
import { X } from "lucide-react";
import { TagDropdown } from "./TagDropdown";
import { Badge } from "./ui/badge";
import { RxCrossCircled } from "react-icons/rx";

interface PinterestModalProps {
  item: ImageItem | null;
  onClose: () => void;
  onNavigate?: (direction: "prev" | "next") => void;
  tags?: Tag[];
  onCreateTag: (name: string, color: string) => Promise<Tag>;
  onDeleteTag?: (tagId: number) => void;
  onAssignTag: (imageId: number, tagId: number) => void;
  onRemoveTag: (imageId: number, tagId: number) => void;
  /** Free-text annotation for this image (Phase 11) */
  notes?: string;
  onSaveNotes?: (imageId: number, notes: string) => void;
}

/**
 * Fullscreen image inspector.
 *
 * Layout: image fills the left ~60% of the viewport, details drawer on
 * the right with tag editor + notes textarea + dimensions metadata.
 *
 * Navigation: left/right arrow keys move through the displayed list.
 * The previous arrow buttons are gone — keyboard nav is enough and the
 * buttons made the modal feel cluttered.
 */
export function PinterestModal(props: PinterestModalProps) {
  const [comboboxOpen, setComboboxOpen] = useState(false);
  const [selectedTags, setSelectedTags] = useState<number[]>([]);
  const [notesValue, setNotesValue] = useState(props.notes ?? "");

  useEffect(() => {
    if (props.item) {
      setSelectedTags(props.item.tags.map((t) => t.id));
    }
  }, [props.item]);

  useEffect(() => {
    setNotesValue(props.notes ?? "");
  }, [props.notes, props.item?.id]);

  // Keyboard handlers: arrow keys navigate, esc closes.
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't intercept when the user is typing in an input/textarea.
      const target = e.target as HTMLElement | null;
      const inEditable =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.getAttribute("contenteditable") === "true";

      if (e.key === "Escape") {
        props.onClose();
      } else if (!inEditable && e.key === "ArrowLeft" && props.onNavigate) {
        props.onNavigate("prev");
      } else if (!inEditable && e.key === "ArrowRight" && props.onNavigate) {
        props.onNavigate("next");
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [props.onClose, props.onNavigate]);

  const persistNotesSoon = () => {
    if (!props.onSaveNotes || !props.item) return;
    props.onSaveNotes(props.item.id, notesValue);
  };

  if (!props.item) return null;

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.15 }}
        className="fixed inset-0 z-[100] flex items-center justify-center"
        onClick={props.onClose}
      >
        {/* Backdrop */}
        <div className="absolute inset-0 bg-black/80 backdrop-blur-md" />

        {/* Modal — spring scale-in for a more physical feel */}
        <motion.div
          initial={{ scale: 0.96, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          exit={{ scale: 0.96, opacity: 0 }}
          transition={{ type: "spring", stiffness: 400, damping: 32 }}
          className="relative z-10 flex max-h-[90vh] max-w-[95vw] overflow-hidden rounded-2xl bg-card shadow-2xl shadow-black/50 border border-border"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Close button */}
          <button
            onClick={props.onClose}
            className="absolute top-4 left-4 z-20 flex h-9 w-9 items-center justify-center rounded-full bg-background/80 backdrop-blur-sm transition hover:bg-background"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>

          {/* Image — fills left side, capped at 60vw */}
          <div className="flex max-w-[60vw] items-center justify-center bg-background/50">
            <img
              src={props.item.url}
              alt={props.item.name}
              className="max-h-[90vh] w-auto object-contain"
              loading="eager"
              decoding="async"
            />
          </div>

          {/* Details panel */}
          <div className="flex w-80 flex-col bg-card p-6 overflow-y-auto">
            {/* Tag dropdown */}
            <div className="mb-5">
              <TagDropdown
                tags={props.tags}
                open={comboboxOpen}
                setOpen={setComboboxOpen}
                selected={selectedTags}
                setSelected={setSelectedTags}
                placeholder="Add Tags"
                instruction="Select tags to add"
                onCreateTag={props.onCreateTag}
                onDeleteTag={props.onDeleteTag}
                imageId={props.item.id}
                onAssignTag={props.onAssignTag}
                onRemoveTag={props.onRemoveTag}
              />
            </div>

            <h2 className="mb-3 text-base font-semibold text-foreground line-clamp-2">
              {props.item.name}
            </h2>

            {/* Active tags */}
            <div className="flex flex-wrap gap-2 mb-5">
              <AnimatePresence mode="popLayout">
                {props.item.tags.map((tag) => (
                  <motion.div
                    key={tag.id}
                    layout
                    initial={{ opacity: 0, scale: 0.8 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.8 }}
                    transition={{ type: "spring", stiffness: 500, damping: 30 }}
                  >
                    <Badge
                      className="px-2.5 py-1 text-xs"
                      style={{
                        backgroundColor: tag.color,
                        color: pickContrastingText(tag.color),
                      }}
                    >
                      {tag.name}
                      <button
                        className="ml-1.5 hover:opacity-70 transition"
                        onClick={() =>
                          props.onRemoveTag(props.item!.id, tag.id)
                        }
                        aria-label={`Remove ${tag.name}`}
                      >
                        <RxCrossCircled className="h-3 w-3" />
                      </button>
                    </Badge>
                  </motion.div>
                ))}
              </AnimatePresence>
            </div>

            {/* Notes textarea (Phase 11) */}
            {props.onSaveNotes && (
              <div className="mb-5">
                <label className="text-xs font-medium text-muted-foreground mb-1.5 block">
                  Notes
                </label>
                <textarea
                  value={notesValue}
                  onChange={(e) => setNotesValue(e.target.value)}
                  onBlur={persistNotesSoon}
                  placeholder="Add a note about this image..."
                  className="w-full min-h-[80px] rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary resize-none"
                />
              </div>
            )}

            <div className="flex-1" />

            {/* Image dimensions */}
            <div className="border-t border-border pt-4 text-xs text-muted-foreground">
              {props.item.width} × {props.item.height}px
            </div>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}

/**
 * Pick black or white text colour to maximise contrast against a
 * given hex background. Used so a custom-coloured tag pill (Phase 11)
 * stays readable.
 */
function pickContrastingText(hex: string): string {
  // Naive luma — sum of RGB channels normalised to 0-1, scaled by
  // perceptual weights. Threshold 0.5 picks black on light bgs.
  if (!/^#[0-9a-fA-F]{6}$/.test(hex)) return "#000";
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  const luma = 0.299 * r + 0.587 * g + 0.114 * b;
  return luma > 0.6 ? "#000" : "#fff";
}
