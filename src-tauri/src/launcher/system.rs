//! System commands (shut down, lock, sleep, empty the Recycle Bin, ...) and
//! the process killer.

use serde::Serialize;
use std::collections::HashMap;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};

use super::actions::shell_execute;

/// Runs one of the search bar's system commands.
#[tauri::command]
pub async fn launcher_system_command(command: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || unsafe {
        match command.as_str() {
            "shutdown" => shell_execute("shutdown.exe", Some("/s /t 0"), None, None, false),
            "restart" => shell_execute("shutdown.exe", Some("/r /t 0"), None, None, false),
            "restart-advanced" => {
                shell_execute("shutdown.exe", Some("/r /o /t 0"), None, None, false)
            }
            "logoff" => {
                use windows::Win32::System::Shutdown::{
                    ExitWindowsEx, EWX_LOGOFF, SHUTDOWN_REASON,
                };
                ExitWindowsEx(EWX_LOGOFF, SHUTDOWN_REASON(0)).map_err(|e| e.message())
            }
            "lock" => {
                use windows::Win32::System::Shutdown::LockWorkStation;
                LockWorkStation().map_err(|e| e.message())
            }
            "sleep" | "hibernate" => {
                use windows::Win32::System::Power::SetSuspendState;
                if SetSuspendState(command == "hibernate", false, false) {
                    Ok(())
                } else {
                    Err(format!("Windows refused to {command}"))
                }
            }
            "empty-recycle-bin" => {
                use windows::Win32::UI::Shell::SHEmptyRecycleBinW;
                // Windows asks for confirmation itself.
                SHEmptyRecycleBinW(None, windows::core::PCWSTR::null(), 0).map_err(|_| {
                    "The Recycle Bin is already empty, or some items are in use".to_string()
                })
            }
            "open-recycle-bin" => shell_execute("shell:RecycleBinFolder", None, None, None, true),
            "indexing-options" => {
                shell_execute("control.exe", Some("srchadmin.dll"), None, None, true)
            }
            other => Err(format!("unknown command {other}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    /// Title of the process's main visible window, if it has one.
    pub title: Option<String>,
}

/// Processes nobody should kill from a search bar: Windows itself, the shell,
/// and Bloom.
const PROTECTED: &[&str] = &[
    "system",
    "registry",
    "idle",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsm.exe",
    "svchost.exe",
    "fontdrvhost.exe",
    "dwm.exe",
    "spoolsv.exe",
    "conhost.exe",
    "sihost.exe",
    "ctfmon.exe",
    "explorer.exe",
    "memory compression",
    "secure system",
    "msmpeng.exe",
];

fn window_titles() -> HashMap<u32, String> {
    use windows::core::BOOL;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
    };
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let titles = &mut *(lparam.0 as *mut HashMap<u32, String>);
        if !IsWindowVisible(hwnd).as_bool()
            || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.0.is_null())
        {
            return true.into();
        }
        let mut text = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut text);
        if len > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            titles
                .entry(pid)
                .or_insert_with(|| String::from_utf16_lossy(&text[..len as usize]));
        }
        true.into()
    }
    let mut titles: HashMap<u32, String> = HashMap::new();
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut titles as *mut HashMap<u32, String> as isize),
        );
    }
    titles
}

/// Running processes for the process killer.
#[tauri::command]
pub async fn launcher_processes() -> Vec<ProcessEntry> {
    tauri::async_runtime::spawn_blocking(|| unsafe {
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        let titles = window_titles();
        let own = std::process::id();
        let mut out = Vec::new();
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snapshot, &mut entry).is_ok();
        while ok {
            let pid = entry.th32ProcessID;
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            let lower = name.to_lowercase();
            if pid > 4 && pid != own && !PROTECTED.contains(&lower.as_str()) && lower != "bloom.exe"
            {
                out.push(ProcessEntry {
                    pid,
                    path: crate::commands::process_image_path(pid),
                    title: titles.get(&pid).cloned(),
                    name,
                });
            }
            ok = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        out
    })
    .await
    .unwrap_or_default()
}

/// Ends processes. Returns how many could not be ended (usually ones that
/// run as administrator).
#[tauri::command]
pub async fn launcher_kill(pids: Vec<u32>) -> u32 {
    tauri::async_runtime::spawn_blocking(move || unsafe {
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
        let own = std::process::id();
        let mut failed = 0;
        for pid in pids {
            if pid <= 4 || pid == own {
                failed += 1;
                continue;
            }
            match OpenProcess(PROCESS_TERMINATE, false, pid) {
                Ok(handle) => {
                    if TerminateProcess(handle, 1).is_err() {
                        failed += 1;
                    }
                    let _ = CloseHandle(handle);
                }
                Err(_) => failed += 1,
            }
        }
        failed
    })
    .await
    .unwrap_or(0)
}
