//! Toggle Touchscreen from a shortcut on another keyboard (Settings >
//! Integrations > G-Helper > Key Bindings), the action G-Helper runs for a
//! key set to it. G-Helper can't be asked to run it, so Bloom does it the
//! same way and shows G-Helper's own pop-up for it (ghelper_toast.rs).
//!
//! Turning a device off or on needs administrator rights, which Bloom doesn't
//! run with. The first time, Windows asks once (UAC) to set up two scheduled
//! tasks, "Bloom Touchscreen Off" and "Bloom Touchscreen On"; after that they
//! run with no prompt. Each runs Bloom itself as administrator for a moment
//! (`bloom.exe --touchscreen-device off|on "<ids>"`), which turns exactly this
//! touchscreen off or on through Windows' device API, the same change as
//! Device Manager's Disable/Enable (and G-Helper's Disable-PnpDevice), and
//! notes the result in ghelper.log. The tasks are registered by Bloom itself,
//! started as administrator for that one moment
//! (`bloom.exe --register-touchscreen-tasks`), through Windows' Task Scheduler.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter};
use windows::core::PCWSTR;

const TASK_PREFIX: &str = "Bloom";
const TASK_OFF: &str = "Touchscreen Off";
const TASK_ON: &str = "Touchscreen On";
const IDS_KEY: &str = "bloom-touchscreen-ids";
pub const REGISTER_ARG: &str = "--register-touchscreen-tasks";
pub const DEVICE_ARG: &str = "--touchscreen-device";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// CM_PROB_DISABLED: the device was turned off.
const PROBLEM_DISABLED: u32 = 0x16;

struct Device {
    id: String,
    enabled: bool,
}

fn system32(file: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join(file)
}

/// The touch screens (what G-Helper finds as "*touch*screen*").
fn touch_screens() -> Vec<Device> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_Status, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
        SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW,
        CM_DEVNODE_STATUS_FLAGS, CM_PROB, CR_SUCCESS, DIGCF_ALLCLASSES, DIGCF_PRESENT,
        SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME, SP_DEVINFO_DATA,
    };
    let mut found = Vec::new();
    unsafe {
        let Ok(set) =
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES | DIGCF_PRESENT)
        else {
            return found;
        };
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(set, index, &mut info).is_err() {
                break;
            }
            index += 1;
            let name = [SPDRP_FRIENDLYNAME, SPDRP_DEVICEDESC]
                .into_iter()
                .find_map(|property| {
                    let mut buffer = [0u8; 512];
                    SetupDiGetDeviceRegistryPropertyW(
                        set,
                        &info,
                        property,
                        None,
                        Some(&mut buffer),
                        None,
                    )
                    .ok()?;
                    let wide: Vec<u16> = buffer
                        .chunks_exact(2)
                        .map(|b| u16::from_le_bytes([b[0], b[1]]))
                        .take_while(|&c| c != 0)
                        .collect();
                    Some(String::from_utf16_lossy(&wide))
                });
            let Some(name) = name else {
                continue;
            };
            let lower = name.to_lowercase();
            let Some(touch) = lower.find("touch") else {
                continue;
            };
            if !lower[touch..].contains("screen") {
                continue;
            }
            let mut id = [0u16; 512];
            if SetupDiGetDeviceInstanceIdW(set, &info, Some(&mut id), None).is_err() {
                continue;
            }
            let len = id.iter().position(|&c| c == 0).unwrap_or(0);
            let mut status = CM_DEVNODE_STATUS_FLAGS(0);
            let mut problem = CM_PROB(0);
            let enabled = CM_Get_DevNode_Status(&mut status, &mut problem, info.DevInst, 0)
                != CR_SUCCESS
                || problem.0 != PROBLEM_DISABLED;
            found.push(Device {
                id: String::from_utf16_lossy(&id[..len]),
                enabled,
            });
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    found
}

fn log(line: &str) {
    crate::ghelper::log(&format!("touchscreen: {line}"));
}

/// What a task runs: Bloom, to turn these touchscreens off ("off") or on.
fn task_command(exe: &str, ids: &[String], state: &str) -> (String, String) {
    (
        exe.to_string(),
        format!("{DEVICE_ARG} {state} \"{}\"", ids.join("|")),
    )
}

/// `bloom.exe --touchscreen-device off|on "<id>|<id>"`, run as administrator
/// by a task: turns each device off or on, as Device Manager does (kept
/// after a restart). True when every one took the change.
pub fn set_devices(rest: &[std::ffi::OsString]) -> bool {
    let arg = |i: usize| {
        rest.get(i)
            .map(|a| a.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    let on = match arg(0).as_str() {
        "on" => true,
        "off" => false,
        other => {
            log(&format!("device: unknown state \"{other}\""));
            return false;
        }
    };
    let ids: Vec<String> = arg(1)
        .split('|')
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        log("device: no device IDs given");
        return false;
    }
    let mut all = true;
    for id in &ids {
        match unsafe { change_device(id, on) } {
            Ok(()) => log(&format!(
                "device: turned {} {id}",
                if on { "on" } else { "off" }
            )),
            Err(e) => {
                all = false;
                log(&format!(
                    "device: couldn't turn {} {id}: {e}",
                    if on { "on" } else { "off" }
                ));
            }
        }
    }
    all
}

/// Device Manager's Disable/Enable for one device (by its instance ID).
unsafe fn change_device(id: &str, on: bool) -> Result<(), String> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList, SetupDiDestroyDeviceInfoList,
        SetupDiOpenDeviceInfoW, SetupDiSetClassInstallParamsW, DICS_DISABLE, DICS_ENABLE,
        DICS_FLAG_GLOBAL, DIF_PROPERTYCHANGE, SP_CLASSINSTALL_HEADER, SP_DEVINFO_DATA,
        SP_PROPCHANGE_PARAMS,
    };
    let step = |what: &str, e: windows::core::Error| format!("{what}: {e} ({:#010x})", e.code().0);
    let set = SetupDiCreateDeviceInfoList(None, None).map_err(|e| step("device list", e))?;
    let result = (|| {
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        let mut info = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        SetupDiOpenDeviceInfoW(set, PCWSTR(wide.as_ptr()), None, 0, Some(&mut info))
            .map_err(|e| step("find the device", e))?;
        let params = SP_PROPCHANGE_PARAMS {
            ClassInstallHeader: SP_CLASSINSTALL_HEADER {
                cbSize: std::mem::size_of::<SP_CLASSINSTALL_HEADER>() as u32,
                InstallFunction: DIF_PROPERTYCHANGE,
            },
            StateChange: if on { DICS_ENABLE } else { DICS_DISABLE },
            Scope: DICS_FLAG_GLOBAL,
            HwProfile: 0,
        };
        SetupDiSetClassInstallParamsW(
            set,
            Some(&info),
            Some(&params.ClassInstallHeader),
            std::mem::size_of::<SP_PROPCHANGE_PARAMS>() as u32,
        )
        .map_err(|e| step("prepare the change", e))?;
        SetupDiCallClassInstaller(DIF_PROPERTYCHANGE, set, Some(&info))
            .map_err(|e| step("change", e))
    })();
    let _ = SetupDiDestroyDeviceInfoList(set);
    result
}

/// `bloom.exe --register-touchscreen-tasks "<id>|<id>"`, run as
/// administrator: registers both tasks. True when both are in place.
pub fn register_tasks(rest: &[std::ffi::OsString]) -> bool {
    let ids: Vec<String> = rest
        .first()
        .map(|a| a.to_string_lossy().to_string())
        .unwrap_or_default()
        .split('|')
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        log("register: no device IDs given");
        return false;
    }
    match unsafe { register(&ids) } {
        Ok(()) => {
            log("register: both tasks are in place");
            true
        }
        Err(e) => {
            log(&format!("register failed: {e}"));
            false
        }
    }
}

unsafe fn register(ids: &[String]) -> Result<(), String> {
    use windows::core::{Interface, BSTR};
    use windows::Win32::Foundation::{VARIANT_FALSE, VARIANT_TRUE};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ITaskService, TaskScheduler, TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE,
        TASK_INSTANCES_IGNORE_NEW, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_HIGHEST,
    };
    use windows::Win32::System::Variant::VARIANT;

    let step = |what: &str, e: windows::core::Error| format!("{what}: {e} ({:#010x})", e.code().0);
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let sid = crate::ghelper::user_sid().ok_or("the Windows account couldn't be read")?;
    let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
        .map_err(|e| step("Task Scheduler", e))?;
    let none = VARIANT::default();
    service
        .Connect(&none, &none, &none, &none)
        .map_err(|e| step("connect", e))?;
    let root = service
        .GetFolder(&BSTR::from("\\"))
        .map_err(|e| step("root folder", e))?;
    // The tasks go straight under Task Scheduler's root, as "Bloom
    // Touchscreen Off/On". Nothing else in Task Scheduler is touched: "Bloom"
    // there is Bloom's own startup task (an earlier version took that name
    // for a folder and removed it, which turned off starting with Windows).
    // Registered as administrator, a task belongs to the administrators:
    // without this, Bloom (not running as administrator) couldn't start it.
    // Administrators and Windows keep full control; this account may read
    // and run it, nothing more.
    let access = VARIANT::from(BSTR::from(format!(
        "D:(A;;FA;;;BA)(A;;FA;;;SY)(A;;GRGX;;;{sid})"
    )));
    let exe = std::env::current_exe()
        .map_err(|e| format!("Bloom's own path: {e}"))?
        .display()
        .to_string();
    for (name, state, what) in [(TASK_OFF, "off", "off"), (TASK_ON, "on", "on")] {
        let task = service.NewTask(0).map_err(|e| step("new task", e))?;
        task.RegistrationInfo()
            .and_then(|info| {
                info.SetDescription(&BSTR::from(format!(
                    "Bloom: turns the touchscreen {what} (Settings > Integrations > G-Helper)"
                )))
            })
            .map_err(|e| step("description", e))?;
        let principal = task.Principal().map_err(|e| step("principal", e))?;
        principal
            .SetUserId(&BSTR::from(sid.as_str()))
            .and_then(|_| principal.SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN))
            .and_then(|_| principal.SetRunLevel(TASK_RUNLEVEL_HIGHEST))
            .map_err(|e| step("account", e))?;
        let settings = task.Settings().map_err(|e| step("settings", e))?;
        settings
            .SetDisallowStartIfOnBatteries(VARIANT_FALSE)
            .and_then(|_| settings.SetStopIfGoingOnBatteries(VARIANT_FALSE))
            .and_then(|_| settings.SetAllowDemandStart(VARIANT_TRUE))
            .and_then(|_| settings.SetExecutionTimeLimit(&BSTR::from("PT1M")))
            .and_then(|_| settings.SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW))
            .map_err(|e| step("settings", e))?;
        let (path, arguments) = task_command(&exe, ids, state);
        let action: IExecAction = task
            .Actions()
            .and_then(|actions| actions.Create(TASK_ACTION_EXEC))
            .and_then(|action| action.cast())
            .map_err(|e| step("action", e))?;
        action
            .SetPath(&BSTR::from(path))
            .and_then(|_| action.SetArguments(&BSTR::from(arguments)))
            .map_err(|e| step("command", e))?;
        root.RegisterTaskDefinition(
            &BSTR::from(task_name(name)),
            &task,
            TASK_CREATE_OR_UPDATE.0,
            &none,
            &none,
            TASK_LOGON_INTERACTIVE_TOKEN,
            &access,
        )
        .map_err(|e| step(&format!("register \"{name}\""), e))?;
    }
    Ok(())
}

/// Starts Bloom as administrator to register the tasks (Windows asks once).
fn set_up(ids: &[String]) -> Result<(), String> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let verb = wide("runas");
    let file = wide(&exe.display().to_string());
    let params = wide(&format!("{REGISTER_ARG} \"{}\"", ids.join("|")));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        // ShellExecuteEx wants COM on the calling thread.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        ShellExecuteExW(&mut info).map_err(|e| {
            log(&format!(
                "set up: the permission prompt failed or was declined ({e})"
            ));
            "Windows' permission prompt was declined".to_string()
        })?;
        if info.hProcess.is_invalid() {
            log("set up: no process handle came back");
            return Err("The touchscreen tasks couldn't be set up (see ghelper.log)".into());
        }
        WaitForSingleObject(info.hProcess, 60_000);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = windows::Win32::Foundation::CloseHandle(info.hProcess);
        if code != 0 {
            log(&format!("set up: the administrator step ended with {code}"));
            return Err("The touchscreen tasks couldn't be set up (see ghelper.log)".into());
        }
    }
    Ok(())
}

/// A task's name in Task Scheduler ("Bloom Touchscreen Off").
fn task_name(name: &str) -> String {
    format!("{TASK_PREFIX} {name}")
}

fn run_task(name: &str) -> Result<(), String> {
    let out = std::process::Command::new(system32("schtasks.exe"))
        .args(["/Run", "/TN", &format!("\\{}", task_name(name))])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// A switch is under way (a second press meanwhile is let go of, so two
/// presses never undo each other).
static BUSY: AtomicBool = AtomicBool::new(false);

/// Turns the touchscreen off when it is on, else on, and shows G-Helper's
/// pop-up once it has changed.
pub fn toggle(app: &AppHandle) -> Result<(), String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let started = switch(app);
    if started.is_err() {
        BUSY.store(false, Ordering::SeqCst);
    }
    started
}

fn switch(app: &AppHandle) -> Result<(), String> {
    let devices = touch_screens();
    if devices.is_empty() {
        return Err("No touchscreen was found".into());
    }
    let mut ids: Vec<String> = devices.iter().map(|d| d.id.clone()).collect();
    ids.sort();
    let turn_off = devices.iter().any(|d| d.enabled);
    // The version of the setup is part of what is remembered, so tasks made
    // by an earlier Bloom are made again (once) when that changes.
    // Bloom's path is part of it too: the tasks start Bloom.
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let ids_text = format!("v5|{exe}|{}", ids.join("|"));
    if crate::utils::get_setting_str(app, IDS_KEY).as_deref() != Some(ids_text.as_str()) {
        set_up(&ids)?;
        let _ = crate::commands::save_setting(app.clone(), IDS_KEY.into(), Value::String(ids_text));
    }
    let task = if turn_off { TASK_OFF } else { TASK_ON };
    if let Err(e) = run_task(task) {
        // Made again (one more Windows prompt) and tried once more.
        log(&format!(
            "{task} couldn't run: {e}; setting the tasks up again"
        ));
        set_up(&ids)?;
        if let Err(e) = run_task(task) {
            log(&format!("{task} still couldn't run: {e}"));
            let _ = crate::commands::save_setting(
                app.clone(),
                IDS_KEY.into(),
                Value::String(String::new()),
            );
            return Err("The touchscreen task couldn't run (see ghelper.log)".into());
        }
    }
    log(&format!("ran {task}"));
    let handle = app.clone();
    std::thread::spawn(move || {
        // The change takes a moment; the result is checked, not assumed.
        let mut now_on = !turn_off;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(250));
            now_on = touch_screens().iter().any(|d| d.enabled);
            if now_on != turn_off {
                break;
            }
        }
        if now_on == turn_off {
            log("the task ran but the touchscreen didn't change within 10 s (the task's own lines above say why)");
        } else {
            log(if now_on {
                "touchscreen is on"
            } else {
                "touchscreen is off"
            });
        }
        if now_on != turn_off && crate::ghelper::osd_enabled() {
            // G-Helper's own words and icon: "Touchscreen On" / "Off".
            crate::ghelper_toast::show(
                if now_on {
                    "Touchscreen On"
                } else {
                    "Touchscreen Off"
                },
                Some(crate::ghelper_toast::Icon::Touchpad),
            );
        }
        let _ = handle.emit("ghelper-changed", ());
        BUSY.store(false, Ordering::SeqCst);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_starts_bloom_for_the_given_devices() {
        let (path, arguments) = task_command(
            "C:\\Bloom\\bloom.exe",
            &["HID\\VID_1&PID_2\\3&ABC".to_string(), "HID\\X".to_string()],
            "off",
        );
        assert_eq!(path, "C:\\Bloom\\bloom.exe");
        assert_eq!(
            arguments,
            "--touchscreen-device off \"HID\\VID_1&PID_2\\3&ABC|HID\\X\""
        );
    }
}
