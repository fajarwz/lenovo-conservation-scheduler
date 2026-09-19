# Security Policy

## Reporting a vulnerability

Please report privately, not in a public issue:

- Preferred: GitHub's [private vulnerability reporting](https://github.com/fajarwz/lenovo-conservation-scheduler/security/advisories/new)
  (Security tab -> Report a vulnerability). It keeps the report private and attached to the code.
- Or email `hi@fajarwz.com`.

Include what the issue is, how to reproduce it, and the version you tested. I aim to reply within a
few days.

## What the app does, security-wise

Useful context for judging a report:

- Runs as a **normal user process**. No administrator rights, no service, no kernel driver, no
  scheduled task.
- **No network access at all** - no telemetry, no update check, no accounts. The only outside
  contact is reading a local DLL.
- Writes three files: the config at
  `%APPDATA%\com.fajarwz.lenovo-conservation-scheduler\config.json`, a log under
  `%LOCALAPPDATA%\com.fajarwz.lenovo-conservation-scheduler\logs\`, and that directory's `icon.png` -
  the app's own icon, written when it is missing so Windows has an image to show beside the app's
  notifications.
- Touches two registry keys, both under `HKCU` and both as a normal user:
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, and only for the "Start with Windows"
  setting; and `HKCU\Software\Classes\AppUserModelId\com.fajarwz.lenovo-conservation-scheduler`, which
  is the name and icon Windows shows beside the app's notifications.
- Loads `PowerBattery.dll` **in place** from the machine's own Lenovo Vantage installation
  (`C:\ProgramData\Lenovo\Vantage\Addins\IdeaNotebookAddin\<version>\`). Nothing from Lenovo is
  bundled, copied or redistributed. The app does not verify that DLL's authenticity, so a tampered
  Vantage installation is outside the threat model.
- `unsafe` Rust is confined to the modules that call Windows: `src-tauri/src/lenovo.rs` (that DLL),
  `src-tauri/src/timer.rs` (kernel32 timers and power notifications), `src-tauri/src/power.rs`
  (`GetSystemPowerStatus`) and `src-tauri/src/i18n.rs` (the user's UI language and time format).

## In scope

- Anything that lets the app be abused to run code, escalate privileges, or write outside the paths
  above.
- A crafted `config.json` - for example a `schedule` value - that makes the app panic, corrupt the
  file, or crash on startup. It is designed to fall back to defaults instead.
- The frontend: the settings window renders only local data, but report anything that looks like
  injection through the config, a schedule, or the interface language strings.

## Out of scope

- The absence of code signing on development builds.
- Vulnerabilities in Lenovo Vantage, `PowerBattery.dll`, WebView2, or Windows itself - report those
  to the respective vendor.
- The proprietary `PowerBattery.dll` interface being undocumented; that is a known constraint
  described in the README.
