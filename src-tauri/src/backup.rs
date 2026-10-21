//! Settings > About > Export and Import: one backup file with everything of
//! the user's that Bloom keeps, to restore on this PC or set up another.
//!
//! - Every setting: all of settings.json, so a setting added later is in
//!   the backup without anything changing here.
//! - The dock's pinned apps. On import only the ones found on this PC are
//!   pinned (a program that is there, or a Store app it knows).
//! - Custom icons set on dock apps, and the window sizes Bloom remembers
//!   (used only where they still fit a screen).
//! - What only the web views keep (the search bar's history and ranking),
//!   handed over by the settings page.
//!
//! Not in it: caches, logs, the shelf (it points at files on this PC), the
//! notification history, Blitzit's sign-in and LocalSend's certificate
//! (each PC has its own).
//!
//! Anything new that keeps the user's own data in a file of its own belongs
//! in here too (see AGENTS.md). Backups from before this format (settings
//! only) still import.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

/// This format's number, in the file as `bloom-backup`.
const FORMAT: u64 = 2;
const ICONS_DIR: &str = "custom_icons";
const ICONS_MANIFEST: &str = "custom_icons.json";

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The custom icons: each picture (base64 PNG) by file name, and the list
/// saying which app each is for.
fn export_icons(dir: &Path) -> Value {
    let folder = dir.join(ICONS_DIR);
    let mut files = Map::new();
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("png") {
                continue;
            }
            let (Some(name), Ok(bytes)) = (
                path.file_name().and_then(|n| n.to_str()),
                std::fs::read(&path),
            ) else {
                continue;
            };
            files.insert(
                name.to_string(),
                Value::String(base64::engine::general_purpose::STANDARD.encode(bytes)),
            );
        }
    }
    json!({
        "manifest": read_json(&folder.join(ICONS_MANIFEST)).unwrap_or_else(|| json!({})),
        "files": files,
    })
}

/// Everything in one backup; `local` is what only the web views keep.
#[tauri::command]
pub fn export_backup(app: AppHandle, local: HashMap<String, String>) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let backup = json!({
        "bloom-backup": FORMAT,
        "version": app.package_info().version.to_string(),
        "settings": read_json(&dir.join("settings.json")).unwrap_or_else(|| json!({})),
        "pinned_apps": read_json(&dir.join("pinned_apps.json")).unwrap_or_else(|| json!([])),
        "custom_icons": export_icons(&dir),
        "window_positions": read_json(&dir.join("window_positions.json")).unwrap_or_else(|| json!({})),
        "local": local,
    });
    serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())
}

/// Whether a pinned app is on this PC: a program or file that is there, a
/// Store app (or other shell item) this PC's app list has, or Start.
fn on_this_pc(app: &Value, installed: &[String]) -> bool {
    let path = app.get("path").and_then(Value::as_str).unwrap_or_default();
    if path.is_empty() {
        return false;
    }
    if path == "start" {
        return true;
    }
    let bytes = path.as_bytes();
    let file = (bytes.len() > 2 && bytes[1] == b':') || path.starts_with("\\\\");
    if file {
        return Path::new(path).exists();
    }
    // Not read yet: kept rather than lost.
    installed.is_empty()
        || installed
            .iter()
            .any(|known| known.eq_ignore_ascii_case(path))
}

fn installed_paths() -> Vec<String> {
    crate::state::INSTALLED_APPS_CACHE
        .get()
        .and_then(|cache| {
            cache
                .lock()
                .ok()
                .map(|apps| apps.iter().map(|a| a.path.clone()).collect())
        })
        .unwrap_or_default()
}

fn import_icons(dir: &Path, icons: &Value) -> Result<(), String> {
    let folder = dir.join(ICONS_DIR);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    // The backup's icons replace the ones here.
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    if let Some(files) = icons.get("files").and_then(Value::as_object) {
        for (name, data) in files {
            // Only a plain file name that is a PNG.
            let plain = Path::new(name)
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|n| *n == name && n.ends_with(".png"));
            let (Some(name), Some(data)) = (plain, data.as_str()) else {
                continue;
            };
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                let _ = std::fs::write(folder.join(name), bytes);
            }
        }
    }
    let manifest = icons.get("manifest").cloned().unwrap_or_else(|| json!({}));
    std::fs::write(
        folder.join(ICONS_MANIFEST),
        serde_json::to_string(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Restored {
    /// What only the web views keep, for the settings page to put back.
    local: HashMap<String, String>,
    /// Pinned apps restored, and the names of ones not on this PC.
    pinned: usize,
    skipped: Vec<String>,
}

/// Restores a backup (or an older settings-only export). Bloom restarts
/// afterwards to take all of it up.
#[tauri::command]
pub fn import_backup(app: AppHandle, content: String) -> Result<Restored, String> {
    let backup: Value =
        serde_json::from_str(&content).map_err(|e| format!("Not a Bloom backup: {e}"))?;
    let mut restored = Restored {
        local: HashMap::new(),
        pinned: 0,
        skipped: Vec::new(),
    };
    if backup.get("bloom-backup").is_none() {
        // An export from before: its settings only.
        crate::commands::import_settings(app, content)?;
        return Ok(restored);
    }
    let dir = data_dir(&app)?;
    let settings = backup
        .get("settings")
        .filter(|value| value.is_object())
        .ok_or("The backup has no settings")?;
    crate::commands::import_settings(
        app.clone(),
        serde_json::to_string(settings).map_err(|e| e.to_string())?,
    )?;

    if let Some(apps) = backup.get("pinned_apps").and_then(Value::as_array) {
        let installed = installed_paths();
        let (kept, skipped): (Vec<&Value>, Vec<&Value>) =
            apps.iter().partition(|app| on_this_pc(app, &installed));
        restored.pinned = kept.len();
        restored.skipped = skipped
            .iter()
            .filter_map(|app| app.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        let text = serde_json::to_string(&kept).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("pinned_apps.json"), text).map_err(|e| e.to_string())?;
    }
    if let Some(icons) = backup.get("custom_icons").filter(|v| v.is_object()) {
        import_icons(&dir, icons)?;
    }
    if let Some(places) = backup.get("window_positions").filter(|v| v.is_object()) {
        let text = serde_json::to_string(places).map_err(|e| e.to_string())?;
        let _ = std::fs::write(dir.join("window_positions.json"), text);
    }
    if let Some(local) = backup.get("local").and_then(Value::as_object) {
        restored.local = local
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
            .collect();
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_apps_on_this_pc() {
        let installed = vec!["Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".to_string()];
        let app = |path: &str| json!({ "name": "x", "path": path });
        assert!(on_this_pc(&app("start"), &installed));
        assert!(on_this_pc(
            &app("microsoft.windowscalculator_8wekyb3d8bbwe!app"),
            &installed
        ));
        assert!(!on_this_pc(&app("Some.Other_123!App"), &installed));
        // The app list not read yet: kept.
        assert!(on_this_pc(&app("Some.Other_123!App"), &[]));
        assert!(!on_this_pc(&app("Z:\\nowhere\\missing.exe"), &installed));
        assert!(!on_this_pc(&app(""), &installed));
        let here = std::env::current_exe().unwrap();
        assert!(on_this_pc(&app(here.to_str().unwrap()), &installed) || cfg!(not(windows)));
    }
}
