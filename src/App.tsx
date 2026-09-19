import { useEffect, useRef, useState } from "react";
import {
  DAYS,
  type Config,
  type Day,
  type Schedule,
  type Snapshot,
  type Status,
  getState,
  newScheduleId,
  onStateChanged,
  saveConfig,
  setConservation,
} from "./api";
import { Banner } from "./components/Banner";
import { ConservationSwitch } from "./components/ConservationSwitch";
import { Fact } from "./components/Fact";
import { ScheduleRow } from "./components/ScheduleRow";
import { Toggle } from "./components/Toggle";
import {
  LOCALES,
  LocaleProvider,
  translate,
  type Locale,
  type TranslationKey,
  type TranslationParams,
} from "./i18n";

function newSchedule(): Schedule {
  return {
    id: newScheduleId(),
    enabled: true,
    time: "05:00",
    days: [...DAYS],
    action: "conservation_off",
  };
}

type Translate = (key: TranslationKey, params?: TranslationParams) => string;

/**
 * What the "Charging" cell says. The flag behind it means "current is flowing into the battery right
 * now", so a full battery on the charger is not charging, and neither is one a charge cap is holding
 * - which is why those two states are named instead of left as a bare "No" that reads like a fault.
 * When the reason is unknown, "No" is the honest answer: this is not the place to guess.
 */
function chargingText(status: Status, t: Translate, unavailable: string): string {
  if (status.charging === null) {
    return unavailable;
  }
  if (status.charging) {
    return t("common.yes");
  }
  if (status.acOnline === false) {
    return t("common.no");
  }
  if ((status.batteryPercent ?? 0) >= 100) {
    return t("common.full");
  }
  return status.conservationOn === true ? t("common.paused") : t("common.no");
}

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [draft, setDraft] = useState<Config | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const lastKnown = useRef<Snapshot | null>(null);

  // Adopt a snapshot pushed by the backend, keeping unsaved edits if the user has any.
  function adopt(next: Snapshot) {
    const previous = lastKnown.current;
    lastKnown.current = next;
    setSnapshot(next);
    setDraft((current) =>
      previous && current && JSON.stringify(current) !== JSON.stringify(previous.config)
        ? current
        : next.config,
    );
  }

  useEffect(() => {
    let active = true;
    getState()
      .then((next) => {
        if (active) adopt(next);
      })
      .catch((problem) => setError(String(problem)));

    const unlisten = onStateChanged((next) => {
      if (active) adopt(next);
    });

    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
    // Runs once: the backend pushes every later change through the event above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const locale: Locale = draft?.locale ?? "en-US";
  const t = (key: TranslationKey, params?: TranslationParams) =>
    translate(locale, key, params);

  const dirty =
    snapshot !== null && draft !== null && JSON.stringify(draft) !== JSON.stringify(snapshot.config);

  async function run(action: () => Promise<Snapshot>) {
    setError(null);
    setSaved(false);
    try {
      adopt(await action());
      setSaved(true);
    } catch (problem) {
      setError(String(problem));
    }
  }

  function editConfig(change: Partial<Config>) {
    setDraft((current) => (current ? { ...current, ...change } : current));
    setSaved(false);
  }

  function editSchedule(id: string, change: Partial<Schedule>) {
    setDraft((current) =>
      current
        ? {
            ...current,
            schedules: current.schedules.map((schedule) =>
              schedule.id === id ? { ...schedule, ...change } : schedule,
            ),
          }
        : current,
    );
    setSaved(false);
  }

  function addSchedule() {
    setDraft((current) =>
      current ? { ...current, schedules: [...current.schedules, newSchedule()] } : current,
    );
    setSaved(false);
  }

  function removeSchedule(id: string) {
    setDraft((current) =>
      current
        ? { ...current, schedules: current.schedules.filter((schedule) => schedule.id !== id) }
        : current,
    );
    setSaved(false);
  }

  function toggleDay(schedule: Schedule, day: Day) {
    const days = schedule.days.includes(day)
      ? schedule.days.filter((selected) => selected !== day)
      : [...schedule.days, day].sort((a, b) => DAYS.indexOf(a) - DAYS.indexOf(b));
    editSchedule(schedule.id, { days });
  }

  // The language is not a draft like the other settings: it applies at once, so the window, the
  // tray menu and the notifications all change together instead of waiting for Save.
  function changeLocale(next: Locale) {
    if (!draft) {
      return;
    }
    const updated = { ...draft, locale: next };
    setDraft(updated);
    void run(() => saveConfig(updated));
  }

  if (!draft || !snapshot) {
    return (
      <main className="mx-auto flex max-w-3xl flex-col gap-3.5 p-4">
        <p className="muted">
          {error ? t("load.failed", { error }) : t("common.loading")}
        </p>
      </main>
    );
  }

  const { status } = snapshot;
  const unavailable = t("common.unavailable");

  return (
    <LocaleProvider locale={draft.locale}>
      <main className="mx-auto flex max-w-3xl flex-col gap-3.5 p-4">
        <header>
          <h1 className="text-lg font-semibold">Lenovo Conservation Scheduler</h1>
          <p className="muted mt-0.5 text-sm">
            {status.conservationAvailable ? t("app.subtitle") : t("app.unavailable")}
          </p>
        </header>

        {error && <Banner tone="error">{error}</Banner>}
        {!error && status.warning && <Banner tone="warn">{status.warning}</Banner>}

        <section className="card">
          <h2 className="section-title">{t("battery.title")}</h2>
          <dl className="grid grid-cols-4 gap-2">
            <Fact
              label={t("battery.charge")}
              value={
                status.batteryPercent === null ? unavailable : `${status.batteryPercent}%`
              }
            />
            <Fact
              label={t("battery.power")}
              value={
                status.acOnline === null
                  ? unavailable
                  : status.acOnline
                    ? t("battery.pluggedIn")
                    : t("battery.onBattery")
              }
            />
            <Fact
              label={t("battery.charging")}
              value={chargingText(status, t, unavailable)}
            />
            <Fact
              label={t("battery.conservation")}
              value={
                status.conservationAvailable && status.conservationOn !== null ? (
                  <ConservationSwitch
                    on={status.conservationOn}
                    onChange={(on) => run(() => setConservation(on))}
                  />
                ) : (
                  unavailable
                )
              }
            />
          </dl>

          {/* The switch shows the state, so this line only has to explain why there is none. */}
          {!status.conservationAvailable && (
            <p className="muted text-sm">
              {status.conservationMessage ?? t("battery.unavailableFallback")}
            </p>
          )}
        </section>

        <section className="card">
          <h2 className="section-title">{t("scheduler.title")}</h2>
          <Toggle
            label={t("scheduler.apply")}
            checked={draft.scheduleEnabled}
            onChange={(scheduleEnabled) => editConfig({ scheduleEnabled })}
          />

          <p className="muted text-sm">
            {status.nextEventAt && status.nextEventAction
              ? t("scheduler.next", {
                  action:
                    status.nextEventAction === "conservation_on"
                      ? t("action.on")
                      : t("action.off"),
                  time: status.nextEventAt.slice(11),
                })
              : t("scheduler.nothing")}
          </p>

          <table className="w-full border-collapse">
            <thead>
              <tr>
                <th className="head-cell">{t("scheduler.on")}</th>
                <th className="head-cell">{t("scheduler.time")}</th>
                <th className="head-cell">{t("scheduler.days")}</th>
                <th className="head-cell">{t("scheduler.action")}</th>
                <th className="head-cell" />
              </tr>
            </thead>
            <tbody>
              {draft.schedules.length === 0 && (
                <tr>
                  <td colSpan={5} className="cell muted">
                    {t("scheduler.empty")}
                  </td>
                </tr>
              )}
              {draft.schedules.map((schedule) => (
                <ScheduleRow
                  key={schedule.id}
                  schedule={schedule}
                  onEdit={(change) => editSchedule(schedule.id, change)}
                  onToggleDay={(day) => toggleDay(schedule, day)}
                  onDelete={() => removeSchedule(schedule.id)}
                />
              ))}
            </tbody>
          </table>
        </section>

        <section className="card">
          <h2 className="section-title">{t("settings.title")}</h2>
          <Toggle
            label={t("settings.startWithWindows")}
            checked={draft.startWithWindows}
            onChange={(startWithWindows) => editConfig({ startWithWindows })}
          />
          <Toggle
            label={t("settings.notify")}
            checked={draft.notifyOnChange}
            onChange={(notifyOnChange) => editConfig({ notifyOnChange })}
          />
          <label className="flex items-center">
            <span className="mr-2">{t("settings.language")}</span>
            <select
              className="field"
              value={draft.locale}
              onChange={(event) => changeLocale(event.target.value as Locale)}
            >
              {LOCALES.map((entry) => (
                <option key={entry.value} value={entry.value}>
                  {entry.label}
                </option>
              ))}
            </select>
          </label>
        </section>

        <footer className="flex items-center gap-2.5 pb-2">
          <button type="button" className="btn" onClick={addSchedule}>
            {t("footer.add")}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={!dirty}
            onClick={() => run(() => saveConfig(draft))}
          >
            {t("footer.save")}
          </button>
          {saved && !dirty && <span className="muted text-sm">{t("footer.saved")}</span>}
        </footer>
      </main>
    </LocaleProvider>
  );
}
