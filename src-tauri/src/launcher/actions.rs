//! What search bar results do: open things with a verb, reveal them in
//! Explorer, copy or recycle files, and run shell commands.

use serde::Serialize;
use std::ffi::c_void;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC,
    SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Expands %VARIABLES% the way the shell does.
pub fn expand_env(s: &str) -> String {
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
    if !s.contains('%') {
        return s.to_string();
    }
    let source = wide(s);
    unsafe {
        let needed = ExpandEnvironmentStringsW(PCWSTR(source.as_ptr()), None);
        if needed == 0 {
            return s.to_string();
        }
        let mut buf = vec![0u16; needed as usize];
        let written = ExpandEnvironmentStringsW(PCWSTR(source.as_ptr()), Some(&mut buf));
        if written == 0 {
            return s.to_string();
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }
}

/// Runs `file` through the shell, like double-clicking it or typing it into
/// the Run dialog. `verb` is "open", "runas", "openas", "properties", ...
pub fn shell_execute(
    file: &str,
    args: Option<&str>,
    verb: Option<&str>,
    cwd: Option<&str>,
    show: bool,
) -> Result<(), String> {
    let file_w = wide(file);
    let args_w = args.filter(|a| !a.is_empty()).map(wide);
    let verb_w = verb.filter(|v| !v.is_empty()).map(wide);
    let cwd_w = cwd.filter(|c| !c.is_empty()).map(wide);
    let needs_idlist = matches!(verb, Some("properties") | Some("openas"));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC
            | SEE_MASK_FLAG_NO_UI
            | if needs_idlist {
                SEE_MASK_INVOKEIDLIST
            } else {
                0
            },
        lpVerb: verb_w
            .as_ref()
            .map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr())),
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: args_w
            .as_ref()
            .map_or(PCWSTR::null(), |a| PCWSTR(a.as_ptr())),
        lpDirectory: cwd_w
            .as_ref()
            .map_or(PCWSTR::null(), |c| PCWSTR(c.as_ptr())),
        nShow: if show { SW_SHOWNORMAL.0 } else { SW_HIDE.0 },
        ..Default::default()
    };
    // "Open with" shows its own picker; the flag above would suppress it.
    if verb == Some("openas") {
        info.fMask &= !SEE_MASK_FLAG_NO_UI;
    }
    // What opens comes to the front even though the search bar just hid.
    crate::foreground::grant();
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| e.message())
}

/// Splits "program arguments" at the first unquoted space.
pub fn split_command_line(command: &str) -> (String, String) {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return (rest[..end].to_string(), rest[end + 1..].trim().to_string());
        }
    }
    match command.split_once(' ') {
        Some((file, args)) => (file.to_string(), args.trim().to_string()),
        None => (command.to_string(), String::new()),
    }
}

/// Opens a file, folder, app, URL or settings page. `verb` picks how
/// ("runas" for administrator, "openas" for the Open with picker,
/// "properties", "edit", ...).
#[tauri::command]
pub async fn launcher_open(
    path: String,
    verb: Option<String>,
    args: Option<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = expand_env(path.trim());
        shell_execute(
            &path,
            args.as_deref(),
            verb.as_deref(),
            cwd.as_deref(),
            true,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Runs a settings or control panel command line such as
/// "%windir%\system32\rundll32.exe shell32.dll,Control_RunDLL desk.cpl" or
/// "ms-settings:display".
#[tauri::command]
pub async fn launcher_run_command_line(command: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let command = command.trim();
        if command.contains(":") && !command.contains(' ') && !command.contains('\\') {
            // A URI: ms-settings:, shell:, ms-availablenetworks: ...
            return shell_execute(command, None, None, None, true);
        }
        let (file, args) = split_command_line(command);
        shell_execute(
            &expand_env(&file),
            Some(&expand_env(&args)),
            None,
            None,
            true,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens the folder that contains `path` with the item selected.
#[tauri::command]
pub async fn launcher_reveal(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || unsafe {
        use windows::Win32::System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::{ILFree, SHOpenFolderAndSelectItems, SHParseDisplayName};
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let path_w = wide(&expand_env(&path));
        let mut pidl = std::ptr::null_mut();
        let result = SHParseDisplayName(PCWSTR(path_w.as_ptr()), None, &mut pidl, 0, None)
            .and_then(|_| SHOpenFolderAndSelectItems(pidl, None, 0))
            .map_err(|e| e.message());
        if !pidl.is_null() {
            ILFree(Some(pidl));
        }
        if com {
            CoUninitialize();
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Puts files on the clipboard the way Explorer's Copy does, so they can be
/// pasted into a folder.
#[tauri::command]
pub async fn launcher_copy_files(paths: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || unsafe {
        use windows::Win32::Foundation::{HANDLE, HGLOBAL};
        use windows::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
        };
        use windows::Win32::System::Memory::{
            GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
        };
        use windows::Win32::System::Ole::CF_HDROP;
        use windows::Win32::UI::Shell::DROPFILES;

        let mut list: Vec<u16> = Vec::new();
        for path in &paths {
            list.extend(path.encode_utf16());
            list.push(0);
        }
        list.push(0);
        let header = std::mem::size_of::<DROPFILES>();
        let bytes = header + list.len() * 2;
        let memory: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| e.message())?;
        let ptr = GlobalLock(memory) as *mut u8;
        if ptr.is_null() {
            return Err("could not lock clipboard memory".into());
        }
        let drop_files = DROPFILES {
            pFiles: header as u32,
            fWide: true.into(),
            ..Default::default()
        };
        std::ptr::copy_nonoverlapping(&drop_files as *const DROPFILES as *const u8, ptr, header);
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, ptr.add(header), list.len() * 2);
        let _ = GlobalUnlock(memory);

        OpenClipboard(None).map_err(|e| e.message())?;
        let _ = EmptyClipboard();
        let result = SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(memory.0)))
            .map(|_| ())
            .map_err(|e| e.message());
        let _ = CloseClipboard();
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Moves a file or folder to the Recycle Bin.
#[tauri::command]
pub async fn launcher_recycle(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || unsafe {
        use windows::Win32::UI::Shell::{
            SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
            FO_DELETE, SHFILEOPSTRUCTW,
        };
        let mut from: Vec<u16> = path.encode_utf16().collect();
        from.push(0);
        from.push(0);
        let mut op = SHFILEOPSTRUCTW {
            wFunc: FO_DELETE,
            pFrom: PCWSTR(from.as_ptr()),
            fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI).0 as u16,
            ..Default::default()
        };
        let code = SHFileOperationW(&mut op);
        if code == 0 && !op.fAnyOperationsAborted.as_bool() {
            Ok(())
        } else {
            Err(format!("could not move to the Recycle Bin (error {code})"))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Runs a command typed after the shell keyword.
///
/// - `shell`: "cmd", "powershell", "pwsh" or "run" (like the Run dialog).
/// - `after`: "close", "pause" (wait for a key) or "keep" (leave it open).
#[tauri::command]
pub async fn launcher_run_shell(
    command: String,
    shell: String,
    after: String,
    admin: bool,
    terminal: bool,
    cwd: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let command = command.trim().to_string();
        if command.is_empty() {
            return Ok(());
        }
        let verb = admin.then_some("runas");
        let cwd = cwd.map(|c| expand_env(&c));
        if shell == "run" {
            let (file, args) = split_command_line(&expand_env(&command));
            return shell_execute(&file, Some(&args), verb, cwd.as_deref(), true);
        }
        let (program, args) = match shell.as_str() {
            "powershell" | "pwsh" => {
                let exe = if shell == "pwsh" {
                    "pwsh.exe"
                } else {
                    "powershell.exe"
                };
                let escaped = command.replace('"', "\\\"");
                let script = match after.as_str() {
                    "pause" => format!("{escaped}; Write-Host; Read-Host 'Press Enter to close'"),
                    _ => escaped,
                };
                let no_exit = if after == "keep" { "-NoExit " } else { "" };
                (exe, format!("{no_exit}-Command \"{script}\""))
            }
            _ => {
                let args = match after.as_str() {
                    "keep" => format!("/k {command}"),
                    "pause" => format!("/c {command} & pause"),
                    _ => format!("/c {command}"),
                };
                ("cmd.exe", args)
            }
        };
        if terminal {
            let args = format!("{program} {args}");
            if shell_execute("wt.exe", Some(&args), verb, cwd.as_deref(), true).is_ok() {
                return Ok(());
            }
        }
        shell_execute(program, Some(&args), verb, cwd.as_deref(), true)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct FileInfo {
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    /// Milliseconds since the Unix epoch.
    pub modified: Option<i64>,
    pub created: Option<i64>,
}

fn to_ms(time: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    time.ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as i64)
}

/// Size and dates for the preview panel.
#[tauri::command]
pub async fn launcher_file_info(path: String) -> Option<FileInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let meta = std::fs::metadata(&path).ok()?;
        Some(FileInfo {
            is_dir: meta.is_dir(),
            size: (!meta.is_dir()).then_some(meta.len()),
            modified: to_ms(meta.modified()),
            created: to_ms(meta.created()),
            path,
        })
    })
    .await
    .ok()
    .flatten()
}

#[derive(Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

/// Lists a folder for path search ("C:\Users\..."). Folders first.
#[tauri::command]
pub async fn launcher_list_dir(path: String, max: usize) -> Result<Vec<DirEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = expand_env(&path);
        let mut entries: Vec<DirEntry> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().ok()?.is_dir();
                Some(DirEntry {
                    path: entry.path().to_string_lossy().to_string(),
                    name,
                    is_dir,
                })
            })
            .take(max.max(1) * 4)
            .collect();
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        entries.truncate(max.max(1));
        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Executables found directly in the PATH folders (the Programs "PATH" source).
#[tauri::command]
pub async fn launcher_path_programs() -> Vec<DirEntry> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let path = std::env::var("PATH").unwrap_or_default();
        for dir in path.split(';').filter(|d| !d.trim().is_empty()) {
            let Ok(entries) = std::fs::read_dir(expand_env(dir.trim())) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.to_lowercase().ends_with(".exe") || !seen.insert(name.to_lowercase()) {
                    continue;
                }
                out.push(DirEntry {
                    path: entry.path().to_string_lossy().to_string(),
                    name,
                    is_dir: false,
                });
            }
        }
        out
    })
    .await
    .unwrap_or_default()
}

/// Resolves a Start menu app path to a file on disk, for "Open file
/// location": known-folder prefixes such as "{6D809377-...}\App\app.exe" are
/// expanded. None for packaged apps, which have no file to show.
#[tauri::command]
pub fn launcher_resolve_app_path(path: String) -> Option<String> {
    let path = expand_env(path.trim());
    if let Some(rest) = path.strip_prefix('{') {
        let (guid, tail) = rest.split_once('}')?;
        let guid = windows::core::GUID::try_from(guid).ok()?;
        let base = unsafe {
            use windows::Win32::System::Com::CoTaskMemFree;
            use windows::Win32::UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT};
            let p = SHGetKnownFolderPath(&guid, KF_FLAG_DEFAULT, None).ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const c_void));
            s?
        };
        let full = format!("{base}{tail}");
        return std::path::Path::new(&full).exists().then_some(full);
    }
    std::path::Path::new(&path).exists().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_command_lines() {
        assert_eq!(
            split_command_line("control /name Microsoft.AddHardware"),
            ("control".into(), "/name Microsoft.AddHardware".into())
        );
        assert_eq!(
            split_command_line("\"C:\\Program Files\\a b.exe\" -x"),
            ("C:\\Program Files\\a b.exe".into(), "-x".into())
        );
        assert_eq!(split_command_line("notepad"), ("notepad".into(), "".into()));
    }
}
