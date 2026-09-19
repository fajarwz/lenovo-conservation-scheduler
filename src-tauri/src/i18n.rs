//! The languages the app speaks, and every string it says in them.
//!
//! English is the reference table: an Indonesian string that is missing will not compile.
//! Rust keeps its own copy because the tray menu, the tray tooltip and Windows notifications are
//! produced by Windows and never pass through the webview. The React side has its own table for
//! the window; both sides read the same `locale` from the config file.
//!
//! Logs deliberately stay in English - they are for debugging, not for the user.

use serde::{Deserialize, Serialize};

/// The product name. Not translated: it is the app's name in every language.
pub const APP_NAME: &str = "Lenovo Conservation Scheduler";

/// A language the app can speak. Serialised as the value used in the config file and the frontend.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    #[serde(rename = "en-US")]
    EnUs,
    #[serde(rename = "id")]
    Id,
}

impl Lang {
    /// Reads a config value such as `"id"`, `"id-ID"` or `"en-us"`. Anything else is `None`,
    /// which the caller turns into the OS language.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en_us" | "en-gb" | "english" => Some(Lang::EnUs),
            "id" | "id-id" | "id_id" | "in" | "indonesian" => Some(Lang::Id),
            _ => None,
        }
    }

    /// The language Windows is set to, so a first run already speaks the user's language.
    pub fn detect() -> Self {
        // SAFETY: no arguments and no out-parameters; it only reports the user's UI language.
        let langid = unsafe { GetUserDefaultUILanguage() };
        // A LANGID keeps the primary language in its low ten bits.
        match langid & 0x03FF {
            LANG_INDONESIAN => Lang::Id,
            _ => Lang::EnUs,
        }
    }

    /// The strings to use for this language.
    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::EnUs => &EN_US,
            Lang::Id => &ID,
        }
    }
}

/// An unknown or malformed value falls back to the OS language rather than failing the whole
/// file, so a hand-edited config keeps loading.
impl<'de> Deserialize<'de> for Lang {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Option::<serde_json::Value>::deserialize(deserializer)?;
        Ok(value
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .and_then(Lang::parse)
            .unwrap_or_else(Lang::detect))
    }
}

const LANG_INDONESIAN: u16 = 0x21;

/// `LOCALE_USER_DEFAULT`: the settings of whoever is logged in. `LOCALE_ITIME` is the one that says
/// whether Windows writes times as 24-hour ("1") or with AM/PM ("0").
const LOCALE_USER_DEFAULT: u32 = 0x0400;
const LOCALE_ITIME: u32 = 0x0000_0023;
const LOCALE_BUFFER: usize = 8;

#[link(name = "kernel32")]
extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
    fn GetLocaleInfoW(locale: u32, kind: u32, buffer: *mut u16, size: i32) -> i32;
}

/// How times are written. Deliberately not part of the language: plenty of people read English and
/// still write 17:30, so it is its own choice, defaulting to whatever Windows is set to.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeFormat {
    #[default]
    #[serde(rename = "24h")]
    TwentyFourHour,
    #[serde(rename = "12h")]
    TwelveHour,
}

impl TimeFormat {
    /// Reads a config value such as `"12h"` or `"24h"`. Anything else is `None`, which the caller
    /// turns into the OS setting.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "24h" | "24" | "h24" | "24-hour" => Some(TimeFormat::TwentyFourHour),
            "12h" | "12" | "h12" | "12-hour" | "am/pm" | "ampm" => Some(TimeFormat::TwelveHour),
            _ => None,
        }
    }

    /// What Windows is set to.
    pub fn detect() -> Self {
        let mut buffer = [0u16; LOCALE_BUFFER];
        // SAFETY: a fixed-size buffer is passed together with its own length, and the call writes at
        // most that many UTF-16 units into it.
        let written = unsafe {
            GetLocaleInfoW(
                LOCALE_USER_DEFAULT,
                LOCALE_ITIME,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            )
        };
        // A failed query keeps 24-hour: it is the unambiguous one, and the setting exists precisely
        // so the user can say otherwise.
        if written <= 1 {
            return TimeFormat::TwentyFourHour;
        }
        if buffer[0] == u16::from(b'1') {
            TimeFormat::TwentyFourHour
        } else {
            TimeFormat::TwelveHour
        }
    }
}

/// A malformed value falls back to the OS setting rather than failing the whole file, the same rule
/// the language follows.
impl<'de> Deserialize<'de> for TimeFormat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Option::<serde_json::Value>::deserialize(deserializer)?;
        Ok(value
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .and_then(TimeFormat::parse)
            .unwrap_or_else(TimeFormat::detect))
    }
}

/// Fills `{}` placeholders left to right, so a translation is free to reorder the parts.
pub fn fill(template: &str, values: &[&str]) -> String {
    let mut filled = template.to_string();
    for value in values {
        match filled.find("{}") {
            Some(at) => filled.replace_range(at..at + 2, value),
            None => break,
        }
    }
    filled
}

/// Every string the Rust side needs. `{}` marks a placeholder for [`fill`].
///
/// Fields are shared words and whole sentences, never fragments the caller assembles by hand:
/// a language must be able to reorder a sentence without touching the code.
pub struct Strings {
    // The tray.
    pub tray_starting: &'static str,
    pub tray_open_settings: &'static str,
    pub tray_conservation_on: &'static str,
    pub tray_conservation_off: &'static str,
    pub tray_schedule_enabled: &'static str,
    pub tray_exit: &'static str,
    /// `Battery {}% - Conservation {}` (percent, on/off word).
    pub tray_status: &'static str,
    /// `Battery {}% - Lenovo control unavailable` (percent).
    pub tray_status_unavailable: &'static str,
    /// The state word used in the tray status and in the window.
    pub word_on: &'static str,
    pub word_off: &'static str,

    // Notifications.
    pub notification_on: &'static str,
    pub notification_off: &'static str,
    /// Shown when a manual launch lands in the tray with no window: the double-click must not look
    /// like it did nothing.
    pub notification_background: &'static str,

    // Lenovo failures (shown in the window and on the tray's status line).
    pub lenovo_missing: &'static str,
    pub lenovo_load_failed: &'static str,
    pub lenovo_missing_export: &'static str,
    pub lenovo_not_supported: &'static str,
    pub lenovo_unknown_mode: &'static str,
    pub lenovo_not_applied: &'static str,

    // Config problems. The first three carry the file paths and the underlying error.
    pub config_unreadable: &'static str,
    pub config_damaged: &'static str,
    pub config_damaged_unkept: &'static str,
    pub config_missing_id: &'static str,
    pub config_duplicate_id: &'static str,
    pub config_invalid_time: &'static str,
    pub config_needs_a_day: &'static str,
    pub config_save_failed: &'static str,
    pub config_save_failed_detail: &'static str,
}

static EN_US: Strings = Strings {
    tray_starting: "Starting…",
    tray_open_settings: "Open settings",
    tray_conservation_on: "Conservation Mode ON",
    tray_conservation_off: "Conservation Mode OFF",
    tray_schedule_enabled: "Schedule enabled",
    tray_exit: "Exit",
    tray_status: "Battery {}% - Conservation {}",
    tray_status_unavailable: "Battery {}% - Lenovo control unavailable",
    word_on: "ON",
    word_off: "OFF",

    notification_on: "Conservation Mode enabled",
    notification_off: "Conservation Mode disabled",
    notification_background: "Running in the background. Open it any time from the tray icon.",

    lenovo_missing: "Lenovo battery control is unavailable on this device.",
    lenovo_load_failed: "Could not load Lenovo battery control.",
    lenovo_missing_export:
        "Lenovo battery control is not supported by the installed Lenovo Vantage version.",
    lenovo_not_supported: "This device does not support battery Conservation Mode.",
    lenovo_unknown_mode: "Could not read the Lenovo battery mode.",
    lenovo_not_applied: "Could not change Conservation Mode.",

    config_unreadable: "Could not read {} ({}); using defaults.",
    config_damaged: "{} was not valid JSON ({}); it was kept as {} and settings were reset.",
    config_damaged_unkept:
        "{} was not valid JSON ({}); settings were reset but the file could not be kept ({}).",
    config_missing_id: "A schedule is missing its id.",
    config_duplicate_id: "Two schedules share the same id.",
    config_invalid_time: "\"{}\" is not a valid time. Use 24-hour HH:MM, for example 05:00.",
    config_needs_a_day: "Each schedule needs at least one day.",
    config_save_failed: "Could not save the schedule settings.",
    config_save_failed_detail: "Could not save the schedule settings: {}",
};

static ID: Strings = Strings {
    tray_starting: "Memulai…",
    tray_open_settings: "Buka pengaturan",
    tray_conservation_on: "Mode Konservasi AKTIF",
    tray_conservation_off: "Mode Konservasi NONAKTIF",
    tray_schedule_enabled: "Jadwal aktif",
    tray_exit: "Keluar",
    tray_status: "Baterai {}% - Konservasi {}",
    tray_status_unavailable: "Baterai {}% - kontrol Lenovo tidak tersedia",
    word_on: "AKTIF",
    word_off: "NONAKTIF",

    notification_on: "Mode Konservasi diaktifkan",
    notification_off: "Mode Konservasi dinonaktifkan",
    notification_background: "Berjalan di latar belakang. Buka kapan saja dari ikon tray.",

    lenovo_missing: "Kontrol baterai Lenovo tidak tersedia di perangkat ini.",
    lenovo_load_failed: "Gagal memuat kontrol baterai Lenovo.",
    lenovo_missing_export: "Kontrol baterai Lenovo tidak didukung oleh versi Lenovo Vantage yang terpasang.",
    lenovo_not_supported: "Perangkat ini tidak mendukung Mode Konservasi baterai.",
    lenovo_unknown_mode: "Gagal membaca mode baterai Lenovo.",
    lenovo_not_applied: "Gagal mengubah Mode Konservasi.",

    config_unreadable: "Gagal membaca {} ({}); memakai pengaturan bawaan.",
    config_damaged: "{} bukan JSON yang valid ({}); berkas disimpan sebagai {} dan pengaturan direset.",
    config_damaged_unkept: "{} bukan JSON yang valid ({}); pengaturan direset tetapi berkasnya tidak dapat disimpan ({}).",
    config_missing_id: "Ada jadwal yang tidak memiliki id.",
    config_duplicate_id: "Ada dua jadwal dengan id yang sama.",
    config_invalid_time: "\"{}\" bukan waktu yang valid. Gunakan format 24 jam HH:MM, misalnya 05:00.",
    config_needs_a_day: "Setiap jadwal memerlukan minimal satu hari.",
    config_save_failed: "Gagal menyimpan pengaturan jadwal.",
    config_save_failed_detail: "Gagal menyimpan pengaturan jadwal: {}",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_accepts_the_languages_and_their_variants() {
        for value in ["en-US", "en-us", "en", "English"] {
            assert_eq!(Lang::parse(value), Some(Lang::EnUs), "{value}");
        }
        for value in ["id", "ID", "id-ID", "indonesian"] {
            assert_eq!(Lang::parse(value), Some(Lang::Id), "{value}");
        }
        assert_eq!(Lang::parse("fr"), None);
        assert_eq!(Lang::parse(""), None);
    }

    #[test]
    fn an_unknown_language_falls_back_to_the_os_language_instead_of_failing() {
        // A hand-edited file must still load: the field is optional and never rejects a value.
        let config: crate::config::Config =
            serde_json::from_str(r#"{"locale":"fr"}"#).expect("load");
        assert_eq!(config.locale, Lang::detect());

        let chosen: crate::config::Config =
            serde_json::from_str(r#"{"locale":"id"}"#).expect("load");
        assert_eq!(chosen.locale, Lang::Id);
    }

    #[test]
    fn both_languages_are_actually_translated() {
        // Guards against adding a language by copying the English table and forgetting to edit it.
        let en = Lang::EnUs.strings();
        let id = Lang::Id.strings();
        assert_ne!(en.tray_open_settings, id.tray_open_settings);
        assert_ne!(en.notification_on, id.notification_on);
        assert_ne!(en.notification_background, id.notification_background);
        assert_ne!(en.lenovo_not_applied, id.lenovo_not_applied);
        assert_ne!(en.config_needs_a_day, id.config_needs_a_day);
    }

    #[test]
    fn every_translation_keeps_the_placeholders_the_code_fills() {
        let en = Lang::EnUs.strings();
        let id = Lang::Id.strings();
        let pairs = [
            (en.tray_status, id.tray_status, 2),
            (en.tray_status_unavailable, id.tray_status_unavailable, 1),
            (en.config_unreadable, id.config_unreadable, 2),
            (en.config_damaged, id.config_damaged, 3),
            (en.config_damaged_unkept, id.config_damaged_unkept, 3),
            (en.config_invalid_time, id.config_invalid_time, 1),
            (
                en.config_save_failed_detail,
                id.config_save_failed_detail,
                1,
            ),
        ];
        for (english, indonesian, expected) in pairs {
            assert_eq!(
                english.matches("{}").count(),
                expected,
                "English placeholder count: {english}"
            );
            assert_eq!(
                indonesian.matches("{}").count(),
                expected,
                "Indonesian placeholder count: {indonesian}"
            );
        }
    }

    #[test]
    fn fill_substitutes_in_order_and_ignores_extra_values() {
        assert_eq!(fill("a {} c {}", &["b", "d"]), "a b c d");
        assert_eq!(fill("{} {}", &["b"]), "b {}");
        assert_eq!(fill("nothing", &["b"]), "nothing");
    }

    #[test]
    fn detection_reports_a_supported_language() {
        assert!(matches!(Lang::detect(), Lang::EnUs | Lang::Id));
    }

    #[test]
    fn a_time_format_parses_from_the_config_or_falls_back_to_windows() {
        assert_eq!(TimeFormat::parse("24h"), Some(TimeFormat::TwentyFourHour));
        assert_eq!(TimeFormat::parse("12h"), Some(TimeFormat::TwelveHour));
        assert_eq!(TimeFormat::parse("12H"), Some(TimeFormat::TwelveHour));
        assert_eq!(TimeFormat::parse("nonsense"), None);

        // Whatever Windows says, it has to be one of the two.
        assert!(matches!(
            TimeFormat::detect(),
            TimeFormat::TwentyFourHour | TimeFormat::TwelveHour
        ));

        let chosen: crate::config::Config =
            serde_json::from_str(r#"{"timeFormat":"12h"}"#).expect("load");
        assert_eq!(chosen.time_format, TimeFormat::TwelveHour);

        // An unreadable value is the OS setting, never an error.
        let nonsense: crate::config::Config =
            serde_json::from_str(r#"{"timeFormat":"nonsense"}"#).expect("load");
        assert_eq!(nonsense.time_format, TimeFormat::detect());
    }
}
