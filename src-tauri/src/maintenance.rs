//! Keeping Bloom's hold on Windows healthy, and handing Windows back.
//!
//! Bloom runs alongside Explorer (Explorer stays Windows' shell, so every
//! native feature keeps working) and takes over its taskbar while it runs.
//! These are the ways that hold is repaired, given up, and looked into, from
//! the command line (also the Start menu's "Bloom Recovery"), from Settings >
//! About, and on their own after a Windows update:
//!
//! - `--recovery`: Windows as it was, now: a running Bloom is asked to quit,
//!   the taskbar (with its own auto-hide setting and place), Windows' volume,
//!   brightness and notification pop-ups and its task list come back, Bloom
//!   no longer starts with Windows, and a left-over "Shell" policy that would
//!   keep Explorer from being the shell is removed. Bloom stays installed.
//! - `--disable` / `--enable`: Bloom off (as recovery, quietly), or on again
//!   (starts with Windows, and starts now).
//! - `--repair`: puts back what Bloom needs: its startup task, pointing at
//!   this copy, and Explorer as the shell. Run by itself once after Windows
//!   was updated to a new build, since feature updates can reset such things.
//! - `--export-logs`: Bloom's logs and a health report in one zip on the
//!   desktop, to see what went wrong (no passwords, sign-in tokens or
//!   notification texts).

use std::path::PathBuf;

use tauri::AppHandle;

/// Policies' per-user "Shell" (Group Policy's Custom User Interface).
const POLICY_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System";
const WINLOGON_KEY: &str = "Software\\Microsoft\\Windows NT\\CurrentVersion\\Winlogon";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn read_string(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    value: &str,
) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};
    let key = wide(key);
    let value = wide(value);
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            root,
            windows::core::PCWSTR(key.as_ptr()),
            windows::core::PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    let length = (size as usize / 2).min(buffer.len());
    Some(
        String::from_utf16_lossy(&buffer[..length])
            .trim_end_matches('\0')
            .to_string(),
    )
}

fn is_explorer(shell: &str) -> bool {
    let file = shell
        .trim()
        .trim_matches('"')
        .rsplit('\\')
        .next()
        .unwrap_or_default()
        .to_lowercase();
    file == "explorer.exe" || file == "explorer"
}

/// The shell Windows starts at sign-in, as (per-user policy, machine value).
fn shell_values() -> (Option<String>, Option<String>) {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    (
        read_string(HKEY_CURRENT_USER, POLICY_KEY, "Shell"),
        read_string(HKEY_LOCAL_MACHINE, WINLOGON_KEY, "Shell"),
    )
}

/// Removes a per-user "Shell" policy that isn't Explorer: Bloom needs
/// Explorer as Windows' shell. The machine-wide value needs an
/// administrator, so it is only reported.
fn make_explorer_the_shell() -> Option<String> {
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};
    let (policy, _) = shell_values();
    let policy = policy.filter(|shell| !shell.trim().is_empty() && !is_explorer(shell))?;
    let key = wide(POLICY_KEY);
    let value = wide("Shell");
    let removed = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(key.as_ptr()),
            windows::core::PCWSTR(value.as_ptr()),
        )
        .is_ok()
    };
    Some(if removed {
        format!("Removed a Shell policy that started \"{policy}\" instead of Explorer")
    } else {
        format!("A Shell policy starts \"{policy}\" instead of Explorer and couldn't be removed")
    })
}

/// Windows' build, as "26300.1234" (build and update revision).
pub fn windows_build() -> String {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD};
    let key = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let build = read_string(HKEY_LOCAL_MACHINE, key, "CurrentBuild").unwrap_or_default();
    let mut revision = 0u32;
    let mut size = 4u32;
    let key = wide(key);
    let value = wide("UBR");
    unsafe {
        let _ = RegGetValueW(
            HKEY_LOCAL_MACHINE,
            windows::core::PCWSTR(key.as_ptr()),
            windows::core::PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut revision as *mut u32 as *mut _),
            Some(&mut size),
        );
    }
    format!("{build}.{revision}")
}

fn windows_edition() -> String {
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let key = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let name = read_string(HKEY_LOCAL_MACHINE, key, "ProductName").unwrap_or_default();
    let version = read_string(HKEY_LOCAL_MACHINE, key, "DisplayVersion").unwrap_or_default();
    format!("{name} {version}").trim().to_string()
}

/// Puts back what Bloom needs to start and run as it should. Returns what
/// was done, one line each.
pub fn repair() -> Vec<String> {
    let mut done = Vec::new();
    if let Some(note) = make_explorer_the_shell() {
        done.push(note);
    }
    // Starting with Windows: kept as it was, but pointing at this copy.
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if let Some(xml) = crate::startup::task_xml() {
        let current =
            xml.to_lowercase().contains(&exe) && !xml.contains("<Enabled>false</Enabled>");
        if !current && crate::startup::create_task() {
            done.push("Set the startup task to start this copy of Bloom again".into());
        }
    }
    if let Some(path) = crate::utils::data_file("windows-build.txt") {
        let _ = std::fs::write(path, windows_build());
    }
    if done.is_empty() {
        done.push("Nothing needed repairing".into());
    }
    for line in &done {
        crate::utils::note_crash(&format!("Repair: {line}"));
    }
    done
}

/// At start: Windows updated to another build since Bloom last ran repairs
/// what a feature update may have reset (once per build).
pub fn repair_after_windows_update() {
    std::thread::spawn(|| {
        // After the notch and dock are up (this runs schtasks).
        std::thread::sleep(std::time::Duration::from_secs(10));
        let Some(path) = crate::utils::data_file("windows-build.txt") else {
            return;
        };
        let now = windows_build();
        match std::fs::read_to_string(&path) {
            Ok(before) if before.trim() == now => {}
            Ok(before) => {
                crate::utils::note_crash(&format!(
                    "Windows was updated from {} to {now}: repairing",
                    before.trim()
                ));
                repair();
            }
            // First start: just noted.
            Err(_) => {
                let _ = std::fs::write(&path, now);
            }
        }
    });
}

fn message(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
    let text = wide(text);
    let title = wide("Bloom");
    unsafe {
        MessageBoxW(
            None,
            windows::core::PCWSTR(text.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

/// `--recovery`: Windows as it was, at once. Also from the Start menu.
pub fn recovery() {
    crate::startup::restore_windows();
    let mut lines = vec![
        "Windows' taskbar and pop-ups are back, and Bloom no longer starts with Windows."
            .to_string(),
    ];
    if let Some(note) = make_explorer_the_shell() {
        lines.push(note);
    }
    crate::utils::note_crash("Recovery: Windows handed back, startup turned off");
    lines.push("Start Bloom from the Start menu to use it again.".into());
    message(&lines.join("\n\n"));
}

/// `--disable`: Bloom off and not starting with Windows (quietly).
pub fn disable() {
    crate::startup::restore_windows();
    crate::utils::note_crash("Disabled from the command line");
}

/// `--enable`: Bloom starts with Windows again, and starts now.
pub fn enable() {
    let _ = crate::startup::create_task();
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg(crate::startup::AUTOSTART_ARG)
            .spawn();
    }
}

/// What Bloom's parts are doing now, for the diagnostics.
fn health(app: Option<&AppHandle>) -> String {
    let (policy, machine) = shell_values();
    let shell_ok =
        policy.as_deref().is_none_or(is_explorer) && machine.as_deref().is_none_or(is_explorer);
    let state = |ok: bool| if ok { "Healthy" } else { "Failed" };
    let mut lines = vec![
        format!("Bloom {}", env!("CARGO_PKG_VERSION")),
        format!("Windows {} (build {})", windows_edition(), windows_build()),
        format!(
            "Shell: {} (policy: {}, machine: {})",
            if shell_ok { "Explorer" } else { "NOT Explorer" },
            policy.as_deref().unwrap_or("none"),
            machine.as_deref().unwrap_or("none")
        ),
        format!(
            "Starts with Windows: {}",
            if crate::startup::task_exists() {
                "yes (startup task)"
            } else {
                "no task"
            }
        ),
    ];
    if app.is_some() {
        let tray = crate::tray_host::window() != 0;
        let tasks = crate::taskbar_progress::window() != 0;
        lines.push(format!("Tray host: {}", state(tray)));
        lines.push(format!("Task list: {}", state(tasks)));
        lines.push(format!(
            "Windows' taskbar hidden: {}",
            crate::state::NATIVE_TASKBAR_HIDDEN.load(std::sync::atomic::Ordering::Relaxed)
        ));
        lines.push(format!(
            "Full-screen app in front: {}",
            crate::state::CURRENT_FOREGROUND_FULLSCREEN.load(std::sync::atomic::Ordering::Relaxed)
        ));
        lines.push(format!(
            "Notifications: {}",
            crate::notifications::notifications_status()
        ));
    }
    lines.join("\r\n")
}

/// Windows' own shell windows (notification center, quick settings, Start,
/// pop-ups), as they are now: process, class, title, whether hidden. Only
/// what each window is called, never what it shows. Used to find which
/// window is which on a Windows build that moved them.
fn class_name_of(hwnd: windows::Win32::Foundation::HWND) -> String {
    let mut class = [0u16; 128];
    let len = unsafe { windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut class) }
        as usize;
    String::from_utf16_lossy(&class[..len.min(128)])
}

fn shell_windows() -> String {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible,
    };
    unsafe extern "system" fn visit(hwnd: HWND, lines: LPARAM) -> BOOL {
        let lines = &mut *(lines.0 as *mut Vec<String>);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let program = crate::commands::process_image_path(pid)
            .and_then(|p| p.rsplit('\\').next().map(str::to_lowercase))
            .unwrap_or_default();
        // Every program under the Windows folder (Windows' own UI moves
        // between programs from one build to the next).
        let root = std::env::var("SystemRoot")
            .unwrap_or_else(|_| "C:\\Windows".into())
            .to_lowercase();
        let in_windows = crate::commands::process_image_path(pid)
            .is_some_and(|p| p.to_lowercase().starts_with(&root));
        let skip = matches!(
            class_name_of(hwnd).as_str(),
            "IME" | "MSCTFIME UI" | "WorkerW"
        );
        if !in_windows || skip {
            return true.into();
        }
        let mut class = [0u16; 128];
        let class_len = GetClassNameW(hwnd, &mut class) as usize;
        let mut title = [0u16; 128];
        let title_len = GetWindowTextW(hwnd, &mut title) as usize;
        let mut cloaked = 0u32;
        let _ = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            4,
        );
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        lines.push(format!(
            "{program} | {} | \"{}\" | visible={} cloaked={} | {}x{}",
            String::from_utf16_lossy(&class[..class_len.min(128)]),
            String::from_utf16_lossy(&title[..title_len.min(128)]),
            IsWindowVisible(hwnd).as_bool(),
            cloaked != 0,
            rect.right - rect.left,
            rect.bottom - rect.top
        ));
        true.into()
    }
    let mut lines: Vec<String> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut lines as *mut _ as isize));
    }
    lines.join("\r\n")
}

/// Bloom's logs and a health report, zipped on the desktop. Returns its path.
pub fn export_logs(app: Option<&AppHandle>) -> Result<PathBuf, String> {
    use std::io::Write;
    let data = crate::utils::data_file("crash.log")
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .ok_or("Bloom's data folder isn't there")?;
    let desktop = std::env::var_os("USERPROFILE")
        .map(|home| PathBuf::from(home).join("Desktop"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let path = desktop.join(format!(
        "Bloom diagnostics {:04}-{:02}-{:02} {:02}.{:02}.zip",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute
    ));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("health.txt", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(health(app).as_bytes())
        .map_err(|e| e.to_string())?;
    if zip.start_file("shell-windows.txt", options).is_ok() {
        let _ = zip.write_all(shell_windows().as_bytes());
    }
    if zip.start_file("shell-events.txt", options).is_ok() {
        let _ = zip.write_all(crate::shell_probe::report().as_bytes());
    }
    // The logs only: never settings.json (sign-in tokens), and nothing that
    // holds what notifications said.
    for name in [
        "crash.log",
        "focus.log",
        "taskbar-progress.log",
        "notification-clicks.log",
        "ghelper.log",
        "tray-app-hidden.txt",
        "tray-icons.txt",
        "restarts.txt",
        "windows-build.txt",
        "taskbar_hidden.flag",
    ] {
        if let Ok(bytes) = std::fs::read(data.join(name)) {
            if zip.start_file(name, options).is_ok() {
                let _ = zip.write_all(&bytes);
            }
        }
    }
    // G-Helper's own log, its last 300 lines (which keys it got, which mode
    // it set), when it is installed: to see whether a shortcut reached it.
    if let Some(log) = std::env::var_os("APPDATA")
        .map(|dir| PathBuf::from(dir).join("GHelper").join("log.txt"))
        .and_then(|file| std::fs::read(file).ok())
    {
        let text = String::from_utf8_lossy(&log);
        let lines: Vec<&str> = text.lines().collect();
        let tail = lines[lines.len().saturating_sub(300)..].join("\n");
        if zip.start_file("ghelper-app-log.txt", options).is_ok() {
            let _ = zip.write_all(tail.as_bytes());
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

/// Handles Bloom's maintenance switches. Returns true when one was given
/// (then Bloom itself doesn't start).
pub fn run_command(arg: &std::ffi::OsStr, rest: &[std::ffi::OsString]) -> bool {
    match arg.to_string_lossy().to_lowercase().as_str() {
        "--recovery" | "/recovery" => recovery(),
        "--disable" | "/disable" => disable(),
        "--enable" | "/enable" => enable(),
        "--repair" | "/repair" => {
            let done = repair();
            message(&format!("Bloom repaired:\n\n{}", done.join("\n")));
        }
        "--export-logs" | "/exportlogs" => match export_logs(None) {
            Ok(path) => message(&format!("Saved {}", path.display())),
            Err(e) => message(&format!("Couldn't save the diagnostics: {e}")),
        },
        // Run as administrator once, to set up the touchscreen tasks.
        arg if arg == crate::touchscreen::REGISTER_ARG => {
            let done = crate::touchscreen::register_tasks(rest);
            std::process::exit(if done { 0 } else { 1 });
        }
        // Run as administrator by one of those tasks: the touchscreen off/on.
        arg if arg == crate::touchscreen::DEVICE_ARG => {
            let done = crate::touchscreen::set_devices(rest);
            std::process::exit(if done { 0 } else { 1 });
        }
        "--update-watchdog" => {
            if let Some(marker) = rest.first() {
                crate::utils::run_update_watchdog(PathBuf::from(marker));
            }
        }
        _ => return false,
    }
    true
}

/// Settings > About > Export Diagnostics.
#[tauri::command]
pub async fn diagnostics_export(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = export_logs(Some(&app))?;
        let _ = crate::launcher::actions::shell_execute(
            "explorer.exe",
            Some(&format!("/select,\"{}\"", path.display())),
            None,
            None,
            true,
        );
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Settings > About > Repair Bloom.
#[tauri::command]
pub async fn bloom_repair() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(repair)
        .await
        .unwrap_or_default()
}
