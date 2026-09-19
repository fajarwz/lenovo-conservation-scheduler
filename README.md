# Lenovo Conservation Scheduler

<img src="assets/logo.png" width="128" alt="App icon: a half-charged battery with a clock badge on a red badge">

A small Windows tray utility that switches Lenovo battery **Conservation Mode** on a schedule.

The case it exists for: keep Conservation Mode on while the laptop is at home so the battery sits
around 75-80%, and have it switch off early enough to charge to 100% before leaving.

```
05:00  ->  Conservation Mode OFF   (Monday to Friday)
09:00  ->  Conservation Mode ON    (Monday to Friday)
```

> Not affiliated with, endorsed by, or sponsored by Lenovo. The name refers to the feature the app
> automates: it drives Lenovo's own software in place and bundles nothing from Lenovo.

## How it works

- **Lenovo control** uses Lenovo Vantage's own native `PowerBattery.dll`, loaded in place from
  `C:\ProgramData\Lenovo\Vantage\Addins\IdeaNotebookAddin\<version>\PowerBattery.dll`. Nothing from
  Vantage is bundled, copied or redistributed. If the DLL is missing - Vantage not installed, or a
  model without conservation-mode support - the app says so and keeps running with manual switching
  disabled instead of failing.
- **No polling.** A single background thread blocks on a waitable timer armed for the next
  occurrence, plus Windows events for "settings edited" and "resumed from sleep". While idle the
  process has one sleeping thread: no ticks, no intervals, no CPU.
- **Reconciliation.** Whenever it wakes - at startup, after a resume, after an edit - it asks "what
  does the schedule expect right now?" and writes only if the current mode differs. That single rule
  is what makes a laptop that slept through 05:00 correct itself on waking at 07:30.
- **Manual changes win until the next occurrence.** Flip the mode by hand from the window or the tray
  and the schedule leaves it alone until its next scheduled change.
- **Nothing runs until you need it.** At startup only the tray icon exists. The settings window -
  and with it the whole WebView2 process tree, roughly 340 MB - is created when you open it and
  destroyed when you close it, so while idle the app is one process of about 10-20 MB (2-4 MB
  private) whether or not settings have ever been opened. Closing the window does not quit: the app
  lives in the tray until you choose Exit.
- **One copy at a time.** Launching the executable again does not start a second copy: the instance
  already running shows its window and the new process exits. Two copies would mean two tray icons
  and two schedulers racing over the same setting.
- **Notifications carry the app's own name and icon.** Windows attributes a notification to an app
  identity, and an app without an installer has to register its own (two values under
  `HKCU\Software\Classes\AppUserModelId\com.fajarwz.lenovo-conservation-scheduler`). Without that the
  toast arrives nameless and with a generic icon - when it appears at all.
- **It tells you when it is invisible.** With settings already saved, launching it goes straight to
  the tray, so it sends a notification saying it is running in the background. A start from the
  Windows login entry stays quiet on purpose.

## Performance

Measured on the release build, on the laptop this was written for (Lenovo Yoga Slim 7 14IMH9,
Windows 11). The whole process tree is counted - the app plus every WebView2 process it starts - so
these are the numbers Task Manager shows for it.

| State | Processes | Working set | Private memory | CPU used while idle |
| --- | --- | --- | --- | --- |
| Running, settings never opened | 1 | ~16 MB | ~4 MB | 0.000 s over 62 s |
| Settings window open | 7 | ~350 MB | ~180 MB | the browser's, while you look at it |
| After closing the window | 1 | ~23 MB | ~5 MB | 0.000 s |

- **Battery.** Idle, the process is asleep on a waitable timer: no polling loop, no repeating
  interval, nothing to wake up for. It used 0.000 s of CPU across a full minute of idling, sampled
  62 seconds apart, so its cost is below the clock's own resolution. It wakes a handful of times a
  day - once per scheduled change, and once when you edit a setting.
- **Memory.** About 4 MB private while idle: that is the entire cost of having the tray icon there.
  The ~350 MB belongs to WebView2, Windows' own Chromium engine, and only while the settings window
  is open. No app can shrink Chromium; it can only avoid starting it, which is why the window is
  created on demand and destroyed on close. Close it and all six browser processes go away, leaving
  the one process at about 5 MB.
- **Disk and network.** While idle the app owns no sockets (measured: 0) and writes nothing. The
  only writes in normal use are the log line for a scheduled change and the config file when you
  edit a setting.
- **One copy.** Launching it again does not add a second icon or a second scheduler: the running
  instance shows its window and the new process exits.

If you would rather check than take my word for it:

```powershell
$before = (Get-Process lenovo-conservation-scheduler).CPU
Start-Sleep 60
$after = (Get-Process lenovo-conservation-scheduler).CPU
"CPU used in 60 s: {0:N3} s" -f ($after - $before)
```

## Requirements

- Windows 10/11 on a Lenovo laptop that supports Conservation Mode.
- Lenovo Vantage installed (the app reads `PowerBattery.dll` from it) and the
  `Lenovo Notebook ITS Service` running.
- No administrator rights: it runs as a normal user process.
- No network access, no telemetry, no database, no backend, no account.

## Settings

Configuration is one JSON file at
`%APPDATA%\com.fajarwz.lenovo-conservation-scheduler\config.json`, which can be edited by hand:

```json
{
  "scheduleEnabled": true,
  "startWithWindows": false,
  "notifyOnChange": true,
  "locale": "en-US",
  "timeFormat": "24h",
  "schedules": [
    {
      "id": "weekday-morning",
      "enabled": true,
      "time": "05:00",
      "days": ["monday", "tuesday", "wednesday", "thursday", "friday"],
      "action": "conservation_off"
    }
  ]
}
```

`time` is 24-hour local `HH:MM`, `action` is `conservation_on` or `conservation_off`, and a schedule
with an empty `days` list never fires. A file that cannot be parsed is kept as `config.corrupt` and
defaults are used, so nothing is lost silently; missing or unknown fields fall back to defaults.

`locale` picks the language of the window, the tray menu and the notifications: `en-US` or `id`.
A fresh install uses whatever Windows is set to, and the value is read leniently - `en`, `en-GB`,
`id-ID` all work, and anything unrecognised falls back to the OS language rather than failing the
file.

`timeFormat` is how times are written in the window: `24h` for `17:00`, `12h` for `5:00 PM`. It is a
separate choice from the language on purpose - plenty of people read English and still write 17:30 -
and a fresh install again follows Windows. The file always stores 24-hour `HH:MM` in `time` whatever
this says: the setting changes what you read, never what is saved.

The time in a schedule row is picked from dropdowns rather than a browser time field, because
Chromium's own field takes its format from Windows and ignores the language attribute, so it cannot
follow this setting.

## Adding a language

1. `src-tauri/src/i18n.rs`: add the code to `Lang`, its `parse` arm, its `detect` arm if the OS can
   report it, and a `Strings` table (the compiler refuses a table with a missing field).
2. `src/i18n/<code>.ts`: copy `en-US.ts`, translate the values, and type it as `typeof enUS` so a
   missing or misspelled key fails the build.
3. `src/i18n/index.tsx`: add the code to `Locale`, `LOCALES` and `DICTIONARIES`.

The `id` pair is a worked example of all three steps.

## Logs

The app logs what it does and why, one line per event: the config it loaded, the moment it armed,
each wake-up ("scheduled moment reached" / "resumed from sleep" / "schedules changed"), every mode
it changed, and any failure. Nothing is logged while it waits.

In a development build that goes to the console; in a release build a windowed process has no
console, so it is also written to:

```
%LOCALAPPDATA%\com.fajarwz.lenovo-conservation-scheduler\logs\lenovo-conservation-scheduler.log
```

If a switch did not happen, that file says why.

## Development

```bash
npm install
npm run tauri dev      # run the app
npm run tauri build    # executable + installers
```

Tests, from `src-tauri`:

```bash
cargo test -- --test-threads=1
```

`--test-threads=1` matters: one test toggles the real charging mode on the laptop and restores it.
Other tests cover the scheduler logic (next event, weekday filtering, midnight crossing, duplicates,
disabled entries, startup and wake reconciliation) and the JSON persistence round trip.

Always build the executable through the Tauri CLI:

```bash
npm run tauri build -- --no-bundle   # executable only; drop the flag to also make installers
```

Two traps, and both of them look like the app is broken rather than the build:

- **Never use a bare `cargo build --release`.** Without the `custom-protocol` feature the `tauri`
  crate compiles as *dev* (`tauri/build.rs`: `let dev = !custom_protocol`), so the window loads
  `devUrl` (`http://localhost:1420`) instead of the embedded frontend - and with no Vite server
  running you get Chromium's "can't reach this page". The CLI enables that feature; cargo on its
  own does not. Diagnose by listening on port 1420 while the app starts: a production build makes
  zero connections to it. The production binary is also noticeably larger, because it now carries
  the compressed frontend.
- **A rebuilt `dist/` does not relink the binary** (cargo does not treat it as a change), so a
  frontend-only edit needs the crate touched before the build:

```bash
npm run build && touch src-tauri/src/lib.rs src-tauri/src/main.rs && npm run tauri build -- --no-bundle
```

With `npm run tauri dev` the dev URL is the *correct* target, because Vite is running.

## Layout

| Path | Purpose |
| --- | --- |
| `src-tauri/src/config.rs` | schedules and options, JSON persistence, validation |
| `src-tauri/src/scheduler.rs` | pure decisions: what is expected now, when the next event is |
| `src-tauri/src/timer.rs` | waitable timer, settings event, resume notification |
| `src-tauri/src/lenovo.rs` | the only module with Lenovo FFI (`PowerBattery.dll`) |
| `src-tauri/src/power.rs` | battery percentage, AC state, charging flag |
| `src-tauri/src/i18n.rs` | the strings Rust needs itself: tray menu, notifications, errors |
| `src-tauri/src/lib.rs` | state, tray menu, commands, scheduler thread |
| `src/App.tsx` | settings window: state, save/add/delete, the three sections |
| `src/components/*.tsx` | `Fact`, `Toggle`, `Banner`, `DayPicker`, `ScheduleRow` |
| `src/api.ts` | typed wrappers for the three commands and the state event |
| `src/i18n/*.ts(x)` | the window's dictionaries, `t()`, and the locale provider |
| `src/index.css` | Tailwind entry: base styles plus the shared `@layer components` recipes |

## Contributing

Issues and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers what to put in a bug
report, how to build and test, and what a change has to keep. The short version: keep it small, keep
it Lenovo-only, and add a test when the logic allows one. Reports from Lenovo models other than the
one this was built on are especially useful.

[AGENTS.md](AGENTS.md) documents the architecture, the conventions and the build traps that have
already cost time - read it first if you are pointing a coding agent at this repository.

## Security

Report vulnerabilities privately, either through GitHub's
[private vulnerability reporting](https://github.com/fajarwz/lenovo-conservation-scheduler/security/advisories/new)
or by email to `hi@fajarwz.com`. [SECURITY.md](SECURITY.md) lists what is in scope - and what the app
touches, so you can judge a report yourself: no network, no admin rights, two files, one registry key.

## License

MIT - see [LICENSE](LICENSE). Copyright (c) 2026 Fajar Windhu Zulfikar.
