//! Icons for file and folder results, from the shell like Explorer shows them.
//! Ordinary files share one icon per extension; files that carry their own
//! icon (programs, shortcuts, icon files) and folders are cached by path.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

static CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const OWN_ICON: &[&str] = &[
    "exe",
    "lnk",
    "ico",
    "url",
    "appref-ms",
    "msc",
    "cpl",
    "scr",
    "dll",
    "msi",
];

fn cache_key(path: &str) -> String {
    let p = std::path::Path::new(path);
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
    {
        Some(ext) if !OWN_ICON.contains(&ext.as_str()) && !p.is_dir() => format!("*.{ext}"),
        _ => path.to_lowercase(),
    }
}

unsafe fn shell_icon(path: &str, size: i32) -> Option<String> {
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK,
        SIIGBF_ICONONLY,
    };
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let factory: IShellItemImageFactory =
        SHCreateItemFromParsingName(windows::core::PCWSTR(wide.as_ptr()), None).ok()?;
    let hbitmap = factory
        .GetImage(
            SIZE { cx: size, cy: size },
            SIIGBF(SIIGBF_ICONONLY.0 | SIIGBF_BIGGERSIZEOK.0),
        )
        .ok()?;
    if hbitmap.0.is_null() {
        return None;
    }
    let result = crate::utils::hbitmap_to_base64(hbitmap);
    let _ = windows::Win32::Graphics::Gdi::DeleteObject(hbitmap.into());
    result
}

/// The icon of a file, folder or URL as a PNG data URI.
#[tauri::command]
pub async fn launcher_file_icon(path: String) -> Option<String> {
    let key = cache_key(&path);
    if let Some(hit) = CACHE.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let icon = tauri::async_runtime::spawn_blocking(move || unsafe {
        use windows::Win32::System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
        };
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let icon = shell_icon(&path, 64);
        if com {
            CoUninitialize();
        }
        icon
    })
    .await
    .ok()
    .flatten();
    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() > 2000 {
            cache.clear();
        }
        cache.insert(key, icon.clone());
    }
    icon
}

/// A large thumbnail for the preview panel (images show their picture).
#[tauri::command]
pub async fn launcher_thumbnail(path: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || unsafe {
        use windows::Win32::Foundation::SIZE;
        use windows::Win32::System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::{
            IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK,
        };
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let result = SHCreateItemFromParsingName::<_, _, IShellItemImageFactory>(
            windows::core::PCWSTR(wide.as_ptr()),
            None,
        )
        .ok()
        .and_then(|factory| {
            factory
                .GetImage(SIZE { cx: 256, cy: 256 }, SIIGBF_BIGGERSIZEOK)
                .ok()
        })
        .and_then(|hbitmap| {
            if hbitmap.0.is_null() {
                return None;
            }
            let data = crate::utils::hbitmap_to_base64(hbitmap);
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(hbitmap.into());
            data
        });
        if com {
            CoUninitialize();
        }
        result
    })
    .await
    .ok()
    .flatten()
}
