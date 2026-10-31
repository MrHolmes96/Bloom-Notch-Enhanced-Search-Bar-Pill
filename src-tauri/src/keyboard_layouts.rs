//! The keyboard layouts set up in Windows (Settings > Time & language >
//! Language & region), listed the way Windows' own switcher lists them, and
//! switching to one from the notch.
//!
//! The list comes from Windows' text services (the same list Win+Space
//! shows: enabled keyboard layouts and input methods such as IMEs). A switch
//! is asked of the app window in front, the way Windows' own switcher does
//! it, so it applies to the app being typed in (and, with Windows' default
//! setting, to every app). It is then checked: Bloom only says it switched
//! when Windows reports the new layout.

use serde::Serialize;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardLayoutList, HKL};
use windows::Win32::UI::TextServices::{
    CLSID_TF_InputProcessorProfiles, ITfInputProcessorProfileMgr, ITfInputProcessorProfiles,
    GUID_TFCAT_TIP_KEYBOARD, TF_INPUTPROCESSORPROFILE, TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
    TF_IPPMF_FORSESSION, TF_IPP_FLAG_ENABLED, TF_PROFILETYPE_INPUTPROCESSOR,
    TF_PROFILETYPE_KEYBOARDLAYOUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow, PostMessageW,
    WM_INPUTLANGCHANGEREQUEST,
};

#[derive(Clone, Debug, Serialize)]
pub struct Layout {
    /// What to pass back to switch to it.
    id: String,
    /// "Danish (Denmark)".
    language: String,
    /// "Danish", "United States-International", "Microsoft IME".
    keyboard: String,
    /// "DAN", as on Windows' own language button.
    short: String,
    /// In use in the app in front.
    active: bool,
}

/// One enabled keyboard of Windows' list.
#[derive(Clone, Copy)]
struct Profile {
    keyboard_layout: bool,
    lang: u16,
    clsid: GUID,
    profile: GUID,
    hkl: isize,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Runs `f` on a thread of its own with COM set up the way text services
/// expect (an apartment thread), so nothing else of Bloom's is affected.
fn on_com_thread<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    std::thread::spawn(move || unsafe {
        let started = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = f();
        if started {
            CoUninitialize();
        }
        result
    })
    .join()
    .map_err(|_| "Windows' keyboard list couldn't be read".to_string())?
}

fn profiles_api() -> Result<(ITfInputProcessorProfiles, ITfInputProcessorProfileMgr), String> {
    use windows::core::Interface;
    unsafe {
        let profiles: ITfInputProcessorProfiles =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Windows' keyboard list isn't available ({e})"))?;
        let manager: ITfInputProcessorProfileMgr = profiles.cast().map_err(|e| e.to_string())?;
        Ok((profiles, manager))
    }
}

/// The enabled keyboards, in Windows' order (the languages' order).
fn enabled_profiles(
    profiles: &ITfInputProcessorProfiles,
    manager: &ITfInputProcessorProfileMgr,
) -> Vec<Profile> {
    let mut found = Vec::new();
    unsafe {
        let mut langs: *mut u16 = std::ptr::null_mut();
        let mut count = 0u32;
        if profiles.GetLanguageList(&mut langs, &mut count).is_err() || langs.is_null() {
            return found;
        }
        let languages = std::slice::from_raw_parts(langs, count as usize).to_vec();
        CoTaskMemFree(Some(langs as *const _));
        for lang in languages {
            let Ok(list) = manager.EnumProfiles(lang) else {
                continue;
            };
            loop {
                let mut one = [TF_INPUTPROCESSORPROFILE::default()];
                let mut got = 0u32;
                if list.Next(&mut one, &mut got).is_err() || got == 0 {
                    break;
                }
                let p = one[0];
                if p.dwFlags & TF_IPP_FLAG_ENABLED == 0 {
                    continue;
                }
                let keyboard_layout = p.dwProfileType == TF_PROFILETYPE_KEYBOARDLAYOUT;
                // Input methods other than keyboards (speech, handwriting) aren't
                // in Windows' switcher either.
                if !keyboard_layout
                    && (p.dwProfileType != TF_PROFILETYPE_INPUTPROCESSOR
                        || p.catid != GUID_TFCAT_TIP_KEYBOARD)
                {
                    continue;
                }
                if keyboard_layout && p.hkl.0.is_null() {
                    continue;
                }
                found.push(Profile {
                    keyboard_layout,
                    lang: p.langid,
                    clsid: p.clsid,
                    profile: p.guidProfile,
                    hkl: p.hkl.0 as isize,
                });
            }
        }
    }
    found
}

/// The registry name of a layout ("00000406") from its handle: the low word
/// is the language, the high word the keyboard (or an index into the
/// registry's "Layout Id"s for variants, or an IME's own number).
fn klid_of(hkl: isize) -> Option<String> {
    let value = hkl as usize as u32;
    let device = value >> 16;
    match device & 0xF000 {
        0xF000 => {
            let layout_id = device & 0x0FFF;
            keyboard_layout_keys().into_iter().find(|klid| {
                registry_string(&format!("{LAYOUTS_KEY}\\{klid}"), "Layout Id")
                    .and_then(|id| u32::from_str_radix(id.trim(), 16).ok())
                    == Some(layout_id)
            })
        }
        0xE000 => Some(format!("{value:08X}")),
        _ => Some(format!("{device:08X}")),
    }
}

const LAYOUTS_KEY: &str = "SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts";

fn keyboard_layout_keys() -> Vec<String> {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };
    let mut names = Vec::new();
    unsafe {
        let mut key = HKEY::default();
        let path = wide(LAYOUTS_KEY);
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path.as_ptr()),
            Some(0),
            KEY_READ,
            &mut key,
        )
        .is_err()
        {
            return names;
        }
        for index in 0..2048u32 {
            let mut buffer = [0u16; 64];
            let mut length = buffer.len() as u32;
            if RegEnumKeyExW(
                key,
                index,
                Some(windows::core::PWSTR(buffer.as_mut_ptr())),
                &mut length,
                None,
                Some(windows::core::PWSTR::null()),
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            names.push(String::from_utf16_lossy(&buffer[..length as usize]));
        }
        let _ = RegCloseKey(key);
    }
    names
}

fn registry_string(path: &str, value: &str) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let path = wide(path);
    let value = wide(value);
    let mut buffer = vec![0u16; 512];
    let mut size = (buffer.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    let text = from_wide(&buffer);
    (!text.is_empty()).then_some(text)
}

/// A layout's name as Windows shows it, in Windows' language.
fn layout_name(hkl: isize) -> Option<String> {
    let klid = klid_of(hkl)?;
    let key = format!("{LAYOUTS_KEY}\\{klid}");
    if let Some(display) = registry_string(&key, "Layout Display Name") {
        let source = wide(&display);
        let mut out = [0u16; 256];
        if unsafe {
            windows::Win32::UI::Shell::SHLoadIndirectString(PCWSTR(source.as_ptr()), &mut out, None)
        }
        .is_ok()
        {
            let name = from_wide(&out);
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    registry_string(&key, "Layout Text")
}

fn locale_info(lang: u16, kind: u32) -> String {
    use windows::Win32::Globalization::{GetLocaleInfoEx, LCIDToLocaleName};
    let mut name = [0u16; 85];
    unsafe {
        if LCIDToLocaleName(lang as u32, Some(&mut name), 0) == 0 {
            return String::new();
        }
        let mut out = [0u16; 128];
        if GetLocaleInfoEx(PCWSTR(name.as_ptr()), kind, Some(&mut out)) == 0 {
            return String::new();
        }
        from_wide(&out)
    }
}

fn language_name(lang: u16) -> String {
    let name = locale_info(
        lang,
        windows::Win32::Globalization::LOCALE_SLOCALIZEDDISPLAYNAME,
    );
    if name.is_empty() {
        format!("Language {lang:04X}")
    } else {
        name
    }
}

fn language_short(lang: u16) -> String {
    locale_info(lang, windows::Win32::Globalization::LOCALE_SISO639LANGNAME2).to_uppercase()
}

/// The app window keyboard input goes to: the one in front, or the last app
/// window in front when that's one of Bloom's own.
fn target_window() -> Option<HWND> {
    unsafe {
        let front = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(front, Some(&mut pid));
        if !front.0.is_null() && pid != std::process::id() {
            return Some(front);
        }
        let last = HWND(crate::services::last_front_window() as *mut _);
        (!last.0.is_null() && IsWindow(Some(last)).as_bool()).then_some(last)
    }
}

/// The layout in use in that window (Bloom's own when there's none).
fn layout_in(window: Option<HWND>) -> isize {
    unsafe {
        let thread = window
            .map(|w| GetWindowThreadProcessId(w, None))
            .unwrap_or(0);
        GetKeyboardLayout(thread).0 as isize
    }
}

fn lang_of(hkl: isize) -> u16 {
    (hkl as usize & 0xFFFF) as u16
}

fn id_of(p: &Profile) -> String {
    if p.keyboard_layout {
        format!("kl:{:x}", p.hkl as usize)
    } else {
        format!(
            "tip:{:x}:{:032x}:{:032x}",
            p.lang,
            p.clsid.to_u128(),
            p.profile.to_u128()
        )
    }
}

fn describe(profiles: &ITfInputProcessorProfiles, list: &[Profile], active: isize) -> Vec<Layout> {
    let exact = list
        .iter()
        .position(|p| p.keyboard_layout && p.hkl == active);
    let by_language = list
        .iter()
        .position(|p| !p.keyboard_layout && p.lang == lang_of(active));
    let current = exact.or(by_language);
    list.iter()
        .enumerate()
        .map(|(index, p)| {
            let keyboard = if p.keyboard_layout {
                layout_name(p.hkl)
            } else {
                unsafe {
                    profiles
                        .GetLanguageProfileDescription(&p.clsid, p.lang, &p.profile)
                        .ok()
                        .map(|b| b.to_string())
                }
            }
            .unwrap_or_default();
            Layout {
                id: id_of(p),
                language: language_name(p.lang),
                keyboard,
                short: language_short(p.lang),
                active: current == Some(index),
            }
        })
        .collect()
}

/// The keyboards set up in Windows, the one in use marked.
#[tauri::command]
pub async fn keyboard_layouts() -> Result<Vec<Layout>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        on_com_thread(|| {
            let (profiles, manager) = profiles_api()?;
            let list = enabled_profiles(&profiles, &manager);
            if list.is_empty() {
                return Err("Windows didn't list any keyboard layouts".into());
            }
            Ok(describe(&profiles, &list, layout_in(target_window())))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A loaded layout for a language (for input methods, which are asked for
/// by their language).
fn loaded_layout_for(lang: u16) -> isize {
    unsafe {
        let count = GetKeyboardLayoutList(None);
        let mut list = vec![HKL::default(); count.max(0) as usize];
        let got = GetKeyboardLayoutList(Some(&mut list));
        list.truncate(got.max(0) as usize);
        list.iter()
            .map(|h| h.0 as isize)
            .find(|&h| lang_of(h) == lang)
            .unwrap_or(((lang as isize) << 16) | lang as isize)
    }
}

/// Switches the app in front to a keyboard from `keyboard_layouts`.
#[tauri::command]
pub async fn switch_keyboard_layout(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        on_com_thread(move || {
            let (profiles, manager) = profiles_api()?;
            let list = enabled_profiles(&profiles, &manager);
            let profile = list
                .iter()
                .find(|p| id_of(p) == id)
                .copied()
                .ok_or("That keyboard isn't set up in Windows any more")?;
            let window = target_window();
            let hkl = if profile.keyboard_layout {
                profile.hkl
            } else {
                loaded_layout_for(profile.lang)
            };
            let switched = || {
                let now = layout_in(window);
                if profile.keyboard_layout {
                    now == hkl
                } else {
                    lang_of(now) == profile.lang
                }
            };
            let wait_for_switch = || {
                for _ in 0..16 {
                    if switched() {
                        return true;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                false
            };
            unsafe {
                if !profile.keyboard_layout {
                    // An input method (an IME) is chosen through text services.
                    let _ = manager.ActivateProfile(
                        TF_PROFILETYPE_INPUTPROCESSOR,
                        profile.lang,
                        &profile.clsid,
                        &profile.profile,
                        HKL::default(),
                        TF_IPPMF_FORSESSION | TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
                    );
                }
                // The app's window is asked, as Windows' own switcher does.
                if let Some(window) = window {
                    let _ = PostMessageW(
                        Some(window),
                        WM_INPUTLANGCHANGEREQUEST,
                        WPARAM(0),
                        LPARAM(hkl),
                    );
                }
            }
            if wait_for_switch() {
                return Ok(());
            }
            // The app didn't take the request (an app running as administrator
            // doesn't take it from Bloom): ask text services for the session.
            unsafe {
                let _ = manager.ActivateProfile(
                    profile.dwprofile_type(),
                    profile.lang,
                    &profile.clsid,
                    &profile.profile,
                    HKL(hkl as *mut _),
                    TF_IPPMF_FORSESSION | TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
                );
            }
            if wait_for_switch() {
                Ok(())
            } else {
                Err(
                    "Windows didn't switch the keyboard for the app in front. If it runs as administrator, switch from inside it with Win+Space"
                        .into(),
                )
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

impl Profile {
    fn dwprofile_type(&self) -> u32 {
        if self.keyboard_layout {
            TF_PROFILETYPE_KEYBOARDLAYOUT
        } else {
            TF_PROFILETYPE_INPUTPROCESSOR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_handles_give_registry_names() {
        // English (US) language, Danish keyboard.
        assert_eq!(klid_of(0x0406_0409).as_deref(), Some("00000406"));
        // Danish language and keyboard.
        assert_eq!(klid_of(0x0406_0406).as_deref(), Some("00000406"));
        // An IME's own handle.
        assert_eq!(
            klid_of(0xE001_0411u32 as isize).as_deref(),
            Some("E0010411")
        );
        assert_eq!(lang_of(0x0406_0409), 0x0409);
    }
}
