import { useUserPreferences } from "../../hooks/useUserPreferences";
import { Section, Field, Slider, SegmentedButtons } from "./controls";

export function SearchSection() {
  const { prefs, update } = useUserPreferences();

  return (
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
  );
}
