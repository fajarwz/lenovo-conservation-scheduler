import type { ReactNode } from "react";

const TONES = {
  error: "border-red-600 text-red-700 dark:border-red-500 dark:text-red-400",
  warn: "border-amber-600 text-amber-700 dark:border-amber-500 dark:text-amber-400",
};

/** Something the user should see before the settings: a failure or a warning from startup. */
export function Banner({ tone, children }: { tone: keyof typeof TONES; children: ReactNode }) {
  return <p className={`rounded-md border px-2.5 py-2 text-sm ${TONES[tone]}`}>{children}</p>;
}
