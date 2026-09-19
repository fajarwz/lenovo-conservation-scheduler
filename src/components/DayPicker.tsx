import { DAYS, type Day, type Schedule } from "../api";
import { useTranslation } from "../i18n";

/** The weekday buttons of one schedule. */
export function DayPicker({
  schedule,
  onToggle,
}: {
  schedule: Schedule;
  onToggle: (day: Day) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap gap-[3px]">
      {DAYS.map((day) => (
        <button
          key={day}
          type="button"
          className={schedule.days.includes(day) ? "day-btn day-btn-on" : "day-btn"}
          onClick={() => onToggle(day)}
        >
          {t(`day.${day}`)}
        </button>
      ))}
    </div>
  );
}
