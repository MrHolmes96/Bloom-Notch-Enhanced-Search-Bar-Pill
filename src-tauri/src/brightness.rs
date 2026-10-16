//! Display brightness: Bloom's slider, and the brightness keys of any keyboard.
//!
//! Brightness keys reach Windows in two ways:
//! - As the HID "display brightness" keys that Logitech and most other
//!   keyboards (and many laptops) send. Bloom sees each press the moment it
//!   happens through Raw Input, which only listens: no key is blocked or
//!   changed, and no hook or extra program is involved.
//! - Through a laptop's firmware, which changes the built-in screen directly
//!   and never produces input. Windows' brightness event, a jump of its own
//!   10% step, is the only sign of such a press.
//!
//! Either way Bloom moves the brightness by the step chosen in Settings >
//! Overlays, counted from where it was, and the overlay shows that value.
//! Windows still applies its own step to keys it handles, so for a key Raw
//! Input saw Bloom sets the right value at once and puts it back if Windows'
//! step lands afterwards. On a PC without a built-in screen (a desktop, or a
//! laptop with its lid closed) Windows does nothing with these keys, and Bloom
//! changes the external monitors over DDC/CI instead, like its slider does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use wmi::{COMLibrary, WMIConnection};

use crate::state::{BRIGHTNESS_SENDER, CURRENT_BRIGHTNESS, LAST_BRIGHTNESS_CHANGE};
use crate::types::{BrightnessChangeEvent, WmiMonitorBrightness, WmiMonitorBrightnessEvent};
use crate::utils::get_now_ms;

/// A brightness to apply, and which screens it is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    pub value: u32,
    /// The built-in screen (Windows' brightness, through WMI).
    pub panel: bool,
    /// External monitors (DDC/CI).
    pub external: bool,
}

impl Job {
    /// Bloom's slider: every screen it can reach.
    pub fn everywhere(value: u32) -> Self {
        Self {
            value,
            panel: true,
            external: true,
        }
    }
    fn panel(value: u32) -> Self {
        Self {
            value,
            panel: true,
            external: false,
        }
    }
    fn external(value: u32) -> Self {
        Self {
            value,
            panel: false,
            external: true,
        }
    }
}

enum Signal {
    /// The built-in screen's brightness when Bloom starts; None without one.
    Start(Option<u32>),
    /// Windows reported a new brightness for the built-in screen.
    Observed(u32),
    /// A brightness key went down: +1 brighter, -1 darker.
    KeyDown(i64),
    /// The brightness key came back up.
    KeyUp,
    /// A write for the built-in screen found none (lid closed, or unplugged).
    NoPanel,
}

static SIGNALS: OnceLock<Sender<Signal>> = OnceLock::new();

fn signal(s: Signal) {
    if let Some(tx) = SIGNALS.get() {
        let _ = tx.send(s);
    }
}

/// Starts the writer that applies brightness for the slider and the keys.
/// Runs before the app is built, so the slider works from the first moment.
pub fn setup_worker() {
    let (tx, rx) = channel::<Job>();
    let _ = BRIGHTNESS_SENDER.set(tx);
    std::thread::Builder::new()
        .name("bloom-brightness-writer".into())
        .spawn(move || run_writer(rx))
        .ok();
}

/// Follows brightness changes for the overlay and handles the brightness keys.
pub fn start(app: AppHandle) {
    let (tx, rx) = channel::<Signal>();
    if SIGNALS.set(tx).is_err() {
        return;
    }
    start_wmi_watch();
    start_key_listener();
    std::thread::Builder::new()
        .name("bloom-brightness-keys".into())
        .spawn(move || run_controller(app, rx))
        .ok();
}

// ---------------------------------------------------------------- the step

/// Brightness key step from Settings > Overlays, in percent; 0 keeps the step
/// Windows uses.
fn brightness_key_step(app: &AppHandle) -> u32 {
    crate::utils::get_setting_str(app, "bloom-brightness-key-step")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(2)
        .min(50)
}

/// +1 or -1 when going from `before` to `now` looks like a brightness key
/// handled by the firmware: exactly Windows' 10% step, or a smaller one that
/// stopped at 0 or 100. Sliders and automatic dimming move by other amounts.
fn key_press_direction(before: u32, now: u32) -> Option<i64> {
    let delta = now as i64 - before as i64;
    let key_press = delta.abs() == 10 || ((now == 0 || now == 100) && delta.abs() < 10);
    (key_press && delta != 0).then_some(delta.signum())
}

fn stepped(level: u32, dir: i64, step: u32) -> u32 {
    (level as i64 + dir * step as i64).clamp(0, 100) as u32
}

// ---------------------------------------------------------------- key logic

/// How long after Bloom set a key's value Windows' own step for that press may
/// still land (it is then put back).
const EXPECT_MS: i64 = 1000;
/// Holding a key: first repeat after this long, then every `REPEAT_EVERY_MS`.
const REPEAT_DELAY_MS: i64 = 450;
const REPEAT_EVERY_MS: i64 = 110;
/// A press Windows reported a moment before Raw Input did is the same press.
const SAME_PRESS_MS: i64 = 400;
/// How long a value Bloom wrote is expected back from Windows.
const ECHO_MS: i64 = 1500;
/// No input for this long: a change is the screen dimming, not a key.
const IDLE_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Set(Job),
    Show(u32),
}

#[derive(Debug, Clone, Copy)]
struct Expect {
    target: u32,
    dir: i64,
    until: i64,
    windows_moved: bool,
    at_limit: bool,
}

/// The brightness key logic, free of side effects: each input returns what to
/// set and what the overlay shows.
#[derive(Debug, Default)]
struct Keys {
    /// Whether there is a built-in screen Windows controls; None until known.
    panel: Option<bool>,
    /// The brightness the keys count from.
    level: Option<u32>,
    /// The last value Windows reported.
    reported: Option<u32>,
    /// Values Bloom wrote, until when Windows may still report them back.
    echoes: Vec<(u32, i64)>,
    /// After a key Raw Input saw: the value Bloom set for it.
    expect: Option<Expect>,
    /// Whether Windows itself steps the brightness for keys Raw Input sees;
    /// learned from the first press.
    windows_steps: Option<bool>,
    /// The key held down right now, and since when.
    held: Option<(i64, i64)>,
    /// When Bloom repeats a held key itself (Windows doesn't handle it).
    next_repeat: Option<i64>,
    /// A press already recognised from Windows' event, and when.
    guessed: Option<(i64, i64)>,
}

impl Keys {
    fn start(&mut self, initial: Option<u32>) {
        if self.panel.is_none() {
            self.panel = Some(initial.is_some());
        }
        if let Some(value) = initial {
            self.level.get_or_insert(value);
            self.reported = Some(value);
        }
    }

    /// Windows reported `value`. `slider` is Bloom's slider value while it is
    /// being used; `idle` is true when there has been no input for a while.
    fn observed(
        &mut self,
        value: u32,
        now: i64,
        step: u32,
        slider: Option<u32>,
        idle: bool,
    ) -> Vec<Action> {
        self.panel = Some(true);
        self.echoes.retain(|(_, until)| *until > now);
        if let Some(position) = slider {
            self.level = Some(position);
            self.reported = Some(value);
            self.expect = None;
            return vec![];
        }

        // A key Raw Input saw, whose value Bloom already set.
        if let Some(expect) = self.expect.as_mut() {
            if now < expect.until {
                self.reported = Some(value);
                if value == expect.target {
                    return vec![];
                }
                expect.windows_moved = true;
                let (target, dir) = (expect.target, expect.dir);
                self.windows_steps = Some(true);
                // Windows repeats a key that is held down: one more step each.
                let repeating = self
                    .held
                    .is_some_and(|(d, since)| d == dir && now - since >= REPEAT_DELAY_MS);
                if repeating {
                    return self.press(dir, now, step);
                }
                return vec![Action::Set(Job::panel(target))];
            }
            self.expect = None;
        }

        // A value Bloom wrote coming back.
        if let Some(index) = self.echoes.iter().position(|(v, _)| *v == value) {
            self.echoes.remove(index);
            self.reported = Some(value);
            return vec![];
        }

        let before = self.level.unwrap_or(value);
        let reported = self.reported.replace(value);
        if value == before {
            return vec![];
        }
        // A firmware key: recognised from Windows' step, counted from where
        // the brightness was. A quick second press can start from Windows'
        // uncorrected value, so the step is recognised from either.
        let dir = if (1..10).contains(&step) && !idle {
            key_press_direction(before, value)
                .or_else(|| reported.and_then(|r| key_press_direction(r, value)))
        } else {
            None
        };
        match dir {
            Some(dir) => {
                let target = stepped(before, dir, step);
                self.level = Some(target);
                self.guessed = Some((dir, now));
                let mut actions = Vec::new();
                if target != value {
                    self.echoes.push((target, now + ECHO_MS));
                    actions.push(Action::Set(Job::panel(target)));
                }
                actions.push(Action::Show(target));
                actions
            }
            None => {
                self.level = Some(value);
                vec![Action::Show(value)]
            }
        }
    }

    fn key_down(&mut self, dir: i64, now: i64, step: u32) -> Vec<Action> {
        self.held = Some((dir, now));
        // Windows' event for this press arrived first and was handled then.
        if let Some((d, at)) = self.guessed.take() {
            if d == dir && now - at < SAME_PRESS_MS {
                self.windows_steps = Some(true);
                return vec![];
            }
        }
        let actions = self.press(dir, now, step);
        self.next_repeat = (self.panel == Some(false) || self.windows_steps != Some(true))
            .then_some(now + REPEAT_DELAY_MS);
        actions
    }

    fn key_up(&mut self) {
        self.held = None;
        self.next_repeat = None;
    }

    fn press(&mut self, dir: i64, now: i64, step: u32) -> Vec<Action> {
        let Some(base) = self.level else {
            return vec![];
        };
        if step == 0 {
            return vec![];
        }
        let target = stepped(base, dir, step);
        self.level = Some(target);
        if self.panel == Some(false) {
            let mut actions = Vec::new();
            if target != base {
                actions.push(Action::Set(Job::external(target)));
            }
            actions.push(Action::Show(target));
            return actions;
        }
        self.expect = Some(Expect {
            target,
            dir,
            until: now + EXPECT_MS,
            windows_moved: false,
            at_limit: target == base,
        });
        let mut actions = Vec::new();
        if target != base {
            actions.push(Action::Set(Job::panel(target)));
        }
        actions.push(Action::Show(target));
        actions
    }

    fn tick(&mut self, now: i64, step: u32) -> Vec<Action> {
        if let Some(expect) = self.expect {
            if now >= expect.until {
                // Nothing came from Windows for a press it could have moved:
                // it leaves these keys to Bloom.
                if !expect.windows_moved && !expect.at_limit && self.windows_steps.is_none() {
                    self.windows_steps = Some(false);
                }
                self.expect = None;
            }
        }
        let mut actions = Vec::new();
        if let Some(at) = self.next_repeat {
            if now >= at {
                match self.held {
                    Some((dir, _))
                        if self.panel == Some(false) || self.windows_steps != Some(true) =>
                    {
                        actions = self.press(dir, now, step);
                        self.next_repeat = Some(now + REPEAT_EVERY_MS);
                    }
                    _ => self.next_repeat = None,
                }
            }
        }
        actions
    }

    fn next_deadline(&self) -> Option<i64> {
        [self.expect.map(|e| e.until), self.next_repeat]
            .into_iter()
            .flatten()
            .min()
    }
}

fn user_idle_ms() -> u64 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        GetTickCount().wrapping_sub(info.dwTime) as u64
    }
}

fn run_controller(app: AppHandle, rx: Receiver<Signal>) {
    let mut keys = Keys::default();
    // External monitors are read once, on the first key press that needs them.
    let mut external_read = false;
    loop {
        let signal = match keys.next_deadline() {
            Some(at) => {
                let wait = (at - get_now_ms()).max(0) as u64;
                match rx.recv_timeout(Duration::from_millis(wait)) {
                    Ok(s) => Some(s),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            None => match rx.recv() {
                Ok(s) => Some(s),
                Err(_) => return,
            },
        };
        let now = get_now_ms();
        let step = brightness_key_step(&app);
        let slider_in_use = now - LAST_BRIGHTNESS_CHANGE.load(Ordering::Relaxed) < 2000;
        let mut actions = match signal {
            Some(Signal::Start(initial)) => {
                if let Some(value) = initial {
                    CURRENT_BRIGHTNESS.store(value, Ordering::Relaxed);
                }
                keys.start(initial);
                vec![]
            }
            Some(Signal::Observed(value)) => keys.observed(
                value,
                now,
                step,
                slider_in_use.then(|| CURRENT_BRIGHTNESS.load(Ordering::Relaxed)),
                user_idle_ms() > IDLE_MS,
            ),
            Some(Signal::KeyDown(dir)) => {
                if keys.panel == Some(false) && !external_read {
                    external_read = true;
                    if let Some(value) = read_external_brightness() {
                        CURRENT_BRIGHTNESS.store(value, Ordering::Relaxed);
                    }
                }
                // The slider and the overlay keep this current, so the keys
                // count from what the screen shows now.
                if keys.expect.is_none() || slider_in_use {
                    keys.level = Some(CURRENT_BRIGHTNESS.load(Ordering::Relaxed));
                }
                keys.key_down(dir, now, step)
            }
            Some(Signal::KeyUp) => {
                keys.key_up();
                vec![]
            }
            Some(Signal::NoPanel) => {
                if keys.panel != Some(false) {
                    keys.panel = Some(false);
                    keys.expect = None;
                    external_read = false;
                }
                vec![]
            }
            None => vec![],
        };
        actions.extend(keys.tick(now, step));
        for action in actions {
            match action {
                Action::Set(job) => {
                    if let Some(tx) = BRIGHTNESS_SENDER.get() {
                        let _ = tx.send(job);
                    }
                }
                Action::Show(value) => {
                    CURRENT_BRIGHTNESS.store(value, Ordering::Relaxed);
                    let _ = app.emit(
                        "brightness-change",
                        BrightnessChangeEvent { brightness: value },
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------- Windows' brightness events

fn start_wmi_watch() {
    std::thread::Builder::new()
        .name("bloom-brightness-watch".into())
        .spawn(|| {
            let Ok(com) = COMLibrary::new() else {
                signal(Signal::Start(None));
                return;
            };
            let Ok(wmi) = WMIConnection::with_namespace_path("root\\WMI", com) else {
                signal(Signal::Start(None));
                return;
            };
            let initial = wmi
                .query::<WmiMonitorBrightness>()
                .ok()
                .and_then(|r| r.first().map(|b| b.current_brightness as u32));
            signal(Signal::Start(initial));
            // Every change to the built-in screen, whatever made it, the
            // moment it happens.
            if let Ok(events) = wmi.raw_notification::<WmiMonitorBrightnessEvent>(
                "SELECT * FROM WmiMonitorBrightnessEvent",
            ) {
                for event in events {
                    let Ok(event) = event else { break };
                    signal(Signal::Observed(event.brightness as u32));
                }
            }
            // Without the event, look every 1.5 seconds instead.
            let mut last = initial;
            loop {
                std::thread::sleep(Duration::from_millis(1500));
                if let Some(value) = wmi
                    .query::<WmiMonitorBrightness>()
                    .ok()
                    .and_then(|r| r.first().map(|b| b.current_brightness as u32))
                {
                    if last != Some(value) {
                        last = Some(value);
                        signal(Signal::Observed(value));
                    }
                }
            }
        })
        .ok();
}

// ---------------------------------------------------------------- the keys, through Raw Input

/// HID Consumer page usages: Display Brightness Increment / Decrement.
const BRIGHTNESS_UP: u16 = 0x6F;
const BRIGHTNESS_DOWN: u16 = 0x70;

#[derive(Default)]
struct RawKeys {
    /// Each keyboard's HID report layout.
    layouts: HashMap<isize, Vec<u64>>,
    /// Which brightness key each keyboard has down (+1 / -1).
    down: HashMap<isize, i64>,
}

thread_local! {
    static RAW: RefCell<RawKeys> = RefCell::new(RawKeys::default());
}

fn start_key_listener() {
    std::thread::Builder::new()
        .name("bloom-brightness-input".into())
        .spawn(|| unsafe {
            use windows::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows::Win32::UI::Input::{
                RegisterRawInputDevices, RAWINPUTDEVICE, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK,
            };
            use windows::Win32::UI::WindowsAndMessaging::*;

            let class_name = windows::core::w!("BloomBrightnessKeys");
            let instance: windows::Win32::Foundation::HINSTANCE =
                GetModuleHandleW(None).unwrap_or_default().into();
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(raw_input_proc),
                hInstance: instance,
                lpszClassName: class_name,
                ..Default::default()
            };
            if RegisterClassExW(&class) == 0 {
                return;
            }
            // Never shown; it only receives the keys' input.
            let Ok(hwnd) = CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0),
                class_name,
                windows::core::PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            ) else {
                return;
            };
            // Consumer Control: the media, volume and brightness keys. Listen
            // only, in the background; Windows and every app still get them.
            let device = RAWINPUTDEVICE {
                usUsagePage: 0x0C,
                usUsage: 0x01,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            };
            if RegisterRawInputDevices(&[device], std::mem::size_of::<RAWINPUTDEVICE>() as u32)
                .is_err()
            {
                let _ = DestroyWindow(hwnd);
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .ok();
}

unsafe extern "system" fn raw_input_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GIDC_REMOVAL, WM_INPUT, WM_INPUT_DEVICE_CHANGE,
    };
    match msg {
        WM_INPUT => {
            read_raw_input(lparam);
        }
        WM_INPUT_DEVICE_CHANGE if wparam.0 as u32 == GIDC_REMOVAL => {
            let device = lparam.0;
            RAW.with(|raw| {
                let mut raw = raw.borrow_mut();
                raw.layouts.remove(&device);
                if raw.down.remove(&device).is_some() {
                    signal(Signal::KeyUp);
                }
            });
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn read_raw_input(lparam: windows::Win32::Foundation::LPARAM) {
    use windows::Win32::UI::Input::{
        GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTHEADER, RID_INPUT, RIM_TYPEHID,
    };
    let handle = HRAWINPUT(lparam.0 as *mut _);
    let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0u32;
    if GetRawInputData(handle, RID_INPUT, None, &mut size, header_size) != 0 || size == 0 {
        return;
    }
    // u64 cells keep the RAWINPUT structure aligned.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    let read = GetRawInputData(
        handle,
        RID_INPUT,
        Some(buffer.as_mut_ptr() as *mut _),
        &mut size,
        header_size,
    );
    if read == u32::MAX || read < header_size {
        return;
    }
    let input = &*(buffer.as_ptr() as *const RAWINPUT);
    if input.header.dwType != RIM_TYPEHID.0 {
        return;
    }
    let device = input.header.hDevice.0 as isize;
    let report_size = input.data.hid.dwSizeHid as usize;
    let count = input.data.hid.dwCount as usize;
    if report_size == 0 || count == 0 {
        return;
    }
    let reports_start = std::ptr::addr_of!(input.data.hid.bRawData) as *const u8;
    let offset = reports_start as usize - buffer.as_ptr() as usize;
    let available = (read as usize).saturating_sub(offset);
    if report_size * count > available {
        return;
    }
    for index in 0..count {
        let mut report =
            std::slice::from_raw_parts(reports_start.add(index * report_size), report_size)
                .to_vec();
        if let Some(dir) = brightness_in_report(device, &mut report) {
            RAW.with(|raw| {
                let mut raw = raw.borrow_mut();
                let before = raw.down.get(&device).copied().unwrap_or(0);
                if dir == before {
                    return;
                }
                if dir == 0 {
                    raw.down.remove(&device);
                    signal(Signal::KeyUp);
                } else {
                    raw.down.insert(device, dir);
                    signal(Signal::KeyDown(dir));
                }
            });
        }
    }
}

/// The brightness key a Consumer Control report has down: +1, -1, 0 for
/// none, or None when the report isn't one this keyboard describes as such.
unsafe fn brightness_in_report(device: isize, report: &mut [u8]) -> Option<i64> {
    use windows::Win32::Devices::HumanInterfaceDevice::{
        HidP_GetUsages, HidP_Input, HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
    };
    RAW.with(|raw| {
        let mut raw = raw.borrow_mut();
        if let std::collections::hash_map::Entry::Vacant(entry) = raw.layouts.entry(device) {
            entry.insert(report_layout(device)?);
        }
        let layout = raw.layouts.get(&device)?;
        let mut usages = [0u16; 32];
        let mut length = usages.len() as u32;
        let status = HidP_GetUsages(
            HidP_Input,
            0x0C,
            None,
            usages.as_mut_ptr(),
            &mut length,
            PHIDP_PREPARSED_DATA(layout.as_ptr() as isize),
            report,
        );
        if status != HIDP_STATUS_SUCCESS {
            return None;
        }
        let pressed = &usages[..(length as usize).min(usages.len())];
        let up = pressed.contains(&BRIGHTNESS_UP);
        let down = pressed.contains(&BRIGHTNESS_DOWN);
        Some(match (up, down) {
            (true, false) => 1,
            (false, true) => -1,
            _ => 0,
        })
    })
}

/// The keyboard's HID report layout ("preparsed data"), which says where in a
/// report each key sits.
unsafe fn report_layout(device: isize) -> Option<Vec<u64>> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::UI::Input::{GetRawInputDeviceInfoW, RIDI_PREPARSEDDATA};
    let handle = HANDLE(device as *mut _);
    let mut size = 0u32;
    GetRawInputDeviceInfoW(Some(handle), RIDI_PREPARSEDDATA, None, &mut size);
    if size == 0 || size > 1 << 20 {
        return None;
    }
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    let read = GetRawInputDeviceInfoW(
        Some(handle),
        RIDI_PREPARSEDDATA,
        Some(buffer.as_mut_ptr() as *mut _),
        &mut size,
    );
    (read != u32::MAX && read > 0).then_some(buffer)
}

// ---------------------------------------------------------------- writing brightness

fn run_writer(rx: Receiver<Job>) {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut panel = PanelWriter::connect();
    while let Ok(mut job) = rx.recv() {
        // A slider drag or a held key queues a value per step and each write
        // takes a moment, so only the latest value is written, to every
        // screen any of the queued values was for.
        while let Ok(newer) = rx.try_recv() {
            job = Job {
                value: newer.value,
                panel: job.panel || newer.panel,
                external: job.external || newer.external,
            };
        }
        let value = job.value.min(100);
        if job.panel {
            let written = panel.as_mut().is_some_and(|p| p.set(value));
            // Only the keys write the built-in screen alone; with none there,
            // they turn to the external monitors from the next press.
            if !written && !job.external {
                signal(Signal::NoPanel);
            }
        }
        if job.external {
            set_external_brightness(value);
        }
    }
}

/// Writes the built-in screen's brightness through WMI, keeping the
/// connection and the screen's object path between writes so a key press is
/// corrected as fast as possible.
struct PanelWriter {
    services: windows::Win32::System::Wmi::IWbemServices,
    params: windows::Win32::System::Wmi::IWbemClassObject,
    paths: Vec<windows::core::BSTR>,
}

impl PanelWriter {
    fn connect() -> Option<Self> {
        use windows::core::BSTR;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoSetProxyBlanket, CLSCTX_ALL, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
        };
        use windows::Win32::System::Wmi::{
            IWbemClassObject, IWbemLocator, WbemLocator, WBEM_GENERIC_FLAG_TYPE,
        };
        unsafe {
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_ALL).ok()?;
            let empty = BSTR::new();
            let services = locator
                .ConnectServer(
                    &BSTR::from("root\\WMI"),
                    &empty,
                    &empty,
                    &empty,
                    0,
                    &empty,
                    None,
                )
                .ok()?;
            // Without impersonation on the proxy, WMI rejects calls with
            // WBEM_E_ACCESS_DENIED whenever process-wide COM security was not
            // initialized for it. 10 = RPC_C_AUTHN_WINNT, 0 = RPC_C_AUTHZ_NONE.
            let _ = CoSetProxyBlanket(
                &services,
                10,
                0,
                windows::core::PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            );
            // GetMethod only works on the class definition, not an instance.
            let mut class: Option<IWbemClassObject> = None;
            services
                .GetObject(
                    &BSTR::from("WmiMonitorBrightnessMethods"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    Some(&mut class),
                    None,
                )
                .ok()?;
            let mut params: Option<IWbemClassObject> = None;
            class?
                .GetMethod(
                    windows::core::w!("WmiSetBrightness"),
                    0i32,
                    &mut params,
                    std::ptr::null_mut(),
                )
                .ok()?;
            Some(Self {
                services,
                params: params?,
                paths: Vec::new(),
            })
        }
    }

    /// The object path of each built-in screen.
    fn find_screens(&self) -> Vec<windows::core::BSTR> {
        use windows::core::BSTR;
        use windows::Win32::System::Variant::{VariantClear, VARIANT};
        use windows::Win32::System::Wmi::{IWbemClassObject, WBEM_GENERIC_FLAG_TYPE};
        let mut paths = Vec::new();
        unsafe {
            let Ok(found) = self.services.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from("SELECT * FROM WmiMonitorBrightnessMethods"),
                WBEM_GENERIC_FLAG_TYPE(0),
                None,
            ) else {
                return paths;
            };
            let mut row = [None::<IWbemClassObject>; 1];
            let mut returned = 0u32;
            while found.Next(-1i32, &mut row, &mut returned).is_ok() && returned > 0 {
                let Some(object) = row[0].take() else {
                    continue;
                };
                let mut value = VARIANT::default();
                if object
                    .Get(windows::core::w!("__RELPATH"), 0i32, &mut value, None, None)
                    .is_ok()
                {
                    let path = value.Anonymous.Anonymous.Anonymous.bstrVal.to_string();
                    let _ = VariantClear(&mut value);
                    if !path.is_empty() {
                        paths.push(BSTR::from(path.as_str()));
                    }
                }
            }
        }
        paths
    }

    fn write(&self, path: &windows::core::BSTR, value: u32) -> bool {
        use windows::core::BSTR;
        use windows::Win32::System::Variant::{VARENUM, VARIANT};
        use windows::Win32::System::Wmi::WBEM_GENERIC_FLAG_TYPE;
        unsafe {
            let Ok(input) = self.params.SpawnInstance(0i32) else {
                return false;
            };
            let mut brightness = VARIANT::default();
            let inner = &mut brightness.Anonymous.Anonymous;
            inner.vt = VARENUM(17); // VT_UI1
            inner.Anonymous.bVal = value as u8;
            let _ = input.Put(windows::core::w!("Brightness"), 0i32, &brightness, 0);
            let mut timeout = VARIANT::default();
            let inner = &mut timeout.Anonymous.Anonymous;
            inner.vt = VARENUM(3); // VT_I4
            inner.Anonymous.lVal = 0;
            let _ = input.Put(windows::core::w!("Timeout"), 0i32, &timeout, 0);
            self.services
                .ExecMethod(
                    path,
                    &BSTR::from("WmiSetBrightness"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    Some(&input),
                    None,
                    None,
                )
                .is_ok()
        }
    }

    /// False when there is no built-in screen to write to.
    fn set(&mut self, value: u32) -> bool {
        if !self.paths.is_empty() && self.paths.iter().all(|p| self.write(p, value)) {
            return true;
        }
        // The screens changed (lid, dock) or were never looked up: look again.
        self.paths = self.find_screens();
        let mut written = false;
        for path in &self.paths {
            written |= self.write(path, value);
        }
        written
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PhysicalMonitor {
    handle: windows::Win32::Foundation::HANDLE,
    description: [u16; 128],
}

#[link(name = "dxva2")]
extern "system" {
    fn GetNumberOfPhysicalMonitorsFromHMONITOR(
        monitor: windows::Win32::Graphics::Gdi::HMONITOR,
        count: *mut u32,
    ) -> windows::core::BOOL;
    fn GetPhysicalMonitorsFromHMONITOR(
        monitor: windows::Win32::Graphics::Gdi::HMONITOR,
        count: u32,
        monitors: *mut PhysicalMonitor,
    ) -> windows::core::BOOL;
    fn DestroyPhysicalMonitors(count: u32, monitors: *mut PhysicalMonitor) -> windows::core::BOOL;
    fn SetMonitorBrightness(
        monitor: windows::Win32::Foundation::HANDLE,
        brightness: u32,
    ) -> windows::core::BOOL;
    fn GetMonitorBrightness(
        monitor: windows::Win32::Foundation::HANDLE,
        minimum: *mut u32,
        current: *mut u32,
        maximum: *mut u32,
    ) -> windows::core::BOOL;
}

/// Runs `f` on every external monitor that takes DDC/CI commands, until it
/// returns false.
fn for_each_physical_monitor(mut f: impl FnMut(windows::Win32::Foundation::HANDLE) -> bool) {
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        list: LPARAM,
    ) -> windows::core::BOOL {
        let list = &mut *(list.0 as *mut Vec<HMONITOR>);
        list.push(monitor);
        true.into()
    }

    let mut monitors: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut monitors as *mut Vec<HMONITOR> as isize),
        );
        for monitor in monitors {
            let mut count = 0u32;
            if !GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut count).as_bool() || count == 0
            {
                continue;
            }
            let mut physical = vec![
                PhysicalMonitor {
                    handle: Default::default(),
                    description: [0; 128],
                };
                count as usize
            ];
            if !GetPhysicalMonitorsFromHMONITOR(monitor, count, physical.as_mut_ptr()).as_bool() {
                continue;
            }
            let mut go_on = true;
            for item in &physical {
                if go_on && !item.handle.is_invalid() {
                    go_on = f(item.handle);
                }
            }
            let _ = DestroyPhysicalMonitors(count, physical.as_mut_ptr());
            if !go_on {
                return;
            }
        }
    }
}

/// External monitors (DDC/CI). Slow, so only the latest value is ever sent.
fn set_external_brightness(value: u32) {
    for_each_physical_monitor(|monitor| {
        unsafe {
            let _ = SetMonitorBrightness(monitor, value);
        }
        true
    });
}

/// The first external monitor's brightness, in percent.
fn read_external_brightness() -> Option<u32> {
    let mut found = None;
    for_each_physical_monitor(|monitor| {
        let (mut min, mut current, mut max) = (0u32, 0u32, 0u32);
        if unsafe { GetMonitorBrightness(monitor, &mut min, &mut current, &mut max) }.as_bool()
            && max > min
        {
            found = Some(((current.clamp(min, max) - min) * 100 / (max - min)).min(100));
            return false;
        }
        true
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel_keys(level: u32) -> Keys {
        let mut keys = Keys::default();
        keys.start(Some(level));
        keys
    }

    #[test]
    fn a_firmware_key_moves_by_the_chosen_step() {
        let mut keys = panel_keys(50);
        let actions = keys.observed(60, 0, 2, None, false);
        assert_eq!(actions, vec![Action::Set(Job::panel(52)), Action::Show(52)]);
        // Bloom's own write coming back changes nothing.
        assert!(keys.observed(52, 30, 2, None, false).is_empty());
        // The next press counts from 52.
        let actions = keys.observed(62, 500, 2, None, false);
        assert_eq!(actions, vec![Action::Set(Job::panel(54)), Action::Show(54)]);
    }

    #[test]
    fn a_quick_second_firmware_press_still_counts() {
        let mut keys = panel_keys(50);
        keys.observed(60, 0, 2, None, false);
        // Windows stepped again from its own 60 before Bloom's 52 landed.
        let actions = keys.observed(70, 20, 2, None, false);
        assert_eq!(actions, vec![Action::Set(Job::panel(54)), Action::Show(54)]);
        // Both of Bloom's writes come back late and are ignored.
        assert!(keys.observed(52, 40, 2, None, false).is_empty());
        assert!(keys.observed(54, 60, 2, None, false).is_empty());
        assert_eq!(keys.level, Some(54));
    }

    #[test]
    fn a_raw_key_is_set_at_once_and_windows_step_is_put_back() {
        let mut keys = panel_keys(50);
        let actions = keys.key_down(1, 0, 2);
        assert_eq!(actions, vec![Action::Set(Job::panel(52)), Action::Show(52)]);
        // Bloom's write, then Windows' own 10% for the same press.
        assert!(keys.observed(52, 15, 2, None, false).is_empty());
        assert_eq!(
            keys.observed(60, 25, 2, None, false),
            vec![Action::Set(Job::panel(52))]
        );
        assert_eq!(keys.windows_steps, Some(true));
        keys.key_up();
        assert!(keys.tick(2000, 2).is_empty());
        assert_eq!(keys.level, Some(52));
    }

    #[test]
    fn windows_event_before_raw_input_is_one_press() {
        let mut keys = panel_keys(50);
        keys.observed(60, 0, 2, None, false);
        assert!(keys.key_down(1, 50, 2).is_empty());
        assert_eq!(keys.level, Some(52));
    }

    #[test]
    fn keys_windows_ignores_are_handled_and_repeat_when_held() {
        let mut keys = panel_keys(50);
        keys.key_down(-1, 0, 2);
        assert_eq!(keys.level, Some(48));
        // Nothing from Windows: Bloom repeats the held key itself.
        assert_eq!(
            keys.tick(450, 2),
            vec![Action::Set(Job::panel(46)), Action::Show(46)]
        );
        assert_eq!(
            keys.tick(560, 2),
            vec![Action::Set(Job::panel(44)), Action::Show(44)]
        );
        keys.key_up();
        assert!(keys.tick(2000, 2).is_empty());
        assert_eq!(keys.windows_steps, Some(false));
    }

    #[test]
    fn windows_repeat_of_a_held_key_steps_again() {
        let mut keys = panel_keys(50);
        keys.key_down(1, 0, 2);
        keys.observed(60, 20, 2, None, false);
        // Windows repeats the held key after its delay.
        assert_eq!(
            keys.observed(62, 520, 2, None, false),
            vec![Action::Set(Job::panel(54)), Action::Show(54)]
        );
        // Windows stepping, so Bloom doesn't repeat it as well.
        assert!(keys.tick(600, 2).is_empty());
    }

    #[test]
    fn without_a_built_in_screen_keys_change_external_monitors() {
        let mut keys = Keys::default();
        keys.start(None);
        keys.level = Some(40);
        assert_eq!(
            keys.key_down(1, 0, 5),
            vec![Action::Set(Job::external(45)), Action::Show(45)]
        );
    }

    #[test]
    fn slider_dimming_and_windows_step_are_left_alone() {
        let mut keys = panel_keys(50);
        assert!(keys.observed(37, 0, 2, Some(37), false).is_empty());
        assert_eq!(keys.level, Some(37));
        // Screen dimming after a long idle time.
        assert_eq!(
            keys.observed(27, 5000, 2, None, true),
            vec![Action::Show(27)]
        );
        // "Windows default" leaves the key's step as it is.
        assert_eq!(
            keys.observed(37, 9000, 0, None, false),
            vec![Action::Show(37)]
        );
        assert!(keys.key_down(1, 9500, 0).is_empty());
    }

    #[test]
    fn limits_hold() {
        let mut keys = panel_keys(99);
        assert_eq!(
            keys.key_down(1, 0, 2),
            vec![Action::Set(Job::panel(100)), Action::Show(100)]
        );
        keys.key_up();
        keys.tick(2000, 2);
        assert_eq!(keys.key_down(1, 3000, 2), vec![Action::Show(100)]);
        assert_eq!(key_press_direction(95, 100), Some(1));
        assert_eq!(key_press_direction(50, 53), None);
    }
}
