// Writing times the way the user asked for them.
//
// Pure, and free of React, so the conversions that are easy to get wrong - midnight, noon, and the
// trip through the 12-hour view - can be reasoned about and run on their own.
import type { TimeFormat } from "../api";

export const TIME_FORMATS: TimeFormat[] = ["24h", "12h"];

/** The parts of a stored "HH:MM", for a picker that has to show them. */
export function timeParts(time: string): { hour: number; minute: number } {
  const [hours, minutes] = time.split(":").map(Number);
  return {
    hour: Number.isFinite(hours) && hours >= 0 && hours < 24 ? hours : 0,
    minute: Number.isFinite(minutes) && minutes >= 0 && minutes < 60 ? minutes : 0,
  };
}

/** The stored form: always 24-hour "HH:MM", two digits each, whatever the window shows. */
export function storedTime(hour: number, minute: number): string {
  return `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
}

/** From the 12-hour view back to the stored form: 12 AM is 00 and 12 PM is 12. */
export function toStoredTime(shown: number, minute: number, meridiem: "AM" | "PM"): string {
  const hour = meridiem === "PM" ? (shown % 12) + 12 : shown % 12;
  return storedTime(hour, minute);
}

/** "17:30" as the user asked to read it: "17:30", or "5:30 PM". */
export function formatTime(time: string, format: TimeFormat): string {
  if (format === "24h") {
    return time;
  }
  const { hour, minute } = timeParts(time);
  const suffix = hour < 12 ? "AM" : "PM";
  return `${hour % 12 === 0 ? 12 : hour % 12}:${String(minute).padStart(2, "0")} ${suffix}`;
}
