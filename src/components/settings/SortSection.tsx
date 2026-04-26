import {
  useUserPreferences,
  type SortMode,
} from "../../hooks/useUserPreferences";
import { recordAction } from "../../services/perf";
import { Section, SegmentedButtons } from "./controls";

export function SortSection() {
  const { prefs, update } = useUserPreferences();

  return (
    <Section title="Sort order">
      <SegmentedButtons
        value={prefs.sortMode}
        onChange={(v) => {
          recordAction("sort_change", {
            from: prefs.sortMode,
            to: v,
          });
          update("sortMode", v);
        }}
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
  );
}
