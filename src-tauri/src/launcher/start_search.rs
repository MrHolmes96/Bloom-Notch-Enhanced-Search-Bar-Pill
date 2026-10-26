//! What Windows' own Start search finds that the search bar's other sources
//! don't: the commands Start offers as "Run command" (programs registered
//! under App Paths, and the programs, consoles and Control Panel files in
//! Windows' folders, such as regedit, dxdiag or services.msc), and Windows'
//! own index of its settings (every setting the Settings app's search
//! knows, with its search words). Both are read once and kept.

use serde::Serialize;
use std::collections::HashSet;
use std::sync::OnceLock;

#[derive(Serialize, Clone)]
pub struct RunCommand {
    /// The file name, as typed in Run ("regedit.exe", "services.msc").
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Clone)]
pub struct SystemSetting {
    pub name: String,
    /// Other words the setting is found by.
    pub words: String,
    /// What opens it: an ms-settings: link or a command line.
    pub command: String,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The programs registered under App Paths (`chrome.exe`, `winword.exe`),
/// for this user and for everyone.
fn app_paths() -> Vec<RunCommand> {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    };
    let mut found = Vec::new();
    let root_key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\App Paths");
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(
                root,
                windows::core::PCWSTR(root_key.as_ptr()),
                None,
                KEY_READ,
                &mut key,
            )
            .is_err()
            {
                continue;
            }
            for index in 0..2048u32 {
                let mut name = [0u16; 260];
                let mut length = name.len() as u32;
                if RegEnumKeyExW(
                    key,
                    index,
                    Some(windows::core::PWSTR(name.as_mut_ptr())),
                    &mut length,
                    None,
                    None,
                    None,
                    None,
                )
                .is_err()
                {
                    break;
                }
                let name = String::from_utf16_lossy(&name[..length as usize]);
                let sub = wide(&name);
                let mut buffer = vec![0u16; 1024];
                let mut size = (buffer.len() * 2) as u32;
                let read = RegGetValueW(
                    key,
                    windows::core::PCWSTR(sub.as_ptr()),
                    windows::core::PCWSTR::null(),
                    RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
                    None,
                    Some(buffer.as_mut_ptr() as *mut _),
                    Some(&mut size),
                );
                if read.is_err() {
                    continue;
                }
                let raw =
                    String::from_utf16_lossy(&buffer[..(size as usize / 2).min(buffer.len())]);
                let path = crate::launcher::actions::expand_env(
                    raw.trim_end_matches('\0').trim().trim_matches('"'),
                );
                if std::path::Path::new(&path).is_file() {
                    found.push(RunCommand { name, path });
                }
            }
            let _ = RegCloseKey(key);
        }
    }
    found
}

/// Programs, consoles (.msc) and Control Panel files (.cpl) in Windows' own
/// folders: what Start offers as "Run command".
fn windows_programs() -> Vec<RunCommand> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let mut found = Vec::new();
    for folder in [format!("{root}\\System32"), root.clone()] {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            if !(lower.ends_with(".exe") || lower.ends_with(".msc") || lower.ends_with(".cpl")) {
                continue;
            }
            if entry.file_type().is_ok_and(|t| t.is_file()) {
                found.push(RunCommand {
                    name,
                    path: entry.path().to_string_lossy().into_owned(),
                });
            }
        }
    }
    found
}

static RUN_COMMANDS: OnceLock<Vec<RunCommand>> = OnceLock::new();

/// Every run command, App Paths first (one per file name).
#[tauri::command]
pub async fn launcher_run_commands() -> Vec<RunCommand> {
    tauri::async_runtime::spawn_blocking(|| {
        RUN_COMMANDS
            .get_or_init(|| {
                let mut seen = HashSet::new();
                app_paths()
                    .into_iter()
                    .chain(windows_programs())
                    .filter(|c| seen.insert(c.name.to_lowercase()))
                    .collect()
            })
            .clone()
    })
    .await
    .unwrap_or_default()
}

/// Resolves "@file.dll,-123" and "@{package?ms-resource://...}" texts, as
/// Windows does for its settings' names.
fn indirect(text: &str) -> Option<String> {
    let text = text.trim();
    if !text.starts_with('@') {
        return (!text.is_empty()).then(|| text.to_string());
    }
    use windows::Win32::UI::Shell::SHLoadIndirectString;
    let source = wide(text);
    let mut out = vec![0u16; 1024];
    unsafe {
        SHLoadIndirectString(windows::core::PCWSTR(source.as_ptr()), &mut out, None).ok()?;
    }
    let end = out.iter().position(|&c| c == 0).unwrap_or(out.len());
    let resolved = String::from_utf16_lossy(&out[..end]).trim().to_string();
    (!resolved.is_empty()).then_some(resolved)
}

/// The text of every `<tag>` inside `block`.
fn tag_values<'a>(block: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut rest = block;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        values.push(after[..end].trim());
        rest = &after[end + close.len()..];
    }
    values
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// One setting from Windows' settings index: its name (resolved), search
/// words and the link or command that opens it. None for entries without
/// either.
fn parse_setting(block: &str) -> Option<SystemSetting> {
    let name = ["Description", "DisplayName", "Name"]
        .iter()
        .flat_map(|tag| tag_values(block, tag))
        .find_map(|value| indirect(&unescape(value)))?;
    let command = tag_values(block, "DeepLink")
        .into_iter()
        .chain(tag_values(block, "Command"))
        .map(unescape)
        .find(|value| !value.is_empty())?;
    let words: Vec<String> = ["HighKeywords", "Keywords", "LowKeywords"]
        .iter()
        .flat_map(|tag| tag_values(block, tag))
        .filter_map(|value| indirect(&unescape(value)))
        .collect();
    Some(SystemSetting {
        name,
        words: words.join(" ").replace(';', " "),
        command,
    })
}

static SYSTEM_SETTINGS: OnceLock<Vec<SystemSetting>> = OnceLock::new();

/// Windows' own index of its settings
/// (`%SystemRoot%\ImmersiveControlPanel\Settings\AllSystemSettings_*.xml`),
/// which the Settings app's search uses. Empty when this Windows has none.
#[tauri::command]
pub async fn launcher_system_settings() -> Vec<SystemSetting> {
    tauri::async_runtime::spawn_blocking(|| {
        SYSTEM_SETTINGS
            .get_or_init(|| {
                let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
                let folder = std::path::Path::new(&root).join("ImmersiveControlPanel\\Settings");
                let Ok(entries) = std::fs::read_dir(folder) else {
                    return Vec::new();
                };
                let mut seen = HashSet::new();
                let mut found = Vec::new();
                for entry in entries.flatten() {
                    let file = entry.file_name().to_string_lossy().to_lowercase();
                    if !file.starts_with("allsystemsettings") || !file.ends_with(".xml") {
                        continue;
                    }
                    let Ok(bytes) = std::fs::read(entry.path()) else {
                        continue;
                    };
                    let text = decode(&bytes);
                    for block in text.split("<SearchableContent").skip(1) {
                        let block = block.split("</SearchableContent>").next().unwrap_or(block);
                        if let Some(setting) = parse_setting(block) {
                            if seen.insert((setting.name.to_lowercase(), setting.command.clone())) {
                                found.push(setting);
                            }
                        }
                    }
                }
                found
            })
            .clone()
    })
    .await
    .unwrap_or_default()
}

/// The file as text: UTF-16 with a byte order mark, or UTF-8.
fn decode(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_setting_entry() {
        let block = r#"
            <ApplicationInformation><DeepLink>ms-settings:nightlight</DeepLink></ApplicationInformation>
            <SettingInformation>
              <Description>Night light</Description>
              <HighKeywords>blue light;warm</HighKeywords>
            </SettingInformation>"#;
        let setting = parse_setting(block).unwrap();
        assert_eq!(setting.name, "Night light");
        assert_eq!(setting.command, "ms-settings:nightlight");
        assert_eq!(setting.words, "blue light warm");
        assert!(parse_setting("<Description>No link</Description>").is_none());
    }
}
