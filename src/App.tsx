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

/** How long the typing can pause before a change is written. */
const SAVE_DELAY_MS = 500;
/** How long a removed schedule can be brought back. */
const UNDO_MS = 6000;

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

/** Next switch as one status line, with the weekday named in the current language. */
function nextEventText(status: Status, locale: Locale, t: Translate): string {
  if (!status.nextEventAt || !status.nextEventAction) {
    return t("scheduler.nothing");
  }
  const at = new Date(status.nextEventAt);
  const day = Number.isNaN(at.getTime())
    ? ""
    : new Intl.DateTimeFormat(locale, { weekday: "long" }).format(at);
  return t("scheduler.next", {
    action: status.nextEventAction === "conservation_on" ? t("action.on") : t("action.off"),
    day,
    time: status.nextEventAt.slice(11),
  });
}

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [draft, setDraft] = useState<Config | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved">("idle");
  const [removed, setRemoved] = useState<{ schedule: Schedule; index: number } | null>(null);

  const lastKnown = useRef<Snapshot | null>(null);
  const pending = useRef<Config | null>(null);
  const saveTimer = useRef<number | null>(null);
  const saving = useRef(false);
  const undoTimer = useRef<number | null>(null);

  // Adopt a snapshot pushed by the backend, keeping edits that have not been written yet.
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

  // Closing the window must not leave a timer holding a change that never gets written.
  useEffect(
    () => () => {
      if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
      if (undoTimer.current !== null) window.clearTimeout(undoTimer.current);
      const last = pending.current;
      if (last) void saveConfig(last).catch(() => undefined);
    },
    [],
  );

  const locale: Locale = draft?.locale ?? "en-US";
  const t = (key: TranslationKey, params?: TranslationParams) => translate(locale, key, params);

  /**
   * The only way the settings change: update what the user sees, then write it a moment later.
   * There is no Save button, because one would sit nowhere near whatever was just edited.
   */
  function edit(next: Config) {
    setDraft(next);
    pending.current = next;
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => void flush(), SAVE_DELAY_MS);
  }

  async function flush() {
    saveTimer.current = null;
    if (saving.current || !pending.current) {
      return;
    }
    const next = pending.current;
    pending.current = null;
    saving.current = true;
    setSaveState("saving");
    try {
      adopt(await saveConfig(next));
      setSaveState("saved");
    } catch (problem) {
      setError(String(problem));
      setSaveState("idle");
    } finally {
      saving.current = false;
      // Edits that arrived while writing: write them too.
      if (pending.current) {
        saveTimer.current = window.setTimeout(() => void flush(), SAVE_DELAY_MS);
      }
    }
  }

  function editConfig(change: Partial<Config>) {
    if (!draft) {
      return;
    }
    setError(null);
    edit({ ...draft, ...change });
  }

  function editSchedule(id: string, change: Partial<Schedule>) {
    if (!draft) {
      return;
    }
    setError(null);
    edit({
      ...draft,
      schedules: draft.schedules.map((schedule) =>
        schedule.id === id ? { ...schedule, ...change } : schedule,
      ),
    });
  }

  function toggleDay(schedule: Schedule, day: Day) {
    const days = schedule.days.includes(day)
      ? schedule.days.filter((selected) => selected !== day)
      : [...schedule.days, day].sort((a, b) => DAYS.indexOf(a) - DAYS.indexOf(b));
    editSchedule(schedule.id, { days });
  }

  function addSchedule() {
    if (!draft) {
      return;
    }
    setError(null);
    edit({ ...draft, schedules: [...draft.schedules, newSchedule()] });
  }

  function removeSchedule(id: string) {
    if (!draft) {
      return;
    }
    const index = draft.schedules.findIndex((schedule) => schedule.id === id);
    if (index < 0) {
      return;
    }
    // Removing is instant, so offer it back rather than asking first.
    setRemoved({ schedule: draft.schedules[index], index });
    if (undoTimer.current !== null) {
      window.clearTimeout(undoTimer.current);
    }
    undoTimer.current = window.setTimeout(() => setRemoved(null), UNDO_MS);
    edit({ ...draft, schedules: draft.schedules.filter((schedule) => schedule.id !== id) });
  }

  function undoRemove() {
    if (!draft || !removed) {
      return;
    }
    if (undoTimer.current !== null) {
      window.clearTimeout(undoTimer.current);
    }
    const schedules = [...draft.schedules];
    schedules.splice(Math.min(removed.index, schedules.length), 0, removed.schedule);
    setRemoved(null);
    edit({ ...draft, schedules });
  }

  /** Manual switching is a command, not a setting: it takes effect at once and returns a snapshot. */
  async function run(action: () => Promise<Snapshot>) {
    setError(null);
    try {
      adopt(await action());
    } catch (problem) {
      setError(String(problem));
    }
  }

  if (!draft || !snapshot) {
    return (
      <main className="mx-auto flex max-w-3xl flex-col gap-3.5 p-4">
        <p className="muted">{error ? t("load.failed", { error }) : t("common.loading")}</p>
      </main>
    );
  }

  const { status } = snapshot;
  const unavailable = t("common.unavailable");
  const empty = draft.schedules.length === 0;

  return (
    <LocaleProvider locale={draft.locale}>
      <main className="mx-auto flex max-w-3xl flex-col gap-3.5 p-4">
        <header className="flex items-start justify-between gap-3">
          <div>
            <h1 className="text-lg font-semibold">Lenovo Conservation Scheduler</h1>
            <p className="muted mt-0.5 text-sm">
              {status.conservationAvailable ? t("app.subtitle") : t("app.unavailable")}
            </p>
          </div>
          <span className="muted shrink-0 text-sm" aria-live="polite">
            {saveState === "saving" ? t("status.saving") : null}
            {saveState === "saved" ? t("status.saved") : null}
          </span>
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
            <Fact label={t("battery.charging")} value={chargingText(status, t, unavailable)} />
            <Fact
              label={t("battery.conservation")}
              value={
                status.conservationAvailable && status.conservationOn !== null ? (
                  <Toggle
                    checked={status.conservationOn}
                    ariaLabel={t("battery.conservation")}
                    title={status.conservationOn ? t("common.on") : t("common.off")}
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
          <div className="flex items-center justify-between gap-3">
            <h2 className="section-title">{t("scheduler.title")}</h2>
            {!empty && (
              <button type="button" className="btn btn-sm" onClick={addSchedule}>
                {t("scheduler.add")}
              </button>
            )}
          </div>

          <p className="text-sm font-medium">{nextEventText(status, draft.locale, t)}</p>

          <Toggle
            label={t("scheduler.apply")}
            checked={draft.scheduleEnabled}
            onChange={(scheduleEnabled) => editConfig({ scheduleEnabled })}
          />
          {!draft.scheduleEnabled && <p className="muted text-sm">{t("scheduler.off")}</p>}

          {empty ? (
            <div className="flex flex-col items-start gap-2 rounded-md border border-dashed border-neutral-300 p-3 dark:border-neutral-600">
              <p className="muted text-sm">{t("scheduler.empty")}</p>
              <button type="button" className="btn btn-primary btn-sm" onClick={addSchedule}>
                {t("scheduler.add")}
              </button>
            </div>
          ) : (
            <table
              className={`w-full border-collapse ${draft.scheduleEnabled ? "" : "opacity-50"}`}
            >
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
                {draft.schedules.map((schedule) => (
                  <ScheduleRow
                    key={schedule.id}
                    schedule={schedule}
                    onEdit={(change) => editSchedule(schedule.id, change)}
                    onToggleDay={(day) => toggleDay(schedule, day)}
                    onSetDays={(days) => editSchedule(schedule.id, { days })}
                    onDelete={() => removeSchedule(schedule.id)}
                  />
                ))}
              </tbody>
            </table>
          )}

          {removed && (
            <p className="flex items-center gap-2 text-sm">
              {t("scheduler.removed")}
              <button type="button" className="btn btn-sm" onClick={undoRemove}>
                {t("scheduler.undo")}
              </button>
            </p>
          )}
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
              onChange={(event) => editConfig({ locale: event.target.value as Locale })}
            >
              {LOCALES.map((entry) => (
                <option key={entry.value} value={entry.value}>
                  {entry.label}
                </option>
              ))}
            </select>
          </label>
        </section>
      </main>
    </LocaleProvider>
  );
}
