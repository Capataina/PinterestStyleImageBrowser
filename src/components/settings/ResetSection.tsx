import { RotateCcw } from "lucide-react";
import { useUserPreferences } from "../../hooks/useUserPreferences";
import { Section } from "./controls";

export function ResetSection() {
  const { resetAll } = useUserPreferences();

  return (
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
  );
}
