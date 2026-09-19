import type { ReactNode } from "react";

/** One figure in the battery summary. The value may be text or a control such as the switch. */
export function Fact({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <dt className="muted text-xs">{label}</dt>
      <dd className="mt-0.5 font-semibold">{value}</dd>
    </div>
  );
}
