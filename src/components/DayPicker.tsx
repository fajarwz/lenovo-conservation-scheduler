import { DAYS, WEEKDAYS, type Day, type Schedule } from "../api";
import { useTranslation } from "../i18n";

/** The weekday buttons of one schedule, with the two sets people actually use as one click. */
export function DayPicker({
  schedule,
  onToggle,
  onSet,
}: {
  schedule: Schedule;
  onToggle: (day: Day) => void;
  onSet: (days: Day[]) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-[3px]">
      {DAYS.map((day) => (
        <button
          key={day}
          type="button"
          className={schedule.days.includes(day) ? "day-btn day-btn-on" : "day-btn"}
          aria-pressed={schedule.days.includes(day)}
          // Two letters save the row a lot of width, so the full name is a hover away - and it is
          // what a screen reader announces, which "Sn" alone would not be.
          aria-label={t(`day.long.${day}`)}
          title={t(`day.long.${day}`)}
          onClick={() => onToggle(day)}
        >
          {t(`day.${day}`)}
        </button>
      ))}
      <span className="ml-1 flex gap-1">
        <button type="button" className="preset" onClick={() => onSet([...WEEKDAYS])}>
          {t("scheduler.presetWeekdays")}
        </button>
        <button type="button" className="preset" onClick={() => onSet([...DAYS])}>
          {t("scheduler.presetAll")}
        </button>
      </span>
    </div>
  );
}
