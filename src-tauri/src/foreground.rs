//! Letting the app the user picks in Bloom come to the front, as it does from
//! the taskbar.
//!
//! Windows only lets a process put a window in front when that process took
//! part in the last input. Clicks on the dock, the notch and the cards land in
//! their web view, which runs in a separate process, so Bloom itself does not
//! count: an app it opened or switched to stayed behind whatever was in front
//! (Portals, Explorer, the previous app) and only flashed.
//!
//! Supplying one empty input event makes Bloom the latest input provider, which
//! is what Windows checks. Bloom then hands that right on to the app with
//! `AllowSetForegroundWindow`, so a launched app, a tray icon's menu or a
//! notification's app can bring its own window forward. The event moves nothing
//! and presses nothing.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, ASFW_ANY, SW_RESTORE, SW_SHOW,
};

/// Lets the next window any app activates come to the front. Call right
/// before launching or activating something on the user's behalf.
pub fn grant() {
    unsafe {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT::default(),
            },
        };
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

/// Grants foreground rights to one process (an icon's owner, a launched app).
pub fn grant_to(pid: u32) {
    grant();
    if pid != 0 {
        unsafe {
            let _ = AllowSetForegroundWindow(pid);
        }
    }
}

/// Restores a minimized window the way its taskbar button does: through
/// the window menu's Restore (`WM_SYSCOMMAND` `SC_RESTORE`), which the app
/// handles itself, as for a click on its own restore button. That works for
/// every kind of app: ones that draw their own frame (browsers, Electron,
/// Java, games), ones that keep their own window state, and ones running as
/// administrator, which Bloom can't restore directly. An app that is hung
/// (or doesn't restore) is restored by Windows itself.
pub fn restore(hwnd: HWND) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, ShowWindowAsync, SC_RESTORE, SMTO_ABORTIFHUNG, SMTO_NORMAL,
        WM_SYSCOMMAND,
    };
    unsafe {
        // It activates itself as it restores.
        grant_to(process_of(hwnd));
        let mut answer = 0usize;
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_SYSCOMMAND,
            WPARAM(SC_RESTORE as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_NORMAL,
            400,
            Some(&mut answer),
        );
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
        }
    }
}

/// Minimizes a window the way its taskbar button does: the window menu's
/// Minimize, which the app handles itself (an app that goes to the tray
/// when minimized still does). Windows minimizes one that is hung.
pub fn minimize(hwnd: HWND) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, ShowWindowAsync, SC_MINIMIZE, SMTO_ABORTIFHUNG, SMTO_NORMAL,
        SW_MINIMIZE, WM_SYSCOMMAND,
    };
    unsafe {
        // Windows hands the foreground to the window below it.
        let _ = AllowSetForegroundWindow(process_of(hwnd));
        let mut answer = 0usize;
        let sent = SendMessageTimeoutW(
            hwnd,
            WM_SYSCOMMAND,
            WPARAM(SC_MINIMIZE as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_NORMAL,
            400,
            Some(&mut answer),
        );
        if sent.0 == 0 && !IsIconic(hwnd).as_bool() {
            let _ = ShowWindowAsync(hwnd, SW_MINIMIZE);
        }
    }
}

/// The part of a window to activate: its open dialog when it has one (a
/// "Save changes?" box), as the taskbar does, else the window itself.
unsafe fn active_part(hwnd: HWND) -> HWND {
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::GetLastActivePopup;
    let popup = GetLastActivePopup(hwnd);
    if !popup.is_invalid()
        && popup != hwnd
        && IsWindowVisible(popup).as_bool()
        && IsWindowEnabled(popup).as_bool()
    {
        popup
    } else {
        hwnd
    }
}

/// Whether the window (or another window of its app, such as its dialog) is
/// the one in front.
unsafe fn in_front(hwnd: HWND) -> bool {
    let foreground = GetForegroundWindow();
    !foreground.is_invalid() && (foreground == hwnd || process_of(foreground) == process_of(hwnd))
}

/// Shows, restores and activates a window, like clicking its taskbar button.
pub fn bring_to_front(hwnd: HWND) -> bool {
    bring_to_front_how(hwnd).0
}

/// `bring_to_front`, and the way that did it (for `focus.log`).
fn bring_to_front_how(hwnd: HWND) -> (bool, &'static str) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::SwitchToThisWindow;
    unsafe {
        if hwnd.is_invalid() {
            return (false, "no window");
        }
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        if IsIconic(hwnd).as_bool() {
            restore(hwnd);
        }
        let target = active_part(hwnd);
        grant_to(process_of(target));
        if SetForegroundWindow(target).as_bool() && in_front(target) {
            return (true, "SetForegroundWindow");
        }
        // What Alt+Tab uses to switch.
        SwitchToThisWindow(target, true);
        if in_front(target) {
            return (true, "SwitchToThisWindow");
        }
        // Windows still refused (a full-screen app holding the foreground):
        // share the foreground thread's input for a moment, which it accepts.
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let own_thread = GetCurrentThreadId();
        let attached = foreground_thread != 0
            && foreground_thread != own_thread
            && AttachThreadInput(own_thread, foreground_thread, true).as_bool();
        let _ = BringWindowToTop(target);
        let done = SetForegroundWindow(target).as_bool();
        if attached {
            let _ = AttachThreadInput(own_thread, foreground_thread, false);
        }
        let done = done || in_front(target);
        (done, if done { "shared input" } else { "refused" })
    }
}

/// True while the Start menu or Search has the foreground (it is hosted by
/// StartMenuExperienceHost, or SearchHost on newer builds).
fn start_menu_open() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(foreground, Some(&mut pid));
        crate::commands::process_image_path(pid).is_some_and(|path| {
            let file = path.rsplit('\\').next().unwrap_or("").to_lowercase();
            file == "startmenuexperiencehost.exe" || file == "searchhost.exe"
        })
    }
}

/// Closes the Start menu when it is open, and waits for it to be gone. As it
/// closes it hands the foreground back to the window before it (a full-screen
/// game, say), which would undo a switch made while it was still open.
pub fn leave_start_menu() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_ESCAPE,
    };
    if !start_menu_open() {
        return;
    }
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_ESCAPE,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    unsafe {
        SendInput(
            &[key(Default::default()), key(KEYEVENTF_KEYUP)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        if !start_menu_open() {
            // Its window hands the foreground on as it fades out.
            std::thread::sleep(std::time::Duration::from_millis(30));
            return;
        }
    }
}

fn process_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Whether the window is the active one, as the taskbar shows it: the window
/// in front, or its dialog in front (Bloom's own windows aside: then the app
/// window that was in front before them).
pub fn is_active(hwnd: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GA_ROOTOWNER};
    unsafe {
        let mut front = GetForegroundWindow();
        if !front.is_invalid() && process_of(front) == std::process::id() {
            front = HWND(crate::services::last_front_window() as *mut _);
        }
        !front.is_invalid() && (front == hwnd || GetAncestor(front, GA_ROOTOWNER) == hwnd)
    }
}

/// Switches to a window as its taskbar button does: the Start menu (open
/// from the Windows key, over a game say) closes first, as it does when the
/// taskbar is clicked, then the window is restored and activated. Should the
/// window that was in front take the foreground straight back (a full-screen
/// game does), it is activated once more.
pub fn switch_to(hwnd: HWND) -> bool {
    use std::sync::atomic::{AtomicU64, Ordering};
    /// Bumped by every switch, so an earlier one never undoes a later one.
    static SWITCHES: AtomicU64 = AtomicU64::new(0);
    if hwnd.is_invalid() {
        return false;
    }
    let this = SWITCHES.fetch_add(1, Ordering::SeqCst) + 1;
    let started = std::time::Instant::now();
    leave_start_menu();
    let before = unsafe { GetForegroundWindow() };
    let (done, how) = bring_to_front_how(hwnd);
    let took = started.elapsed().as_millis();
    std::thread::sleep(std::time::Duration::from_millis(150));
    if SWITCHES.load(Ordering::SeqCst) != this {
        note(hwnd, &format!("{how} in {took} ms"));
        return done;
    }
    let now = unsafe { GetForegroundWindow() };
    let taken_back = !before.is_invalid()
        && process_of(before) != process_of(hwnd)
        && (now == before || start_menu_open());
    if taken_back {
        let (again, how_again) = bring_to_front_how(hwnd);
        note(
            hwnd,
            &format!("{how} in {took} ms, taken back by the window before, then {how_again}"),
        );
        return again;
    }
    let done = done || unsafe { in_front(hwnd) };
    note(
        hwnd,
        &format!(
            "{how} in {took} ms{}",
            if done { "" } else { ", not in front" }
        ),
    );
    done
}

/// One line in `focus.log` (Bloom's data folder) per switch from the dock:
/// the app, the way Windows let it come forward and how long that took, to
/// see why an app that was slow to come forward was.
fn note(hwnd: HWND, what: &str) {
    use std::io::Write;
    let Some(path) = crate::tray_host::config_dir().map(|dir| dir.join("focus.log")) else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    let program = unsafe { crate::commands::any_process_path(process_of(hwnd)) }
        .and_then(|path| path.rsplit('\\').next().map(str::to_string))
        .unwrap_or_default();
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {program}: {what}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
        );
    }
}
