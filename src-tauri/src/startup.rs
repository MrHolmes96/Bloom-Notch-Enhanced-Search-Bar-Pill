//! Starting with Windows, and handing Windows back on uninstall.
//!
//! "Launch at startup" used to be an entry under the `Run` key. Explorer starts
//! those late (after the desktop, and Windows holds desktop apps back a while
//! after sign-in), so the taskbar showed before Bloom replaced it. Bloom now
//! registers a Task Scheduler task with a sign-in trigger for this user, which
//! Windows starts as part of signing in, at normal priority (tasks default to
//! below normal). During its first half minute after such a start Bloom also
//! runs above normal priority, so it is up before the rest of the startup apps;
//! that changes the order work is done in, not how much is done. If the task
//! can't be registered, the `Run` entry is used as before.
//!
//! `bloom.exe --restore-windows` is what the uninstaller runs: a running Bloom
//! is asked to quit (which restores everything), and then the taskbar, its
//! auto-hide setting and position (from the crash marker), Windows' volume and
//! brightness pop-ups and its notification pop-ups are put back in any case,
//! and the startup task is removed.

use std::os::windows::process::CommandExt;
use std::process::Command;

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

const TASK_NAME: &str = "Bloom";
/// Passed by the startup task, so Bloom knows it is starting with Windows.
pub const AUTOSTART_ARG: &str = "--autostart";
pub const RESTORE_ARG: &str = "--restore-windows";
/// Named event a running Bloom waits on to quit (the uninstaller and the
/// installer set it).
pub const QUIT_EVENT: &str = "BloomQuit";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn schtasks(args: &[&str]) -> Option<std::process::Output> {
    let system = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    Command::new(format!("{system}\\System32\\schtasks.exe"))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
}

fn task_xml() -> Option<String> {
    let output = schtasks(&["/Query", "/TN", TASK_NAME, "/XML"])?;
    if !output.status.success() {
        return None;
    }
    // schtasks prints the task as UTF-16 when it can, else in the console's
    // code page; both contain the plain path text this needs.
    let bytes = output.stdout;
    let utf16 = bytes.len() >= 2 && (bytes[0] == 0xFF && bytes[1] == 0xFE || bytes[1] == 0);
    Some(if utf16 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn task_exists() -> bool {
    schtasks(&["/Query", "/TN", TASK_NAME])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The task: at this user's sign-in, run Bloom at normal priority, never
/// stopped for battery or after a time limit, one copy only.
pub fn task_definition(user: &str, exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Starts Bloom when you sign in, ahead of other startup apps.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>false</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>"{exe}"</Command>
      <Arguments>{arg}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        user = escape(user),
        exe = escape(exe),
        arg = AUTOSTART_ARG,
    )
}

/// Registers (or replaces) the startup task for this user.
fn create_task() -> bool {
    let (Some(user), Ok(exe)) = (crate::utils::current_user_sid(), std::env::current_exe()) else {
        return false;
    };
    let xml = task_definition(&user, &exe.to_string_lossy());
    // schtasks wants the definition as UTF-16 with a byte order mark.
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let file = std::env::temp_dir().join("bloom-startup-task.xml");
    if std::fs::write(&file, bytes).is_err() {
        return false;
    }
    let path = file.to_string_lossy().into_owned();
    let created = schtasks(&["/Create", "/TN", TASK_NAME, "/XML", &path, "/F"])
        .map(|o| o.status.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(file);
    created
}

fn delete_task() {
    let _ = schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]);
}

/// Whether Bloom starts with Windows (by its task, or the older `Run` entry).
pub fn is_enabled(app: &AppHandle) -> bool {
    task_exists() || app.autolaunch().is_enabled().unwrap_or(false)
}

/// Turns starting with Windows on or off. On uses the startup task, falling
/// back to the `Run` entry when the task can't be registered.
pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    if !enabled {
        delete_task();
        let _ = autolaunch.disable();
        return Ok(());
    }
    if create_task() {
        // Only one way of starting, or the second start opens Settings.
        let _ = autolaunch.disable();
        Ok(())
    } else {
        autolaunch.enable().map_err(|e| e.to_string())
    }
}

/// Moves an existing "launch at startup" over to the startup task, and keeps
/// the task pointing at this copy of Bloom (after it was moved or reinstalled).
pub fn migrate(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if let Some(xml) = task_xml() {
            if !xml.to_lowercase().contains(&exe) {
                let _ = create_task();
            }
            let _ = app.autolaunch().disable();
            return;
        }
        if app.autolaunch().is_enabled().unwrap_or(false) && create_task() {
            let _ = app.autolaunch().disable();
        }
    });
}

#[tauri::command]
pub async fn startup_enabled(app: AppHandle) -> bool {
    tauri::async_runtime::spawn_blocking(move || is_enabled(&app))
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn startup_set(app: AppHandle, enabled: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        set_enabled(&app, enabled)?;
        Ok(is_enabled(&app))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// After a start by the startup task: above normal priority for the first
/// half minute, so Bloom is ready before the other startup apps, then normal.
pub fn boost_startup() {
    use windows::Win32::System::Threading::{
        GetCurrentProcess, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
    };
    unsafe {
        let _ = SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS);
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(30));
        unsafe {
            let _ = SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
        }
    });
}

/// Waits (on its own thread) for the uninstaller or installer to ask Bloom
/// to quit, then quits the normal way, which restores everything.
pub fn listen_for_quit(app: AppHandle) {
    std::thread::spawn(move || unsafe {
        use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
        let name: Vec<u16> = QUIT_EVENT
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let Ok(event) = CreateEventW(None, true, false, windows::core::PCWSTR(name.as_ptr()))
        else {
            return;
        };
        if WaitForSingleObject(event, INFINITE) == windows::Win32::Foundation::WAIT_OBJECT_0 {
            crate::commands::restore_taskbar_and_exit(&app);
        }
    });
}

/// `bloom.exe --restore-windows`: asks a running Bloom to quit, waits for it,
/// then puts back whatever is still changed and removes the startup task.
pub fn restore_windows() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenEventW, OpenMutexW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
        SYNCHRONIZATION_SYNCHRONIZE,
    };
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    unsafe {
        let quit = wide(QUIT_EVENT);
        if let Ok(event) = OpenEventW(
            EVENT_MODIFY_STATE,
            false,
            windows::core::PCWSTR(quit.as_ptr()),
        ) {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
        }
        // The running Bloom holds this until it has exited.
        let mutex = wide("BloomSingleInstance");
        if let Ok(handle) = OpenMutexW(
            SYNCHRONIZATION_SYNCHRONIZE,
            false,
            windows::core::PCWSTR(mutex.as_ptr()),
        ) {
            // Signalled (or abandoned) once it has exited; given up after
            // 10 s, in which case the restore below still runs.
            let _ = WaitForSingleObject(handle, 10_000);
            let _ = CloseHandle(handle);
        }
    }
    delete_task();
    crate::utils::restore_windows_now();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_runs_bloom_at_sign_in_at_normal_priority() {
        let xml = task_definition("S-1-5-21-1-2-3-1001", "C:\\Users\\K & K\\bloom.exe");
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains("<UserId>S-1-5-21-1-2-3-1001</UserId>"));
        assert!(xml.contains("<Priority>4</Priority>"));
        assert!(xml.contains("<Command>\"C:\\Users\\K &amp; K\\bloom.exe\"</Command>"));
        assert!(xml.contains("<Arguments>--autostart</Arguments>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
    }
}
