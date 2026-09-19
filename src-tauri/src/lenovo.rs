//! Lenovo charging-mode control (Conservation Mode) through Lenovo Vantage's `PowerBattery.dll`.
//!
//! Why this DLL: on consumer Yoga/IdeaPad hardware there is no documented interface for
//! conservation mode. This machine's WMI surface (`LENOVO_GAMEZONE_DATA`, `LENOVO_OTHER_METHOD`)
//! exposes fan/overclock/lighting only, and the Legion-style `EnergyDriver` is not present.
//! `PowerBattery.dll` is the native x64 library Lenovo Vantage itself uses; it is loaded in
//! place from Vantage's install directory and is never copied or redistributed.
//!
//! This is the only module in the app containing `unsafe`: a few kernel32 calls plus five C++
//! member functions reached by their exported mangled names. On Windows x64 there is a single
//! calling convention, so `extern "system"` matches the DLL's `__cdecl` symbols.
//!
//! Verified on a Yoga Slim 7 14IMH9 (83CV), non-admin, with the Lenovo Notebook ITS Service
//! running: `DoesSupportConservationMode() == 1`, `GetChargingMode() == 0`,
//! `SetChargingMode(1) -> 1`, `SetChargingMode(0) -> 0`.

use crate::i18n::Strings;
use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

// C++ mangled exports. These names are stable across Lenovo Vantage builds observed so far;
// if a future build renames them, resolution fails and the app reports "unavailable".
const SYM_CTOR: &[u8] = b"??0CChargingMode@PowerBattery@@QEAA@XZ\0";
const SYM_DTOR: &[u8] = b"??1CChargingMode@PowerBattery@@QEAA@XZ\0";
const SYM_GET: &[u8] = b"?GetChargingMode@CChargingMode@PowerBattery@@QEBAHXZ\0";
const SYM_SET: &[u8] = b"?SetChargingMode@CChargingMode@PowerBattery@@QEBAHH@Z\0";
const SYM_SUPPORTS_CONSERVATION: &[u8] =
    b"?DoesSupportConservationMode@CChargingMode@PowerBattery@@QEBAHXZ\0";

// Kernel32 only. `LOAD_WITH_ALTERED_SEARCH_PATH` makes the DLL resolve its sibling
// vcruntime140.dll / msvcp140.dll from its own directory first.
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(file: *const u16, reserved: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn GetLastError() -> u32;
}

const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x0000_0008;

type Module = *mut c_void;
type CtorFn = unsafe extern "system" fn(*mut RawChargingMode);
type DtorFn = unsafe extern "system" fn(*mut RawChargingMode);
type GetFn = unsafe extern "system" fn(*mut RawChargingMode) -> i32;
type SetFn = unsafe extern "system" fn(*mut RawChargingMode, i32) -> i32;

/// The native `CChargingMode` object: exactly 16 bytes (two interface pointers), which the
/// constructor fills in. It must be zeroed before the constructor runs.
#[repr(C, align(8))]
struct RawChargingMode {
    interfaces: [usize; 2],
}

/// Battery charging mode as reported by Lenovo's driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargingMode {
    Normal,
    Conservation,
    Rapid,
}

impl ChargingMode {
    /// The mode to apply for "Conservation Mode ON / OFF".
    pub fn for_conservation(on: bool) -> Self {
        if on {
            Self::Conservation
        } else {
            Self::Normal
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Conservation => "Conservation",
            Self::Rapid => "Rapid",
        }
    }

    /// Whether conservation mode is the active mode. `Rapid` is a separate Lenovo mode this app
    /// does not manage, so it counts as "conservation off" and is never overwritten.
    pub fn is_conservation(self) -> bool {
        matches!(self, Self::Conservation)
    }

    fn as_raw(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::Conservation => 1,
            Self::Rapid => 2,
        }
    }

    fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(Self::Normal),
            1 => Some(Self::Conservation),
            2 => Some(Self::Rapid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LenovoError {
    /// `PowerBattery.dll` is not on this machine (no Lenovo Vantage, not a Lenovo).
    NotFound,
    LoadFailed {
        path: PathBuf,
        code: u32,
    },
    MissingExport {
        symbol: &'static str,
    },
    /// The device/BIOS reports no conservation-mode support.
    NotSupported,
    UnknownMode(i32),
    NotApplied {
        requested: ChargingMode,
        actual: ChargingMode,
    },
}

impl LenovoError {
    /// Short, non-technical text safe to show in the UI or a notification, in one language.
    pub fn user_message(&self, strings: &Strings) -> &'static str {
        match self {
            Self::NotFound => strings.lenovo_missing,
            Self::LoadFailed { .. } => strings.lenovo_load_failed,
            Self::MissingExport { .. } => strings.lenovo_missing_export,
            Self::NotSupported => strings.lenovo_not_supported,
            Self::UnknownMode(_) => strings.lenovo_unknown_mode,
            Self::NotApplied { .. } => strings.lenovo_not_applied,
        }
    }

    /// Technical detail for logs and debugging only.
    pub fn detail(&self) -> String {
        match self {
            Self::NotFound => {
                "PowerBattery.dll not found (checked app dir and Lenovo Vantage addin)".into()
            }
            Self::LoadFailed { path, code } => {
                format!(
                    "LoadLibraryExW failed for {} (error {code})",
                    path.display()
                )
            }
            Self::MissingExport { symbol } => format!("export not found: {symbol}"),
            Self::NotSupported => "DoesSupportConservationMode() returned 0".into(),
            Self::UnknownMode(raw) => format!("unexpected charging mode value {raw}"),
            Self::NotApplied { requested, actual } => format!(
                "requested {} but the driver reports {}",
                requested.label(),
                actual.label()
            ),
        }
    }
}

/// Loaded handle plus the resolved member functions.
struct Dll {
    /// Kept to hold a reference on the module; the DLL is never unloaded.
    _module: Module,
    ctor: CtorFn,
    dtor: Option<DtorFn>,
    get: GetFn,
    set: SetFn,
    supports_conservation: GetFn,
}

impl Dll {
    fn load() -> Result<Self, LenovoError> {
        let path = locate_dll().ok_or(LenovoError::NotFound)?;
        let wide: Vec<u16> = OsStr::new(&path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
        let module = unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_WITH_ALTERED_SEARCH_PATH,
            )
        };
        if module.is_null() {
            // SAFETY: no invariants; reads the calling thread's last error.
            let code = unsafe { GetLastError() };
            return Err(LenovoError::LoadFailed { path, code });
        }

        // SAFETY: `module` is a valid module handle from LoadLibraryExW.
        unsafe {
            Ok(Self {
                _module: module,
                ctor: resolve(module, SYM_CTOR, "CChargingMode::CChargingMode")?,
                dtor: resolve_optional(module, SYM_DTOR),
                get: resolve(module, SYM_GET, "CChargingMode::GetChargingMode")?,
                set: resolve(module, SYM_SET, "CChargingMode::SetChargingMode")?,
                supports_conservation: resolve(
                    module,
                    SYM_SUPPORTS_CONSERVATION,
                    "CChargingMode::DoesSupportConservationMode",
                )?,
            })
        }
    }

    /// Constructs a fresh native object ("this") for one call.
    ///
    /// SAFETY: every method below passes only this object to functions of the same class.
    /// Passing it to another class (`CAdapter`, `CBatteryInformation`) faults - their layouts differ.
    unsafe fn new_object(&self) -> RawChargingMode {
        let mut object = RawChargingMode { interfaces: [0; 2] };
        (self.ctor)(&mut object);
        object
    }

    unsafe fn drop_object(&self, object: &mut RawChargingMode) {
        if let Some(dtor) = self.dtor {
            (dtor)(object);
        }
    }
}

/// Resolves a mangled export to a function pointer.
///
/// SAFETY: the caller must name an export with the exact signature of `T`.
unsafe fn resolve<T: Copy>(
    module: Module,
    mangled: &'static [u8],
    readable: &'static str,
) -> Result<T, LenovoError> {
    let address = GetProcAddress(module, mangled.as_ptr());
    if address.is_null() {
        return Err(LenovoError::MissingExport { symbol: readable });
    }
    Ok(std::mem::transmute_copy(&address))
}

/// Same as [`resolve`] for exports we can live without (the destructor).
unsafe fn resolve_optional<T: Copy>(module: Module, mangled: &'static [u8]) -> Option<T> {
    let address = GetProcAddress(module, mangled.as_ptr());
    if address.is_null() {
        return None;
    }
    Some(std::mem::transmute_copy(&address))
}

/// Finds `PowerBattery.dll`: next to our own executable first (portable copy), otherwise in
/// the newest Lenovo Vantage `IdeaNotebookAddin` version directory.
fn locate_dll() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("PowerBattery.dll");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let addin = PathBuf::from(std::env::var_os("ProgramData")?)
        .join("Lenovo")
        .join("Vantage")
        .join("Addins")
        .join("IdeaNotebookAddin");

    let mut newest: Option<(Vec<u32>, PathBuf)> = None;
    for entry in std::fs::read_dir(&addin).ok()?.flatten() {
        let version_dir = entry.path();
        if !version_dir.is_dir() {
            continue;
        }
        // Sibling layouts exist: <version>\PowerBattery.dll and <version>\x64\PowerBattery.dll.
        for relative in ["PowerBattery.dll", "x64\\PowerBattery.dll"] {
            let candidate = version_dir.join(relative);
            if !candidate.is_file() {
                continue;
            }
            let version = parse_version(&entry.file_name().to_string_lossy());
            if newest.as_ref().is_none_or(|(best, _)| version > *best) {
                newest = Some((version, candidate));
            }
            break;
        }
    }
    newest.map(|(_, path)| path)
}

fn parse_version(name: &str) -> Vec<u32> {
    name.split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// `Ok(())` when conservation mode can be controlled on this machine.
pub fn availability() -> Result<(), LenovoError> {
    if supports_conservation()? {
        Ok(())
    } else {
        Err(LenovoError::NotSupported)
    }
}

pub fn supports_conservation() -> Result<bool, LenovoError> {
    let dll = Dll::load()?;
    // SAFETY: the object comes from this DLL's own constructor.
    let supported = unsafe {
        let mut object = dll.new_object();
        let supported = (dll.supports_conservation)(&mut object);
        dll.drop_object(&mut object);
        supported
    };
    Ok(supported == 1)
}

/// Reads the mode currently active in the Lenovo driver/EC.
pub fn get_mode() -> Result<ChargingMode, LenovoError> {
    let dll = Dll::load()?;
    // SAFETY: as above; `get` takes only the object it was constructed from.
    let raw = unsafe {
        let mut object = dll.new_object();
        let raw = (dll.get)(&mut object);
        dll.drop_object(&mut object);
        raw
    };
    ChargingMode::from_raw(raw).ok_or(LenovoError::UnknownMode(raw))
}

/// Applies `mode` and returns the mode the driver reports afterwards.
///
/// A fresh object is used for the read-back, so the result is the actual state rather than the
/// write call's own return value. If it disagrees with `mode`, the change did not take.
pub fn set_mode(mode: ChargingMode) -> Result<ChargingMode, LenovoError> {
    let dll = Dll::load()?;
    // SAFETY: both objects come from this DLL's constructor and are only passed to its methods.
    let raw = unsafe {
        let mut writer = dll.new_object();
        (dll.set)(&mut writer, mode.as_raw());
        dll.drop_object(&mut writer);

        let mut reader = dll.new_object();
        let raw = (dll.get)(&mut reader);
        dll.drop_object(&mut reader);
        raw
    };
    let actual = ChargingMode::from_raw(raw).ok_or(LenovoError::UnknownMode(raw))?;
    if actual == mode {
        Ok(actual)
    } else {
        Err(LenovoError::NotApplied {
            requested: mode,
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints what this machine reports:
    /// `cargo test --manifest-path src-tauri/Cargo.toml lenovo -- --nocapture --test-threads=1`
    #[test]
    fn reports_availability_and_current_mode() {
        match availability() {
            Ok(()) => println!("lenovo: available"),
            Err(error) => {
                println!("lenovo: unavailable ({})", error.detail());
                return;
            }
        }
        println!(
            "lenovo: supports conservation = {:?}",
            supports_conservation()
        );
        println!("lenovo: current mode        = {:?}", get_mode());
    }

    /// Enables conservation, disables it again, and restores whatever was there before.
    /// Skips itself on machines without Lenovo battery control so the suite stays portable.
    #[test]
    fn conservation_mode_round_trip() {
        let Ok(original) = get_mode() else {
            eprintln!("skipping: Lenovo battery control unavailable on this machine");
            return;
        };
        assert!(
            supports_conservation().unwrap_or(false),
            "device advertises no conservation-mode support"
        );

        assert_eq!(
            set_mode(ChargingMode::Conservation).expect("enable conservation mode"),
            ChargingMode::Conservation
        );
        assert_eq!(
            get_mode().expect("read mode after enabling"),
            ChargingMode::Conservation
        );

        assert_eq!(
            set_mode(ChargingMode::Normal).expect("disable conservation mode"),
            ChargingMode::Normal
        );
        assert_eq!(
            get_mode().expect("read mode after disabling"),
            ChargingMode::Normal
        );

        set_mode(original).expect("restore original mode");
        assert_eq!(get_mode().expect("read mode after restore"), original);
    }
}
