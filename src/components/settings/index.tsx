import { motion, AnimatePresence } from "framer-motion";
import { X } from "lucide-react";
import { useEffect } from "react";
import { ThemeSection } from "./ThemeSection";
import { DisplaySection } from "./DisplaySection";
import { SearchSection } from "./SearchSection";
import { SortSection } from "./SortSection";
import { FoldersSection } from "./FoldersSection";
import { StatsSection } from "./StatsSection";
import { ResetSection } from "./ResetSection";

interface SettingsDrawerProps {
  open: boolean;
  onClose: () => void;
}

/**
 * Slide-out settings drawer from the right edge.
 *
 * Open via cmd/ctrl + , or via the gear icon next to the search bar.
 * Closes on Escape, click-outside, or × button.
 *
 * Sections (each lives in its own file under this directory):
 * 1. Theme
 * 2. Display (column count, tile scale, animation level)
 * 3. Search (similar / semantic result counts, tag filter mode)
 * 4. Sort order
 * 5. Folders (add / remove / toggle / list)
 * 6. Reset
 *
 * The shell here owns purely structural concerns: enter/exit animation,
 * backdrop click-to-dismiss, the Escape-key handler, header chrome, and
 * the scroll container. Each section consumes useUserPreferences (and the
 * roots hooks where applicable) directly so they remain testable in
 * isolation without prop-drilling.
 */
export function SettingsDrawer({ open, onClose }: SettingsDrawerProps) {
  // Esc closes the drawer.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  return (
    <AnimatePresence>
      {open && (
        <>
          {/* Backdrop — click to dismiss */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-[90] bg-black/40 backdrop-blur-[2px]"
            onClick={onClose}
          />

          {/* Drawer panel */}
          <motion.aside
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={{ type: "spring", stiffness: 360, damping: 36 }}
            className="fixed top-0 right-0 z-[91] h-screen w-[400px] max-w-[100vw] flex flex-col bg-card border-l border-border shadow-2xl shadow-black/50 overflow-y-auto"
          >
            {/* Header */}
            <div className="flex items-center justify-between p-5 border-b border-border sticky top-0 bg-card/95 backdrop-blur-md">
              <h2 className="text-base font-semibold">Settings</h2>
              <button
                onClick={onClose}
                aria-label="Close settings"
                className="rounded-full p-1.5 hover:bg-accent transition"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-7 text-sm">
              <ThemeSection />
              <DisplaySection />
              <SearchSection />
              <SortSection />
              <FoldersSection />
              <StatsSection />
              <ResetSection />
            </div>
          </motion.aside>
        </>
      )}
    </AnimatePresence>
  );
}
