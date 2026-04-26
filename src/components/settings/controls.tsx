import type React from "react";

/**
 * Small layout + input primitives shared across the settings sections.
 *
 * These were originally co-located inside SettingsDrawer.tsx; lifting them
 * here keeps each section file focused on its own controls while every
 * section consumes the same visual + behavioural primitives.
 */

export function Section({
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

export function Field({
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

export function Slider({
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

export function SegmentedButtons<T extends string>({
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

export function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  // h-5 (20px) track, w-9 (36px) wide, p-0.5 (2px) inset.
  // Thumb is h-4 (16px) — leaves 36 - 16 - 2*2 = 16px of horizontal
  // travel, exactly translate-x-4. Thumb stays inside the track in both
  // states, no margin-juggling.
  // Off state uses bg-input + bg-foreground thumb so it's clearly
  // visible against the dark theme (bg-card thumb on bg-secondary track
  // had no contrast and made the off state look broken).
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={[
        "relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors focus:outline-none focus:ring-2 focus:ring-primary/50",
        checked ? "bg-primary" : "bg-input",
      ].join(" ")}
    >
      <span
        className={[
          "inline-block h-4 w-4 transform rounded-full shadow-sm transition-transform",
          checked
            ? "translate-x-4 bg-primary-foreground"
            : "translate-x-0 bg-foreground",
        ].join(" ")}
      />
    </button>
  );
}
