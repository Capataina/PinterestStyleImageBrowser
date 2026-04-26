import {
  useUserPreferences,
  type AnimationLevel,
} from "../../hooks/useUserPreferences";
import { Section, Field, Slider, SegmentedButtons } from "./controls";

export function DisplaySection() {
  const { prefs, update } = useUserPreferences();

  return (
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
  );
}
