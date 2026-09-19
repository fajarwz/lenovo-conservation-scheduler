# Contributing

Thanks for wanting to help. This is a small utility, so the most valuable contributions are bug
reports with detail, translations, and reports from Lenovo models other than the one it was built on.

## Reporting a bug

Please include:

- Windows version, and the Lenovo model (e.g. `Yoga Slim 7 14IMH9`).
- Whether `Lenovo Vantage` is installed and which version, and whether **Conservation Mode** works
  from Vantage itself. The app drives Vantage's own DLL, so if Vantage cannot do it, neither can we.
- The log file: `%LOCALAPPDATA%\com.fajarwz.lenovo-conservation-scheduler\logs\lenovo-conservation-scheduler.log`
  It says what the app loaded, when it armed, why it woke, and what it changed.
- What you expected and what happened - for example "conservation stayed on after 05:00".

Screenshots of the window help for anything visual.

## Reporting a Lenovo model

Useful to know and cheap to give: does the app say *"Lenovo battery control is unavailable on this
device"* on your machine? The app checks for `PowerBattery.dll` from Vantage and for conservation
support in the firmware. Models that report support but behave differently are worth an issue.

## Development

Prerequisites: Windows 10/11, [Rust](https://rustup.rs), Node 20+, and the WebView2 runtime (already
present on Windows 11). Then:

```bash
npm install
npm run tauri dev      # Vite + the real app window
```

Before opening a pull request:

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets && cargo test --lib -- --test-threads=1
cd .. && npm run build
npm run tauri build -- --no-bundle
```

`--test-threads=1` is not optional: one test writes the real charging mode and restores it.

Note that `npm run dev` alone only serves the frontend to a browser, where every command fails - use
`npm run tauri dev`.

## Pull requests

- Keep it small and focused. One fix or one feature per PR.
- Commits follow conventional commits (`fix:`, `feat:`, `docs:`, `chore:`) with a short body saying
  why, not a replay of the diff.
- Add a test when the logic allows one. `src-tauri/src/scheduler.rs` is pure decision-making and the
  best place for them.
- For UI changes, a screenshot before/after is worth more than a description.
- Read `AGENTS.md` first: it lists the architecture rules, the conventions, and the build traps that
  have already cost time (a bare `cargo build --release` ships a broken app).

## Translating

The app ships English and Indonesian. Adding a language is three steps, described in the
[README](README.md#adding-a-language): a `Lang` variant plus a `Strings` table in
`src-tauri/src/i18n.rs`, a dictionary in `src/i18n/<code>.ts` typed as `typeof enUS`, and an entry in
`LOCALES`. Both tables are compiler-checked, so a missing key fails the build rather than showing an
empty label.

Corrections to the existing Indonesian text are welcome too - it was not written by a native speaker.

## What is out of scope

- Other vendors' battery controls. Windows has no vendor-neutral API for charge limits; each vendor
  needs its own reverse-engineered interface, so this app stays Lenovo-only on purpose.
- Requesting administrator rights, installing a kernel driver, or running as a service.
- A database, a cloud sync, accounts, telemetry, or any network access.

## Questions

`hi@fajarwz.com`.
