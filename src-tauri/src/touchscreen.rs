//! Toggle Touchscreen from a Bloom shortcut (Settings > Integrations >
//! G-Helper), the action G-Helper runs for FN+V.
//!
//! Turning a device off or on needs administrator rights, which Bloom doesn't
//! run with. The first time, Windows asks once (UAC) to set up two scheduled
//! tasks that run Windows' own device tool (pnputil) for exactly this screen:
//! "Bloom\Touchscreen Off" and "Bloom\Touchscreen On". After that Bloom runs
//! them like G-Helper runs its own startup task, with no prompt. The tasks
//! carry fixed commands with the touchscreen's device ID, so they can do
//! nothing else. If the touchscreen changes (a new device ID), they are made
//! again, with one more prompt.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter};
use windows::core::PCWSTR;

const TASK_OFF: &str = "Bloom\\Touchscreen Off";
const TASK_ON: &str = "Bloom\\Touchscreen On";
const IDS_KEY: &str = "bloom-touchscreen-ids";
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

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn task_xml(sid: &str, description: &str, ids: &[String], verb: &str) -> String {
    // System32's tools by their full paths, unquoted (no spaces), so cmd keeps
    // the device IDs' own quotes as they are.
    let pnputil = system32("pnputil.exe").display().to_string();
    let commands: Vec<String> = ids
        .iter()
        .map(|id| xml_escape(&format!("{pnputil} {verb} \"{id}\"")))
        .collect();
    let arguments = format!(
        "--headless {} /c {}",
        xml_escape(&system32("cmd.exe").display().to_string()),
        commands.join(" &amp; ")
    );
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>{}</Description></RegistrationInfo>
  <Principals>
    <Principal id="Author">
      <UserId>{}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <ExecutionTimeLimit>PT1M</ExecutionTimeLimit>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{}</Command>
      <Arguments>{}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        xml_escape(description),
        xml_escape(sid),
        xml_escape(&system32("conhost.exe").display().to_string()),
        arguments
    )
}

/// Writes a task file as Windows' task tool reads it (UTF-16 with a BOM).
fn write_utf16(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(path, bytes)
}

/// Sets up both tasks with one administrator prompt.
fn create_tasks(ids: &[String]) -> Result<(), String> {
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let sid = crate::ghelper::user_sid().ok_or("Your Windows account couldn't be read")?;
    let dir = std::env::temp_dir().join("bloom-touchscreen");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let off = dir.join("off.xml");
    let on = dir.join("on.xml");
    write_utf16(
        &off,
        &task_xml(
            &sid,
            "Bloom: turns the touchscreen off (Settings > Integrations > G-Helper)",
            ids,
            "/disable-device",
        ),
    )
    .map_err(|e| e.to_string())?;
    write_utf16(
        &on,
        &task_xml(
            &sid,
            "Bloom: turns the touchscreen on (Settings > Integrations > G-Helper)",
            ids,
            "/enable-device",
        ),
    )
    .map_err(|e| e.to_string())?;
    let schtasks = system32("schtasks.exe");
    let parameters = format!(
        "/c \"\"{s}\" /Create /F /TN \"{TASK_OFF}\" /XML \"{}\" && \"{s}\" /Create /F /TN \"{TASK_ON}\" /XML \"{}\"\"",
        off.display(),
        on.display(),
        s = schtasks.display()
    );
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let verb = wide("runas");
    let file = wide(&system32("cmd.exe").display().to_string());
    let params = wide(&parameters);
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
        ShellExecuteExW(&mut info).map_err(|_| "Windows' permission prompt was declined")?;
        if info.hProcess.is_invalid() {
            return Err("The tasks couldn't be set up".into());
        }
        WaitForSingleObject(info.hProcess, 60_000);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = windows::Win32::Foundation::CloseHandle(info.hProcess);
        let _ = std::fs::remove_dir_all(&dir);
        if code != 0 {
            return Err("The tasks couldn't be set up".into());
        }
    }
    Ok(())
}

fn run_task(name: &str) -> bool {
    std::process::Command::new(system32("schtasks.exe"))
        .args(["/Run", "/TN", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Turns the touchscreen off when it is on, else on.
pub fn toggle(app: &AppHandle) -> Result<(), String> {
    let devices = touch_screens();
    if devices.is_empty() {
        return Err("No touchscreen was found".into());
    }
    let mut ids: Vec<String> = devices.iter().map(|d| d.id.clone()).collect();
    ids.sort();
    let turn_off = devices.iter().any(|d| d.enabled);
    let known = crate::utils::get_setting_str(app, IDS_KEY);
    let ids_text = ids.join("|");
    if known.as_deref() != Some(ids_text.as_str()) {
        create_tasks(&ids)?;
        let _ = crate::commands::save_setting(app.clone(), IDS_KEY.into(), Value::String(ids_text));
    }
    let task = if turn_off { TASK_OFF } else { TASK_ON };
    crate::ghelper::log(&format!(
        "touchscreen: running {task} for {}",
        ids.join(", ")
    ));
    if !run_task(task) {
        // Removed by hand: made again next time.
        let _ = crate::commands::save_setting(
            app.clone(),
            IDS_KEY.into(),
            Value::String(String::new()),
        );
        return Err("Bloom's touchscreen task couldn't run. Try again to set it up".into());
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        // pnputil takes a moment; the result is checked, not assumed.
        let mut now_on = !turn_off;
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(500));
            now_on = touch_screens().iter().any(|d| d.enabled);
            if now_on != turn_off {
                break;
            }
        }
        let (title, body) = if now_on == turn_off {
            crate::ghelper::log("touchscreen: the task ran but the touchscreen didn't change");
            (
                "Touchscreen didn't change",
                "Windows' device tool didn't switch it",
            )
        } else if now_on {
            ("Touchscreen on", "")
        } else {
            ("Touchscreen off", "")
        };
        crate::notifications::show_bloom_notice(&handle, title, body);
        let _ = handle.emit("ghelper-changed", ());
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_runs_only_pnputil_for_the_given_devices() {
        let xml = task_xml(
            "S-1-5-21-1",
            "test",
            &["HID\\VID_1&PID_2\\3&ABC".to_string(), "HID\\X".to_string()],
            "/disable-device",
        );
        assert!(xml.contains("<UserId>S-1-5-21-1</UserId>"));
        assert!(xml.contains(
            "pnputil.exe /disable-device &quot;HID\\VID_1&amp;PID_2\\3&amp;ABC&quot; &amp; "
        ));
        assert!(xml.contains("/c "));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
    }
}
