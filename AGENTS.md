# AGENTS.md

Guidance for coding agents (and humans) working in this repository.

## What this is

A Windows tray app that switches Lenovo **Conservation Mode** on and off on a weekly schedule.
Tauri 2 (Rust) + React/TypeScript/Vite, one JSON config file, no backend, no database, no network,
no admin rights, no service.

## Commands

| Task | Command |
| --- | --- |
| Run the app in development (Vite + the window) | `npm run tauri dev` |
| Type check + frontend build | `npm run build` |
| Rust tests (serialized: one test touches the real battery) | `cd src-tauri && cargo test --lib -- --test-threads=1` |
| Format + lint | `cd src-tauri && cargo fmt && cargo clippy --all-targets` |
| Build the executable | `npm run tauri build -- --no-bundle` |
| Build installers too | `npm run tauri build` |

`npm run dev` on its own serves the frontend to a plain browser, where every Tauri command fails with
`Cannot read properties of undefined (reading 'invoke')`. It is not a way to preview the app.

## Traps that have already cost time

1. **Never build with a bare `cargo build --release`.** Without the `custom-protocol` feature the
   `tauri` crate compiles as *dev* (`tauri/build.rs`: `let dev = !custom_protocol`), so
   `WebviewUrl::App` resolves to `devUrl` (`http://localhost:1420`) instead of the embedded frontend -
   and with no Vite running the window shows Chromium's "can't reach this page". Only
   `npm run tauri build` sets it. A production binary is also noticeably larger, because it carries
   the compressed frontend. To check a build: bind a listener to port 1420 and launch the binary; a
   production build makes **zero** connections to it.
2. **A rebuilt `dist/` does not relink the binary.** Cargo does not treat it as a change, so after a
   frontend-only edit: `npm run build && touch src-tauri/src/lib.rs src-tauri/src/main.rs` before the
   production build, then confirm the executable's timestamp actually moved.
3. **A running instance locks the executable** and the build fails with
   `failed to remove file ... Access is denied`. Kill it first
   (`taskkill /F /IM lenovo-conservation-scheduler.exe`), and read the app's log before doing so.
4. **`lenovo::tests::conservation_mode_round_trip` writes the real charging mode** and restores it.
   That is why the suite runs with `--test-threads=1`; do not parallelize it.
5. **A failed frontend build leaves the previous `dist/` in place** and the production build embeds
   it happily. `tsc` stops before Vite runs, and piping through `tail` hides the exit status.
6. **Replacing the app icon does not re-embed the Windows icon resource.** The exe icon is written by
   the *build script*, so touching `lib.rs`/`main.rs` rebuilds the crate but reuses the cached
   resource: the exe keeps the old icon while the window and tray already show the new one. Regenerate
   the icons with `npm run tauri icon assets/logo.svg` (the source of truth is that SVG), then
   `touch src-tauri/build.rs` before the production build. To verify, extract the icon from a **copy**
   of the exe at a fresh path - the shell icon cache otherwise returns the previous build's icon - or
   check that the frames of `src-tauri/icons/icon.ico` appear in the binary.

## Architecture

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/config.rs` | schedules + options, JSON persistence, validation. No Tauri import |
| `src-tauri/src/scheduler.rs` | pure decisions: what is expected now, when the next change is |
| `src-tauri/src/timer.rs` | waitable timer, settings-changed event, resume notification (`unsafe`) |
| `src-tauri/src/lenovo.rs` | the only Lenovo FFI: Vantage's `PowerBattery.dll` (`unsafe`) |
| `src-tauri/src/power.rs` | battery percentage, AC state, charging flag (kernel32) |
| `src-tauri/src/i18n.rs` | the strings Rust needs itself: tray menu, notifications, errors |
| `src-tauri/src/identity.rs` | registers the app's toast identity so Windows shows its name and icon |
| `src-tauri/src/lib.rs` | state, commands, tray menu, the single scheduler thread |
| `src/App.tsx` | the window: state, auto-save, the three cards |
| `src/components/*` | `Fact`, `Toggle`, `TimeField`, `Banner`, `DayPicker`, `ScheduleRow` |
| `src/api.ts` | typed wrappers for the three commands and the state event |
| `src/i18n/*` | the window's dictionaries, `t()`, and `time.ts` for formatting times |

Rules worth keeping:

- **Logic modules stay free of `tauri::`.** Pass paths and handles in as parameters and resolve app
  directories in `lib.rs`. That is what keeps the suite runnable without an `AppHandle`.
- **`unsafe` lives in the modules that call Windows**, one block per native call, each with a
  SAFETY comment: `lenovo.rs` (the vendor DLL), `timer.rs` (the waitable timer and resume
  notifications), `power.rs` (`GetSystemPowerStatus`) and `i18n.rs` (`GetUserDefaultUILanguage`,
  `GetLocaleInfoW`). Read the vendor header before changing a signature: a wrong parameter count
  corrupts memory instead of failing to compile.
- **One reconcile path.** The scheduler never applies "this event's action"; it asks what the schedule
  expects *now* and writes only if the current mode differs. Startup, resume, a slept-through event,
  duplicates and edits are all the same code.
- **No polling.** One waitable timer plus Windows events. While idle the process is asleep.
- **One instance, one tray icon.** `tauri-plugin-single-instance` is registered before every other
  plugin, so a second launch hands over to `open_settings` in the running instance and exits instead
  of adding a tray icon and a second scheduler. The autostart entry passes `--autostart`, which is
  how a login start is told apart from a double-click.
- **The window costs nothing until it is opened.** It is not declared in `tauri.conf.json` and is
  created on demand; closing it destroys the WebView2 tree. Keep it that way.
- **The frontend never polls either.** It loads once on mount and then re-renders from the snapshot
  the backend pushes on `state-changed`.
- **Settings save themselves.** Edits go through `edit()` in `App.tsx`, which debounces a write and
  shows `Saving… / Saved` in the header. There is deliberately no Save button; don't add one back
  without a reason.
- **The theme follows Windows** via `prefers-color-scheme`, and `html` declares `color-scheme: light
  dark` so *native* controls (time picker, selects, checkboxes, scrollbars) flip with it. Recipes
  declare their own colours rather than leaning on user-agent control colours.
- **Config back-compat.** Every field has a default, unknown fields are ignored, and a damaged file is
  kept aside rather than reset silently.
- **User-facing text goes through i18n on both sides.** Rust needs its own table because the tray
  menu and notifications never pass through the webview. Logs stay English.

## Conventions

- **Conventional commits** (`feat:`, `fix:`, `docs:`, `chore:`) with a short body saying why, not a
  replay of the diff.
- **One component per file** under `src/components/`. Tailwind utilities are written inline; a recipe
  shared by several components belongs in `src/index.css` under `@layer components`. Never a module
  that exists only to hold class strings.
- **Don't add**: a database, a service, an event bus, a state-management library, polling, or a
  dependency to save a few lines. Preference order: official Tauri plugin, then a mature crate, then
  Win32 FFI, then a small custom implementation.
- **Don't bundle or redistribute anything from Lenovo Vantage.** `PowerBattery.dll` is loaded in
  place from the user's own installation, and the app degrades gracefully when it is missing.
- **Every fix gets a test** where the logic allows it (`scheduler.rs` especially), and the suite must
  pass before pushing.

## Verifying a change

Build, run the tests, build the production binary, launch it, and read the log:

```bash
npm run build
cd src-tauri && cargo test --lib -- --test-threads=1
cd .. && touch src-tauri/src/lib.rs src-tauri/src/main.rs && npm run tauri build -- --no-bundle
```

There is **no visual test**: nothing here can tell you whether the window *looks* right. Say what was
built and verified, and leave the appearance to the user rather than claiming it renders correctly.
