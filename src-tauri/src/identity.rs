//! Makes Windows know who this app is.
//!
//! A toast notification is attributed to an AppUserModelID. Tauri's notification plugin uses the
//! bundle identifier for that (see its `desktop.rs`, `Notification::new(identifier)`), and Windows
//! shows a name and an icon for an id only if something registered it. An unpackaged app has no
//! installer to do that, so the app does it itself, under `HKCU`, where no administrator rights are
//! needed:
//!
//! `HKCU\Software\Classes\AppUserModelId\<identifier>` with `DisplayName` and `IconUri`.
//!
//! Without it the toast arrives with no app name and a generic icon - when it appears at all.

use std::fs;
use std::path::{Path, PathBuf};

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// The app's own icon, compiled into the binary.
///
/// Windows wants a path to an *image* for the toast identity (`IconUri`), and an unpackaged app has
/// nothing else to put on disk: pointing that value at the executable itself produced a notification
/// with no icon at all.
const ICON_PNG: &[u8] = include_bytes!("../icons/128x128.png");

/// Writes the icon where `IconUri` can point at it, and returns the path. Called on every start, so
/// it only writes when the file is missing or not the icon this build carries.
pub fn write_icon(directory: &Path) -> Result<PathBuf, String> {
    let path = directory.join("icon.png");
    if fs::metadata(&path).map(|found| found.len()).ok() == Some(ICON_PNG.len() as u64) {
        return Ok(path);
    }
    fs::create_dir_all(directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    fs::write(&path, ICON_PNG)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(path)
}

/// Writes the app's toast identity. Called on every start: the values are tiny and rewriting them
/// keeps the icon path correct if the executable or the data directory ever moves.
pub fn register(app_name: &str, identifier: &str, icon: &Path) -> Result<(), String> {
    let path = key_path(identifier);
    write(&path, app_name, &icon.to_string_lossy())
}

/// The identity of the app as Windows notification centre will show it.
fn key_path(identifier: &str) -> String {
    format!(r"Software\Classes\AppUserModelId\{identifier}")
}

fn write(path: &str, app_name: &str, icon: &str) -> Result<(), String> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(path)
        .map_err(|error| format!("could not open {path}: {error}"))?
        .0;

    key.set_value("DisplayName", &app_name.to_string())
        .and_then(|()| key.set_value("IconUri", &icon.to_string()))
        .map_err(|error| format!("could not write the identity values: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes to a throwaway key rather than the app's real one, and removes it afterwards.
    #[test]
    fn writes_the_display_name_and_the_icon_path() {
        let path = key_path("com.fajarwz.lenovo-conservation-scheduler-test");

        write(&path, "Test App", r"C:\somewhere\app.exe").expect("write");

        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(&path)
            .expect("the key should exist after writing");
        assert_eq!(
            key.get_value::<String, _>("DisplayName").expect("name"),
            "Test App"
        );
        assert_eq!(
            key.get_value::<String, _>("IconUri").expect("icon"),
            r"C:\somewhere\app.exe"
        );

        RegKey::predef(HKEY_CURRENT_USER)
            .delete_subkey_all(&path)
            .expect("cleanup");
    }

    #[test]
    fn the_icon_lands_on_disk_with_this_builds_bytes() {
        let directory = std::env::temp_dir().join("lenovo-conservation-scheduler-icon-test");
        let path = write_icon(&directory).expect("write");
        assert_eq!(fs::read(&path).expect("read"), ICON_PNG);

        // Running again with the icon already there is not an error: this happens on every start.
        write_icon(&directory).expect("write again");
        assert_eq!(fs::read(&path).expect("read"), ICON_PNG);

        fs::remove_file(&path).ok();
    }

    #[test]
    fn the_identity_key_hangs_off_the_users_own_hive() {
        // HKCU, not HKLM: this has to work without administrator rights.
        assert_eq!(
            key_path("com.fajarwz.example"),
            r"Software\Classes\AppUserModelId\com.fajarwz.example"
        );
    }
}
