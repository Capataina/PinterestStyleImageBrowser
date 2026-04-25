import { motion, AnimatePresence } from "framer-motion";
import {
  X,
  FolderPlus,
  Trash2,
  RotateCcw,
  Monitor,
  Sun,
  Moon,
} from "lucide-react";
import { useEffect } from "react";
import {
  useUserPreferences,
  type AnimationLevel,
  type SortMode,
  type ThemeMode,
} from "../hooks/useUserPreferences";
import {
  useAddRoot,
  useRemoveRoot,
  useRoots,
  useSetRootEnabled,
} from "../queries/useRoots";
import { pickScanFolder } from "../services/images";

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
 * Sections:
 * 1. Theme
 * 2. Display (column count, tile scale, animation level)
 * 3. Search (similar / semantic result counts, tag filter mode)
 * 4. Sort order
 * 5. Folders (add / remove / toggle / list)
 * 6. Reset
 *
 * All settings persist via the useUserPreferences hook (localStorage).
 * Folder management hits the multi-folder Tauri commands directly.
 */
export function SettingsDrawer({ open, onClose }: SettingsDrawerProps) {
  const { prefs, update, resetAll } = useUserPreferences();
  const { data: roots } = useRoots();
  const addRootMutation = useAddRoot();
  const removeRootMutation = useRemoveRoot();
  const toggleRootMutation = useSetRootEnabled();

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
              {/* Theme */}
              <Section title="Theme">
                <SegmentedButtons
                  value={prefs.theme}
                  onChange={(v) => update("theme", v)}
                  options={[
                    {
                      value: "system",
                      label: "System",
                      icon: <Monitor className="h-3.5 w-3.5" />,
                    },
                    {
                      value: "light",
                      label: "Light",
                      icon: <Sun className="h-3.5 w-3.5" />,
                    },
                    {
                      value: "dark",
                      label: "Dark",
                      icon: <Moon className="h-3.5 w-3.5" />,
                    },
                  ] satisfies Array<{
                    value: ThemeMode;
                    label: string;
                    icon: React.ReactNode;
                  }>}
                />
              </Section>

              {/* Display */}
              <Section title="Display">
                <Field
                  label="Columns"
                  hint={
                    prefs.columnCount === 0
                      ? "Auto (computed from window width)"
                      : `${prefs.columnCount} columns`
                  }
                >
                  <Slider
                    min={0}
                    max={8}
                    step={1}
                    value={prefs.columnCount}
                    onChange={(v) => update("columnCount", v)}
                  />
                </Field>

                <Field
                  label="Tile size"
                  hint={`${prefs.tileScale.toFixed(2)}× base size`}
                >
                  <Slider
                    min={0.6}
                    max={2.0}
                    step={0.05}
                    value={prefs.tileScale}
                    onChange={(v) => update("tileScale", v)}
                  />
                </Field>

                <Field label="Animations">
                  <SegmentedButtons
                    value={prefs.animationLevel}
                    onChange={(v) => update("animationLevel", v)}
                    options={[
                      { value: "off", label: "Off" },
                      { value: "subtle", label: "Subtle" },
                      { value: "standard", label: "Standard" },
                    ] satisfies Array<{
                      value: AnimationLevel;
                      label: string;
                    }>}
                  />
                </Field>
              </Section>

              {/* Search */}
              <Section title="Search">
                <Field
                  label="More like this — result count"
                  hint={`${prefs.similarResultCount} images`}
                >
                  <Slider
                    min={5}
                    max={75}
                    step={5}
                    value={prefs.similarResultCount}
                    onChange={(v) => update("similarResultCount", v)}
                  />
                </Field>

                <Field
                  label="Semantic search — result count"
                  hint={`${prefs.semanticResultCount} images`}
                >
                  <Slider
                    min={10}
                    max={100}
                    step={10}
                    value={prefs.semanticResultCount}
                    onChange={(v) => update("semanticResultCount", v)}
                  />
                </Field>

                <Field
                  label="Tag filter"
                  hint={
                    prefs.tagFilterMode === "all"
                      ? "Image must have ALL selected tags"
                      : "Image must have ANY selected tag"
                  }
                >
                  <SegmentedButtons
                    value={prefs.tagFilterMode}
                    onChange={(v) => update("tagFilterMode", v)}
                    options={[
                      { value: "any", label: "Any" },
                      { value: "all", label: "All" },
                    ]}
                  />
                </Field>
              </Section>

              {/* Sort */}
              <Section title="Sort order">
                <SegmentedButtons
                  value={prefs.sortMode}
                  onChange={(v) => update("sortMode", v)}
                  options={[
                    { value: "shuffle", label: "Shuffle" },
                    { value: "name", label: "Name" },
                    { value: "added", label: "Added" },
                  ] satisfies Array<{
                    value: SortMode;
                    label: string;
                  }>}
                />
              </Section>

              {/* Folders */}
              <Section title="Folders">
                <p className="text-xs text-muted-foreground -mt-1">
                  The app indexes every enabled folder recursively. Disable
                  to exclude without losing the index; remove to delete the
                  index entirely.
                </p>
                <div className="flex flex-col gap-2">
                  {(roots ?? []).map((root) => (
                    <div
                      key={root.id}
                      className="flex items-center gap-3 rounded-lg border border-border bg-secondary/40 px-3 py-2.5"
                    >
                      <Toggle
                        checked={root.enabled}
                        onChange={(enabled) =>
                          toggleRootMutation.mutate({ id: root.id, enabled })
                        }
                      />
                      <div className="flex-1 min-w-0">
                        <p
                          className={[
                            "text-xs truncate",
                            root.enabled
                              ? "text-foreground"
                              : "text-muted-foreground",
                          ].join(" ")}
                          title={root.path}
                        >
                          {root.path}
                        </p>
                      </div>
                      <button
                        onClick={() => {
                          if (
                            window.confirm(
                              `Remove ${root.path}?\n\nThe images from this folder will be removed from the index. The actual files on disk are not touched.`,
                            )
                          ) {
                            removeRootMutation.mutate(root.id);
                          }
                        }}
                        aria-label="Remove folder"
                        className="rounded p-1.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  ))}

                  {(roots ?? []).length === 0 && (
                    <p className="text-xs text-muted-foreground italic">
                      No folders configured yet.
                    </p>
                  )}
                </div>

                <button
                  className="flex items-center gap-2 rounded-lg bg-primary text-primary-foreground px-3 py-2 text-xs font-medium hover:opacity-90 transition w-full justify-center"
                  onClick={async () => {
                    try {
                      const folder = await pickScanFolder();
                      if (!folder) return;
                      await addRootMutation.mutateAsync(folder);
                    } catch (err) {
                      window.alert(
                        `Could not add folder: ${err instanceof Error ? err.message : String(err)}`,
                      );
                    }
                  }}
                >
                  <FolderPlus className="h-3.5 w-3.5" />
                  Add folder
                </button>
              </Section>

              {/* Reset */}
              <Section title="Reset">
                <button
                  onClick={() => {
                    if (
                      window.confirm(
                        "Reset all UI preferences to defaults? Your images, tags, and folder list are NOT affected.",
                      )
                    ) {
                      resetAll();
                    }
                  }}
                  className="flex items-center gap-2 rounded-lg border border-border bg-transparent px-3 py-2 text-xs font-medium hover:bg-accent transition"
                >
                  <RotateCcw className="h-3.5 w-3.5" />
                  Reset all preferences
                </button>
              </Section>
            </div>
          </motion.aside>
        </>
      )}
    </AnimatePresence>
  );
}

// ---------- small layout helpers, kept inline for cohesion ----------

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-3">
      <h3 className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">
        {title}
      </h3>
      {children}
    </section>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-xs">
        <span className="font-medium">{label}</span>
        {hint && <span className="text-muted-foreground tabular-nums">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

function Slider({
  min,
  max,
  step,
  value,
  onChange,
}: {
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <input
      type="range"
      min={min}
      max={max}
      step={step}
      value={value}
      onChange={(e) => onChange(parseFloat(e.target.value))}
      className="w-full h-1.5 rounded-full appearance-none bg-secondary [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:h-3.5 [&::-webkit-slider-thumb]:w-3.5 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:cursor-pointer [&::-webkit-slider-thumb]:shadow-sm"
    />
  );
}

function SegmentedButtons<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: Array<{ value: T; label: string; icon?: React.ReactNode }>;
}) {
  return (
    <div className="flex rounded-lg bg-secondary/60 p-1 border border-border">
      {options.map((opt) => (
        <button
          key={opt.value}
          onClick={() => onChange(opt.value)}
          className={[
            "flex-1 flex items-center justify-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-medium transition",
            value === opt.value
              ? "bg-card text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground",
          ].join(" ")}
        >
          {opt.icon}
          {opt.label}
        </button>
      ))}
    </div>
  );
}

function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={[
        "relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full transition-colors",
        checked ? "bg-primary" : "bg-secondary",
      ].join(" ")}
    >
      <span
        className={[
          "inline-block h-3.5 w-3.5 transform rounded-full bg-card shadow-sm transition-transform mt-0.75 ml-0.75",
          checked ? "translate-x-4" : "translate-x-0",
        ].join(" ")}
        style={{
          marginTop: 3,
          marginLeft: checked ? 18 : 3,
        }}
      />
    </button>
  );
}
