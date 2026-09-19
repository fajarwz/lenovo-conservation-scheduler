import { useTranslation } from "../i18n";

/**
 * Conservation Mode as a switch, replacing the old pair of ON/OFF buttons: one control that shows
 * the state and changes it, instead of two that only change it.
 */
export function ConservationSwitch({
  on,
  onChange,
}: {
  on: boolean;
  onChange: (on: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <input
      type="checkbox"
      role="switch"
      className="switch"
      checked={on}
      aria-label={t("battery.conservation")}
      title={on ? t("common.on") : t("common.off")}
      onChange={(event) => onChange(event.target.checked)}
    />
  );
}
