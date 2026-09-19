//! Battery percentage, AC line and charging state from `GetSystemPowerStatus`.
//!
//! Windows reports this without WMI or COM, so no Lenovo interface is involved: the tray tooltip
//! and the settings window keep working on a machine where conservation mode is unavailable.

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SystemPowerStatus {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
    battery_full_life_time: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
}

/// Unknown-value markers used by `GetSystemPowerStatus`.
const UNKNOWN_PERCENT: u8 = 255;
const UNKNOWN_STATUS: u8 = 255;
const BATTERY_FLAG_CHARGING: u8 = 0x08;
const BATTERY_FLAG_NO_BATTERY: u8 = 0x80;
const BATTERY_FLAG_UNKNOWN: u8 = 0xFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerStatus {
    pub percent: Option<u8>,
    pub ac_online: Option<bool>,
    pub charging: Option<bool>,
}

/// Reads the current power status, or `None` if Windows refuses the call.
pub fn status() -> Option<PowerStatus> {
    let mut raw = SystemPowerStatus::default();
    // SAFETY: `raw` is a valid, correctly sized output buffer.
    let ok = unsafe { GetSystemPowerStatus(&mut raw) };
    (ok != 0).then(|| interpret(&raw))
}

fn interpret(raw: &SystemPowerStatus) -> PowerStatus {
    let has_battery =
        raw.battery_flag != BATTERY_FLAG_UNKNOWN && raw.battery_flag & BATTERY_FLAG_NO_BATTERY == 0;

    PowerStatus {
        percent: (raw.battery_life_percent != UNKNOWN_PERCENT && has_battery)
            .then_some(raw.battery_life_percent),
        ac_online: (raw.ac_line_status != UNKNOWN_STATUS).then_some(raw.ac_line_status == 1),
        charging: (raw.battery_flag != BATTERY_FLAG_UNKNOWN)
            .then_some(raw.battery_flag & BATTERY_FLAG_CHARGING != 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(ac: u8, flag: u8, percent: u8) -> SystemPowerStatus {
        SystemPowerStatus {
            ac_line_status: ac,
            battery_flag: flag,
            battery_life_percent: percent,
            ..SystemPowerStatus::default()
        }
    }

    #[test]
    fn reads_the_live_power_status() {
        let status = status().expect("GetSystemPowerStatus should succeed on a laptop");
        println!("power: {status:?}");
        if let Some(percent) = status.percent {
            assert!(percent <= 100, "percent out of range: {percent}");
        }
    }

    #[test]
    fn interprets_plugged_in_and_charging() {
        let status = interpret(&raw(1, BATTERY_FLAG_CHARGING, 78));
        assert_eq!(status.percent, Some(78));
        assert_eq!(status.ac_online, Some(true));
        assert_eq!(status.charging, Some(true));
    }

    #[test]
    fn interprets_on_battery() {
        let status = interpret(&raw(0, 0x01, 42));
        assert_eq!(status.ac_online, Some(false));
        assert_eq!(status.charging, Some(false));
    }

    #[test]
    fn reports_no_battery_as_unknown() {
        let status = interpret(&raw(1, BATTERY_FLAG_NO_BATTERY, 100));
        assert_eq!(status.percent, None, "a desktop has no battery percentage");
        assert_eq!(status.ac_online, Some(true));
    }

    #[test]
    fn reports_unknown_markers_as_none() {
        let status = interpret(&raw(UNKNOWN_STATUS, BATTERY_FLAG_UNKNOWN, UNKNOWN_PERCENT));
        assert_eq!(status.percent, None);
        assert_eq!(status.ac_online, None);
        assert_eq!(status.charging, None);
    }
}
