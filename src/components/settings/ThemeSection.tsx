import { Monitor, Sun, Moon } from "lucide-react";
import {
  useUserPreferences,
  type ThemeMode,
} from "../../hooks/useUserPreferences";
import { Section, SegmentedButtons } from "./controls";

export function ThemeSection() {
  const { prefs, update } = useUserPreferences();

  return (
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
  );
}
