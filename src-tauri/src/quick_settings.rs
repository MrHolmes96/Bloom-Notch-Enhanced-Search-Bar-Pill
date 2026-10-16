//! The Windows quick settings the notch's control center can show next to its
//! own buttons: Night light, Airplane mode, Mobile hotspot, Rotation lock and
//! Live captions as real toggles, and the ones Windows itself opens a flyout or
//! a Settings page for (Project, Cast, Keyboard layout, Accessibility, Nearby
//! sharing, VPN, Color filters) the same way Windows does.

#![allow(non_snake_case)]

use serde::Serialize;
use windows::core::{GUID, HRESULT, PCSTR, PCWSTR};
use windows::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_BINARY, RRF_RT_REG_BINARY,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_LWIN, VK_SPACE,
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------- night light

/// Windows keeps Night light's on/off state in this CloudStore blob; writing a
/// changed blob is what the Settings toggle does too.
const NIGHT_LIGHT_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\CloudStore\\Store\\DefaultAccount\\Current\\default$windows.data.bluelightreduction.bluelightreductionstate\\windows.data.bluelightreduction.bluelightreductionstate";

fn read_binary(subkey: &str, value: &str) -> Option<Vec<u8>> {
    let subkey = wide(subkey);
    let value = wide(value);
    let mut size = 0u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let mut data = vec![0u8; size as usize];
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        data.truncate(size as usize);
        Some(data)
    }
}

fn night_light_on() -> Option<bool> {
    let data = read_binary(NIGHT_LIGHT_KEY, "Data")?;
    (data.len() > 24).then(|| data[18] == 0x15)
}

/// The blob with the state flipped: byte 18 is 0x15 when on and 0x13 when
/// off, and "on" carries two extra bytes (0x10 0x00) at offset 23. The
/// timestamp at 10-14 is bumped so Windows sees a new state.
fn flipped_night_light(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 25 {
        return None;
    }
    let mut next: Vec<u8> = if data[18] == 0x15 {
        let mut v = data[..23].to_vec();
        v.extend_from_slice(&data[25..]);
        v[18] = 0x13;
        v
    } else {
        let mut v = data[..23].to_vec();
        v.extend_from_slice(&[0x10, 0x00]);
        v.extend_from_slice(&data[23..]);
        v[18] = 0x15;
        v
    };
    for byte in next.iter_mut().take(15).skip(10) {
        if *byte != 0xff {
            *byte += 1;
            break;
        }
    }
    Some(next)
}

fn toggle_night_light() -> Result<(), String> {
    let data =
        read_binary(NIGHT_LIGHT_KEY, "Data").ok_or("Night light isn't available on this PC")?;
    let next = flipped_night_light(&data).ok_or("Night light isn't available on this PC")?;
    let subkey = wide(NIGHT_LIGHT_KEY);
    let value = wide("Data");
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            REG_BINARY.0,
            Some(next.as_ptr() as *const _),
            next.len() as u32,
        )
        .ok()
        .map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------- airplane mode

/// The radio manager behind the Airplane mode switch. Undocumented; the
/// layout is the reverse-engineered one other tools use.
#[windows::core::interface("db3afbfb-08e6-46c6-aa70-bf9a34c30ab7")]
unsafe trait IRadioManager: windows::core::IUnknown {
    fn IsRMSupported(&self, state: *mut u32) -> HRESULT;
    fn GetUIRadioInstances(&self, instances: *mut *mut core::ffi::c_void) -> HRESULT;
    fn GetSystemRadioState(&self, enabled: *mut i32, param2: *mut i32, reason: *mut i32)
        -> HRESULT;
    fn SetSystemRadioState(&self, enabled: i32) -> HRESULT;
}

const CLSID_RADIO_MANAGEMENT_API: GUID = GUID::from_u128(0x581333f6_28db_41be_bc7a_ff201f12f3f6);

fn radio_manager() -> Option<IRadioManager> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER};
    unsafe { CoCreateInstance(&CLSID_RADIO_MANAGEMENT_API, None, CLSCTX_LOCAL_SERVER).ok() }
}

fn airplane_on() -> Option<bool> {
    let manager = radio_manager()?;
    let (mut enabled, mut p2, mut p3) = (0i32, 0i32, 0i32);
    unsafe { manager.GetSystemRadioState(&mut enabled, &mut p2, &mut p3) }
        .ok()
        .ok()?;
    // "Radios enabled" is the opposite of Airplane mode.
    Some(enabled == 0)
}

fn toggle_airplane() -> Result<(), String> {
    let on = airplane_on().ok_or("Airplane mode isn't available")?;
    let manager = radio_manager().ok_or("Airplane mode isn't available")?;
    unsafe { manager.SetSystemRadioState(if on { 1 } else { 0 }) }
        .ok()
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- hotspot

fn tethering_manager(
) -> Option<windows::Networking::NetworkOperators::NetworkOperatorTetheringManager> {
    use windows::Networking::Connectivity::NetworkInformation;
    use windows::Networking::NetworkOperators::NetworkOperatorTetheringManager;
    let profile = NetworkInformation::GetInternetConnectionProfile().ok()?;
    NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile).ok()
}

fn hotspot_on() -> Option<bool> {
    use windows::Networking::NetworkOperators::TetheringOperationalState;
    let state = tethering_manager()?.TetheringOperationalState().ok()?;
    Some(state == TetheringOperationalState::On || state == TetheringOperationalState::InTransition)
}

fn toggle_hotspot() -> Result<(), String> {
    use windows::Networking::NetworkOperators::{
        TetheringOperationStatus, TetheringOperationalState,
    };
    let manager = tethering_manager().ok_or("Mobile hotspot needs an internet connection")?;
    let on = manager
        .TetheringOperationalState()
        .map_err(|e| e.to_string())?
        == TetheringOperationalState::On;
    let result = if on {
        manager.StopTetheringAsync()
    } else {
        manager.StartTetheringAsync()
    }
    .and_then(|op| op.get())
    .map_err(|e| e.to_string())?;
    match result.Status() {
        Ok(TetheringOperationStatus::Success) => Ok(()),
        _ => Err(result
            .AdditionalErrorMessage()
            .map(|m| m.to_string())
            .ok()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "Mobile hotspot couldn't be changed".into())),
    }
}

// ---------------------------------------------------------------- rotation lock

#[link(name = "user32")]
extern "system" {
    fn GetAutoRotationState(state: *mut i32) -> i32;
}

const AR_DISABLED: i32 = 0x1;
const AR_NOSENSOR: i32 = 0x10;
const AR_NOT_SUPPORTED: i32 = 0x20;

/// None when the PC has no rotation sensor.
fn rotation_locked() -> Option<bool> {
    let mut state = 0i32;
    if unsafe { GetAutoRotationState(&mut state) } == 0 {
        return None;
    }
    if state & (AR_NOSENSOR | AR_NOT_SUPPORTED) != 0 {
        return None;
    }
    Some(state & AR_DISABLED != 0)
}

/// `SetAutoRotation` is exported by user32 only by ordinal (2507); it is what
/// the Rotation lock quick setting calls.
fn toggle_rotation_lock() -> Result<(), String> {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    let locked = rotation_locked().ok_or("This PC has no rotation sensor")?;
    unsafe {
        let user32 =
            LoadLibraryW(PCWSTR(wide("user32.dll").as_ptr())).map_err(|e| e.to_string())?;
        let proc = GetProcAddress(user32, PCSTR(2507usize as *const u8))
            .ok_or("Rotation lock can't be changed here")?;
        let set_auto_rotation: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(proc);
        // Unlocking means turning auto-rotation on.
        if set_auto_rotation(if locked { 1 } else { 0 }) == 0 {
            return Err("Rotation lock can't be changed here".into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- live captions

fn process_running(exe: &str) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case(exe) {
                    found = true;
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

// ---------------------------------------------------------------- shortcuts

/// Presses a Windows shortcut (Win+P, Win+K, ...). Injected input, so Bloom's
/// own keyboard hook lets it through to the shell.
fn press_shortcut(keys: &[VIRTUAL_KEY]) -> Result<(), String> {
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    };
    let mut inputs: Vec<INPUT> = keys.iter().map(|&vk| key(vk, false)).collect();
    inputs.extend(keys.iter().rev().map(|&vk| key(vk, true)));
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err("The shortcut couldn't be sent".into())
    }
}

fn open_uri(uri: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let uri = wide(uri);
    let verb = wide("open");
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(uri.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err("Couldn't open Settings".into())
    }
}

const VK_P: VIRTUAL_KEY = VIRTUAL_KEY(0x50);
const VK_K: VIRTUAL_KEY = VIRTUAL_KEY(0x4B);
const VK_L: VIRTUAL_KEY = VIRTUAL_KEY(0x4C);

// ---------------------------------------------------------------- commands

#[derive(Serialize, Default)]
pub struct QuickSettingsState {
    /// None: not available on this PC.
    night_light: Option<bool>,
    airplane: Option<bool>,
    hotspot: Option<bool>,
    rotation_lock: Option<bool>,
    live_captions: bool,
}

fn with_com<T>(f: impl FnOnce() -> T) -> T {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
    let result = f();
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

/// Which toggles are on. `ids` limits the work to the buttons on show.
#[tauri::command]
pub async fn quick_settings_state(ids: Vec<String>) -> QuickSettingsState {
    tauri::async_runtime::spawn_blocking(move || {
        with_com(|| {
            let wants = |id: &str| ids.iter().any(|i| i == id);
            QuickSettingsState {
                night_light: if wants("night-light") {
                    night_light_on()
                } else {
                    None
                },
                airplane: if wants("airplane") {
                    airplane_on()
                } else {
                    None
                },
                hotspot: if wants("hotspot") { hotspot_on() } else { None },
                rotation_lock: if wants("rotation-lock") {
                    rotation_locked()
                } else {
                    None
                },
                live_captions: wants("live-captions") && process_running("LiveCaptions.exe"),
            }
        })
    })
    .await
    .unwrap_or_default()
}

/// Runs one quick setting the way Windows' own button does.
#[tauri::command]
pub async fn quick_setting_action(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_com(|| match id.as_str() {
            "night-light" => toggle_night_light().or_else(|_| open_uri("ms-settings:nightlight")),
            "airplane" => {
                toggle_airplane().or_else(|_| open_uri("ms-settings:network-airplanemode"))
            }
            "hotspot" => toggle_hotspot(),
            "rotation-lock" => {
                toggle_rotation_lock().or_else(|_| open_uri("ms-settings:screenrotation"))
            }
            "live-captions" => press_shortcut(&[VK_LWIN, VK_CONTROL, VK_L]),
            "project" => press_shortcut(&[VK_LWIN, VK_P]),
            "cast" => press_shortcut(&[VK_LWIN, VK_K]),
            "keyboard-layout" => press_shortcut(&[VK_LWIN, VK_SPACE]),
            "accessibility" => open_uri("ms-settings:easeofaccess"),
            "nearby-sharing" => open_uri("ms-settings:crossdevice"),
            "vpn" => open_uri("ms-settings:network-vpn"),
            "color-filters" => open_uri("ms-settings:easeofaccess-colorfilter"),
            _ => Err(format!("Unknown quick setting: {}", id)),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_light_blob_round_trips() {
        // Off: 41 bytes with 0x13 at 18.
        let mut off = vec![0u8; 41];
        off[18] = 0x13;
        off[10] = 0x05;
        off[40] = 0xAA;
        let on = flipped_night_light(&off).unwrap();
        assert_eq!(on.len(), 43);
        assert_eq!(on[18], 0x15);
        assert_eq!(&on[23..25], &[0x10, 0x00]);
        assert_eq!(on[10], 0x06);
        assert_eq!(on[42], 0xAA);
        let back = flipped_night_light(&on).unwrap();
        assert_eq!(back.len(), 41);
        assert_eq!(back[18], 0x13);
        assert_eq!(back[40], 0xAA);
    }
}
