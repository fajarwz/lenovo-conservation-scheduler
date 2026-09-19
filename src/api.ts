import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Locale } from "./i18n";

export type Day =
  | "monday"
  | "tuesday"
  | "wednesday"
  | "thursday"
  | "friday"
  | "saturday"
  | "sunday";

export type Action = "conservation_on" | "conservation_off";

/** How times are written in the window. Stored as "24h" or "12h". */
export type TimeFormat = "24h" | "12h";

export interface Schedule {
  id: string;
  enabled: boolean;
  /** Local time, 24-hour "HH:MM". */
  time: string;
  days: Day[];
  action: Action;
}

export interface Config {
  /** Master switch for automatic switching; manual control always works. */
  scheduleEnabled: boolean;
  startWithWindows: boolean;
  notifyOnChange: boolean;
  /** Language of the window, the tray and notifications. */
  locale: Locale;
  /** How times are written in the window. */
  timeFormat: TimeFormat;
  schedules: Schedule[];
}

/** Everything the window displays about the machine right now. */
export interface Status {
  batteryPercent: number | null;
  acOnline: boolean | null;
  charging: boolean | null;
  conservationAvailable: boolean;
  conservationOn: boolean | null;
  conservationMessage: string | null;
  /** Local "YYYY-MM-DDTHH:MM" of the next scheduled switch. */
  nextEventAt: string | null;
  nextEventAction: Action | null;
  warning: string | null;
}

export interface Snapshot {
  config: Config;
  status: Status;
}

export const DAYS: Day[] = [
  "monday",
  "tuesday",
  "wednesday",
  "thursday",
  "friday",
  "saturday",
  "sunday",
];

/** The common case, offered as a one-click preset next to the day buttons. */
export const WEEKDAYS: Day[] = DAYS.slice(0, 5);

export function newScheduleId(): string {
  return `s${Date.now().toString(16)}${Math.floor(Math.random() * 0x10000).toString(16)}`;
}

export function getState(): Promise<Snapshot> {
  return invoke<Snapshot>("get_state");
}

export function setConservation(on: boolean): Promise<Snapshot> {
  return invoke<Snapshot>("set_conservation", { on });
}

export function saveConfig(config: Config): Promise<Snapshot> {
  return invoke<Snapshot>("save_config", { config });
}

export function onStateChanged(
  handler: (snapshot: Snapshot) => void,
): Promise<UnlistenFn> {
  return listen<Snapshot>("state-changed", (event) => handler(event.payload));
}
