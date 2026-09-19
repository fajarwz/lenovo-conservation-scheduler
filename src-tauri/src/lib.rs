//! Lenovo Conservation Scheduler - a small tray utility that switches Lenovo Conservation Mode on a
//! user-defined schedule.
//!
//! Modules: `config` persists schedules, `scheduler` decides what should be true and when,
//! `timer` provides the event-driven wait, `lenovo` talks to Lenovo Vantage's DLL, `power` reads
//! the battery, `i18n` holds the text the user sees. This file is the wiring: app state, the tray,
//! the commands the window calls, and the scheduler thread.

pub mod config;
pub mod i18n;
pub mod lenovo;
pub mod power;
pub mod scheduler;
pub mod timer;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{Local, NaiveDateTime};
use serde::Serialize;
use tauri::menu::{CheckMenuItem, MenuBuilder, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartExt};
use tauri_plugin_notification::NotificationExt;

use config::{Action, Config};
use i18n::{Lang, APP_NAME};
use lenovo::ChargingMode;
use timer::{Waiter, WakeReason};

/// Emitted whenever anything the window displays has changed.
const STATE_CHANGED: &str = "state-changed";

/// Label of the settings window, created on demand and destroyed when closed.
const WINDOW: &str = "main";

const MENU_OPEN: &str = "open-settings";
const MENU_CONSERVATION_ON: &str = "conservation-on";
const MENU_CONSERVATION_OFF: &str = "conservation-off";
const MENU_SCHEDULE: &str = "schedule-enabled";
const MENU_QUIT: &str = "quit";

pub struct AppState {
    inner: Mutex<Inner>,
    config_path: PathBuf,
    /// `None` if Windows refused the timer handles: manual switching still works, the schedule
    /// does not. Kept as an option so an impossible-but-real failure cannot abort startup.
    waiter: Option<Arc<Waiter>>,
    /// Tray icon and the menu entries whose text or tick marks are kept up to date.
    tray: Mutex<Option<Tray>>,
}

struct Inner {
    config: Config,
    /// Anything from startup worth telling the user, for example a damaged config file.
    warning: Option<String>,
    /// When conservation was last switched by hand. The schedule keeps out of the way until an
    /// occurrence later than this.
    manual_at: Option<NaiveDateTime>,
}

/// The tray icon plus the entries that change as the state changes (tick marks) or when the
/// language does (text).
struct Tray {
    icon: TrayIcon<Wry>,
    status: MenuItem<Wry>,
    open: MenuItem<Wry>,
    conservation_on: CheckMenuItem<Wry>,
    conservation_off: CheckMenuItem<Wry>,
    schedule_enabled: CheckMenuItem<Wry>,
    quit: MenuItem<Wry>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the window renders: the saved settings plus the current machine state.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    config: Config,
    status: Status,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    battery_percent: Option<u8>,
    ac_online: Option<bool>,
    charging: Option<bool>,
    /// False when Lenovo battery control is missing on this machine.
    conservation_available: bool,
    conservation_on: Option<bool>,
    /// User-facing explanation when conservation mode cannot be controlled.
    conservation_message: Option<String>,
    next_event_at: Option<String>,
    next_event_action: Option<Action>,
    warning: Option<String>,
}

fn snapshot(state: &AppState) -> Snapshot {
    let (config, warning) = {
        let inner = lock(&state.inner);
        (inner.config.clone(), inner.warning.clone())
    };
    Snapshot {
        status: read_status(&config, warning),
        config,
    }
}

fn read_status(config: &Config, warning: Option<String>) -> Status {
    let strings = config.locale.strings();
    let power = power::status();
    // Two ways Lenovo control can be missing: no PowerBattery.dll on this machine, or a model
    // whose firmware reports no conservation-mode support. The UI must offer neither as working.
    let available = lenovo::availability();
    let mode = lenovo::get_mode();
    let next = scheduler::next_occurrence(config, Local::now().naive_local());

    Status {
        battery_percent: power.and_then(|power| power.percent),
        ac_online: power.and_then(|power| power.ac_online),
        charging: power.and_then(|power| power.charging),
        conservation_available: available.is_ok(),
        conservation_on: mode.as_ref().ok().map(|mode| mode.is_conservation()),
        conservation_message: available
            .err()
            .or_else(|| mode.err())
            .map(|error| error.user_message(strings).to_string()),
        next_event_at: next.map(|(at, _)| at.format("%Y-%m-%dT%H:%M").to_string()),
        next_event_action: next.map(|(_, action)| action),
        warning,
    }
}

// ------------------------------------------------------------------ commands

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> Snapshot {
    snapshot(&state)
}

/// Manual switch from the window. Takes effect at once; the schedule then leaves it alone until
/// its next occurrence, so a mode the user just chose is never argued with.
#[tauri::command]
fn set_conservation(
    on: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    apply_conservation(&app, on)?;
    Ok(snapshot(&state))
}

#[tauri::command]
fn save_config(
    config: Config,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    config.validate()?;
    config::save(&state.config_path, &config).map_err(|error| {
        log::error!("saving {} failed: {error}", state.config_path.display());
        config.locale.strings().config_save_failed.to_string()
    })?;

    {
        let mut inner = lock(&state.inner);
        inner.config = config.clone();
        // A saved config is a valid one, so any startup complaint is now history.
        inner.warning = None;
    }
    apply_autostart(&app, config.start_with_windows);
    // The scheduler wakes immediately, reconciles against the new schedules and re-arms.
    if let Some(waiter) = &state.waiter {
        waiter.notify_settings_changed();
    }

    let snapshot = snapshot(&state);
    publish(&app, &snapshot);
    Ok(snapshot)
}

/// Manual switching, shared by the window and the tray.
fn apply_conservation(app: &AppHandle, on: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let strings = lock(&state.inner).config.locale.strings();

    if let Err(error) = lenovo::set_mode(ChargingMode::for_conservation(on)) {
        log::error!("set_conservation({on}) failed: {}", error.detail());
        return Err(error.user_message(strings).to_string());
    }

    lock(&state.inner).manual_at = Some(Local::now().naive_local());
    let snapshot = snapshot(&state);
    publish(app, &snapshot);
    Ok(())
}

/// Writes the config and wakes the scheduler; used by the tray's schedule entry.
fn apply_schedule_switch(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut config = lock(&state.inner).config.clone();
    config.schedule_enabled = enabled;
    config::save(&state.config_path, &config).map_err(|error| {
        log::error!("saving {} failed: {error}", state.config_path.display());
        i18n::fill(
            config.locale.strings().config_save_failed_detail,
            &[&error.to_string()],
        )
    })?;
    lock(&state.inner).config = config;
    if let Some(waiter) = &state.waiter {
        waiter.notify_settings_changed();
    }
    Ok(())
}

// ------------------------------------------------------ publishing to the UI

/// Sends a new snapshot to the window and refreshes the tray in the same breath, so the two can
/// never disagree.
fn publish(app: &AppHandle, snapshot: &Snapshot) {
    refresh_tray(app, snapshot);
    if let Err(error) = app.emit(STATE_CHANGED, snapshot) {
        log::error!("emitting {STATE_CHANGED} failed: {error}");
    }
}

fn refresh_tray(app: &AppHandle, snapshot: &Snapshot) {
    let state = app.state::<AppState>();
    let guard = lock(&state.tray);
    let Some(tray) = guard.as_ref() else {
        return;
    };

    let strings = snapshot.config.locale.strings();
    let status = &snapshot.status;
    let text = match (status.battery_percent, status.conservation_on) {
        (Some(percent), Some(on)) => i18n::fill(
            strings.tray_status,
            &[
                &percent.to_string(),
                if on {
                    strings.word_on
                } else {
                    strings.word_off
                },
            ],
        ),
        (Some(percent), None) => {
            i18n::fill(strings.tray_status_unavailable, &[&percent.to_string()])
        }
        (None, _) => APP_NAME.to_string(),
    };

    // The text is refreshed as well as the tick marks: the language can change while running.
    let _ = tray.status.set_text(&text);
    let _ = tray.icon.set_tooltip(Some(text.as_str()));
    let _ = tray.open.set_text(strings.tray_open_settings);
    let _ = tray.conservation_on.set_text(strings.tray_conservation_on);
    let _ = tray
        .conservation_off
        .set_text(strings.tray_conservation_off);
    let _ = tray
        .schedule_enabled
        .set_text(strings.tray_schedule_enabled);
    let _ = tray.quit.set_text(strings.tray_exit);
    let _ = tray
        .conservation_on
        .set_checked(status.conservation_on == Some(true));
    let _ = tray
        .conservation_off
        .set_checked(status.conservation_on == Some(false));
    let _ = tray
        .schedule_enabled
        .set_checked(snapshot.config.schedule_enabled);
    // Do not offer manual switching on a machine that cannot do it.
    let _ = tray
        .conservation_on
        .set_enabled(status.conservation_available);
    let _ = tray
        .conservation_off
        .set_enabled(status.conservation_available);
}

// ------------------------------------------------------------------ the tray

fn build_tray(app: &AppHandle, lang: Lang) -> tauri::Result<Tray> {
    let strings = lang.strings();
    let status = MenuItem::with_id(app, "status", strings.tray_starting, false, None::<&str>)?;
    let open = MenuItem::with_id(
        app,
        MENU_OPEN,
        strings.tray_open_settings,
        true,
        None::<&str>,
    )?;
    let conservation_on = CheckMenuItem::with_id(
        app,
        MENU_CONSERVATION_ON,
        strings.tray_conservation_on,
        true,
        false,
        None::<&str>,
    )?;
    let conservation_off = CheckMenuItem::with_id(
        app,
        MENU_CONSERVATION_OFF,
        strings.tray_conservation_off,
        true,
        false,
        None::<&str>,
    )?;
    let schedule_enabled = CheckMenuItem::with_id(
        app,
        MENU_SCHEDULE,
        strings.tray_schedule_enabled,
        true,
        false,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, MENU_QUIT, strings.tray_exit, true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&status)
        .separator()
        .item(&open)
        .separator()
        .item(&conservation_on)
        .item(&conservation_off)
        .separator()
        .item(&schedule_enabled)
        .separator()
        .item(&quit)
        .build()?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip(APP_NAME)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_tray_event);

    // The window icon doubles as the tray icon; without one the tray still works.
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    let tray = builder.build(app)?;

    Ok(Tray {
        icon: tray,
        status,
        open,
        conservation_on,
        conservation_off,
        schedule_enabled,
        quit,
    })
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref().to_string();
    match id.as_str() {
        MENU_OPEN => open_settings(app),
        MENU_CONSERVATION_ON | MENU_CONSERVATION_OFF => {
            let on = id == MENU_CONSERVATION_ON;
            if let Err(message) = apply_conservation(app, on) {
                log::error!("tray: {message}");
            }
        }
        MENU_SCHEDULE => {
            let state = app.state::<AppState>();
            let enabled = !lock(&state.inner).config.schedule_enabled;
            if let Err(message) = apply_schedule_switch(app, enabled) {
                log::error!("tray: {message}");
            } else {
                let snapshot = snapshot(&state);
                publish(app, &snapshot);
            }
        }
        MENU_QUIT => app.exit(0),
        other => log::error!("tray: unhandled menu entry {other}"),
    }
}

fn on_tray_event(tray: &TrayIcon<Wry>, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        open_settings(tray.app_handle());
    }
}

/// Opens the settings window: created on first use, focused if it is already open.
///
/// Nothing is created at startup on purpose. A hidden window still spins up the whole WebView2
/// process tree (six processes, roughly 330 MB working set), which would defeat the point of a
/// tray utility that spends its life idle. The window is destroyed when closed, for the same
/// reason: the app costs ~28 MB whether or not the user has ever opened it.
fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::default())
        .title(APP_NAME)
        .inner_size(780.0, 640.0)
        .min_inner_size(560.0, 420.0)
        .center()
        .build();

    match built {
        Ok(_) => log::info!("settings window opened"),
        Err(error) => log::error!("could not open the settings window: {error}"),
    }
}

// --------------------------------------------------------------- integrations

fn notify(app: &AppHandle, action: Action) {
    let state = app.state::<AppState>();
    let (wanted, lang) = {
        let inner = lock(&state.inner);
        (inner.config.notify_on_change, inner.config.locale)
    };
    if !wanted {
        return;
    }

    let strings = lang.strings();
    let body = match action {
        Action::ConservationOn => strings.notification_on,
        Action::ConservationOff => strings.notification_off,
    };
    log::info!("notified: {body}");

    if let Err(error) = app
        .notification()
        .builder()
        .title(APP_NAME)
        .body(body)
        .show()
    {
        log::error!("notification failed: {error}");
    }
}

/// Keeps the "start with Windows" entry in step with the setting.
///
/// The entry is written afresh whenever the setting is on, not only when it is missing: its command
/// carries the `--autostart` marker, an entry left by an older version has none, and the manager
/// reports only whether the entry exists - never what it says. Rewriting is therefore the only way
/// to repair one. Disabling still touches the registry only when there is something to remove.
fn apply_autostart(app: &AppHandle, enabled: bool) {
    let manager = app.autolaunch();
    let current = match manager.is_enabled() {
        Ok(current) => current,
        Err(error) => {
            log::error!("autostart state could not be read: {error}");
            false
        }
    };

    if !enabled && !current {
        return;
    }

    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    match result {
        Ok(()) => log::info!(
            "autostart: {}",
            if enabled { "enabled" } else { "disabled" }
        ),
        Err(error) => log::error!("autostart could not be changed: {error}"),
    }
}

// --------------------------------------------------------- scheduler thread

fn scheduler_loop(app: AppHandle) {
    if app.state::<AppState>().waiter.is_none() {
        return;
    }

    loop {
        reconcile(&app);
        arm(&app);

        let reason = {
            let state = app.state::<AppState>();
            match &state.waiter {
                Some(waiter) => waiter.wait(),
                None => return,
            }
        };

        match reason {
            Ok(reason) => log_wake(reason),
            Err(error) => {
                // Never spin on a broken wait: back off, then try again.
                log::error!("waiting failed: {error}; retrying in 60s");
                std::thread::sleep(Duration::from_secs(60));
            }
        }
    }
}

/// Applies the state the schedule expects right now, if it differs from the current one. The
/// same code covers first start, a wake-up, a resume, and an edited schedule.
fn reconcile(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (config, manual_at) = {
        let inner = lock(&state.inner);
        (inner.config.clone(), inner.manual_at)
    };

    let Some((occurrence_at, action)) =
        scheduler::expected_occurrence(&config, Local::now().naive_local())
    else {
        return;
    };
    if scheduler::manual_override_stands(occurrence_at, manual_at) {
        return;
    }

    // No Lenovo control: nothing to apply and nothing to retry.
    let Ok(current) = lenovo::get_mode() else {
        return;
    };
    if !scheduler::requires_change(Some(action), current) {
        return;
    }

    match lenovo::set_mode(ChargingMode::for_conservation(action.conservation())) {
        Ok(mode) => {
            log::info!(
                "scheduler: {} (occurrence {}) -> {}",
                action.label(),
                occurrence_at,
                mode.label()
            );
            notify(app, action);
            let snapshot = snapshot(&state);
            publish(app, &snapshot);
        }
        Err(error) => log::error!("scheduler: {} failed: {}", action.label(), error.detail()),
    }
}

/// Points the timer at the next occurrence. With nothing scheduled the timer is cancelled, so the
/// thread waits only for edited settings or a resume.
fn arm(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(waiter) = &state.waiter else {
        return;
    };

    let config = lock(&state.inner).config.clone();
    let Some((at, action)) = scheduler::next_occurrence(&config, Local::now().naive_local()) else {
        if let Err(error) = waiter.cancel() {
            log::error!("cancelling the timer failed: {error}");
        }
        return;
    };

    match scheduler::instant(at) {
        Some(instant) => match waiter.arm(instant) {
            Ok(()) => log::info!("scheduler: next is {} at {at}", action.label()),
            Err(error) => log::error!("arming the timer for {at} failed: {error}"),
        },
        None => log::error!("could not resolve {at} to a local time"),
    }
}

fn log_wake(reason: WakeReason) {
    match reason {
        WakeReason::TimeElapsed => log::info!("scheduler: scheduled moment reached"),
        WakeReason::SettingsChanged => log::info!("scheduler: schedules changed"),
        WakeReason::Resumed => log::info!("scheduler: resumed from sleep"),
    }
}

// ------------------------------------------------------------------ startup

/// Logs to the console during development and to a file in the app's log directory in a release
/// build, where a windowed process has no console to write to.
///
/// `clear_targets` first: the plugin's defaults already include a console target, and `target`
/// appends, which would print every line twice.
fn log_plugin() -> tauri::plugin::TauriPlugin<Wry> {
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .level(log::LevelFilter::Info)
        .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
        .target(tauri_plugin_log::Target::new(
            tauri_plugin_log::TargetKind::Stdout,
        ))
        .target(tauri_plugin_log::Target::new(
            tauri_plugin_log::TargetKind::LogDir {
                file_name: Some("lenovo-conservation-scheduler".to_string()),
            },
        ))
        .build()
}

/// The argument the autostart entry passes, so a login start can be told from a double-click.
const AUTOSTART_FLAG: &str = "--autostart";

/// True when Windows started us from the login entry rather than the user opening the executable.
fn started_by_autostart() -> bool {
    std::env::args().any(|argument| argument == AUTOSTART_FLAG)
}

/// Says the app is running without a window. Deliberately not the mode-change notification, so it
/// does not consult `notify_on_change`: someone who double-clicked the executable gets an answer
/// either way.
fn notify_background(app: &AppHandle) {
    let state = app.state::<AppState>();
    let lang = lock(&state.inner).config.locale;
    let body = lang.strings().notification_background;
    log::info!("told the user it is running in the tray: {body}");

    if let Err(error) = app
        .notification()
        .builder()
        .title(APP_NAME)
        .body(body)
        .show()
    {
        log::error!("notification failed: {error}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Registered first, which it has to be: it claims the instance before anything else starts,
        // so a second launch hands over and exits instead of adding a tray icon.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            log::info!("another launch: showing the settings window");
            open_settings(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(log_plugin())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            // The marker tells a login start apart from a double-click, so only the latter gets the
            // "running in the background" notification.
            Some(vec![AUTOSTART_FLAG]),
        ))
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let first_run = !config_path.exists();
            let loaded = config::load(&config_path);
            match &loaded.warning {
                Some(warning) => log::error!("config: {warning}"),
                None => log::info!(
                    "config: {} schedule(s) loaded from {}",
                    loaded.config.schedules.len(),
                    config_path.display()
                ),
            }

            let waiter = match Waiter::new() {
                Ok(waiter) => Some(Arc::new(waiter)),
                Err(error) => {
                    log::error!(
                        "scheduler disabled: could not create the waitable timer ({error})"
                    );
                    None
                }
            };

            let tray = match build_tray(app.handle(), loaded.config.locale) {
                Ok(tray) => Some(tray),
                Err(error) => {
                    log::error!("tray unavailable: {error}");
                    None
                }
            };

            app.manage(AppState {
                inner: Mutex::new(Inner {
                    config: loaded.config.clone(),
                    warning: loaded.warning,
                    manual_at: None,
                }),
                config_path,
                waiter,
                tray: Mutex::new(tray),
            });

            let handle = app.handle().clone();
            if let Err(error) = std::thread::Builder::new()
                .name("lenovo-conservation-scheduler".to_string())
                .spawn({
                    let handle = handle.clone();
                    move || scheduler_loop(handle)
                })
            {
                log::error!("scheduler disabled: could not start its thread ({error})");
            }

            apply_autostart(&handle, loaded.config.start_with_windows);

            // First run: no config yet, so open the settings window rather than starting invisibly.
            if first_run {
                open_settings(&handle);
            } else if !started_by_autostart() {
                // Launched by hand with settings already saved: the app goes straight to the tray, so
                // say so, or the double-click looks like it did nothing. Windows starting it at login
                // stays quiet on purpose.
                notify_background(&handle);
            }

            let state = app.state::<AppState>();
            let snapshot = snapshot(&state);
            publish(&handle, &snapshot);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_conservation,
            save_config
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, event| {
        // The tray owns the app's lifetime: closing the settings window must not end the app.
        // `code` is None when the exit came from user interaction (the last window closing) and
        // Some when `AppHandle::exit` was called, which is the tray's Exit entry.
        if let RunEvent::ExitRequested { code, api, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
