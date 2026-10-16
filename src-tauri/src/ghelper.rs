//! G-Helper (github.com/seerge/g-helper) handles the brightness keys on ASUS
//! laptops when Armoury Crate is gone. It moves the brightness by its own
//! `brightness_step` (10 unless changed in its config.json), so the clean way
//! to get finer brightness keys is to set that step, not to correct the
//! result afterwards.

use serde::Serialize;
use std::path::PathBuf;

fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let path = PathBuf::from(appdata).join("GHelper").join("config.json");
    path.is_file().then_some(path)
}

#[derive(Serialize)]
pub struct GHelperBrightness {
    /// G-Helper's current brightness key step in percent.
    step: u32,
    running: bool,
}

/// The running G-Helper's process id and program path.
fn running_ghelper() -> Option<(u32, String)> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        let mut ok = Process32FirstW(snapshot, &mut entry).is_ok();
        while ok {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("GHelper.exe") {
                let pid = entry.th32ProcessID;
                if let Some(path) = crate::commands::process_image_path(pid) {
                    found = Some((pid, path));
                    break;
                }
            }
            ok = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

fn read_config() -> Option<serde_json::Map<String, serde_json::Value>> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .as_object()
        .cloned()
}

/// G-Helper's brightness key step, or None when G-Helper isn't set up here.
#[tauri::command]
pub async fn ghelper_brightness() -> Option<GHelperBrightness> {
    tauri::async_runtime::spawn_blocking(|| {
        let config = read_config()?;
        let step = config
            .get("brightness_step")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;
        Some(GHelperBrightness {
            step,
            running: running_ghelper().is_some(),
        })
    })
    .await
    .ok()
    .flatten()
}

/// Sets G-Helper's brightness key step and restarts G-Helper so it takes
/// effect (G-Helper reads its config at start and would otherwise write the
/// old value back).
#[tauri::command]
pub async fn set_ghelper_brightness_step(step: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let step = step.clamp(1, 50);
        let path = config_path().ok_or("G-Helper's settings weren't found")?;
        let running = running_ghelper();

        // Stop G-Helper first so it can't save its old value over ours.
        if let Some((pid, _)) = &running {
            unsafe {
                use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
                use windows::Win32::System::Threading::{
                    OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
                    PROCESS_TERMINATE,
                };
                let stopped = match OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, *pid) {
                    Ok(handle) => {
                        let _ = TerminateProcess(handle, 0);
                        let done = WaitForSingleObject(handle, 3000) == WAIT_OBJECT_0;
                        let _ = CloseHandle(handle);
                        done
                    }
                    Err(_) => false,
                };
                if !stopped {
                    return Err(
                        "G-Helper is running as administrator, so Bloom can't restart it. Close G-Helper, then try again"
                            .to_string(),
                    );
                }
            }
        }

        let mut config = read_config().ok_or("G-Helper's settings couldn't be read")?;
        config.insert("brightness_step".into(), serde_json::Value::from(step));
        let text = serde_json::to_string_pretty(&serde_json::Value::Object(config))
            .map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("Couldn't save G-Helper's settings: {}", e))?;

        if let Some((_, exe)) = running {
            use windows::core::PCWSTR;
            use windows::Win32::UI::Shell::ShellExecuteW;
            use windows::Win32::UI::WindowsAndMessaging::SW_SHOWMINNOACTIVE;
            let file: Vec<u16> = exe.encode_utf16().chain(std::iter::once(0)).collect();
            let verb: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                ShellExecuteW(
                    None,
                    PCWSTR(verb.as_ptr()),
                    PCWSTR(file.as_ptr()),
                    PCWSTR::null(),
                    PCWSTR::null(),
                    SW_SHOWMINNOACTIVE,
                );
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
