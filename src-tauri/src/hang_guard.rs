//! Keeps apps from ever waiting on Bloom.
//!
//! Apps hand Windows' taskbar their tray icons (`Shell_NotifyIcon`) and their
//! progress and tabs (`ITaskbarList`) by sending a message and waiting for the
//! answer. Bloom answers those for the taskbar (`tray_host`,
//! `taskbar_progress`), so while one of Bloom's threads that answers them is
//! stuck, every app that reports something waits with it: its window stops
//! drawing and stops taking clicks, and only quitting the app or Bloom freed
//! it. Those threads only record and pass messages on, so they should answer
//! within milliseconds.
//!
//! This guard asks each of them for an empty answer every two seconds. One
//! that hasn't answered for about six seconds while Explorer does (they wait
//! on Explorer, so its own slow moments don't count) is stuck: Explorer gets its
//! own task list back at once (apps then reach Explorer directly, as without
//! Bloom), the reason goes into crash.log, and Bloom restarts, which hands
//! the tray back to Explorer too and frees every waiting app. Stuck again
//! within its first minute, Bloom doesn't restart in a loop: it keeps the task
//! list with Explorer and notes it.

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::AppHandle;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    IsWindow, SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_NULL,
};

/// How long one answer may take.
const ANSWER_MS: u32 = 1500;
/// Checks in a row without an answer before Bloom restarts.
const MISSES: u32 = 3;

/// Whether the window's thread answers (true also when there is no window).
fn answers(window: isize) -> bool {
    if window == 0 {
        return true;
    }
    let hwnd = HWND(window as *mut _);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return true;
        }
        let mut result = 0usize;
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            ANSWER_MS,
            Some(&mut result),
        )
        .0 != 0
    }
}

pub fn start(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("bloom-hang-guard".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let mut misses = [0u32; 2];
            loop {
                std::thread::sleep(Duration::from_secs(2));
                if crate::state::SHUTTING_DOWN.load(Ordering::Relaxed) {
                    continue;
                }
                let watched = [
                    ("tray", crate::tray_host::window()),
                    ("task list", crate::taskbar_progress::window()),
                ];
                for (index, (name, window)) in watched.into_iter().enumerate() {
                    if answers(window) {
                        misses[index] = 0;
                        continue;
                    }
                    // Bloom's threads pass messages on to Explorer and wait for
                    // it: while Explorer itself doesn't answer (signing in, or
                    // restarting) the wait is Explorer's, not Bloom's.
                    let explorer = crate::tray_host::explorer_taskbar()
                        .map(|hwnd| hwnd.0 as isize)
                        .unwrap_or(0);
                    if explorer == 0 || !answers(explorer) {
                        continue;
                    }
                    misses[index] += 1;
                    if misses[index] < MISSES {
                        continue;
                    }
                    // Apps reporting progress or tabs go straight to Explorer
                    // again from now on, even before Bloom has restarted.
                    crate::taskbar_progress::restore();
                    if started.elapsed() < Duration::from_secs(60) {
                        crate::utils::note_crash(&format!(
                            "Bloom's {name} window stopped answering apps right after start; the task list stays with Explorer"
                        ));
                        misses[index] = 0;
                        continue;
                    }
                    crate::utils::note_crash(&format!(
                        "Bloom's {name} window stopped answering apps for {} s; restarting Bloom so no app waits on it",
                        MISSES * 2
                    ));
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::commands::restart_bloom(app).await;
                    });
                    return;
                }
            }
        });
}
