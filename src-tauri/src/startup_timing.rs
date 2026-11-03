//! startup-timing.log (in the diagnostics zip): how long after Windows signed
//! you in each step of Bloom's and Portals' start happened, so a slow start
//! shows exactly where its time went.
//!
//! Each start of Bloom writes a block; the Portals launcher (run as
//! administrator by Bloom's task) writes its own lines into the same file.
//! Each step is written once per process, and only in its first three
//! minutes.

use std::collections::HashSet;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const FILE: &str = "startup-timing.log";
const MAX_BYTES: u64 = 48 * 1024;
const WINDOW: Duration = Duration::from_secs(180);

static STARTED: OnceLock<Instant> = OnceLock::new();
static WRITTEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// When Windows signed this session in, in FILETIME ticks.
fn sign_in_ticks() -> Option<i64> {
    use windows::core::PWSTR;
    use windows::Win32::System::RemoteDesktop::{
        WTSFreeMemory, WTSQuerySessionInformationW, WTSSessionInfo, WTSINFOW, WTS_CURRENT_SESSION,
    };
    unsafe {
        let mut buffer = PWSTR::null();
        let mut bytes = 0u32;
        WTSQuerySessionInformationW(
            None,
            WTS_CURRENT_SESSION,
            WTSSessionInfo,
            &mut buffer,
            &mut bytes,
        )
        .ok()?;
        if buffer.is_null() {
            return None;
        }
        let logon = if bytes as usize >= std::mem::size_of::<WTSINFOW>() {
            Some((*(buffer.0 as *const WTSINFOW)).LogonTime)
        } else {
            None
        };
        WTSFreeMemory(buffer.0 as *mut _);
        logon.filter(|t| *t > 0)
    }
}

/// When this process was started, in FILETIME ticks.
fn process_start_ticks() -> Option<i64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .ok()?;
    }
    Some(((created.dwHighDateTime as i64) << 32) | created.dwLowDateTime as i64)
}

fn seconds(ticks: i64) -> f64 {
    ticks as f64 / 10_000_000.0
}

fn append(line: &str) {
    let Some(path) = crate::utils::data_file(FILE) else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        // Keep the newest half.
        if let Ok(text) = std::fs::read_to_string(&path) {
            let cut = text.len() / 2;
            let from = text[cut..]
                .find("\n=== ")
                .map(|i| cut + i + 1)
                .unwrap_or(cut);
            let _ = std::fs::write(&path, &text[from..]);
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Starts this process's block: `who` is what started it ("Bloom, started by
/// its startup task", "Portals launcher (administrator)").
pub fn begin(who: &str) {
    let _ = STARTED.set(Instant::now());
    if let Ok(mut written) = WRITTEN.lock() {
        *written = Some(HashSet::new());
    }
    let local = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let since = sign_in_ticks()
        .zip(process_start_ticks())
        .map(|(sign_in, process)| seconds(process - sign_in));
    let header = match since {
        Some(s) if s < 600.0 => format!("{s:.1} s after Windows signed you in"),
        Some(s) => format!("{:.0} min after sign-in (not a sign-in start)", s / 60.0),
        None => "sign-in time unknown".to_string(),
    };
    append(&format!(
        "\n=== {:04}-{:02}-{:02} {:02}:{:02}:{:02} {who}: process started {header} ===",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute, local.wSecond
    ));
}

/// One step, written once per process: "+3.4 s after sign-in (+0.8 s after
/// this process started): the notch is on screen".
pub fn mark(what: &str) {
    let Some(started) = STARTED.get() else {
        return;
    };
    if started.elapsed() > WINDOW {
        return;
    }
    let first = WRITTEN
        .lock()
        .ok()
        .and_then(|mut w| w.as_mut().map(|set| set.insert(what.to_string())))
        .unwrap_or(false);
    if !first {
        return;
    }
    let after_start = started.elapsed().as_secs_f64();
    let after_sign_in = sign_in_ticks()
        .zip(process_start_ticks())
        .map(|(sign_in, process)| seconds(process - sign_in) + after_start);
    let sign_in = match after_sign_in {
        Some(s) if s < 600.0 => format!("+{s:5.1} s after sign-in"),
        _ => "             ".to_string(),
    };
    append(&format!(
        "  {sign_in}  (+{after_start:4.1} s after start)  {what}"
    ));
}

/// The notch's and the dock's pages report their steps here.
#[tauri::command]
pub fn startup_mark(what: String) {
    // Only Bloom's own words: a short label, nothing else.
    let what: String = what.chars().take(80).collect();
    mark(&what);
}
