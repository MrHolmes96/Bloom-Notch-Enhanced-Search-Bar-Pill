//! Tray Icon Manager's hidden icons, so the notch's tray hides the same ones.
//!
//! Tray Icon Manager keeps them in `%AppData%\TrayIconManager\config.json`
//! (`HiddenIcons`: name, program path, icon GUID). Bloom only reads that file
//! and never writes it. The reader takes whatever PowerShell wrote: UTF-8 with
//! or without a byte order mark, UTF-16, or the Windows code page (Danish
//! letters in names), and a list of one written as a lone object.

use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ManagerHidden {
    pub name: String,
    /// The program's file name, lower case (`discord.exe`); empty if unknown.
    pub exe: String,
    /// The icon's GUID, lower case without braces; empty if none.
    pub guid: String,
}

pub struct ManagerConfig {
    pub path: String,
    pub entries: Vec<ManagerHidden>,
}

/// Windows-1252 for the bytes 0x80-0x9F; the rest is Latin-1.
const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

pub fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |data: &[u8], big: bool| {
        let units: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| {
                if big {
                    u16::from_be_bytes([c[0], c[1]])
                } else {
                    u16::from_le_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, false);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, true);
    }
    if bytes.len() >= 4 && bytes[1] == 0 && bytes[3] == 0 {
        return utf16(bytes, false);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes
            .iter()
            .map(|&b| match b {
                0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
                _ => b as char,
            })
            .collect(),
    }
}

fn exe_name(path: &str) -> String {
    let path = path.trim().trim_matches('"');
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    name.to_lowercase()
}

pub fn normalize_guid(text: &str) -> String {
    let trimmed = text.trim().trim_start_matches('{').trim_end_matches('}');
    let hex: String = trimmed.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 || hex.chars().all(|c| c == '0') {
        return String::new();
    }
    trimmed.to_lowercase()
}

fn field<'a>(map: &'a Map<String, Value>, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| {
        map.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .and_then(|(_, v)| v.as_str())
            .filter(|s| !s.trim().is_empty())
    })
}

fn entry_from_object(map: &Map<String, Value>) -> Option<ManagerHidden> {
    let name = field(map, &["DisplayName", "Name", "Tooltip", "Title", "Label"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let path = field(
        map,
        &[
            "ExePath",
            "Path",
            "ExecutablePath",
            "ExeKey",
            "Exe",
            "Program",
        ],
    )
    .map(str::to_string)
    .or_else(|| {
        map.values()
            .filter_map(|v| v.as_str())
            .find(|s| s.trim().to_lowercase().ends_with(".exe"))
            .map(str::to_string)
    })
    .unwrap_or_default();
    let guid = field(map, &["IconGuid", "Guid", "GUID", "ItemGuid"])
        .map(normalize_guid)
        .unwrap_or_default();
    let entry = ManagerHidden {
        name,
        exe: if path.is_empty() {
            String::new()
        } else {
            exe_name(&path)
        },
        guid,
    };
    (!entry.name.is_empty() || !entry.exe.is_empty() || !entry.guid.is_empty()).then_some(entry)
}

fn entry_from_text(text: &str) -> Option<ManagerHidden> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.to_lowercase().ends_with(".exe") {
        return Some(ManagerHidden {
            exe: exe_name(text),
            ..Default::default()
        });
    }
    let guid = normalize_guid(text);
    if !guid.is_empty() {
        return Some(ManagerHidden {
            guid,
            ..Default::default()
        });
    }
    Some(ManagerHidden {
        name: text.to_string(),
        ..Default::default()
    })
}

fn is_hidden_key(key: &str) -> bool {
    let key = key.to_lowercase();
    key.contains("hidden") && !key.contains("unhidden")
}

fn marked_hidden(map: &Map<String, Value>) -> bool {
    map.iter().any(|(k, v)| {
        (k.eq_ignore_ascii_case("Hidden") || k.eq_ignore_ascii_case("IsHidden"))
            && v.as_bool() == Some(true)
    })
}

fn collect(value: &Value, under_hidden: bool, out: &mut Vec<ManagerHidden>) {
    match value {
        Value::Object(map) => {
            if under_hidden || marked_hidden(map) {
                if let Some(entry) = entry_from_object(map) {
                    out.push(entry);
                    return;
                }
            }
            for (key, child) in map {
                collect(child, under_hidden || is_hidden_key(key), out);
            }
        }
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(text) if under_hidden => out.extend(entry_from_text(text)),
                    other => collect(other, under_hidden, out),
                }
            }
        }
        _ => {}
    }
}

/// The hidden icons in one settings file's text.
pub fn parse(text: &str) -> Option<Vec<ManagerHidden>> {
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}').trim()).ok()?;
    let mut entries = Vec::new();
    collect(&value, false, &mut entries);
    entries.dedup();
    Some(entries)
}

/// Tray Icon Manager's settings, or None when it isn't set up on this PC.
pub fn read() -> Option<ManagerConfig> {
    let appdata = std::env::var_os("APPDATA")?;
    let base = std::path::PathBuf::from(appdata);
    let mut found: Option<ManagerConfig> = None;
    for dir in ["TrayIconManager", "TrayIconHider", "Tray Icon Manager"] {
        let folder = base.join(dir);
        let Ok(files) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut paths: Vec<std::path::PathBuf> = files
            .flatten()
            .map(|f| f.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            })
            .collect();
        // config.json first.
        paths.sort_by_key(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("config.json"))
        });
        for path in paths {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Some(entries) = parse(&decode_text(&bytes)) else {
                continue;
            };
            let better = found
                .as_ref()
                .is_none_or(|f| f.entries.is_empty() && !entries.is_empty());
            if better {
                found = Some(ManagerConfig {
                    path: path.to_string_lossy().into_owned(),
                    entries,
                });
            }
        }
        if found.as_ref().is_some_and(|f| !f.entries.is_empty()) {
            break;
        }
    }
    found
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn names_match(a: &str, b: &str) -> bool {
    let (a, b) = (squash(a), squash(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Tooltips often add a state ("Phone Link - Disconnected").
    a == b || (a.len() >= 4 && b.starts_with(&a)) || (b.len() >= 4 && a.starts_with(&b))
}

/// What Bloom knows about a live tray icon.
pub struct IconFacts<'a> {
    pub exe: &'a str,
    pub guid: &'a str,
    pub tooltip: &'a str,
    pub name: &'a str,
    /// How many live icons the same program has.
    pub same_program: usize,
}

pub fn hides(entry: &ManagerHidden, icon: &IconFacts) -> bool {
    if !entry.guid.is_empty() && entry.guid == icon.guid {
        return true;
    }
    let by_name = !entry.name.is_empty()
        && (names_match(&entry.name, icon.tooltip)
            || names_match(&entry.name, icon.name)
            || names_match(&entry.name, icon.exe.trim_end_matches(".exe")));
    if !entry.exe.is_empty() {
        // One program's several icons are told apart by name.
        return entry.exe == icon.exe
            && (icon.same_program <= 1 || entry.name.is_empty() || by_name);
    }
    by_name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_manager_settings_are_read() {
        let text = r#"{"HiddenIcons":[{"DisplayName":"Discord","ExePath":"C:\\Users\\k\\AppData\\Local\\Discord\\app-1.0.9\\Discord.exe","UID":1,"IconGuid":"00000000-0000-0000-0000-000000000000","HWnd":"1234","Method":"UID"},
            {"DisplayName":"Phone Link","IconGuid":"{9E7A1B2C-1111-2222-3333-444455556666}"}],"StartWithWindows":true}"#;
        let entries = parse(text).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].exe, "discord.exe");
        assert_eq!(entries[0].guid, "");
        assert_eq!(entries[1].guid, "9e7a1b2c-1111-2222-3333-444455556666");

        // PowerShell writes a list of one as a lone object.
        let single =
            parse(r#"{"HiddenIcons":{"DisplayName":"Steam","ExePath":"C:\\Steam\\steam.exe"}}"#)
                .unwrap();
        assert_eq!(single[0].exe, "steam.exe");
    }

    #[test]
    fn any_encoding_is_read() {
        let json = "{\"HiddenIcons\":[{\"DisplayName\":\"Netværk\"}]}";
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(json.encode_utf16().flat_map(|u| u.to_le_bytes()))
            .collect();
        assert_eq!(parse(&decode_text(&utf16)).unwrap()[0].name, "Netværk");
        let ansi: Vec<u8> = json.chars().map(|c| c as u32 as u8).collect();
        assert_eq!(parse(&decode_text(&ansi)).unwrap()[0].name, "Netværk");
        let bom: Vec<u8> = [0xEF, 0xBB, 0xBF].into_iter().chain(json.bytes()).collect();
        assert_eq!(parse(&decode_text(&bom)).unwrap()[0].name, "Netværk");
    }

    #[test]
    fn icons_match_by_guid_program_and_name() {
        let discord = ManagerHidden {
            name: "Discord".into(),
            exe: "discord.exe".into(),
            guid: String::new(),
        };
        let facts = IconFacts {
            exe: "discord.exe",
            guid: "",
            tooltip: "Discord",
            name: "Discord",
            same_program: 1,
        };
        assert!(hides(&discord, &facts));
        let phone = ManagerHidden {
            name: "Phone Link".into(),
            ..Default::default()
        };
        let phone_icon = IconFacts {
            exe: "phoneexperiencehost.exe",
            guid: "",
            tooltip: "Phone Link - Disconnected",
            name: "Phone Link",
            same_program: 1,
        };
        assert!(hides(&phone, &phone_icon));
        assert!(!hides(&phone, &facts));
    }
}
