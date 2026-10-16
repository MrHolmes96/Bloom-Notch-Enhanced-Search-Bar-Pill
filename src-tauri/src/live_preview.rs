//! Live window previews for the dock (Settings > Dock > Preview Style: Live).
//! Windows' own thumbnails (the ones the taskbar uses) are drawn by the
//! compositor straight onto the dock window, over the preview cards, so they
//! move like the real windows. A minimised window keeps its snapshot, since
//! Windows doesn't draw it.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};

#[derive(Deserialize)]
pub struct LivePreview {
    hwnd: isize,
    /// Where the card's picture is, in the dock window's client pixels.
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

static THUMBNAILS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

fn clear() {
    if let Ok(mut list) = THUMBNAILS.lock() {
        for thumbnail in list.drain(..) {
            unsafe {
                let _ = DwmUnregisterThumbnail(thumbnail);
            }
        }
    }
}

/// Shows live previews over the given card pictures; an empty list removes
/// them.
#[tauri::command]
pub fn dock_live_previews(app: AppHandle, items: Vec<LivePreview>) {
    clear();
    if items.is_empty() {
        return;
    }
    let Some(dock) = app.get_webview_window("dock").and_then(|w| w.hwnd().ok()) else {
        return;
    };
    let dock = HWND(dock.0);
    let mut added = Vec::new();
    for item in items {
        let source = HWND(item.hwnd as *mut _);
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow};
            if !IsWindow(Some(source)).as_bool() || IsIconic(source).as_bool() {
                continue;
            }
            let Ok(thumbnail) = DwmRegisterThumbnail(dock, source) else {
                continue;
            };
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT {
                    left: item.left,
                    top: item.top,
                    right: item.right,
                    bottom: item.bottom,
                },
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: false.into(),
                ..Default::default()
            };
            if DwmUpdateThumbnailProperties(thumbnail, &properties).is_ok() {
                added.push(thumbnail);
            } else {
                let _ = DwmUnregisterThumbnail(thumbnail);
            }
        }
    }
    if let Ok(mut list) = THUMBNAILS.lock() {
        *list = added;
    }
}
