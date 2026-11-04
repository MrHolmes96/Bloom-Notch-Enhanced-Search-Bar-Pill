//! The download panel under a dock preview: what an app downloading shows
//! on its taskbar button, with the app's own numbers where the app keeps them
//! somewhere Bloom can read.
//!
//! - Every app: its stage (downloading, installing, paused, failed), how far
//!   it is, and the time left at the pace of the last minute.
//! - Steam: the game's name, its size and how much of it is in (Steam's own
//!   total from the game's `appmanifest_*.acf`, times the progress Steam
//!   reports), the speed from that, and "Updating files" while Steam stages
//!   the download (`BytesStaged` of `BytesToStage`).
//! - Browsers (Brave, Chrome, Edge, Firefox): the file growing in the
//!   Downloads folder (`.crdownload`, `.part`): its name, size so far, total
//!   (size over the progress) and speed from how fast it grows.
//!
//! Nothing is guessed for other apps: no speed or size when the app keeps
//! none to read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

#[derive(Serialize, Default)]
pub struct DownloadDetails {
    /// "downloading", "installing", "paused" or "error".
    stage: &'static str,
    /// Whose numbers these are: "steam", "browser", or none (the progress
    /// alone).
    source: Option<&'static str>,
    /// From 0 to 1; none while no amount is known.
    value: Option<f64>,
    /// The game or file.
    name: Option<String>,
    /// Bytes in so far, and in all.
    done: Option<u64>,
    total: Option<u64>,
    /// Bytes per second.
    speed: Option<f64>,
    /// Seconds.
    time_left: Option<f64>,
    /// Steam's "Updating files", from 0 to 1, while it stages a download.
    updating: Option<f64>,
}

/// The panel for the window `hwnd` reports progress on.
#[tauri::command]
pub async fn download_details(hwnd: isize) -> Option<DownloadDetails> {
    tauri::async_runtime::spawn_blocking(move || details(hwnd))
        .await
        .ok()
        .flatten()
}

fn details(hwnd: isize) -> Option<DownloadDetails> {
    let progress = crate::taskbar_progress::detail(hwnd)?;
    let stage = match progress.state {
        "installing" => "installing",
        "paused" => "paused",
        "error" => "error",
        _ => "downloading",
    };
    let mut out = DownloadDetails {
        stage,
        value: progress.value,
        time_left: progress.time_left,
        ..Default::default()
    };
    let program = |name: &str| {
        progress
            .programs
            .iter()
            .find(|p| p.to_lowercase().ends_with(name))
            .cloned()
    };
    if let Some(steam) = program("\\steam.exe") {
        steam_details(&steam, &progress, &mut out);
    } else if [
        "\\brave.exe",
        "\\chrome.exe",
        "\\msedge.exe",
        "\\firefox.exe",
    ]
    .iter()
    .any(|b| program(b).is_some())
    {
        browser_details(&progress, &mut out);
    }
    Some(out)
}

// ---------------------------------------------------------------- Steam

/// A game's `appmanifest_*.acf`, the parts the panel uses.
struct Manifest {
    name: String,
    /// Steam's `StateFlags`.
    flags: u64,
    to_download: u64,
    downloaded: u64,
    to_stage: u64,
    staged: u64,
}

/// `"Key"  "Value"` lines of a Valve key-value file, first of each key.
fn vdf_values(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('"').collect();
        // ["\t", key, "\t\t", value, ""]
        if parts.len() >= 5 {
            values
                .entry(parts[1].to_string())
                .or_insert_with(|| parts[3].replace("\\\\", "\\"));
        }
    }
    values
}

fn read_manifest(path: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(path).ok()?;
    let values = vdf_values(&text);
    let number = |key: &str| {
        values
            .get(key)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    Some(Manifest {
        name: values.get("name").cloned().unwrap_or_default(),
        flags: number("StateFlags"),
        to_download: number("BytesToDownload"),
        downloaded: number("BytesDownloaded"),
        to_stage: number("BytesToStage"),
        staged: number("BytesStaged"),
    })
}

/// Steam's library folders: its own and those in `libraryfolders.vdf`.
fn steam_libraries(steam_exe: &str) -> Vec<PathBuf> {
    let Some(root) = Path::new(steam_exe).parent() else {
        return Vec::new();
    };
    let mut libraries = vec![root.join("steamapps")];
    if let Ok(text) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        for line in text.lines() {
            let parts: Vec<&str> = line.split('"').collect();
            if parts.len() >= 5 && parts[1] == "path" {
                let library = PathBuf::from(parts[3].replace("\\\\", "\\")).join("steamapps");
                if !libraries.iter().any(|l| {
                    l.to_string_lossy()
                        .eq_ignore_ascii_case(&library.to_string_lossy())
                }) {
                    libraries.push(library);
                }
            }
        }
    }
    libraries
}

/// Each game's latest state in Steam's own log (`logs/content_log.txt`):
/// "AppID 1203620 state changed : Update Required,Update Running,...".
/// `None` when the log can't be read.
fn steam_states(steam_exe: &str) -> Option<HashMap<String, String>> {
    use std::io::{Read, Seek, SeekFrom};
    let root = Path::new(steam_exe).parent()?;
    let mut file = std::fs::File::open(root.join("logs").join("content_log.txt")).ok()?;
    let length = file.metadata().ok()?.len();
    // The end is enough: the latest change of each game.
    let from = length.saturating_sub(512 * 1024);
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(states_in(&String::from_utf8_lossy(&bytes)))
}

fn states_in(log: &str) -> HashMap<String, String> {
    let mut states = HashMap::new();
    for line in log.lines() {
        let Some(at) = line.find("AppID ") else {
            continue;
        };
        let rest = &line[at + 6..];
        let Some((app, state)) = rest.split_once(" state changed : ") else {
            continue;
        };
        states.insert(app.trim().to_string(), state.trim().to_string());
    }
    states
}

/// A state Steam is working on the game in: updating, not paused or queued.
fn running(state: &str) -> bool {
    state.contains("Update Running") && !state.contains("Update Paused")
}

/// Steam's `StateFlags` bits for a game it is working on: update running,
/// started, downloading, staging, committing, preallocating.
const FLAGS_WORKING: u64 = 256 | 1024 | 1_048_576 | 2_097_152 | 4_194_304 | 524_288;
const FLAG_PAUSED: u64 = 512;

fn has_work_left(m: &Manifest) -> bool {
    (m.to_download > 0 && m.downloaded < m.to_download) || (m.to_stage > 0 && m.staged < m.to_stage)
}

/// The game Steam is downloading or updating now, as Steam itself records
/// it: the games its content log says it is running an update for, else
/// (no log to read) the games whose state flags say so. Of those, the one
/// Steam wrote last. None when Steam is working on nothing: an update
/// that is queued, unscheduled, paused or finished doesn't count.
fn active_manifest(steam_exe: &str) -> Option<Manifest> {
    let states = steam_states(steam_exe).filter(|s| !s.is_empty());
    let mut best: Option<(SystemTime, Manifest)> = None;
    for library in steam_libraries(steam_exe) {
        let Ok(entries) = std::fs::read_dir(&library) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let Some(app) = name
                .strip_prefix("appmanifest_")
                .and_then(|n| n.strip_suffix(".acf"))
            else {
                continue;
            };
            if let Some(states) = &states {
                if !states.get(app).is_some_and(|state| running(state)) {
                    continue;
                }
            }
            let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if best.as_ref().is_some_and(|(at, _)| *at >= modified) {
                continue;
            }
            let Some(manifest) = read_manifest(&entry.path()) else {
                continue;
            };
            let working = if states.is_some() {
                true
            } else {
                manifest.flags & FLAGS_WORKING != 0
                    && manifest.flags & FLAG_PAUSED == 0
                    && has_work_left(&manifest)
            };
            if working {
                best = Some((modified, manifest));
            }
        }
    }
    best.map(|(_, manifest)| manifest)
}

/// Whether Steam's own record says it is downloading or updating a game,
/// the deciding word on Steam's progress: Steam can leave its taskbar
/// progress showing after it has finished. Read at most every 1.5 s.
pub(crate) fn steam_working(steam_exe: &str) -> bool {
    static LAST: Mutex<Option<(Instant, bool, String)>> = Mutex::new(None);
    if let Ok(last) = LAST.lock() {
        if let Some((at, working, _)) = last.as_ref() {
            if at.elapsed() < Duration::from_millis(1500) {
                return *working;
            }
        }
    }
    let manifest = active_manifest(steam_exe);
    let working = manifest.is_some();
    // What Steam's record says, logged when it changes.
    let said = manifest
        .as_ref()
        .map(|m| {
            format!(
                "Steam's record: {} downloaded {} of {}, staged {} of {}",
                m.name, m.downloaded, m.to_download, m.staged, m.to_stage
            )
        })
        .unwrap_or_else(|| "Steam's record: nothing downloading or updating".to_string());
    if let Ok(mut last) = LAST.lock() {
        let changed = last.as_ref().is_none_or(|(_, _, before)| *before != said);
        if changed {
            crate::taskbar_progress::log_line(&said);
        }
        *last = Some((Instant::now(), working, said));
    }
    working
}

fn steam_details(
    steam_exe: &str,
    progress: &crate::taskbar_progress::Detail,
    out: &mut DownloadDetails,
) {
    let Some(manifest) = active_manifest(steam_exe) else {
        return;
    };
    out.source = Some("steam");
    if !manifest.name.is_empty() {
        out.name = Some(manifest.name.clone());
    }
    if manifest.to_download > 0 {
        let total = manifest.to_download;
        // Steam's own count, or (when it hasn't written it yet) its total at
        // the progress it reports.
        let from_progress = match (progress.state, progress.value) {
            ("normal", Some(v)) => (v * total as f64) as u64,
            _ => 0,
        };
        let done = manifest.downloaded.max(from_progress).min(total);
        out.total = Some(total);
        out.done = Some(done);
        if progress.state == "normal" {
            out.value = Some(done as f64 / total as f64);
            out.speed = progress.pace.map(|pace| pace * total as f64);
        }
    }
    if manifest.to_stage > 0 && manifest.staged > 0 {
        let staged = (manifest.staged as f64 / manifest.to_stage as f64).clamp(0.0, 1.0);
        if manifest.to_download == 0 || manifest.downloaded >= manifest.to_download {
            // Downloaded: Steam is only updating files now.
            out.stage = "installing";
            out.value = Some(staged);
            out.speed = None;
            out.time_left = None;
        } else {
            out.updating = Some(staged);
        }
    }
}

// ---------------------------------------------------------------- browsers

/// The last size seen of each file being downloaded, and its speed.
/// By file: its size, when it was seen, and its speed.
type Growth = HashMap<PathBuf, (u64, Instant, Option<f64>)>;
static GROWTH: Mutex<Option<Growth>> = Mutex::new(None);

fn downloads_folder() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

fn browser_details(progress: &crate::taskbar_progress::Detail, out: &mut DownloadDetails) {
    let Some(folder) = downloads_folder() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return;
    };
    // The file being written now: the newest one still in progress.
    let mut newest: Option<(SystemTime, PathBuf, u64)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !name.ends_with(".crdownload") && !name.ends_with(".part") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified.elapsed().unwrap_or(Duration::MAX) > Duration::from_secs(30) {
            continue;
        }
        if newest.as_ref().is_none_or(|(at, _, _)| modified > *at) {
            newest = Some((modified, entry.path(), meta.len()));
        }
    }
    let Some((_, path, size)) = newest else {
        return;
    };
    out.source = Some("browser");
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let shown = file
        .trim_end_matches(".crdownload")
        .trim_end_matches(".part")
        .to_string();
    if !shown.starts_with("Unconfirmed ") && !shown.is_empty() {
        out.name = Some(shown);
    }
    out.done = Some(size);
    if let Some(value) = progress.value.filter(|v| *v >= 0.01) {
        out.total = Some(((size as f64) / value) as u64);
    }
    // Speed from how fast the file grows, smoothed.
    let now = Instant::now();
    if let Ok(mut growth) = GROWTH.lock() {
        let growth = growth.get_or_insert_with(HashMap::new);
        growth.retain(|_, (_, at, _)| now.duration_since(*at) < Duration::from_secs(120));
        let speed = match growth.get(&path) {
            Some(&(before, at, speed)) => {
                let span = now.duration_since(at).as_secs_f64();
                if span >= 0.8 && size >= before {
                    let latest = (size - before) as f64 / span;
                    let smoothed = speed.map_or(latest, |s| s * 0.6 + latest * 0.4);
                    growth.insert(path.clone(), (size, now, Some(smoothed)));
                    Some(smoothed)
                } else {
                    speed
                }
            }
            None => {
                growth.insert(path.clone(), (size, now, None));
                None
            }
        };
        out.speed = speed;
        if let (Some(speed), Some(total)) = (speed, out.total) {
            if speed > 0.0 && total > size {
                out.time_left = Some((total - size) as f64 / speed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_steam_manifest() {
        let text = "\"AppState\"\n{\n\t\"appid\"\t\t\"1203620\"\n\t\"name\"\t\t\"Enshrouded\"\n\t\"StateFlags\"\t\t\"1026\"\n\t\"BytesToDownload\"\t\t\"27594260480\"\n\t\"BytesDownloaded\"\t\t\"9771687936\"\n\t\"BytesToStage\"\t\t\"30000000000\"\n\t\"BytesStaged\"\t\t\"5100000000\"\n}\n";
        let values = vdf_values(text);
        assert_eq!(values["name"], "Enshrouded");
        assert_eq!(values["BytesDownloaded"], "9771687936");
        let dir = std::env::temp_dir().join("bloom-acf-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("appmanifest_1203620.acf");
        std::fs::write(&path, text).unwrap();
        let m = read_manifest(&path).unwrap();
        assert_eq!(m.to_download, 27_594_260_480);
        assert_eq!(m.staged, 5_100_000_000);
    }

    #[test]
    fn steams_log_gives_each_games_latest_state() {
        let log = "[2026-11-04 02:20:01] AppID 730 state changed : Update Required,Update Running,Update Started,Downloading,\n\
                   [2026-11-04 02:21:10] AppID 3999540 state changed : Update Required,Update Queued,\n\
                   [2026-11-04 02:23:40] AppID 730 state changed : Fully Installed,\n";
        let states = states_in(log);
        assert!(!running(&states["730"]), "finished");
        assert!(!running(&states["3999540"]), "queued, not running");
        let log = "[x] AppID 1203620 state changed : Update Required,Update Running,Update Started,Staging,\n";
        assert!(running(&states_in(log)["1203620"]));
        let log = "[x] AppID 1 state changed : Update Required,Update Running,Update Paused,\n";
        assert!(!running(&states_in(log)["1"]), "paused");
    }

    #[test]
    fn library_paths_lose_their_escapes() {
        let values = vdf_values("\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n");
        assert_eq!(values["path"], "D:\\SteamLibrary");
    }
}
