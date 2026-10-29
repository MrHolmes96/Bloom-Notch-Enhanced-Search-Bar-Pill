//! Where Windows draws its own notification pop-ups and notification
//! center, recorded for Export Diagnostics (shell-events.txt).
//!
//! Windows 11 26H2 moved its shell UI to new windows and programs, so Bloom
//! can't take for granted where the pop-ups and the center live. This notes
//! each window of a Windows program (one under the Windows folder) as it
//! appears or comes to the front: its program, class, title, size and when.
//! Only what windows are called, never what they show. The last 200 different
//! windows are kept, in memory only.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;

struct Seen {
    at: String,
    event: &'static str,
    line: String,
}

static SEEN: Mutex<VecDeque<Seen>> = Mutex::new(VecDeque::new());
static PROGRAMS: Mutex<Option<HashMap<u32, Option<String>>>> = Mutex::new(None);

/// The program of a process when it lives under the Windows folder.
fn windows_program(pid: u32) -> Option<String> {
    let mut cache = PROGRAMS.lock().ok()?;
    let cache = cache.get_or_insert_with(HashMap::new);
    if cache.len() > 512 {
        cache.clear();
    }
    cache
        .entry(pid)
        .or_insert_with(|| {
            let root = std::env::var("SystemRoot")
                .unwrap_or_else(|_| "C:\\Windows".into())
                .to_lowercase();
            let path = unsafe { crate::commands::process_image_path(pid) }?.to_lowercase();
            // Also apps whose windows Bloom has to tell apart (Portals: its
            // loading pop-up from its portals).
            let watched_app = path.ends_with("\\portals.exe");
            (path.starts_with(&root) || watched_app).then_some(path)
        })
        .clone()
}

pub(crate) fn describe(hwnd: HWND) -> Option<String> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    };
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        let program = windows_program(pid)?;
        let mut class = [0u16; 128];
        let class_len = GetClassNameW(hwnd, &mut class) as usize;
        let mut title = [0u16; 128];
        let title_len = GetWindowTextW(hwnd, &mut title) as usize;
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let mut cloaked = 0u32;
        let _ = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            4,
        );
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindow, GetWindowLongW, GWL_EXSTYLE, GWL_STYLE, GW_OWNER,
        };
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null());
        Some(format!(
            "{} | {} | \"{}\" | {}x{} at {},{} | cloaked={} | style={:#010x} ex={:#010x} owned={}",
            program.rsplit('\\').next().unwrap_or(&program),
            String::from_utf16_lossy(&class[..class_len.min(128)]),
            String::from_utf16_lossy(&title[..title_len.min(128)]),
            rect.right - rect.left,
            rect.bottom - rect.top,
            rect.left,
            rect.top,
            cloaked != 0,
            style,
            ex_style,
            owned
        ))
    }
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND, GA_ROOT,
    };
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() || GetAncestor(hwnd, GA_ROOT) != hwnd {
        return;
    }
    let name = match event {
        e if e == EVENT_SYSTEM_FOREGROUND => "front",
        e if e == EVENT_OBJECT_SHOW => "shown",
        e if e == EVENT_OBJECT_UNCLOAKED => "uncloaked",
        _ => return,
    };
    let Some(line) = describe(hwnd) else {
        return;
    };
    let time = windows::Win32::System::SystemInformation::GetLocalTime();
    let at = format!("{:02}:{:02}:{:02}", time.wHour, time.wMinute, time.wSecond);
    if let Ok(mut seen) = SEEN.lock() {
        // One entry per window kind and event, the latest time.
        seen.retain(|s| !(s.line == line && s.event == name));
        seen.push_back(Seen {
            at,
            event: name,
            line,
        });
        while seen.len() > 200 {
            seen.pop_front();
        }
    }
}

/// Starts recording (a thread of its own with the window event hooks).
pub fn start() {
    let _ = std::thread::Builder::new()
        .name("bloom-shell-probe".into())
        .spawn(|| unsafe {
            use windows::Win32::UI::Accessibility::SetWinEventHook;
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, TranslateMessage, EVENT_OBJECT_SHOW,
                EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT,
                WINEVENT_SKIPOWNPROCESS,
            };
            let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
            for event in [
                EVENT_SYSTEM_FOREGROUND,
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_UNCLOAKED,
            ] {
                let _ = SetWinEventHook(event, event, None, Some(on_event), 0, 0, flags);
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
}

/// What was recorded, oldest first.
pub fn report() -> String {
    SEEN.lock()
        .map(|seen| {
            seen.iter()
                .map(|s| format!("{} {:9} {}", s.at, s.event, s.line))
                .collect::<Vec<_>>()
                .join("\r\n")
        })
        .unwrap_or_default()
}
