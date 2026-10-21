//! Live window previews for the dock (Settings > Dock > Preview Style: Live).
//! Windows' own thumbnails (the ones the taskbar uses) are drawn by the
//! compositor straight onto the dock window, over the preview cards, so they
//! move like the real windows. A minimised window keeps its snapshot; only
//! one with no snapshot yet shows what Windows keeps of it, when Windows has
//! its picture at its size.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmRegisterThumbnail, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties, DWMWA_EXTENDED_FRAME_BOUNDS, DWM_THUMBNAIL_PROPERTIES,
    DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_RECTSOURCE, DWM_TNP_SOURCECLIENTAREAONLY,
    DWM_TNP_VISIBLE,
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
            use windows::Win32::Graphics::Dwm::DwmQueryThumbnailSourceSize;
            use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow};
            if !IsWindow(Some(source)).as_bool() {
                continue;
            }
            if IsIconic(source).as_bool() && crate::window_capture::cached(source).is_some() {
                continue;
            }
            let Ok(thumbnail) = DwmRegisterThumbnail(dock, source) else {
                continue;
            };
            // Only a minimized title bar (or nothing) to draw: the snapshot.
            let drawable = DwmQueryThumbnailSourceSize(thumbnail)
                .is_ok_and(|size| size.cx >= 100 && size.cy >= 100);
            if !drawable {
                let _ = DwmUnregisterThumbnail(thumbnail);
                continue;
            }
            // The part of the window Windows shows (no invisible resize
            // borders), fitted into the card's picture in its own shape.
            let (source_rect, source_width, source_height) = visible_source(source);
            let card = RECT {
                left: item.left,
                top: item.top,
                right: item.right,
                bottom: item.bottom,
            };
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_RECTSOURCE
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: fit(card, source_width, source_height),
                rcSource: source_rect,
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: false.into(),
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

/// The window's visible frame in its own coordinates, and its size.
unsafe fn visible_source(hwnd: HWND) -> (RECT, i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let mut window = RECT::default();
    let _ = GetWindowRect(hwnd, &mut window);
    let mut frame = window;
    let _ = DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut frame as *mut _ as *mut _,
        std::mem::size_of::<RECT>() as u32,
    );
    let (left, top, width, height) = crate::window_capture::visible_part(window, frame);
    (
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        },
        width,
        height,
    )
}

/// A `width` x `height` picture fitted inside `card`, centred, in its shape.
fn fit(card: RECT, width: i32, height: i32) -> RECT {
    let card_width = card.right - card.left;
    let card_height = card.bottom - card.top;
    if width <= 0 || height <= 0 || card_width <= 0 || card_height <= 0 {
        return card;
    }
    let scale = (card_width as f64 / width as f64).min(card_height as f64 / height as f64);
    let fitted_width = (width as f64 * scale).round() as i32;
    let fitted_height = (height as f64 * scale).round() as i32;
    let left = card.left + (card_width - fitted_width) / 2;
    let top = card.top + (card_height - fitted_height) / 2;
    RECT {
        left,
        top,
        right: left + fitted_width,
        bottom: top + fitted_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_keep_their_shape() {
        let card = RECT {
            left: 10,
            top: 20,
            right: 330,
            bottom: 220,
        };
        // Same shape: the whole card.
        let same = fit(card, 1600, 1000);
        assert_eq!(
            (same.left, same.top, same.right, same.bottom),
            (10, 20, 330, 220)
        );
        // A tall window in a wide card: as tall as the card, centred.
        let tall = fit(card, 1280, 1600);
        assert_eq!(
            (tall.left, tall.top, tall.right, tall.bottom),
            (90, 20, 250, 220)
        );
    }
}
