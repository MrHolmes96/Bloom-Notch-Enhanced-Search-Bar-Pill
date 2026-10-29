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
/// Named event the installer sets before updating Bloom: a running Bloom
/// hands its hidden taskbar to the new version instead of showing it.
pub const UPDATE_EVENT: &str = "BloomUpdate";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn schtasks(args: &[&str]) -> Option<std::process::Output> {
    let system = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    Command::new(format!("{system}\\System32\\schtasks.exe"))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
}

pub(crate) fn task_xml() -> Option<String> {
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

pub(crate) fn task_exists() -> bool {
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
pub(crate) fn create_task() -> bool {
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

pub(crate) fn delete_task() {
    let _ = schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]);
}

/// Whether Bloom starts with Windows (by its task, or the older `Run` entry).
pub fn is_enabled(app: &AppHandle) -> bool {
    task_exists() || app.autolaunch().is_enabled().unwrap_or(false)
}

/// Whether the user wants Bloom to start with Windows, as they last chose it
/// (in Settings, by Bloom Recovery, or `--enable` / `--disable`). Kept in
/// Bloom's data folder, apart from the task itself, so a task that goes
/// missing is noticed and made again (see `keep`).
const WANTED_FILE: &str = "start-with-windows.txt";

fn wanted() -> Option<bool> {
    let text = std::fs::read_to_string(crate::utils::data_file(WANTED_FILE)?).ok()?;
    match text.trim() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

pub(crate) fn set_wanted(on: bool) {
    if let Some(path) = crate::utils::data_file(WANTED_FILE) {
        let _ = std::fs::write(path, if on { "on" } else { "off" });
    }
}

/// Turns starting with Windows on or off. On uses the startup task, falling
/// back to the `Run` entry when the task can't be registered.
pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    set_wanted(enabled);
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

/// Makes sure Bloom starts with Windows as the user chose: the startup task
/// is there and starts this copy of Bloom (after it was moved or
/// reinstalled), an older `Run` entry is moved over to it, and a task that
/// went missing is made again. Returns what it did, if anything.
fn ensure(app: &AppHandle) -> Option<&'static str> {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let run_entry = app.autolaunch().is_enabled().unwrap_or(false);
    let xml = task_xml();
    // First start with this: what is in place now is what the user chose.
    let want = wanted().unwrap_or_else(|| {
        let on = xml.is_some() || run_entry;
        set_wanted(on);
        on
    });
    if !want {
        return None;
    }
    match xml {
        Some(xml) => {
            if run_entry {
                let _ = app.autolaunch().disable();
            }
            if !xml.to_lowercase().contains(&exe) && create_task() {
                return Some("Startup task pointed at another copy of Bloom: set to this one");
            }
            None
        }
        None => {
            if create_task() {
                let _ = app.autolaunch().disable();
                Some("Startup task was missing: made again")
            } else if !run_entry {
                let _ = app.autolaunch().enable();
                Some("Startup task couldn't be made: using the Run entry instead")
            } else {
                None
            }
        }
    }
}

/// Keeps starting with Windows in place while Bloom runs: checked shortly
/// after Bloom starts, then every 15 minutes (one schtasks query, a few
/// milliseconds), so whatever removes the task, Bloom still starts at the
/// next sign-in.
pub fn keep(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("bloom-keep-startup".into())
        .spawn(move || {
            // After the notch and dock are up: this runs schtasks.
            std::thread::sleep(std::time::Duration::from_secs(6));
            loop {
                if crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                if let Some(done) = ensure(&app) {
                    crate::utils::note_crash(done);
                }
                std::thread::sleep(std::time::Duration::from_secs(15 * 60));
            }
        })
        .ok();
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
/// Its web view processes (which draw the notch and the dock) get the same:
/// Windows starts them at normal priority whatever Bloom's is. This changes
/// the order work is done in at sign-in, not how much is done.
pub fn boost_startup() {
    use windows::Win32::System::Threading::{
        GetCurrentProcess, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
    };
    unsafe {
        let _ = SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS);
    }
    std::thread::spawn(|| {
        let started = std::time::Instant::now();
        let mut boosted: Vec<u32> = Vec::new();
        while started.elapsed() < std::time::Duration::from_secs(30) {
            for pid in descendants(std::process::id()) {
                if !boosted.contains(&pid) && set_priority(pid, ABOVE_NORMAL_PRIORITY_CLASS) {
                    boosted.push(pid);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        unsafe {
            let _ = SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
        }
        // Only the ones still Bloom's (a process id can be reused).
        let still = descendants(std::process::id());
        for pid in boosted.into_iter().filter(|pid| still.contains(pid)) {
            set_priority(pid, NORMAL_PRIORITY_CLASS);
        }
    });
}

/// The processes started by `root`, and the ones they started, and so on.
fn descendants(root: u32) -> Vec<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return Vec::new();
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    descendants_of(root, &pairs)
}

/// From (process, parent) pairs, every process below `root`.
fn descendants_of(root: u32, pairs: &[(u32, u32)]) -> Vec<u32> {
    let mut found: Vec<u32> = Vec::new();
    let mut frontier = vec![root];
    while let Some(parent) = frontier.pop() {
        for &(pid, of) in pairs {
            if of == parent && pid != root && pid != 0 && !found.contains(&pid) {
                found.push(pid);
                frontier.push(pid);
            }
        }
    }
    found
}

fn set_priority(
    pid: u32,
    class: windows::Win32::System::Threading::PROCESS_CREATION_FLAGS,
) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, SetPriorityClass, PROCESS_SET_INFORMATION,
    };
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_SET_INFORMATION, false, pid) else {
            return false;
        };
        let done = SetPriorityClass(process, class).is_ok();
        let _ = CloseHandle(process);
        done
    }
}

/// Waits (on its own thread) for the uninstaller or installer to ask Bloom
/// to quit, then quits the normal way, which restores everything.
pub fn listen_for_quit(app: AppHandle) {
    let for_update = app.clone();
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
    let app = for_update;
    std::thread::spawn(move || unsafe {
        use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
        let name: Vec<u16> = UPDATE_EVENT
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let Ok(event) = CreateEventW(None, true, false, windows::core::PCWSTR(name.as_ptr()))
        else {
            return;
        };
        if WaitForSingleObject(event, INFINITE) == windows::Win32::Foundation::WAIT_OBJECT_0 {
            hand_over_for_update(&app);
        }
    });
}

/// The installer is about to update Bloom: the hidden taskbar is handed to
/// the new version (which takes it over when it starts) instead of being
/// shown. A small copy of Bloom outside its install folder watches over it
/// (the install folder must be free to be updated) and shows the taskbar
/// after all should no Bloom take it over within 3 minutes.
fn hand_over_for_update(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    if crate::state::NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed) {
        crate::utils::hand_over_taskbar("update");
        if let (Ok(exe), Some(marker)) = (std::env::current_exe(), crate::utils::taskbar_marker()) {
            let copy =
                std::env::temp_dir().join(format!("bloom-update-watch-{}.exe", std::process::id()));
            if std::fs::copy(&exe, &copy).is_ok() {
                let _ = Command::new(&copy)
                    .arg("--update-watchdog")
                    .arg(marker)
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn();
            }
        }
    }
    crate::commands::restore_taskbar_and_exit(app);
}

/// Removes the update watchers' copies left in the temp folder by earlier
/// updates (each deletes nothing of itself while it runs).
pub fn clean_update_watchers() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("bloom-update-watch-") && name.ends_with(".exe") {
                let _ = std::fs::remove_file(entry.path());
            }
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
    fn finds_every_process_below_bloom() {
        // Bloom 10 started a browser 20, which started renderers 21 and 22;
        // 30 is unrelated, and a reused id can't loop back to Bloom.
        let pairs = [(10, 1), (20, 10), (21, 20), (22, 20), (30, 1), (10, 22)];
        let mut found = descendants_of(10, &pairs);
        found.sort();
        assert_eq!(found, vec![20, 21, 22]);
        assert!(descendants_of(30, &pairs).is_empty());
    }

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
