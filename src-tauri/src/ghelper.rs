//! G-Helper in the notch (Settings > Integrations > G-Helper): its modes, GPU
//! modes, screen and battery charge limit, its key bindings, and Bloom
//! shortcuts for each of them.
//!
//! G-Helper stays in charge: it is what applies a mode with its power limits
//! and fan curves, switches the GPU, and puts its settings back after sleep.
//! It has no way for another app to ask it something, so Bloom uses what it
//! does offer, in this order:
//!
//! 1. Its own keyboard shortcuts (each mode, Eco and Standard GPU, the refresh
//!    rate, its window, the microphone). Bloom presses them; G-Helper does the
//!    rest exactly as if they were pressed on the keyboard.
//! 2. Its settings file, for what has no shortcut (Auto refresh rate,
//!    Optimized GPU, the charge limit, the key bindings). G-Helper reads that
//!    file only when it starts and writes it from memory, so Bloom closes it
//!    with the signal G-Helper made for that ("Global\GHelperApp-Exit"),
//!    makes the change, and starts it again with its own startup task. It
//!    starts with the change applied, as after a reboot.
//! 3. The ASUS hardware interface G-Helper itself uses, for what G-Helper
//!    only reads back from the hardware: Ultimate GPU (with a restart, after
//!    the user confirms) and Multi Zone. And for the temperatures and fans.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};

/// G-Helper's own mode numbers for its three built-in modes; custom modes are
/// 3 and up.
const MODE_BALANCED: i64 = 0;
const MODE_TURBO: i64 = 1;
const MODE_SILENT: i64 = 2;
const GPU_ECO: i64 = 0;
const GPU_STANDARD: i64 = 1;
const GPU_ULTIMATE: i64 = 2;

// The ASUS hardware interface (ATKACPI), as G-Helper's AsusACPI.cs uses it.
const ACPI_CONTROL: u32 = 0x0022_240C;
const ACPI_DSTS: u32 = 0x5354_5344;
const ACPI_DEVS: u32 = 0x5356_4544;
const GPU_ECO_ROG: u32 = 0x0009_0020;
const GPU_ECO_VIVO: u32 = 0x0009_0120;
const GPU_MUX_ROG: u32 = 0x0009_0016;
const GPU_MUX_VIVO: u32 = 0x0009_0026;
const MINILED_1: u32 = 0x0005_001E;
const MINILED_2: u32 = 0x0005_002E;
const TEMP_CPU: u32 = 0x0012_0094;
const TEMP_GPU: u32 = 0x0012_0097;
const FAN_CPU: u32 = 0x0011_0013;
const FAN_GPU: u32 = 0x0011_0014;
const FAN_MID: u32 = 0x0011_0031;
/// Hands a hotkey code to the firmware, as the laptop's own key would.
const UNIVERSAL_CONTROL: u32 = 0x0010_0021;

// Virtual keys.
const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_LWIN: u16 = 0x5B;
const VK_F5: u16 = 0x74;
const VK_F12: u16 = 0x7B;
const VK_F13: u16 = 0x7C;
const VK_F14: u16 = 0x7D;
const VK_F15: u16 = 0x7E;
const VK_F16: u16 = 0x7F;
const VK_F17: u16 = 0x80;
const VK_F18: u16 = 0x81;
const VK_F19: u16 = 0x82;
const VK_F20: u16 = 0x83;

/// The keys G-Helper's key-binding page has on a ROG laptop, in its order.
pub const BINDING_KEYS: [(&str, &str); 7] = [
    ("m1", "M1"),
    ("m2", "M2"),
    ("m3", "M3"),
    ("m4", "M4/ROG"),
    ("fnf4", "FN+F4"),
    ("fnc", "FN+C"),
    ("fnv", "FN+V"),
];

// ---------------------------------------------------------------- state

#[derive(Serialize, Clone)]
pub struct Mode {
    id: i64,
    name: String,
    /// The built-in mode a custom mode is based on (its colour).
    base: i64,
}

#[derive(Serialize, Clone)]
pub struct Binding {
    key: String,
    label: String,
    action: String,
    custom: String,
}

#[derive(Serialize, Clone, Default)]
pub struct Sensors {
    /// G-Helper's own CPU line ("CPU: 71°C Fan: 1900RPM"), from its tray
    /// icon: the very values its window shows.
    cpu_text: Option<String>,
    /// G-Helper's own GPU line, the same way (none while the GPU is off).
    gpu_text: Option<String>,
    cpu_temp: Option<i32>,
    gpu_temp: Option<i32>,
    cpu_fan: Option<i32>,
    gpu_fan: Option<i32>,
    mid_fan: Option<i32>,
}

#[derive(Serialize, Clone, Default)]
pub struct State {
    /// G-Helper's settings file was found.
    installed: bool,
    running: bool,
    modes: Vec<Mode>,
    mode: i64,
    /// 0 Eco, 1 Standard, 2 Ultimate; -1 when the laptop has no GPU modes.
    gpu_mode: i64,
    gpu_auto: bool,
    /// The laptop can switch to Ultimate (has a MUX switch).
    has_ultimate: bool,
    screen_auto: bool,
    refresh: i32,
    refresh_min: i32,
    refresh_max: i32,
    /// Multi Zone: null when the screen has none; 0 off, 1 on, 2 strong.
    miniled: Option<i32>,
    charge_limit: i64,
    bindings: Vec<Binding>,
    /// G-Helper's shortcuts are turned off (its "skip_hotkeys").
    shortcuts_off: bool,
    sensors: Sensors,
    /// Bloom is closing and starting G-Helper to apply a change.
    applying: bool,
}

static APPLYING: AtomicBool = AtomicBool::new(false);
/// One change through the settings file at a time.
static RESTART_LOCK: Mutex<()> = Mutex::new(());
/// Where G-Helper was last seen running from.
static EXE: Mutex<Option<PathBuf>> = Mutex::new(None);

fn emit_changed(app: &AppHandle) {
    let _ = app.emit("ghelper-changed", ());
}

// ---------------------------------------------------------------- config

fn appdata_config() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(Path::new(&appdata).join("GHelper").join("config.json"))
}

/// G-Helper's settings file: next to it when it keeps one there (a portable
/// copy), else in %APPDATA%\GHelper.
fn config_path() -> Option<PathBuf> {
    if let Some(exe) = known_exe() {
        if let Some(beside) = exe.parent().map(|dir| dir.join("config.json")) {
            if beside.exists() {
                return Some(beside);
            }
        }
    }
    appdata_config()
}

fn read_config() -> Option<Map<String, Value>> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn int(config: &Map<String, Value>, key: &str) -> Option<i64> {
    match config.get(key)? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

fn text(config: &Map<String, Value>, key: &str) -> Option<String> {
    match config.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn is_on(config: &Map<String, Value>, key: &str) -> bool {
    int(config, key).unwrap_or(0) == 1
}

fn modes(config: &Map<String, Value>) -> Vec<Mode> {
    let mut list = vec![
        Mode {
            id: MODE_SILENT,
            name: "Silent".into(),
            base: MODE_SILENT,
        },
        Mode {
            id: MODE_BALANCED,
            name: "Balanced".into(),
            base: MODE_BALANCED,
        },
        Mode {
            id: MODE_TURBO,
            name: "Turbo".into(),
            base: MODE_TURBO,
        },
    ];
    for id in 3..20 {
        let Some(base) = int(config, &format!("mode_base_{id}")).filter(|b| *b >= 0) else {
            continue;
        };
        let name = text(config, &format!("mode_name_{id}")).unwrap_or(format!("Custom {}", id - 2));
        list.push(Mode { id, name, base });
    }
    list
}

/// Writes the settings file the way G-Helper does: a new file, swapped in,
/// the old one kept as config.json.bak.
fn write_config(config: &Map<String, Value>) -> Result<(), String> {
    let path = config_path().ok_or("G-Helper's settings file wasn't found")?;
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    let _ = std::fs::copy(&path, path.with_extension("json.bak"));
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

fn config_modified() -> Option<SystemTime> {
    std::fs::metadata(config_path()?).ok()?.modified().ok()
}

// ---------------------------------------------------------------- process

fn known_exe() -> Option<PathBuf> {
    EXE.lock().ok()?.clone()
}

/// G-Helper's process id and program file, when it runs.
fn find_process() -> Option<(u32, Option<PathBuf>)> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        let mut ok = Process32FirstW(snapshot, &mut entry).is_ok();
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("GHelper.exe") {
                found = Some(entry.th32ProcessID);
                break;
            }
            ok = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        let pid = found?;
        let path = crate::commands::process_image_path(pid).map(PathBuf::from);
        if let Some(path) = &path {
            if let Ok(mut exe) = EXE.lock() {
                *exe = Some(path.clone());
            }
        }
        Some((pid, path))
    }
}

/// Remembers where G-Helper lives across Bloom restarts (to start it again
/// when it isn't running).
fn remember_exe(app: &AppHandle) {
    let Some(path) = known_exe() else {
        return;
    };
    let text = path.to_string_lossy().to_string();
    if crate::utils::get_setting_str(app, "bloom-ghelper-exe").as_deref() != Some(text.as_str()) {
        let _ = crate::commands::save_setting(
            app.clone(),
            "bloom-ghelper-exe".into(),
            Value::String(text),
        );
    }
}

fn load_exe(app: &AppHandle) {
    if known_exe().is_some() {
        return;
    }
    if let Some(saved) = crate::utils::get_setting_str(app, "bloom-ghelper-exe") {
        let path = PathBuf::from(saved);
        if path.exists() {
            if let Ok(mut exe) = EXE.lock() {
                *exe = Some(path);
            }
        }
    }
}

/// The current user's SID, for the name of G-Helper's startup task.
pub fn user_sid() -> Option<String> {
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut size = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
        let mut buffer = vec![0u8; size as usize];
        let got = GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr() as *mut _),
            size,
            &mut size,
        );
        let _ = CloseHandle(token);
        got.ok()?;
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut text = windows::core::PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).ok()?;
        let sid = text.to_string().ok();
        let _ = windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            text.0 as *mut _,
        )));
        sid
    }
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn system32(file: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    Path::new(&root).join("System32").join(file)
}

/// Runs a scheduled task by name; true when Windows started it.
fn run_task(name: &str) -> bool {
    std::process::Command::new(system32("schtasks.exe"))
        .args(["/Run", "/TN", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Starts G-Helper the way it starts with Windows (its own task, with the
/// rights it was set up with), or from its program file.
fn start_ghelper() -> Result<(), String> {
    if let Some(sid) = user_sid() {
        if run_task(&format!("GHelper_{sid}")) {
            return Ok(());
        }
    }
    if run_task("GHelper") {
        return Ok(());
    }
    let exe = known_exe().ok_or("G-Helper's program wasn't found")?;
    // Started from its own folder, G-Helper opens its window; from elsewhere
    // it starts in the tray, as at sign-in.
    let home = std::env::var_os("USERPROFILE").unwrap_or_else(|| "C:\\".into());
    std::process::Command::new(exe)
        .current_dir(home)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Asks G-Helper to close, with the signal it made for that, and waits until
/// it has.
fn close_ghelper(pid: u32) -> Result<(), String> {
    use windows::Win32::System::Threading::{
        OpenEventW, OpenProcess, ResetEvent, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
        PROCESS_SYNCHRONIZE, SYNCHRONIZATION_SYNCHRONIZE,
    };
    unsafe {
        let event = OpenEventW(
            EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE,
            false,
            w!("Global\\GHelperApp-Exit"),
        )
        .map_err(|_| "This G-Helper can't be asked to close (it is older than May 2026)")?;
        let process = OpenProcess(PROCESS_SYNCHRONIZE, false, pid).ok();
        let _ = SetEvent(event);
        let closed = match process {
            Some(process) => {
                let waited = WaitForSingleObject(process, 10_000);
                let _ = CloseHandle(process);
                waited.0 == 0
            }
            // Its process couldn't be opened to wait on: look for it.
            None => {
                let waiting = Instant::now();
                let mut gone = false;
                while !gone && waiting.elapsed() < Duration::from_secs(10) {
                    std::thread::sleep(Duration::from_millis(300));
                    gone = find_process().is_none();
                }
                gone
            }
        };
        // A new G-Helper must not find the signal still on.
        let _ = ResetEvent(event);
        let _ = CloseHandle(event);
        if closed {
            Ok(())
        } else {
            Err("G-Helper didn't close".into())
        }
    }
}

/// Changes G-Helper's settings: closes it, writes them, starts it again.
/// G-Helper keeps its settings in memory and writes them two seconds after a
/// change, so Bloom first waits for the file to be quiet.
fn apply_through_config(
    app: &AppHandle,
    change: impl FnOnce(&mut Map<String, Value>),
) -> Result<(), String> {
    let _one_at_a_time = RESTART_LOCK.lock().map_err(|e| e.to_string())?;
    APPLYING.store(true, Ordering::SeqCst);
    emit_changed(app);
    let result = (|| {
        let running = find_process();
        if let Some((pid, _)) = running {
            let waiting = Instant::now();
            while waiting.elapsed() < Duration::from_secs(6) {
                let quiet = config_modified()
                    .and_then(|m| m.elapsed().ok())
                    .is_none_or(|age| age > Duration::from_millis(2500));
                if quiet {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            close_ghelper(pid)?;
        }
        let mut config = read_config().ok_or("G-Helper's settings file couldn't be read")?;
        change(&mut config);
        write_config(&config)?;
        if running.is_some() || known_exe().is_some() {
            start_ghelper()?;
        }
        Ok(())
    })();
    APPLYING.store(false, Ordering::SeqCst);
    // G-Helper writes its settings again as it starts.
    let handle = app.clone();
    std::thread::spawn(move || {
        for _ in 0..4 {
            std::thread::sleep(Duration::from_millis(1500));
            emit_changed(&handle);
        }
    });
    if let Err(e) = &result {
        crate::utils::note_crash(&format!("G-Helper change failed: {e}"));
    }
    result
}

// ---------------------------------------------------------------- keys

/// Presses a shortcut, as if on the keyboard.
fn press(modifiers: &[u16], key: u16) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };
    let event = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let mut inputs: Vec<INPUT> = modifiers.iter().map(|&m| event(m, false)).collect();
    inputs.push(event(key, false));
    inputs.push(event(key, true));
    inputs.extend(modifiers.iter().rev().map(|&m| event(m, true)));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// G-Helper's modifier setting ("control-shift-alt"), as keys.
fn modifier_keys(config: &Map<String, Value>, key: &str, default: &[u16]) -> Vec<u16> {
    let Some(value) = text(config, key).filter(|v| !v.trim().is_empty()) else {
        return default.to_vec();
    };
    let parts: Vec<String> = value
        .split('-')
        .map(|p| p.trim().to_ascii_lowercase())
        .collect();
    let mut keys = Vec::new();
    if parts.iter().any(|p| p == "win") {
        keys.push(VK_LWIN);
    }
    if parts.iter().any(|p| p == "control") {
        keys.push(VK_CONTROL);
    }
    if parts.iter().any(|p| p == "shift") {
        keys.push(VK_SHIFT);
    }
    if parts.iter().any(|p| p == "alt") {
        keys.push(VK_MENU);
    }
    keys
}

fn alt_modifiers(config: &Map<String, Value>) -> Vec<u16> {
    modifier_keys(
        config,
        "modifier_keybind_alt",
        &[VK_CONTROL, VK_SHIFT, VK_MENU],
    )
}

fn main_modifiers(config: &Map<String, Value>) -> Vec<u16> {
    modifier_keys(config, "modifier_keybind", &[VK_CONTROL, VK_SHIFT])
}

/// G-Helper's shortcut key for a mode (its keybind_profile_0 to _4).
fn mode_key(config: &Map<String, Value>, mode: i64) -> Option<u16> {
    let default = match mode {
        0 => VK_F17,
        1 => VK_F18,
        2 => VK_F16,
        3 => VK_F19,
        4 => VK_F20,
        _ => return None,
    };
    let key = int(config, &format!("keybind_profile_{mode}")).unwrap_or(default as i64);
    (key > 0 && key < 0xFF).then_some(key as u16)
}

fn shortcuts_off(config: &Map<String, Value>) -> bool {
    is_on(config, "skip_hotkeys")
}

/// Waits up to `limit` for G-Helper's settings file to show `done`.
fn wait_for(limit: Duration, done: impl Fn(&Map<String, Value>) -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if read_config().is_some_and(|c| done(&c)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    false
}

// ---------------------------------------------------------------- hardware

struct Acpi(HANDLE);
// The handle is only used for DeviceIoControl, which is thread-safe.
unsafe impl Send for Acpi {}

impl Acpi {
    fn open() -> Option<Self> {
        use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
        use windows::Win32::Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        unsafe {
            CreateFileW(
                w!("\\\\.\\ATKACPI"),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
            .ok()
            .filter(|h| !h.is_invalid())
            .map(Acpi)
        }
    }

    fn call(&self, method: u32, args: [u32; 2]) -> Option<i32> {
        use windows::Win32::System::IO::DeviceIoControl;
        let mut input = Vec::with_capacity(16);
        input.extend_from_slice(&method.to_le_bytes());
        input.extend_from_slice(&8u32.to_le_bytes());
        input.extend_from_slice(&args[0].to_le_bytes());
        input.extend_from_slice(&args[1].to_le_bytes());
        let mut output = [0u8; 16];
        let mut returned = 0u32;
        unsafe {
            DeviceIoControl(
                self.0,
                ACPI_CONTROL,
                Some(input.as_ptr() as *const _),
                input.len() as u32,
                Some(output.as_mut_ptr() as *mut _),
                output.len() as u32,
                Some(&mut returned),
                None,
            )
            .ok()?;
        }
        Some(i32::from_le_bytes([
            output[0], output[1], output[2], output[3],
        ]))
    }

    /// A value, as G-Helper reads it (negative: not on this laptop).
    fn get(&self, device: u32) -> i32 {
        self.call(ACPI_DSTS, [device, 0])
            .map(|v| v.wrapping_sub(65536))
            .unwrap_or(-1)
    }

    fn set(&self, device: u32, value: u32) -> i32 {
        self.call(ACPI_DEVS, [device, value]).unwrap_or(-1)
    }

    fn gpu_endpoints(&self) -> (u32, u32) {
        if self.get(GPU_ECO_ROG) >= 0 || self.get(GPU_MUX_ROG) >= 0 {
            (GPU_ECO_ROG, GPU_MUX_ROG)
        } else {
            (GPU_ECO_VIVO, GPU_MUX_VIVO)
        }
    }
}

impl Drop for Acpi {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

static ACPI: Mutex<Option<Acpi>> = Mutex::new(None);

fn with_acpi<T>(f: impl FnOnce(&Acpi) -> T) -> Option<T> {
    let mut guard = ACPI.lock().ok()?;
    if guard.is_none() {
        *guard = Acpi::open();
    }
    guard.as_ref().map(f)
}

/// G-Helper's CPU and GPU lines, as it last wrote them in its tray icon's
/// tooltip. Bloom then tells the icon the pointer is over it, which is when
/// G-Helper reads its sensors again, so the next look is fresh.
fn ghelper_lines() -> Option<(Option<String>, Option<String>)> {
    let icon = crate::tray_host::snapshot_ids().into_iter().find(|icon| {
        Path::new(&icon.path)
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("GHelper.exe"))
    })?;
    crate::tray_host::hover(&icon.id);
    let line = |prefix: &str| {
        icon.tooltip
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with(prefix) && l.contains(':'))
            .map(|l| l.replace(" Fan", "  Fan"))
    };
    let cpu = line("CPU");
    let gpu = line("GPU");
    (cpu.is_some() || gpu.is_some()).then_some((cpu, gpu))
}

fn sensors() -> Sensors {
    let mut read = acpi_sensors();
    if let Some((cpu, gpu)) = ghelper_lines() {
        read.cpu_text = cpu;
        read.gpu_text = gpu;
    }
    read
}

fn acpi_sensors() -> Sensors {
    with_acpi(|acpi| {
        let temp = |id| Some(acpi.get(id)).filter(|t| (1..=120).contains(t));
        // G-Helper's GetFan: hundreds of RPM in the low 16 bits.
        let fan = |id| {
            let raw = acpi.get(id);
            let fan = raw & 0xFFFF;
            (!(fan > 120 || (fan == 0 && raw < 0))).then_some(fan * 100)
        };
        Sensors {
            cpu_text: None,
            gpu_text: None,
            cpu_temp: temp(TEMP_CPU),
            gpu_temp: temp(TEMP_GPU),
            cpu_fan: fan(FAN_CPU),
            gpu_fan: fan(FAN_GPU),
            mid_fan: fan(FAN_MID),
        }
    })
    .unwrap_or_default()
}

/// Multi Zone as G-Helper shows it: 0 off (one zone), 1 on, 2 strong.
fn miniled_state() -> Option<i32> {
    with_acpi(|acpi| {
        let first = acpi.get(MINILED_1);
        if first >= 0 {
            return Some(if first == 1 { 1 } else { 0 });
        }
        match acpi.get(MINILED_2) {
            0 => Some(1),
            1 => Some(2),
            2 => Some(0),
            _ => None,
        }
    })
    .flatten()
}

// ---------------------------------------------------------------- screen

/// The laptop's own screen: its GDI name ("\\.\DISPLAY1").
fn laptop_screen() -> Option<Vec<u16>> {
    use windows::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    };
    unsafe {
        let (mut paths, mut modes) = (0u32, 0u32);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes).0 != 0 {
            return None;
        }
        let mut path_info = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
        let mut mode_info = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut paths,
            path_info.as_mut_ptr(),
            &mut modes,
            mode_info.as_mut_ptr(),
            None,
        )
        .0 != 0
        {
            return None;
        }
        for path in path_info.iter().take(paths as usize) {
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            target.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            target.header.adapterId = path.targetInfo.adapterId;
            target.header.id = path.targetInfo.id;
            if DisplayConfigGetDeviceInfo(&mut target.header) != 0 {
                continue;
            }
            let tech = target.outputTechnology;
            if tech != DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                && tech != DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
            {
                continue;
            }
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            source.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            source.header.adapterId = path.sourceInfo.adapterId;
            source.header.id = path.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut source.header) != 0 {
                continue;
            }
            let len = source
                .viewGdiDeviceName
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(0);
            let mut name = source.viewGdiDeviceName[..len].to_vec();
            name.push(0);
            return Some(name);
        }
        None
    }
}

/// The laptop screen's refresh rate now, and the highest it can do.
fn refresh_rates() -> Option<(i32, i32)> {
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    let screen = laptop_screen()?;
    unsafe {
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        let name = PCWSTR(screen.as_ptr());
        if !EnumDisplaySettingsW(name, ENUM_CURRENT_SETTINGS, &mut mode).as_bool() {
            return None;
        }
        let current = mode.dmDisplayFrequency as i32;
        let mut highest = current;
        let mut index = 0;
        loop {
            let mut each = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            if !EnumDisplaySettingsW(
                name,
                windows::Win32::Graphics::Gdi::ENUM_DISPLAY_SETTINGS_MODE(index),
                &mut each,
            )
            .as_bool()
            {
                break;
            }
            highest = highest.max(each.dmDisplayFrequency as i32);
            index += 1;
        }
        Some((current, highest))
    }
}

/// Sets the laptop screen's refresh rate, as G-Helper's own buttons do.
fn set_refresh(rate: i32) -> bool {
    use windows::Win32::Graphics::Gdi::{
        ChangeDisplaySettingsExW, EnumDisplaySettingsW, CDS_UPDATEREGISTRY, DEVMODEW,
        DISP_CHANGE_SUCCESSFUL, ENUM_CURRENT_SETTINGS,
    };
    let Some(screen) = laptop_screen() else {
        return false;
    };
    unsafe {
        let name = PCWSTR(screen.as_ptr());
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        if !EnumDisplaySettingsW(name, ENUM_CURRENT_SETTINGS, &mut mode).as_bool() {
            return false;
        }
        if mode.dmDisplayFrequency as i32 == rate {
            return true;
        }
        mode.dmDisplayFrequency = rate as u32;
        ChangeDisplaySettingsExW(name, Some(&mode), None, CDS_UPDATEREGISTRY, None)
            == DISP_CHANGE_SUCCESSFUL
    }
}

// ---------------------------------------------------------------- reading

fn power_key() -> i64 {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe {
        if GetSystemPowerStatus(&mut status).is_ok() {
            return status.ACLineStatus as i64;
        }
    }
    1
}

pub fn read_state(with_sensors: bool) -> State {
    let config = read_config();
    let running = find_process().is_some();
    let Some(config) = config else {
        return State {
            installed: false,
            running,
            ..Default::default()
        };
    };
    let (gpu_mode, has_ultimate) = with_acpi(|acpi| {
        let (eco_id, mux_id) = acpi.gpu_endpoints();
        let eco = acpi.get(eco_id);
        let mux = acpi.get(mux_id);
        let mode = if mux == 0 {
            GPU_ULTIMATE
        } else if eco == 1 {
            GPU_ECO
        } else if eco == 0 {
            GPU_STANDARD
        } else {
            -1
        };
        (mode, mux >= 0)
    })
    .unwrap_or((-1, false));
    // Without the hardware interface, G-Helper's last word on it.
    let gpu_mode = if gpu_mode >= 0 {
        gpu_mode
    } else {
        int(&config, "gpu_mode").unwrap_or(-1)
    };
    let (refresh, refresh_max) =
        refresh_rates().unwrap_or((-1, int(&config, "screen_max").unwrap_or(-1) as i32));
    let bindings = BINDING_KEYS
        .iter()
        .map(|(key, label)| Binding {
            key: key.to_string(),
            label: label.to_string(),
            action: text(&config, key).unwrap_or_default(),
            custom: text(&config, &format!("{key}_custom")).unwrap_or_default(),
        })
        .collect();
    State {
        installed: true,
        running,
        modes: modes(&config),
        mode: int(&config, "performance_mode").unwrap_or(MODE_BALANCED),
        gpu_mode,
        gpu_auto: is_on(&config, "gpu_auto"),
        has_ultimate,
        screen_auto: is_on(&config, "screen_auto"),
        refresh,
        refresh_min: int(&config, "min_rate").unwrap_or(60) as i32,
        refresh_max,
        miniled: miniled_state(),
        charge_limit: int(&config, "charge_limit")
            .filter(|l| *l >= 40)
            .unwrap_or(100),
        bindings,
        shortcuts_off: shortcuts_off(&config),
        sensors: if with_sensors {
            sensors()
        } else {
            Sensors::default()
        },
        applying: APPLYING.load(Ordering::SeqCst),
    }
}

// ---------------------------------------------------------------- actions

fn set_mode(app: &AppHandle, mode: i64) -> Result<(), String> {
    let config = read_config().ok_or("G-Helper's settings file wasn't found")?;
    if find_process().is_some() && !shortcuts_off(&config) {
        if let Some(key) = mode_key(&config, mode) {
            press(&alt_modifiers(&config), key);
            if wait_for(Duration::from_secs(5), |c| {
                int(c, "performance_mode") == Some(mode)
            }) {
                emit_changed(app);
                return Ok(());
            }
        }
    }
    // A mode without a shortcut: G-Helper applies the one it starts in.
    let power = power_key();
    apply_through_config(app, |c| {
        c.insert("performance_mode".into(), mode.into());
        c.insert(format!("performance_{power}"), mode.into());
    })
}

fn cycle_mode(app: &AppHandle) -> Result<(), String> {
    let config = read_config().ok_or("G-Helper's settings file wasn't found")?;
    let key = int(&config, "keybind_profile").unwrap_or(VK_F5 as i64);
    if find_process().is_some() && key > 0 && key < 0xFF {
        press(&main_modifiers(&config), key as u16);
        emit_changed(app);
        return Ok(());
    }
    let list = modes(&config);
    let now = int(&config, "performance_mode").unwrap_or(MODE_BALANCED);
    let at = list.iter().position(|m| m.id == now).unwrap_or(0);
    set_mode(app, list[(at + 1) % list.len()].id)
}

/// Ultimate (or back from it): the MUX switch, then a restart, as in G-Helper.
fn set_mux(ultimate: bool) -> Result<(), String> {
    with_acpi(|acpi| -> Result<(), String> {
        let (eco_id, mux_id) = acpi.gpu_endpoints();
        if acpi.get(mux_id) < 0 {
            return Err("This laptop has no Ultimate mode".into());
        }
        if ultimate {
            if acpi.get(eco_id) == 1 {
                acpi.set(eco_id, 0);
                std::thread::sleep(Duration::from_millis(500));
                if acpi.get(eco_id) == 1 {
                    return Err("The GPU is still in Eco. Try again in a moment".into());
                }
            }
            acpi.set(mux_id, 0);
        } else {
            acpi.set(mux_id, 1);
        }
        Ok(())
    })
    .ok_or("The ASUS hardware interface isn't available")??;
    std::process::Command::new(system32("shutdown.exe"))
        .args(["/r", "/t", "1"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn set_gpu(app: &AppHandle, target: &str) -> Result<(), String> {
    let config = read_config().ok_or("G-Helper's settings file wasn't found")?;
    let state = read_state(false);
    let from_ultimate = state.gpu_mode == GPU_ULTIMATE;
    match target {
        "ultimate" => {
            if from_ultimate {
                return Ok(());
            }
            set_mux(true)
        }
        // Leaving Ultimate is the MUX switch and a restart; G-Helper sets the
        // rest after it, as when it does this itself.
        _ if from_ultimate => {
            if target == "optimized" {
                let _ = apply_through_config(app, |c| {
                    c.insert("gpu_auto".into(), 1.into());
                });
            }
            set_mux(false)
        }
        "optimized" => apply_through_config(app, |c| {
            c.insert("gpu_auto".into(), 1.into());
        }),
        "eco" | "standard" => {
            let (key, mode) = if target == "eco" {
                (VK_F14, GPU_ECO)
            } else {
                (VK_F15, GPU_STANDARD)
            };
            if find_process().is_none() {
                return Err("G-Helper isn't running".into());
            }
            if !shortcuts_off(&config) {
                // G-Helper does the switch, with its own checks (it asks
                // first when an app is using the GPU), so there is no
                // second way here if it takes its time.
                press(&alt_modifiers(&config), key);
                wait_for(Duration::from_secs(8), |c| {
                    int(c, "gpu_mode") == Some(mode) && !is_on(c, "gpu_auto")
                });
                emit_changed(app);
                return Ok(());
            }
            Err("G-Helper's own shortcuts are turned off, and Eco and Standard go through them. Turn them on in G-Helper (its \"skip_hotkeys\" setting)".into())
        }
        _ => Err(format!("Unknown GPU mode: {target}")),
    }
}

fn set_screen(app: &AppHandle, target: &str) -> Result<(), String> {
    let config = read_config().ok_or("G-Helper's settings file wasn't found")?;
    let auto = is_on(&config, "screen_auto");
    if target == "auto" {
        if auto {
            return Ok(());
        }
        return apply_through_config(app, |c| {
            c.insert("screen_auto".into(), 1.into());
        });
    }
    let (current, max) = refresh_rates().ok_or("The laptop screen is off")?;
    let min = int(&config, "min_rate").unwrap_or(60) as i32;
    let rate = match target {
        "min" => min,
        "max" => max,
        "toggle" => {
            if current > min {
                min
            } else {
                max
            }
        }
        _ => return Err(format!("Unknown screen setting: {target}")),
    };
    if auto {
        // G-Helper's own refresh shortcut switches Auto off and goes to the
        // other rate; with the rate already right, Auto goes off through its
        // settings.
        if current != rate && find_process().is_some() && !shortcuts_off(&config) {
            press(&alt_modifiers(&config), VK_F13);
            if wait_for(Duration::from_secs(4), |c| !is_on(c, "screen_auto")) {
                emit_changed(app);
                return Ok(());
            }
        }
        set_refresh(rate);
        return apply_through_config(app, |c| {
            c.insert("screen_auto".into(), 0.into());
        });
    }
    if set_refresh(rate) {
        emit_changed(app);
        Ok(())
    } else {
        Err(format!("The screen didn't take {rate} Hz"))
    }
}

/// Multi Zone on or off, as G-Helper's button does it.
fn toggle_miniled(app: &AppHandle) -> Result<(), String> {
    with_acpi(|acpi| -> Result<(), String> {
        let first = acpi.get(MINILED_1);
        if first >= 0 {
            acpi.set(MINILED_1, if first == 1 { 0 } else { 1 });
            return Ok(());
        }
        let second = acpi.get(MINILED_2);
        if second < 0 {
            return Err("This screen has no Multi Zone".into());
        }
        let next = match second {
            1 => 2,
            2 => 0,
            _ => 1,
        };
        acpi.set(MINILED_2, next);
        Ok(())
    })
    .ok_or("The ASUS hardware interface isn't available")??;
    emit_changed(app);
    Ok(())
}

fn set_charge_limit(app: &AppHandle, limit: i64) -> Result<(), String> {
    let limit = limit.clamp(40, 100);
    if read_config().and_then(|c| int(&c, "charge_limit")) == Some(limit) {
        return Ok(());
    }
    apply_through_config(app, |c| {
        c.insert("charge_limit".into(), limit.into());
    })
}

fn open_ghelper() -> Result<(), String> {
    let config = read_config().ok_or("G-Helper's settings file wasn't found")?;
    if find_process().is_none() {
        return start_ghelper();
    }
    let key = int(&config, "keybind_app").unwrap_or(VK_F12 as i64);
    if key > 0 && key < 0xFF {
        press(&main_modifiers(&config), key as u16);
    }
    Ok(())
}

fn toggle_mic() -> Result<(), String> {
    let config = read_config().ok_or("G-Helper's settings file wasn't found")?;
    if find_process().is_none() || shortcuts_off(&config) {
        return Err("G-Helper isn't running".into());
    }
    press(&main_modifiers(&config), VK_F20);
    Ok(())
}

// ---------------------------------------------------------------- keys from any keyboard

/// G-Helper's pop-ups are on (its "disable_osd" isn't set).
pub fn osd_enabled() -> bool {
    !read_config().is_some_and(|c| is_on(&c, "disable_osd"))
}

/// The touchpad's state as Windows keeps it (what G-Helper reads).
fn touchpad_on() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 0u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\PrecisionTouchPad\\Status"),
            w!("Enabled"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
        .is_ok()
            && value == 1
    }
}

/// G-Helper's name for one of its actions.
fn action_name(action: &str) -> &str {
    match action {
        "aura" => "Toggle Aura",
        "visual" => "Visual Mode",
        "fnlock" => "Toggle Fn-Lock",
        "backlight_up" => "Backlight Up",
        "backlight_down" => "Backlight Down",
        other => other,
    }
}

/// What one of G-Helper's keys does when nothing is chosen for it (as in
/// G-Helper's InputDispatcher).
fn default_key_action(key: &str) -> &'static str {
    match key {
        "m1" => "volume_down",
        "m2" => "volume_up",
        "m3" => "micmute",
        "m4" => "ghelper",
        "fnf4" => "aura",
        "fnc" => "fnlock",
        "fnv" => "visual",
        _ => "",
    }
}

/// G-Helper's "Custom" action: up to four keys as hex codes ("0x5B 0x44"),
/// pressed together, or else a command, run as G-Helper runs it.
fn custom_keys(command: &str) -> Option<Vec<u16>> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    let keys: Option<Vec<u16>> = parts
        .iter()
        .map(|p| {
            let digits = p
                .strip_prefix("0x")
                .or_else(|| p.strip_prefix("0X"))
                .unwrap_or(p);
            (!digits.is_empty() && digits.len() <= 2)
                .then(|| u16::from_str_radix(digits, 16).ok())
                .flatten()
        })
        .collect();
    keys.filter(|k| (1..=4).contains(&k.len()))
}

fn run_custom(command: &str) -> Result<(), String> {
    let codes = custom_keys(command);
    match codes {
        Some(keys) => {
            press(&keys[..keys.len() - 1], keys[keys.len() - 1]);
            Ok(())
        }
        _ if command.trim().is_empty() => Err("Custom has nothing set in its action box".into()),
        _ => std::process::Command::new(system32("cmd.exe"))
            .raw_arg(format!("/C {command}"))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
    }
}

/// Does what one of G-Helper's keys (M1 to FN+V) is set to do in G-Helper,
/// for a shortcut on any keyboard. It is read from G-Helper's settings each
/// time, so a change made in G-Helper is followed at once.
fn run_key(app: &AppHandle, key: &str) -> Result<(), String> {
    let config = read_config().ok_or("G-Helper's settings file wasn't found")?;
    let chosen = text(&config, key).unwrap_or_default();
    let action = if chosen.is_empty() {
        default_key_action(key).to_string()
    } else {
        chosen
    };
    let running = find_process().is_some();
    let needs_ghelper = |what: &str| {
        if running {
            Ok(())
        } else {
            Err(format!("{what} goes through G-Helper, which isn't running"))
        }
    };
    match action.as_str() {
        "volume_down" => press(&[], 0xAE),
        "volume_up" => press(&[], 0xAF),
        "mute" => press(&[], 0xAD),
        "play" => press(&[], 0xB3),
        "screenshot" => press(&[], 0x2C),
        "brightness_down" | "brightness_up" => {
            // The same call G-Helper makes for its brightness keys.
            let code = if action == "brightness_up" {
                0x20
            } else {
                0x10
            };
            with_acpi(|acpi| acpi.set(UNIVERSAL_CONTROL, code))
                .ok_or("The ASUS hardware interface isn't available")?;
        }
        "screen" => crate::black_screen::screen_off(),
        "lock" => unsafe {
            windows::Win32::System::Shutdown::LockWorkStation().map_err(|e| e.to_string())?;
        },
        "miniled" => {
            toggle_miniled(app)?;
            // G-Helper's pop-up for it: the new setting's name.
            if osd_enabled() {
                let (name, icon) = match miniled_state() {
                    Some(0) => ("One Zone", crate::ghelper_toast::Icon::BrightnessDown),
                    Some(2) => (
                        "Multi Zone Strong",
                        crate::ghelper_toast::Icon::BrightnessUp,
                    ),
                    _ => ("Multi Zone", crate::ghelper_toast::Icon::BrightnessUp),
                };
                crate::ghelper_toast::show(name, Some(icon));
            }
        }
        "touchscreen" => crate::touchscreen::toggle(app)?,
        "touchpad" => {
            // Windows' own touchpad on/off shortcut, then G-Helper's pop-up
            // with the touchpad's state as Windows keeps it.
            press(&[VK_LWIN, VK_CONTROL], 0x87);
            std::thread::sleep(Duration::from_millis(200));
            if osd_enabled() {
                let on = touchpad_on();
                crate::ghelper_toast::show(
                    if on { "On" } else { "Off" },
                    Some(crate::ghelper_toast::Icon::Touchpad),
                );
            }
        }
        "calculator" => {
            std::process::Command::new(system32("calc.exe"))
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "custom" => run_custom(&text(&config, &format!("{key}_custom")).unwrap_or_default())?,
        "performance" => {
            needs_ghelper("Mode")?;
            cycle_mode(app)?;
        }
        "micmute" => {
            needs_ghelper("Mute Mic")?;
            toggle_mic()?;
        }
        "ghelper" => open_ghelper()?,
        "overlay" => {
            needs_ghelper("Overlay")?;
            let key = int(&config, "keybind_overlay").unwrap_or(0x4F);
            if key > 0 && key < 0xFF {
                press(&alt_modifiers(&config), key as u16);
            }
        }
        // These G-Helper only does inside itself, with no way in for another
        // program: they stay on the laptop's own keys.
        "aura" | "visual" | "fnlock" | "backlight_up" | "backlight_down" => {
            return Err(format!(
                "\"{}\" is done by G-Helper only, from the laptop's own key",
                action_name(&action)
            ))
        }
        other => {
            return Err(format!(
                "G-Helper's action \"{other}\" isn't known to Bloom"
            ))
        }
    }
    Ok(())
}

pub(crate) fn log(line: &str) {
    use std::io::Write;
    let Some(path) = crate::utils::data_file("ghelper.log") else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
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

/// Runs one action from the notch or a shortcut, and notes how it went
/// in ghelper.log (in Export Diagnostics).
pub fn run_action(app: &AppHandle, action: &str) -> Result<(), String> {
    let (what, value) = action.split_once(':').unwrap_or((action, ""));
    let result = match what {
        "key" => run_key(app, value),
        "mode" if value == "cycle" => cycle_mode(app),
        "mode" => set_mode(app, value.parse().map_err(|_| "Unknown mode")?),
        "gpu" => set_gpu(app, value),
        "screen" => set_screen(app, value),
        "miniled" => toggle_miniled(app),
        "charge" => set_charge_limit(app, value.parse().map_err(|_| "Unknown charge limit")?),
        "open" => open_ghelper(),
        "micmute" => toggle_mic(),
        _ => Err(format!("Unknown G-Helper action: {action}")),
    };
    match &result {
        Ok(()) => log(&format!("{action}: done")),
        Err(e) => log(&format!("{action}: {e}")),
    }
    result
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn ghelper_state(app: AppHandle, sensors: Option<bool>) -> State {
    tauri::async_runtime::spawn_blocking(move || {
        load_exe(&app);
        let state = read_state(sensors.unwrap_or(false));
        remember_exe(&app);
        state
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn ghelper_action(app: AppHandle, action: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || run_action(&app, &action))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(serde::Deserialize)]
pub struct BindingChange {
    key: String,
    action: String,
    custom: String,
}

/// Changes what G-Helper's keys (M1 to FN+V) do, all in one go (G-Helper
/// restarts once for them).
#[tauri::command]
pub async fn ghelper_set_bindings(
    app: AppHandle,
    bindings: Vec<BindingChange>,
) -> Result<(), String> {
    if let Some(unknown) = bindings
        .iter()
        .find(|b| !BINDING_KEYS.iter().any(|(k, _)| *k == b.key))
    {
        return Err(format!("Unknown key: {}", unknown.key));
    }
    tauri::async_runtime::spawn_blocking(move || {
        apply_through_config(&app, |c| {
            for binding in bindings {
                c.insert(binding.key.clone(), Value::String(binding.action));
                c.insert(
                    format!("{}_custom", binding.key),
                    Value::String(binding.custom),
                );
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Starts G-Helper (the notch's "Start G-Helper").
#[tauri::command]
pub async fn ghelper_start(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        load_exe(&app);
        let started = start_ghelper();
        emit_changed(&app);
        started
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Tells the windows when G-Helper's settings change (G-Helper's own window,
/// its shortcuts or the laptop's keys) or it starts or stops.
pub fn start_watcher(app: AppHandle) {
    std::thread::Builder::new()
        .name("bloom-ghelper-watch".into())
        .spawn(move || {
            load_exe(&app);
            let mut last: Option<(Option<SystemTime>, bool)> = None;
            loop {
                let enabled = crate::utils::get_setting_str(&app, "bloom-ghelper-enabled")
                    .is_none_or(|v| v != "false");
                if enabled && appdata_config().is_some_and(|p| p.exists()) {
                    let now = (config_modified(), find_process().is_some());
                    if last.is_some_and(|l| l != now) {
                        emit_changed(&app);
                    }
                    if now.1 {
                        remember_exe(&app);
                    }
                    last = Some(now);
                    std::thread::sleep(Duration::from_millis(2500));
                } else {
                    last = None;
                    std::thread::sleep(Duration::from_secs(10));
                }
                if crate::state::SHUTTING_DOWN.load(Ordering::Relaxed) {
                    break;
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(json: &str) -> Map<String, Value> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn reads_modes_with_custom_ones() {
        let c = config(r#"{"mode_base_3": 2, "mode_name_3": "Silent+ 70W", "mode_base_5": -1}"#);
        let list = modes(&c);
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].id, MODE_SILENT);
        assert_eq!(list[3].name, "Silent+ 70W");
        assert_eq!(list[3].base, MODE_SILENT);
    }

    #[test]
    fn reads_shortcut_settings() {
        let c = config(r#"{"modifier_keybind_alt": "shift-alt", "keybind_profile_1": 0}"#);
        assert_eq!(alt_modifiers(&c), vec![VK_SHIFT, VK_MENU]);
        assert_eq!(mode_key(&c, 1), None);
        assert_eq!(mode_key(&c, 2), Some(VK_F16));
        assert_eq!(mode_key(&c, 7), None);
        assert_eq!(main_modifiers(&config("{}")), vec![VK_CONTROL, VK_SHIFT]);
    }

    #[test]
    fn reads_custom_keys_as_g_helper_does() {
        assert_eq!(custom_keys("0x5B 0x44"), Some(vec![0x5B, 0x44]));
        assert_eq!(custom_keys("A0 2c"), Some(vec![0xA0, 0x2C]));
        assert_eq!(custom_keys("C:\\Tools\\app.exe"), None);
        assert_eq!(custom_keys("1 2 3 4 5"), None);
        assert_eq!(default_key_action("fnv"), "visual");
    }

    #[test]
    fn reads_numbers_written_either_way() {
        let c = config(r#"{"a": 80, "b": "60", "c": true}"#);
        assert_eq!(int(&c, "a"), Some(80));
        assert_eq!(int(&c, "b"), Some(60));
        assert_eq!(int(&c, "c"), Some(1));
        assert_eq!(int(&c, "d"), None);
    }
}
