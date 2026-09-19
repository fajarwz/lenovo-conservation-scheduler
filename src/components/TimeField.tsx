import type { TimeFormat } from "../api";
import { useTranslation } from "../i18n";
import { storedTime, timeParts, toStoredTime } from "../i18n/time";

/**
 * The time of one schedule.
 *
 * Deliberately not the browser's own `<input type="time">`: its displayed format comes from Windows,
 * and Chromium ignores the language attribute - a time input with `lang="en-US"` renders and measures
 * exactly like one with `lang="en-GB"`, so the 24-hour/12-hour setting cannot reach it. Dropdowns
 * are native controls, so the keyboard and screen readers behave as they always do, and they show
 * the format the user chose.
 *
 * The value stays the stored 24-hour "HH:MM" whatever is displayed.
 */
export function TimeField({
  time,
  format,
  onChange,
  label,
}: {
  time: string;
  format: TimeFormat;
  onChange: (time: string) => void;
  label: string;
}) {
  const { t } = useTranslation();
  const { hour, minute } = timeParts(time);
  const twelveHour = format === "12h";
  const meridiem: "AM" | "PM" = hour < 12 ? "AM" : "PM";
  const shownHour = twelveHour ? (hour % 12 === 0 ? 12 : hour % 12) : hour;

  const hours = twelveHour
    ? Array.from({ length: 12 }, (_, index) => index + 1)
    : Array.from({ length: 24 }, (_, index) => index);
  const minutes = Array.from({ length: 60 }, (_, index) => index);

  return (
    <div className="flex items-center gap-1" role="group" aria-label={label}>
      <select
        className="field"
        aria-label={t("timeField.hour")}
        value={shownHour}
        onChange={(event) => {
          const shown = Number(event.target.value);
          onChange(twelveHour ? toStoredTime(shown, minute, meridiem) : storedTime(shown, minute));
        }}
      >
        {hours.map((value) => (
          <option key={value} value={value}>
            {String(value).padStart(2, "0")}
          </option>
        ))}
      </select>

      <span aria-hidden="true">:</span>

      <select
        className="field"
        aria-label={t("timeField.minute")}
        value={minute}
        onChange={(event) => onChange(storedTime(hour, Number(event.target.value)))}
      >
        {minutes.map((value) => (
          <option key={value} value={value}>
            {String(value).padStart(2, "0")}
          </option>
        ))}
      </select>

      {twelveHour && (
        <select
          className="field"
          aria-label={t("timeField.amPm")}
          value={meridiem}
          onChange={(event) =>
            onChange(toStoredTime(shownHour, minute, event.target.value as "AM" | "PM"))
          }
        >
          <option value="AM">AM</option>
          <option value="PM">PM</option>
        </select>
      )}
    </div>
  );
}
