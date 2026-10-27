//! Keeps the sound devices chosen in Windows the same across a Screen Off.
//!
//! When the screen goes dark, a screen that carries sound (HDMI or
//! DisplayPort, a monitor's speakers or a headset plugged into it) drops off
//! as a sound device, and Windows moves the sound to another device. When the
//! screen comes back, the device returns, but Windows (and apps that show the
//! current device, like SoundSwitch) stay on the one it moved to. Bloom notes
//! the default output and input devices before turning the screen off, and
//! puts them back once it is on again and they are there.

#![allow(non_snake_case)]

use std::time::{Duration, Instant};

use windows::core::{HRESULT, PCWSTR, PWSTR};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole,
    IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

/// How Windows' own Sound settings set the default device. Undocumented; the
/// layout is the one SoundSwitch and other tools use.
#[windows::core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: windows::core::IUnknown {
    fn GetMixFormat(&self, id: PCWSTR, format: *mut *mut core::ffi::c_void) -> HRESULT;
    fn GetDeviceFormat(
        &self,
        id: PCWSTR,
        default: i32,
        format: *mut *mut core::ffi::c_void,
    ) -> HRESULT;
    fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(
        &self,
        id: PCWSTR,
        endpoint: *mut core::ffi::c_void,
        mix: *mut core::ffi::c_void,
    ) -> HRESULT;
    fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, a: *mut i64, b: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, id: PCWSTR, mode: *mut core::ffi::c_void) -> HRESULT;
    fn SetShareMode(&self, id: PCWSTR, mode: *mut core::ffi::c_void) -> HRESULT;
    fn GetPropertyValue(
        &self,
        id: PCWSTR,
        fx: i32,
        key: *const core::ffi::c_void,
        value: *mut core::ffi::c_void,
    ) -> HRESULT;
    fn SetPropertyValue(
        &self,
        id: PCWSTR,
        fx: i32,
        key: *const core::ffi::c_void,
        value: *mut core::ffi::c_void,
    ) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
}

const CLSID_POLICY_CONFIG: windows::core::GUID =
    windows::core::GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Output and input, each for the three roles Windows keeps a default for.
const SLOTS: [(EDataFlow, ERole); 6] = [
    (eRender, eConsole),
    (eRender, eMultimedia),
    (eRender, eCommunications),
    (eCapture, eConsole),
    (eCapture, eMultimedia),
    (eCapture, eCommunications),
];

/// The default devices at one moment, by device ID.
pub struct Defaults([Option<String>; 6]);

/// How long after the screen is on again Bloom looks for the devices to
/// return (a screen's sound device comes back a few seconds after its picture).
const RETURN_WINDOW: Duration = Duration::from_secs(15);

/// COM for the calling thread while `f` runs.
fn with_com<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let result = f();
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

fn enumerator() -> Option<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() }
}

fn default_id(devices: &IMMDeviceEnumerator, flow: EDataFlow, role: ERole) -> Option<String> {
    unsafe {
        let device = devices.GetDefaultAudioEndpoint(flow, role).ok()?;
        let id: PWSTR = device.GetId().ok()?;
        let text = id.to_string().ok();
        CoTaskMemFree(Some(id.0 as *const core::ffi::c_void));
        text
    }
}

/// Whether the device is plugged in and turned on now.
fn is_active(devices: &IMMDeviceEnumerator, id: &str) -> bool {
    let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        devices
            .GetDevice(PCWSTR(wide.as_ptr()))
            .and_then(|device| device.GetState())
            .is_ok_and(|state| state == DEVICE_STATE_ACTIVE)
    }
}

/// The default output and input devices now.
pub fn snapshot() -> Option<Defaults> {
    with_com(|| {
        let devices = enumerator()?;
        Some(Defaults(
            SLOTS.map(|(flow, role)| default_id(&devices, flow, role)),
        ))
    })
}

/// Puts `saved` back as the default devices wherever Windows moved away from
/// them, as soon as each is there again, and watches the first seconds in
/// case Windows moves it once more; gives up on a device that hasn't come
/// back within [`RETURN_WINDOW`]. Runs on its own thread.
pub fn restore_soon(saved: Defaults) {
    let _ = std::thread::Builder::new()
        .name("bloom-sound-devices".into())
        .spawn(move || {
            with_com(|| {
                let devices = enumerator()?;
                let policy: IPolicyConfig =
                    unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL).ok()? };
                let mut pending: Vec<(EDataFlow, ERole, String)> = SLOTS
                    .iter()
                    .zip(saved.0)
                    .filter_map(|(&(flow, role), id)| Some((flow, role, id?)))
                    .collect();
                let started = Instant::now();
                while !pending.is_empty() && started.elapsed() < RETURN_WINDOW {
                    pending.retain(|(flow, role, id)| {
                        if default_id(&devices, *flow, *role).as_deref() == Some(id.as_str()) {
                            // Still (or again) the one it was.
                            return started.elapsed() < Duration::from_secs(4);
                        }
                        if !is_active(&devices, id) {
                            return true;
                        }
                        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
                        let _ = unsafe { policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), *role) };
                        // Looked at again on the next pass: done once it holds.
                        true
                    });
                    std::thread::sleep(Duration::from_millis(400));
                }
                Some(())
            });
        });
}
