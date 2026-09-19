import type { Action, Day, Schedule } from "../api";
import { useTranslation } from "../i18n";
import { DayPicker } from "./DayPicker";

/** One editable schedule line. */
export function ScheduleRow({
  schedule,
  onEdit,
  onToggleDay,
  onDelete,
}: {
  schedule: Schedule;
  onEdit: (change: Partial<Schedule>) => void;
  onToggleDay: (day: Day) => void;
  onDelete: () => void;
}) {
  const { t } = useTranslation();
  return (
    <tr className={schedule.enabled ? undefined : "opacity-50"}>
      <td className="cell">
        <input
          type="checkbox"
          className="accent-sky-600"
          checked={schedule.enabled}
          aria-label={t("scheduler.on")}
          onChange={(event) => onEdit({ enabled: event.target.checked })}
        />
      </td>
      <td className="cell">
        <input
          type="time"
          className="field"
          value={schedule.time}
          onChange={(event) => onEdit({ time: event.target.value })}
        />
      </td>
      <td className="cell">
        <DayPicker schedule={schedule} onToggle={onToggleDay} />
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
      <td className="cell">
        <button type="button" className="btn" onClick={onDelete}>
          {t("row.delete")}
        </button>
      </td>
    </tr>
  );
}
