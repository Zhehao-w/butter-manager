/** Visual interpolation follows real task progress; unknown totals use a continuous activity cue. */
export function ProgressBar({
  label,
  value,
  max = 1,
}: {
  label: string;
  value?: number;
  max?: number;
}) {
  const determinate = value != null && Number.isFinite(value) && max > 0;
  const current = determinate ? Math.min(max, Math.max(0, value)) : undefined;
  return (
    <div
      className={`progress-track${determinate ? '' : ' indeterminate'}`}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={determinate ? max : undefined}
      aria-valuenow={current}
    >
      <div
        className="progress-fill"
        style={determinate ? { transform: `scaleX(${current! / max})` } : undefined}
      />
    </div>
  );
}
