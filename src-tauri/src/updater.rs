//! Updates: this personal build never looks for or installs updates, so
//! nothing goes online for them (the original project's update server would
//! also replace this build and its changes). New builds come from the fork's
//! "personal-build" release, linked from Settings > About. The commands stay
//! so every window gets a plain "no update" answer.

use serde::Serialize;
use tauri::AppHandle;

#[derive(Clone, Default, Serialize)]
pub struct UpdateCheckResult {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[tauri::command]
pub async fn check_for_updates(_app: AppHandle, _force: bool) -> Result<UpdateCheckResult, String> {
    Ok(UpdateCheckResult::default())
}

#[tauri::command]
pub async fn install_update(_app: AppHandle) -> Result<(), String> {
    Err("this build doesn't install updates; new builds are on the personal-build release".into())
}

#[tauri::command]
pub fn get_update_state(_app: AppHandle) -> UpdateCheckResult {
    UpdateCheckResult::default()
}
