/**
 * The app's only boolean control. A styled checkbox underneath, so keyboard and screen-reader
 * behaviour comes for free: space toggles it, focus is real, and the knob is drawn with ::after.
 *
 * The label is optional. The battery card prints its own label above the control, and a schedule
 * row's label is the column header, so those callers pass an aria-label and a tooltip instead.
 */
export function Toggle({
  label,
  checked,
  onChange,
  ariaLabel,
  title,
}: {
  label?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  ariaLabel?: string;
  title?: string;
}) {
  const control = (
    <input
      type="checkbox"
      role="switch"
      className="switch"
      checked={checked}
      aria-label={ariaLabel}
      title={title}
      onChange={(event) => onChange(event.target.checked)}
    />
  );

  if (!label) {
    return control;
  }

  return (
    <label className="flex items-center gap-2">
      {control}
      {label}
    </label>
  );
}
