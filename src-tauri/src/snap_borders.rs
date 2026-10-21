//! Borders of snapped windows.
//!
//! Windows 11 draws its own border around windows (a light line, or the
//! accent colour), which shows most on windows snapped side by side. Bloom
//! turns it off on every window that is snapped (`IsWindowArranged`: halves,
//! thirds, quarters, Snap Layouts and Snap Groups) with `DWMWA_BORDER_COLOR =
//! DWMWA_COLOR_NONE`, the setting border tools use on other apps' windows, and
//! can draw a border of its own instead: exactly one screen pixel wide, in the
//! colour and opacity picked in Settings > Appearance > Window Borders (Windows
//! itself offers neither, and its border grows with the display scale).
//!
//! Bloom's border is four one-pixel windows along the snapped window's
//! visible edges, kept right above it in the stacking order (so windows on
//! top of it still cover it) and moved with it. They are layered (see-through
//! at the chosen opacity), click-through and never activated, and Windows
//! composites them without any drawing on Bloom's part beyond one fill.
//!
//! The moment a window is no longer snapped (and when Bloom quits or the
//! setting is off) Bloom's border goes and the window gets the border Windows'
//! own setting gives it: the accent border when "Show accent colour on title
//! bars and window borders" is on, and no border when it's off (or the app had
//! turned its border off itself), so it looks as it did before it was snapped.
//!
//! Low on resources by design: a window is looked at when it comes to the
//! front, a move ends, or it is minimized, restored, hidden on another
//! desktop or shown again (system events), and the front window every 300 ms
//! (one quick call; keyboard snaps and Snap Layouts send no event of their
//! own). Only while Bloom draws a border is a snapped window's app followed as
//! it moves, so the border keeps up with a resize. Settings keys:
//! `bloom-windows-snap-borders`, `bloom-windows-snap-border-color`,
//! `bloom-windows-snap-border-opacity`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;

use tauri::AppHandle;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CLOAKED,
    DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, HBRUSH,
    HGDIOBJ, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, GetAncestor,
    GetForegroundWindow, GetMessageW, GetWindow, GetWindowLongW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowArranged, IsWindowVisible, KillTimer,
    PostThreadMessageW, RegisterClassExW, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
    TranslateMessage, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_UNCLOAKED,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND, GA_ROOT, GWL_EXSTYLE, GW_HWNDPREV, HTTRANSPARENT, HWND_NOTOPMOST,
    HWND_TOP, HWND_TOPMOST, LWA_ALPHA, MSG, OBJID_WINDOW, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT,
    WM_QUIT, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const KEY: &str = "bloom-windows-snap-borders";
const COLOR_KEY: &str = "bloom-windows-snap-border-color";
const OPACITY_KEY: &str = "bloom-windows-snap-border-opacity";
/// How often the front window is looked at.
const POLL_MS: u32 = 300;
/// Sent to the borders' thread when the colour or opacity changed.
const MSG_REFRESH: u32 = WM_APP + 0x31;

static RUNNING: AtomicBool = AtomicBool::new(false);
static THREAD: AtomicU32 = AtomicU32::new(0);
/// The border colour picked (`0x00BBGGRR`).
static COLOR: AtomicU32 = AtomicU32::new(0x0080_8080);
/// Its opacity, 0 to 255; 0 is no border at all.
static ALPHA: AtomicU32 = AtomicU32::new(0);
/// The brush the border is filled with (the borders' thread only).
static BRUSH: AtomicIsize = AtomicIsize::new(0);

/// One snapped window Bloom took the border of.
struct Snapped {
    hwnd: isize,
    /// The border colour Windows reported before, when it tells.
    saved: Option<u32>,
    pid: u32,
    /// Bloom's own border, while it draws one: top, bottom, left, right.
    edges: Option<[isize; 4]>,
    /// Where the window's frame was when the border was last placed.
    placed: RECT,
}

/// Every window whose border Bloom changed.
static CHANGED: Mutex<Vec<Snapped>> = Mutex::new(Vec::new());

thread_local! {
    /// Per app with a border drawn: the hook that reports its windows moving.
    static MOVES: RefCell<HashMap<u32, isize>> = RefCell::new(HashMap::new());
}

/// Turns the feature on or off and picks up the colour and opacity, as the
/// settings say (on unless set to "false"; no border of Bloom's own until an
/// opacity is chosen).
pub fn apply(app: &AppHandle) {
    let color = crate::utils::get_setting_str(app, COLOR_KEY)
        .and_then(|value| parse_color(&value))
        .unwrap_or(0x0080_8080);
    let opacity = crate::utils::get_setting_str(app, OPACITY_KEY)
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    COLOR.store(color, Ordering::Release);
    ALPHA.store(alpha_of(opacity), Ordering::Release);
    if crate::utils::get_setting_str(app, KEY).as_deref() == Some("false") {
        stop();
    } else if RUNNING.load(Ordering::Acquire) {
        let thread = THREAD.load(Ordering::Acquire);
        if thread != 0 {
            unsafe {
                let _ = PostThreadMessageW(thread, MSG_REFRESH, WPARAM(0), LPARAM(0));
            }
        }
    } else {
        start();
    }
}

/// `#RRGGBB` as a Windows colour (`0x00BBGGRR`).
fn parse_color(value: &str) -> Option<u32> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    Some(b << 16 | g << 8 | r)
}

/// An opacity in percent as a layered window's alpha.
fn alpha_of(percent: f64) -> u32 {
    if !percent.is_finite() {
        return 0;
    }
    (percent.clamp(0.0, 100.0) * 255.0 / 100.0).round() as u32
}

/// The border colour a window has now, when Windows tells (Windows 11 builds
/// differ in whether it does).
fn current_border(hwnd: HWND) -> Option<u32> {
    let mut color = 0u32;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &mut color as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .ok()?;
    }
    Some(color)
}

/// Whether "Show accent colour on title bars and window borders" is on.
fn accent_borders_on() -> bool {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\DWM"),
            w!("ColorPrevalence"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    read.is_ok() && value == 1
}

/// The border a window gets once it isn't snapped: none when the app had
/// turned it off itself or accent borders are off, else the accent border.
/// (Only "none" is taken from what Windows reports, since some builds report
/// the colour they draw rather than the one the app chose.)
fn border_when_free(saved: Option<u32>, accent_on: bool) -> u32 {
    if saved == Some(DWMWA_COLOR_NONE) || !accent_on {
        DWMWA_COLOR_NONE
    } else {
        DWMWA_COLOR_DEFAULT
    }
}

fn set_border(hwnd: HWND, color: u32) {
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &color as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// On another virtual desktop (or an app Windows keeps out of sight).
fn cloaked(hwnd: HWND) -> bool {
    let mut value = 0u32;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut value as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && value != 0
    }
}

/// The part of a window Windows shows (its frame, without the invisible
/// resize borders), in screen pixels.
fn frame_of(hwnd: HWND) -> RECT {
    let mut frame = RECT::default();
    unsafe {
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
        {
            let _ = GetWindowRect(hwnd, &mut frame);
        }
    }
    frame
}

/// The four one-pixel lines along a frame's edges: top, bottom, left and
/// right, as x, y, width and height.
fn edge_rects(frame: RECT) -> [(i32, i32, i32, i32); 4] {
    let width = (frame.right - frame.left).max(1);
    let height = (frame.bottom - frame.top).max(1);
    [
        (frame.left, frame.top, width, 1),
        (frame.left, frame.bottom - 1, width, 1),
        (frame.left, frame.top, 1, height),
        (frame.right - 1, frame.top, 1, height),
    ]
}

// ---------------------------------------------------------------- Bloom's border

unsafe extern "system" fn edge_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut paint);
            let brush = BRUSH.load(Ordering::Acquire);
            if brush != 0 {
                FillRect(hdc, &paint.rcPaint, HBRUSH(brush as *mut _));
            }
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// The brush for the colour picked, made again when it changed.
fn update_brush() {
    static MADE_FOR: AtomicU32 = AtomicU32::new(u32::MAX);
    let color = COLOR.load(Ordering::Acquire);
    if MADE_FOR.swap(color, Ordering::AcqRel) == color && BRUSH.load(Ordering::Acquire) != 0 {
        return;
    }
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color));
        let old = BRUSH.swap(brush.0 as isize, Ordering::AcqRel);
        if old != 0 {
            let _ = DeleteObject(HGDIOBJ(old as *mut _));
        }
    }
}

fn register_class() -> bool {
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Acquire) {
        return true;
    }
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return false;
        };
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(edge_proc),
            hInstance: module.into(),
            lpszClassName: w!("BloomSnapBorder"),
            ..Default::default()
        };
        let registered = RegisterClassExW(&class) != 0;
        REGISTERED.store(registered, Ordering::Release);
        registered
    }
}

/// Bloom's border for one window: four hidden lines, placed by `place`.
fn create_edges() -> Option<[isize; 4]> {
    if !register_class() {
        return None;
    }
    update_brush();
    let alpha = ALPHA.load(Ordering::Acquire).min(255) as u8;
    let mut edges = [0isize; 4];
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        for edge in edges.iter_mut() {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("BloomSnapBorder"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(module.into()),
                None,
            );
            match hwnd {
                Ok(hwnd) => {
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                    *edge = hwnd.0 as isize;
                }
                Err(_) => {
                    destroy_edges(edges);
                    return None;
                }
            }
        }
    }
    Some(edges)
}

fn destroy_edges(edges: [isize; 4]) {
    for edge in edges {
        if edge != 0 {
            unsafe {
                let _ = DestroyWindow(HWND(edge as *mut _));
            }
        }
    }
}

fn is_topmost(hwnd: HWND) -> bool {
    unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 }
}

/// Whether the border lines are the windows right above `target`.
fn edges_on_top_of(target: HWND, edges: &[isize; 4]) -> bool {
    let mut above = target;
    for _ in 0..4 {
        match unsafe { GetWindow(above, GW_HWNDPREV) } {
            Ok(window) if edges.contains(&(window.0 as isize)) => above = window,
            _ => return false,
        }
    }
    true
}

/// Puts the border on the window's edges, right above it.
fn place(entry: &mut Snapped, force: bool) {
    let Some(edges) = entry.edges else {
        return;
    };
    let target = HWND(entry.hwnd as *mut _);
    let frame = frame_of(target);
    let moved = frame != entry.placed;
    let stacked = edges_on_top_of(target, &edges);
    if !moved && stacked && !force {
        return;
    }
    entry.placed = frame;
    let topmost = is_topmost(target);
    let flags = SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSENDCHANGING | SWP_SHOWWINDOW;
    for (edge, (x, y, width, height)) in edges.iter().zip(edge_rects(frame)) {
        let edge = HWND(*edge as *mut _);
        unsafe {
            if stacked {
                let _ = SetWindowPos(edge, None, x, y, width, height, flags | SWP_NOZORDER);
                continue;
            }
            // In the same band as the window (always-on-top or not), then
            // right above it.
            if is_topmost(edge) != topmost {
                let band = if topmost {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                };
                let _ = SetWindowPos(
                    edge,
                    Some(band),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
            // Under the window above it; for the front window under an
            // always-on-top one (the notch, the dock), the top of the
            // ordinary windows, so the line never joins the always-on-top
            // ones and has to be put back on every look.
            let above = GetWindow(target, GW_HWNDPREV)
                .ok()
                .filter(|window| *window != edge && (topmost || !is_topmost(*window)))
                .unwrap_or(if topmost { HWND_TOPMOST } else { HWND_TOP });
            let _ = SetWindowPos(edge, Some(above), x, y, width, height, flags);
        }
    }
}

/// Bloom's border for an entry, as the settings now say: made, coloured,
/// placed, or taken away.
fn refresh(entry: &mut Snapped) {
    let wanted = ALPHA.load(Ordering::Acquire) > 0;
    match (wanted, entry.edges) {
        (true, None) => {
            entry.edges = create_edges();
            place(entry, true);
        }
        (false, Some(edges)) => {
            destroy_edges(edges);
            entry.edges = None;
        }
        (true, Some(edges)) => {
            let alpha = ALPHA.load(Ordering::Acquire).min(255) as u8;
            for edge in edges {
                let edge = HWND(edge as *mut _);
                unsafe {
                    let _ = SetLayeredWindowAttributes(edge, COLORREF(0), alpha, LWA_ALPHA);
                    let _ = InvalidateRect(Some(edge), None, true);
                }
            }
        }
        (false, None) => {}
    }
}

// ---------------------------------------------------------------- following windows

unsafe extern "system" fn moved(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW.0 || child != 0 || hwnd.0.is_null() {
        return;
    }
    let known = CHANGED
        .lock()
        .is_ok_and(|changed| changed.iter().any(|entry| entry.hwnd == hwnd.0 as isize));
    if known {
        check(hwnd);
    }
}

/// Follows the moves of exactly the apps whose windows have Bloom's border.
fn follow_moves() {
    let wanted: Vec<u32> = CHANGED
        .lock()
        .map(|changed| {
            changed
                .iter()
                .filter(|entry| entry.edges.is_some())
                .map(|entry| entry.pid)
                .collect()
        })
        .unwrap_or_default();
    MOVES.with(|moves| {
        let mut moves = moves.borrow_mut();
        moves.retain(|pid, hook| {
            let keep = wanted.contains(pid);
            if !keep {
                unsafe {
                    let _ = UnhookWinEvent(HWINEVENTHOOK(*hook as *mut _));
                }
            }
            keep
        });
        for pid in wanted {
            moves.entry(pid).or_insert_with(|| unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_LOCATIONCHANGE,
                    EVENT_OBJECT_LOCATIONCHANGE,
                    None,
                    Some(moved),
                    pid,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
                .0 as isize
            });
        }
    });
}

fn unfollow_all() {
    MOVES.with(|moves| {
        for (_, hook) in moves.borrow_mut().drain() {
            unsafe {
                let _ = UnhookWinEvent(HWINEVENTHOOK(hook as *mut _));
            }
        }
    });
}

// ---------------------------------------------------------------- snapped or not

/// Snapped windows lose Windows' border (and get Bloom's); ones no longer
/// snapped get Windows' back.
fn check(hwnd: HWND) {
    unsafe {
        if hwnd.0.is_null() {
            return;
        }
        let Ok(mut changed) = CHANGED.lock() else {
            return;
        };
        let known = changed
            .iter()
            .position(|entry| entry.hwnd == hwnd.0 as isize);
        if !IsWindow(Some(hwnd)).as_bool() {
            if let Some(index) = known {
                let entry = changed.remove(index);
                if let Some(edges) = entry.edges {
                    destroy_edges(edges);
                }
                drop(changed);
                follow_moves();
            }
            return;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return;
        }
        let snapped = IsWindowVisible(hwnd).as_bool()
            && !IsIconic(hwnd).as_bool()
            && IsWindowArranged(hwnd).as_bool()
            && !cloaked(hwnd);
        match (snapped, known) {
            (true, None) => {
                let saved = current_border(hwnd);
                set_border(hwnd, DWMWA_COLOR_NONE);
                let mut entry = Snapped {
                    hwnd: hwnd.0 as isize,
                    saved,
                    pid,
                    edges: None,
                    placed: RECT::default(),
                };
                refresh(&mut entry);
                let followed = entry.edges.is_some();
                changed.push(entry);
                drop(changed);
                if followed {
                    follow_moves();
                }
            }
            (true, Some(index)) => place(&mut changed[index], false),
            (false, Some(index)) => {
                let entry = changed.remove(index);
                if let Some(edges) = entry.edges {
                    destroy_edges(edges);
                }
                set_border(hwnd, border_when_free(entry.saved, accent_borders_on()));
                drop(changed);
                follow_moves();
            }
            (false, None) => {}
        }
    }
}

unsafe extern "system" fn event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW.0 || child != 0 || hwnd.0.is_null() {
        return;
    }
    check(GetAncestor(hwnd, GA_ROOT));
}

unsafe extern "system" fn visit(hwnd: HWND, _: LPARAM) -> windows::core::BOOL {
    check(hwnd);
    true.into()
}

/// Looks at every window Bloom changed (one gone, hidden or moved without an
/// event, say) and at the window in front.
fn check_all() {
    let windows: Vec<isize> = CHANGED
        .lock()
        .map(|changed| changed.iter().map(|entry| entry.hwnd).collect())
        .unwrap_or_default();
    for hwnd in windows {
        check(HWND(hwnd as *mut _));
    }
    check(unsafe { GetForegroundWindow() });
}

/// The colour or opacity changed: every border as they now say.
fn refresh_all() {
    update_brush();
    if let Ok(mut changed) = CHANGED.lock() {
        for entry in changed.iter_mut() {
            refresh(entry);
        }
    }
    follow_moves();
}

/// Gives every window Bloom changed the border Windows' setting gives it.
/// Safe from any thread; Bloom's own lines are left to `teardown`.
fn restore_all() {
    let Ok(changed) = CHANGED.lock() else {
        return;
    };
    if changed.is_empty() {
        return;
    }
    let accent_on = accent_borders_on();
    for entry in changed.iter() {
        let hwnd = HWND(entry.hwnd as *mut _);
        if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            set_border(hwnd, border_when_free(entry.saved, accent_on));
        }
    }
}

/// On the borders' thread as it ends: Windows' borders back, Bloom's gone.
fn teardown() {
    restore_all();
    let entries: Vec<Snapped> = CHANGED
        .lock()
        .map(|mut changed| std::mem::take(&mut *changed))
        .unwrap_or_default();
    for entry in entries {
        if let Some(edges) = entry.edges {
            destroy_edges(edges);
        }
    }
    unfollow_all();
    let brush = BRUSH.swap(0, Ordering::AcqRel);
    if brush != 0 {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(brush as *mut _));
        }
    }
}

fn start() {
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("bloom-snap-borders".into())
        .spawn(|| unsafe {
            THREAD.store(GetCurrentThreadId(), Ordering::Release);
            let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
            let hooks = [
                (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                (EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZEEND),
                (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
                (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
            ]
            .map(|(first, last)| SetWinEventHook(first, last, None, Some(event), 0, 0, flags));
            // Windows already snapped when Bloom starts.
            let _ = EnumWindows(Some(visit), LPARAM(0));
            let timer = SetTimer(None, 0, POLL_MS, None);
            let mut msg = MSG::default();
            while RUNNING.load(Ordering::Acquire) && GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.hwnd.0.is_null() {
                    if msg.message == WM_TIMER {
                        check_all();
                        continue;
                    }
                    if msg.message == MSG_REFRESH {
                        refresh_all();
                        continue;
                    }
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = KillTimer(None, timer);
            for hook in hooks {
                if !hook.is_invalid() {
                    let _ = UnhookWinEvent(hook);
                }
            }
            THREAD.store(0, Ordering::Release);
            teardown();
        });
    if spawned.is_err() {
        RUNNING.store(false, Ordering::Release);
    }
}

/// Stops watching and gives every window its border back. Called when the
/// setting is turned off and on every way Bloom quits.
pub fn stop() {
    if !RUNNING.swap(false, Ordering::AcqRel) {
        return;
    }
    let thread = THREAD.load(Ordering::Acquire);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    // Also here, at once, for a Bloom that exits right after.
    restore_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_windows_follow_windows_setting() {
        // Accent borders off: no border, as before the snap.
        assert_eq!(border_when_free(None, false), DWMWA_COLOR_NONE);
        assert_eq!(
            border_when_free(Some(DWMWA_COLOR_DEFAULT), false),
            DWMWA_COLOR_NONE
        );
        // Accent borders on: the accent border.
        assert_eq!(border_when_free(None, true), DWMWA_COLOR_DEFAULT);
        assert_eq!(
            border_when_free(Some(0x00FF8800), true),
            DWMWA_COLOR_DEFAULT
        );
        // An app that had turned its border off keeps it off.
        assert_eq!(
            border_when_free(Some(DWMWA_COLOR_NONE), true),
            DWMWA_COLOR_NONE
        );
    }

    #[test]
    fn colours_and_opacity() {
        assert_eq!(parse_color("#336699"), Some(0x0099_6633));
        assert_eq!(parse_color("ffffff"), Some(0x00FF_FFFF));
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("#GGGGGG"), None);
        assert_eq!(alpha_of(0.0), 0);
        assert_eq!(alpha_of(100.0), 255);
        assert_eq!(alpha_of(50.0), 128);
        assert_eq!(alpha_of(250.0), 255);
        assert_eq!(alpha_of(f64::NAN), 0);
    }

    #[test]
    fn one_pixel_lines_on_the_frame() {
        let frame = RECT {
            left: 0,
            top: 0,
            right: 1280,
            bottom: 1560,
        };
        assert_eq!(
            edge_rects(frame),
            [
                (0, 0, 1280, 1),
                (0, 1559, 1280, 1),
                (0, 0, 1, 1560),
                (1279, 0, 1, 1560)
            ]
        );
    }
}
