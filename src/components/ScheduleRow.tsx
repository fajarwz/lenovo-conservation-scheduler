import type { Action, Day, Schedule, TimeFormat } from "../api";
import { useTranslation } from "../i18n";
import { DayPicker } from "./DayPicker";
import { TimeField } from "./TimeField";
import { Toggle } from "./Toggle";

/** One editable schedule line. */
export function ScheduleRow({
  schedule,
  timeFormat,
  onEdit,
  onToggleDay,
  onSetDays,
  onDelete,
}: {
  schedule: Schedule;
  timeFormat: TimeFormat;
  onEdit: (change: Partial<Schedule>) => void;
  onToggleDay: (day: Day) => void;
  onSetDays: (days: Day[]) => void;
  onDelete: () => void;
}) {
  const { t } = useTranslation();
  return (
    <tr className={schedule.enabled ? undefined : "opacity-50"}>
      <td className="cell">
        <Toggle
          checked={schedule.enabled}
          ariaLabel={t("scheduler.on")}
          title={schedule.enabled ? t("common.on") : t("common.off")}
          onChange={(enabled) => onEdit({ enabled })}
        />
      </td>
      <td className="cell">
        <TimeField
          time={schedule.time}
          format={timeFormat}
          label={t("scheduler.time")}
          onChange={(time) => onEdit({ time })}
        />
      </td>
      <td className="cell">
        <DayPicker schedule={schedule} onToggle={onToggleDay} onSet={onSetDays} />
      </td>
      <td className="cell">
        <select
          className="field"
          value={schedule.action}
          onChange={(event) => onEdit({ action: event.target.value as Action })}
        >
          <option value="conservation_off">{t("action.off")}</option>
          <option value="conservation_on">{t("action.on")}</option>
        </select>
      </td>
      <td className="cell text-right">
        <button
          type="button"
          className="row-remove"
          aria-label={t("row.delete")}
          title={t("row.delete")}
          onClick={onDelete}
        >
          ×
        </button>
      </td>
    </tr>
  );
}
