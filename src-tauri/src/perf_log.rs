//! `perf.log` (Bloom's data folder, in diagnostics): where Bloom was slow, to
//! find what makes it stutter rather than guess.
//!
//! - Bloom's main thread busy (its windows, the notch and the dock wait on it
//!   for every event): a watcher posts to it every second and notes when it
//!   answered 150 ms late or more.
//! - The notch's, dock's and Settings' pages: frames that took long, and the
//!   code that ran in them (`perf_note`, from `usePerfWatch`).
//! - Pictures of windows for the dock's previews that took long (the app has
//!   to draw itself for one).
//!
//! Lines are few: each source says at most once per 10 s what it saw since.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::AppHandle;

/// Lines are written whole, one at a time (others interleave otherwise).
static WRITE: Mutex<()> = Mutex::new(());

pub fn note(line: &str) {
    note_to("perf.log", line);
}

/// A line in a log of Bloom's data folder (in diagnostics), at most 128 KB.
pub fn note_to(file: &str, line: &str) {
    let Some(path) = crate::tray_host::config_dir().map(|dir| dir.join(file)) else {
        return;
    };
    let Ok(_guard) = WRITE.lock() else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 128 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {line}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
        );
    }
}

/// Late answers from the main thread since the last line: how many, the
/// longest (ms).
static LATE_COUNT: AtomicU64 = AtomicU64::new(0);
static LATE_MAX: AtomicU64 = AtomicU64::new(0);

/// Watches Bloom's main thread from a thread of its own.
pub fn watch_main_thread(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("bloom-perf-watch".into())
        .spawn(move || {
            let mut last_line = Instant::now();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if crate::state::SHUTTING_DOWN.load(Ordering::Relaxed) {
                    return;
                }
                let posted = Instant::now();
                let (tx, rx) = std::sync::mpsc::channel::<()>();
                if app
                    .run_on_main_thread(move || {
                        let _ = tx.send(());
                    })
                    .is_err()
                {
                    continue;
                }
                // Longer than 10 s is the freeze guard's to handle.
                let answered = rx.recv_timeout(Duration::from_secs(10)).is_ok();
                let waited = posted.elapsed();
                // Asleep: Windows suspended everything, not Bloom.
                if !answered || waited > Duration::from_secs(9) {
                    continue;
                }
                let ms = waited.as_millis() as u64;
                if ms >= 150 {
                    LATE_COUNT.fetch_add(1, Ordering::Relaxed);
                    LATE_MAX.fetch_max(ms, Ordering::Relaxed);
                }
                if last_line.elapsed() >= Duration::from_secs(10) {
                    let count = LATE_COUNT.swap(0, Ordering::Relaxed);
                    let max = LATE_MAX.swap(0, Ordering::Relaxed);
                    if count > 0 {
                        note(&format!(
                            "main thread: busy {count} time(s), longest {max} ms (every Bloom window waited)"
                        ));
                    }
                    last_line = Instant::now();
                }
            }
        });
}

/// A page's slow frames (see `usePerfWatch`): at most a line per 10 s per page.
#[tauri::command]
pub fn perf_note(page: String, what: String) {
    static LAST: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());
    let page: String = page.chars().take(24).collect();
    let what: String = what.chars().take(400).collect();
    if let Ok(mut last) = LAST.lock() {
        if let Some((_, at)) = last.iter_mut().find(|(p, _)| *p == page) {
            if at.elapsed() < Duration::from_secs(10) {
                return;
            }
            *at = Instant::now();
        } else {
            last.push((page.clone(), Instant::now()));
        }
    }
    note(&format!("{page} page: {what}"));
}

/// A picture of a window that took long.
pub fn slow_picture(hwnd: windows::Win32::Foundation::HWND, took: Duration) {
    let ms = took.as_millis();
    if ms < 60 {
        return;
    }
    let mut pid = 0u32;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    let program = unsafe { crate::commands::any_process_path(pid) }
        .and_then(|path| path.rsplit('\\').next().map(str::to_string))
        .unwrap_or_default();
    note(&format!(
        "preview picture of {program}: {ms} ms (the app drew itself for it)"
    ));
}
