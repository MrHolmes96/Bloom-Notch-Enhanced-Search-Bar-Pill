//! Bloom's folders on this PC, under its own name (`com.mrholmes.bloom`).
//!
//! Builds before this one kept their files under the original project's name
//! (`com.sehaz.bloom`): settings, logs and lists in `%APPDATA%`, the web
//! views' storage (notes, shelf, search history) in `%LOCALAPPDATA%`, and the
//! install folder in `HKCU\Software\Sehajveer Singh\bloom`. `move_old_data`
//! moves all of it over once, at start, before anything opens those folders,
//! so nothing of the old name is left behind. Settings > About > Data opens
//! the folders (`open_bloom_folder`).

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCopyTreeW, RegCreateKeyExW, RegDeleteKeyW, RegDeleteTreeW, RegOpenKeyExW,
    HKEY, HKEY_CURRENT_USER, KEY_ALL_ACCESS, KEY_READ, REG_OPTION_NON_VOLATILE,
};

/// Bloom's name for its folders; the same as `identifier` in tauri.conf.json.
pub const IDENTIFIER: &str = "com.mrholmes.bloom";
/// The name builds before this one used.
const OLD_IDENTIFIER: &str = "com.sehaz.bloom";
/// The web views' own store: moved whole or not at all, since two halves
/// of one don't make a working store.
const WEBVIEW_STORE: &str = "EBWebView";

/// Moves everything kept under the old name to Bloom's own. Called once at
/// start, after the single-instance check and before the app is built.
pub fn move_old_data() {
    for variable in ["APPDATA", "LOCALAPPDATA"] {
        let Some(base) = std::env::var_os(variable).map(PathBuf::from) else {
            continue;
        };
        let old = base.join(OLD_IDENTIFIER);
        if !old.is_dir() {
            continue;
        }
        let new = base.join(IDENTIFIER);
        move_folder(&old, &new);
        if variable == "APPDATA" {
            forget_old_name_in_files(&new);
        }
    }
    move_old_registry_key();
}

/// The old folder becomes the new one, or, when the new one is there
/// already, gives it whatever it lacks; then the old one goes.
fn move_folder(old: &Path, new: &Path) {
    if !new.exists() {
        // The web views of a Bloom that was just stopped (by the installer)
        // can hold the folder open for a moment.
        for _ in 0..20 {
            if std::fs::rename(old, new).is_ok() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    merge_into(old, new);
    let _ = std::fs::remove_dir_all(old);
}

/// Moves every file of `from` that `to` doesn't have into `to`.
fn merge_into(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    let _ = std::fs::create_dir_all(to);
    for entry in entries.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if entry.file_name().eq_ignore_ascii_case(WEBVIEW_STORE) {
                if !target.exists() {
                    let _ = std::fs::rename(&source, &target);
                }
                continue;
            }
            merge_into(&source, &target);
        } else if !target.exists() && std::fs::rename(&source, &target).is_err() {
            let _ = std::fs::copy(&source, &target);
        }
    }
}

/// Bloom's own lists name its folder in full in places (`custom_icons`,
/// pinned apps): those now name the new one. Also drops the last
/// announcement dismissed, from the announcements Bloom no longer fetches.
fn forget_old_name_in_files(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut changed = text.replace(OLD_IDENTIFIER, IDENTIFIER);
        if entry.file_name() == "settings.json" {
            if let Ok(serde_json::Value::Object(mut settings)) =
                serde_json::from_str::<serde_json::Value>(&changed)
            {
                if settings.remove("bloom-announcement-dismissed").is_some() {
                    if let Ok(text) = serde_json::to_string(&serde_json::Value::Object(settings)) {
                        changed = text;
                    }
                }
            }
        }
        if changed != text {
            let _ = std::fs::write(&path, changed);
        }
    }
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

/// The installer's note of where Bloom is installed, kept under the
/// publisher's name, moves from the old publisher to MrHolmes.
fn move_old_registry_key() {
    const OLD_PARENT: PCWSTR = w!("Software\\Sehajveer Singh");
    const OLD: PCWSTR = w!("Software\\Sehajveer Singh\\bloom");
    const NEW: PCWSTR = w!("Software\\MrHolmes\\bloom");
    unsafe {
        let mut old = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, OLD, Some(0), KEY_READ, &mut old).is_err() {
            return;
        }
        let old = Key(old);
        let mut new = HKEY::default();
        let already = RegOpenKeyExW(HKEY_CURRENT_USER, NEW, Some(0), KEY_READ, &mut new).is_ok();
        if already {
            drop(Key(new));
        } else {
            let mut created = HKEY::default();
            if RegCreateKeyExW(
                HKEY_CURRENT_USER,
                NEW,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_ALL_ACCESS,
                None,
                &mut created,
                None,
            )
            .is_err()
            {
                return;
            }
            let created = Key(created);
            if RegCopyTreeW(old.0, PCWSTR::null(), created.0).is_err() {
                return;
            }
        }
        drop(old);
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, OLD);
        let _ = RegDeleteKeyW(HKEY_CURRENT_USER, OLD);
        // Only goes when nothing else is in it.
        let _ = RegDeleteKeyW(HKEY_CURRENT_USER, OLD_PARENT);
    }
}

/// Opens one of Bloom's folders in File Explorer: "data" (settings, logs,
/// `tray-icons.txt`) or "cache" (the web views' storage and caches).
#[tauri::command]
pub fn open_bloom_folder(app: AppHandle, kind: String) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let folder = match kind.as_str() {
        "data" => app.path().app_config_dir(),
        "cache" => app.path().app_local_data_dir(),
        _ => return Err(format!("no folder called {kind}")),
    }
    .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let wide: Vec<u16> = folder
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Above 32 means it opened.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("Windows couldn't open the folder".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_matches_the_app_config() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], IDENTIFIER);
    }

    #[test]
    fn merging_keeps_newer_files_and_moves_the_rest() {
        let base = std::env::temp_dir().join(format!("bloom-merge-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let old = base.join("old");
        let new = base.join("new");
        std::fs::create_dir_all(old.join("logs")).unwrap();
        std::fs::create_dir_all(old.join(WEBVIEW_STORE)).unwrap();
        std::fs::create_dir_all(new.join(WEBVIEW_STORE)).unwrap();
        std::fs::write(old.join("settings.json"), "old").unwrap();
        std::fs::write(old.join("logs").join("a.log"), "a").unwrap();
        std::fs::write(old.join(WEBVIEW_STORE).join("store"), "old").unwrap();
        std::fs::write(new.join("settings.json"), "new").unwrap();
        move_folder(&old, &new);
        assert!(!old.exists());
        assert_eq!(std::fs::read_to_string(new.join("settings.json")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(new.join("logs").join("a.log")).unwrap(), "a");
        // A web store already there isn't mixed with the old one.
        assert!(!new.join(WEBVIEW_STORE).join("store").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
