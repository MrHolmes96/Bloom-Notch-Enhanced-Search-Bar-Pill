//! G-Helper's own on-screen pop-up, for the few of its key actions Bloom
//! has to carry out itself (G-Helper can't be asked to): the same window,
//! drawn the same way, in the same place, for the same time.
//!
//! Ported from G-Helper's ToastForm.cs and OSDBase.cs (GPL-3.0, like Bloom):
//! a layered, click-through, never-focused window of 100 px high and at
//! least 300 px wide, a black rounded rectangle (radius 10, alpha 150) with
//! the icon at 18,18 (64 px) and the text in Segoe UI Bold 36 px, white,
//! centred (40 px to the right with an icon), centred across the primary
//! screen and 300 px above its bottom edge, for 2 seconds. Sizes are in the
//! screen's own pixels, as G-Helper draws them. The icons are G-Helper's
//! (icons8, as credited by G-Helper). G-Helper's "disable_osd" turns them off
//! here too.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BLENDFUNCTION, HBITMAP,
};
use windows::Win32::Graphics::GdiPlus::{
    FillModeAlternate, FontStyleBold, GdipAddPathArc, GdipClosePathFigure,
    GdipCreateBitmapFromScan0, GdipCreateBitmapFromStream, GdipCreateFont,
    GdipCreateFontFamilyFromName, GdipCreateHBITMAPFromBitmap, GdipCreatePath, GdipCreateSolidFill,
    GdipCreateStringFormat, GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily,
    GdipDeleteGraphics, GdipDeletePath, GdipDeleteStringFormat, GdipDisposeImage,
    GdipDrawImageRectI, GdipDrawString, GdipFillPath, GdipGetImageGraphicsContext,
    GdipSetSmoothingMode, GdipSetStringFormatAlign, GdipSetStringFormatLineAlign, GdiplusStartup,
    GdiplusStartupInput, GpBitmap, GpBrush, GpFont, GpFontFamily, GpGraphics, GpImage, GpPath,
    GpSolidFill, GpStringFormat, RectF, SmoothingModeAntiAlias, Status, StringAlignmentCenter,
    UnitPixel,
};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy)]
pub enum Icon {
    Touchpad,
    BrightnessUp,
    BrightnessDown,
}

impl Icon {
    fn png(self) -> &'static [u8] {
        match self {
            Icon::Touchpad => include_bytes!("../assets/ghelper/icons8-touchpad-96.png"),
            Icon::BrightnessUp => include_bytes!("../assets/ghelper/brightness-up.png"),
            Icon::BrightnessDown => include_bytes!("../assets/ghelper/brightness-down.png"),
        }
    }
}

type Request = (String, Option<Icon>);

static SENDER: OnceLock<Mutex<Sender<Request>>> = OnceLock::new();
static THREAD: AtomicU32 = AtomicU32::new(0);
const WM_SHOW_TOAST: u32 = WM_APP + 0x47;
const TIMER: usize = 1;
/// GDI+'s PixelFormat32bppARGB (.NET's Format32bppArgb).
const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026_200A;

/// Shows G-Helper's pop-up with `text`, replacing one that is showing.
pub fn show(text: &str, icon: Option<Icon>) {
    let sender = SENDER.get_or_init(|| {
        let (sender, receiver) = channel::<Request>();
        let _ = std::thread::Builder::new()
            .name("bloom-ghelper-toast".into())
            .spawn(move || unsafe { run(receiver) });
        Mutex::new(sender)
    });
    if let Ok(sender) = sender.lock() {
        let _ = sender.send((text.to_string(), icon));
    }
    // The thread may still be starting: it shows what is waiting when ready.
    let thread = THREAD.load(Ordering::SeqCst);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_SHOW_TOAST, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn run(requests: Receiver<Request>) {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    // Screen pixels, as G-Helper sizes its pop-up.
    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

    let mut token = 0usize;
    let input = GdiplusStartupInput {
        GdiplusVersion: 1,
        ..Default::default()
    };
    if GdiplusStartup(&mut token, &input, std::ptr::null_mut()) != Status(0) {
        return;
    }

    let module = GetModuleHandleW(None).unwrap_or_default();
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: module.into(),
        lpszClassName: w!("BloomGHelperToast"),
        ..Default::default()
    };
    RegisterClassExW(&class);

    let mut message = MSG::default();
    // The thread's message queue exists from here on.
    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
    THREAD.store(
        windows::Win32::System::Threading::GetCurrentThreadId(),
        Ordering::SeqCst,
    );

    let mut window: Option<HWND> = None;
    let mut pending = true;
    loop {
        if pending {
            pending = false;
            // Only the newest one shows, as when G-Helper replaces its own.
            if let Some((text, icon)) = requests.try_iter().last() {
                window = Some(present(window, module.into(), &text, icon));
            }
        }
        if !GetMessageW(&mut message, None, 0, 0).as_bool() {
            break;
        }
        if message.message == WM_SHOW_TOAST {
            pending = true;
            continue;
        }
        if message.message == WM_TIMER && message.wParam.0 == TIMER {
            if let Some(hwnd) = window.take() {
                let _ = KillTimer(Some(hwnd), TIMER);
                let _ = ShowWindow(hwnd, SW_HIDE);
                let _ = DestroyWindow(hwnd);
            }
            continue;
        }
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}

/// Places, draws and shows the pop-up (making its window if needed).
unsafe fn present(
    window: Option<HWND>,
    module: windows::Win32::Foundation::HINSTANCE,
    text: &str,
    icon: Option<Icon>,
) -> HWND {
    let width = 300.max(100 + text.chars().count() as i32 * 22);
    let height = 100;
    let screen_width = GetSystemMetrics(SM_CXSCREEN);
    let screen_height = GetSystemMetrics(SM_CYSCREEN);
    let x = (screen_width - width) / 2;
    let y = screen_height - 300 - height;

    let hwnd = match window {
        Some(hwnd) => hwnd,
        None => CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            w!("BloomGHelperToast"),
            w!("FloatingNativeWindow"),
            WS_POPUP,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(module),
            None,
        )
        .unwrap_or_default(),
    };
    if hwnd.is_invalid() {
        return hwnd;
    }
    paint(hwnd, x, y, width, height, text, icon);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_TOPMOST),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
    // Two seconds from now, again for a replaced one.
    let _ = SetTimer(Some(hwnd), TIMER, 2000, None);
    hwnd
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Draws the pop-up as G-Helper's PerformPaint does, and hands it to the
/// window as its picture (UpdateLayeredWindow, window alpha 250).
unsafe fn paint(
    hwnd: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    text: &str,
    icon: Option<Icon>,
) {
    let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
    if GdipCreateBitmapFromScan0(width, height, 0, PIXEL_FORMAT_32BPP_ARGB, None, &mut bitmap)
        != Status(0)
    {
        return;
    }
    let mut graphics: *mut GpGraphics = std::ptr::null_mut();
    GdipGetImageGraphicsContext(bitmap as *mut GpImage, &mut graphics);
    GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);

    // The rounded rectangle: radius 10, black at alpha 150.
    let mut path: *mut GpPath = std::ptr::null_mut();
    GdipCreatePath(FillModeAlternate, &mut path);
    let (w, h, d) = (width as f32, height as f32, 20.0f32);
    GdipAddPathArc(path, 0.0, 0.0, d, d, 180.0, 90.0);
    GdipAddPathArc(path, w - d, 0.0, d, d, 270.0, 90.0);
    GdipAddPathArc(path, w - d, h - d, d, d, 0.0, 90.0);
    GdipAddPathArc(path, 0.0, h - d, d, d, 90.0, 90.0);
    GdipClosePathFigure(path);
    let mut back: *mut GpSolidFill = std::ptr::null_mut();
    GdipCreateSolidFill(0x9600_0000, &mut back);
    GdipFillPath(graphics, back as *mut GpBrush, path);

    let mut shift = 0.0f32;
    if let Some(icon) = icon {
        if let Some(image) = png_image(icon.png()) {
            GdipDrawImageRectI(graphics, image as *mut GpImage, 18, 18, 64, 64);
            GdipDisposeImage(image as *mut GpImage);
            shift = 40.0;
        }
    }

    let mut family: *mut GpFontFamily = std::ptr::null_mut();
    GdipCreateFontFamilyFromName(w!("Segoe UI"), std::ptr::null_mut(), &mut family);
    let mut font: *mut GpFont = std::ptr::null_mut();
    GdipCreateFont(family, 36.0, FontStyleBold.0, UnitPixel, &mut font);
    let mut format: *mut GpStringFormat = std::ptr::null_mut();
    GdipCreateStringFormat(0, 0, &mut format);
    GdipSetStringFormatAlign(format, StringAlignmentCenter);
    GdipSetStringFormatLineAlign(format, StringAlignmentCenter);
    let mut white: *mut GpSolidFill = std::ptr::null_mut();
    GdipCreateSolidFill(0xFFFF_FFFF, &mut white);
    // A point, as .NET's DrawString(PointF) passes it: an empty layout box.
    let layout = RectF {
        X: w / 2.0 + shift,
        Y: h / 2.0,
        Width: 0.0,
        Height: 0.0,
    };
    let text = wide(text);
    GdipDrawString(
        graphics,
        PCWSTR(text.as_ptr()),
        -1,
        font,
        &layout,
        format,
        white as *mut GpBrush,
    );

    let mut hbitmap = HBITMAP::default();
    GdipCreateHBITMAPFromBitmap(bitmap, &mut hbitmap, 0);

    GdipDeleteBrush(white as *mut GpBrush);
    GdipDeleteStringFormat(format);
    GdipDeleteFont(font);
    GdipDeleteFontFamily(family);
    GdipDeleteBrush(back as *mut GpBrush);
    GdipDeletePath(path);
    GdipDeleteGraphics(graphics);
    GdipDisposeImage(bitmap as *mut GpImage);

    if hbitmap.is_invalid() {
        return;
    }
    let screen = GetDC(None);
    let memory = CreateCompatibleDC(Some(screen));
    let old = SelectObject(memory, hbitmap.into());
    let position = POINT { x, y };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let source = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 250,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let _ = UpdateLayeredWindow(
        hwnd,
        Some(screen),
        Some(&position),
        Some(&size),
        Some(memory),
        Some(&source),
        windows::Win32::Foundation::COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
    SelectObject(memory, old);
    let _ = DeleteDC(memory);
    ReleaseDC(None, screen);
    let _ = DeleteObject(hbitmap.into());
}

/// A PNG (G-Helper's icon) as a GDI+ bitmap.
unsafe fn png_image(bytes: &[u8]) -> Option<*mut GpBitmap> {
    let stream = windows::Win32::UI::Shell::SHCreateMemStream(Some(bytes))?;
    let mut image: *mut GpBitmap = std::ptr::null_mut();
    (GdipCreateBitmapFromStream(&stream, &mut image) == Status(0) && !image.is_null())
        .then_some(image)
}
