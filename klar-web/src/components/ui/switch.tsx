"use client";

// An on/off switch (role="switch"), shared by the settings toggles. The knob
// sits at an explicit `left-0.5` and moves by translate-x-5 (20px), which
// puts it flush with the right inset of the 44px track; without the explicit
// left, the "off" position had no reliable anchor to translate from.
export function Switch({
  checked,
  onCheckedChange,
  disabled,
  ...aria
}: {
  checked: boolean;
  onCheckedChange: (next: boolean) => void;
  disabled?: boolean;
  "aria-label"?: string;
  "aria-labelledby"?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      {...aria}
      onClick={() => onCheckedChange(!checked)}
      disabled={disabled}
      className={`relative h-6 w-11 shrink-0 rounded-full border transition-colors disabled:opacity-60 ${
        checked ? "border-primary bg-primary" : "border-border bg-input"
      }`}
    >
      <span
        className={`absolute left-0.5 top-0.5 h-5 w-5 rounded-full border border-border/50 bg-white shadow transition-transform ${
          checked ? "translate-x-5" : "translate-x-0"
        }`}
      />
    </button>
  );
}
