//! Pictures of windows for the dock's static previews.
//!
//! A window's picture is taken at the moments its look matters, never on a
//! timer and never while it is being used: a moment after it opens, when it
//! stops being the window in front (it is still on screen then, so its
//! preview is how it was left), when its title changes while it is in front
//! (a browser's tab), when the dock is pointed at, and right before the dock
//! minimizes it. Each is skipped when the window's picture is only seconds
//! old, and full-screen windows (games, video) are never taken in the
//! background.
//!
//! It is taken with PrintWindow, which costs nothing while it isn't used.
//! Only for a window that gives no real picture that way (some apps drawn by
//! the graphics card), and only when its preview is wanted,
//! Windows.Graphics.Capture is tried, the way Windows' capture tools take
//! windows.
//!
//! A picture that is see-through or all black (an app that hasn't drawn yet)
//! is never kept as a window's snapshot.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::Interface;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

/// One picture of a window, ready for a preview card.
pub struct Snapshot {
    /// A PNG data URI.
    pub image: String,
    /// It shows something (not see-through, not all black), so it may be
    /// kept as the window's snapshot.
    pub usable: bool,
}

/// Width, height and RGBA pixels.
type Pixels = (u32, u32, Vec<u8>);

/// The window's picture, scaled down to fit `max_width` x `max_height`.
/// `thorough` also tries Windows.Graphics.Capture when PrintWindow gives no
/// real picture (only when a preview is wanted, never in the background).
pub fn snapshot(hwnd: HWND, max_width: u32, max_height: u32, thorough: bool) -> Option<Snapshot> {
    let printed = print_window(hwnd);
    let pixels = match printed {
        Some(pixels) if looks_real(&pixels.2) => pixels,
        printed => match thorough.then(|| capture(hwnd)).flatten() {
            Some(captured) if looks_real(&captured.2) => captured,
            _ => printed?,
        },
    };
    let usable = looks_real(&pixels.2);
    let image = encode(pixels.0, pixels.1, pixels.2, max_width, max_height)?;
    Some(Snapshot { image, usable })
}

// ---------------------------------------------------------------- each window's snapshot

/// The picture kept for a window (for when it is minimized), and when.
pub fn cached(hwnd: HWND) -> Option<(String, i64)> {
    crate::state::THUMBNAIL_CACHE
        .get()?
        .lock()
        .ok()?
        .get(&(hwnd.0 as isize))
        .cloned()
}

/// How long ago the window's picture was taken (`i64::MAX` without one).
pub fn age_ms(hwnd: HWND) -> i64 {
    cached(hwnd).map_or(i64::MAX, |(_, at)| crate::utils::get_now_ms() - at)
}

fn keep(hwnd: HWND, image: String) {
    let Some(cache) = crate::state::THUMBNAIL_CACHE.get() else {
        return;
    };
    if let Ok(mut cache) = cache.lock() {
        cache.insert(hwnd.0 as isize, (image, crate::utils::get_now_ms()));
        // Windows that are gone; minimized ones are still windows and stay.
        if cache.len() > 15 {
            use windows::Win32::UI::WindowsAndMessaging::IsWindow;
            cache.retain(|&k, _| unsafe { IsWindow(Some(HWND(k as *mut _))).as_bool() });
        }
    }
}

/// Takes the window's picture for a preview card and keeps it as its
/// snapshot when it shows something.
pub fn take(hwnd: HWND, max_width: u32, max_height: u32, thorough: bool) -> Option<Snapshot> {
    // A minimized window can't be taken: its snapshot stays as it was.
    if unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd) }.as_bool() {
        return None;
    }
    let shot = snapshot(hwnd, max_width, max_height, thorough)?;
    if shot.usable {
        keep(hwnd, shot.image.clone());
    }
    Some(shot)
}

// ---------------------------------------------------------------- Windows.Graphics.Capture

struct Devices {
    d3d: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
}

// The device is free-threaded; its immediate context is only used under the
// mutex below.
unsafe impl Send for Devices {}

static DEVICES: Mutex<Option<Devices>> = Mutex::new(None);

fn create_devices() -> Option<Devices> {
    unsafe {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .ok()?;
        let (d3d, context) = (device?, context?);
        let dxgi: IDXGIDevice = d3d.cast().ok()?;
        let winrt: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)
            .ok()?
            .cast()
            .ok()?;
        Some(Devices {
            d3d,
            context,
            winrt,
        })
    }
}

/// Closes the capture when it goes, however the capture ends.
struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// Windows lets Bloom take windows without the yellow frame it otherwise
/// draws around a window being captured (Windows 11; asked once). Without
/// that, windows are taken with PrintWindow instead, so no frame ever shows.
fn borderless_allowed() -> bool {
    static ALLOWED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ALLOWED.get_or_init(|| {
        use windows::Graphics::Capture::{GraphicsCaptureAccess, GraphicsCaptureAccessKind};
        use windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus;
        GraphicsCaptureSession::IsSupported().unwrap_or(false)
            && GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
                .and_then(|request| request.get())
                .is_ok_and(|status| status == AppCapabilityAccessStatus::Allowed)
    })
}

fn capture(hwnd: HWND) -> Option<Pixels> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if !borderless_allowed() {
        return None;
    }
    let mut devices = DEVICES.lock().ok()?;
    if devices.is_none() {
        *devices = create_devices();
    }
    let result = capture_with(devices.as_ref()?, hwnd);
    if matches!(result, Err(Failure::Device)) {
        // The graphics device went away (a driver update, say): made again
        // next time.
        *devices = None;
    }
    result.ok()
}

enum Failure {
    /// Nothing to take (the window can't be captured, no picture came).
    Window,
    /// The graphics device failed.
    Device,
}

fn capture_with(devices: &Devices, hwnd: HWND) -> Result<Pixels, Failure> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|_| Failure::Window)?;
    let item: GraphicsCaptureItem =
        unsafe { interop.CreateForWindow(hwnd) }.map_err(|_| Failure::Window)?;
    let size = item.Size().map_err(|_| Failure::Window)?;
    if size.Width < 16 || size.Height < 16 || size.Width > 7680 || size.Height > 4320 {
        return Err(Failure::Window);
    }
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &devices.winrt,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        size,
    )
    .map_err(|_| Failure::Device)?;
    let session = pool
        .CreateCaptureSession(&item)
        .map_err(|_| Failure::Window)?;
    let capture = Capture { pool, session };
    let _ = capture.session.SetIsCursorCaptureEnabled(false);
    // Without this (older Windows) Windows draws a yellow frame around the
    // window while it is taken: PrintWindow is used instead.
    capture
        .session
        .SetIsBorderRequired(false)
        .map_err(|_| Failure::Window)?;
    capture
        .session
        .StartCapture()
        .map_err(|_| Failure::Window)?;

    let started = Instant::now();
    let frame = loop {
        if let Ok(frame) = capture.pool.TryGetNextFrame() {
            break frame;
        }
        if started.elapsed() > Duration::from_millis(250) {
            return Err(Failure::Window);
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let pixels = read_frame(devices, &frame);
    let _ = frame.Close();
    pixels
}

fn read_frame(
    devices: &Devices,
    frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
) -> Result<Pixels, Failure> {
    unsafe {
        let surface = frame.Surface().map_err(|_| Failure::Window)?;
        let access: IDirect3DDxgiInterfaceAccess = surface.cast().map_err(|_| Failure::Window)?;
        let texture: ID3D11Texture2D = access.GetInterface().map_err(|_| Failure::Window)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);
        let content = frame.ContentSize().map_err(|_| Failure::Window)?;
        let width = (content.Width.max(0) as u32).min(desc.Width);
        let height = (content.Height.max(0) as u32).min(desc.Height);
        if width < 16 || height < 16 {
            return Err(Failure::Window);
        }
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging: Option<ID3D11Texture2D> = None;
        devices
            .d3d
            .CreateTexture2D(&desc, None, Some(&mut staging))
            .map_err(|_| Failure::Device)?;
        let staging = staging.ok_or(Failure::Device)?;
        devices.context.CopyResource(&staging, &texture);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        devices
            .context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|_| Failure::Device)?;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let row = (width * 4) as usize;
        for y in 0..height as usize {
            let source = std::slice::from_raw_parts(
                (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                row,
            );
            let target = &mut pixels[y * row..(y + 1) * row];
            for (to, from) in target.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
                // BGRA (alpha premultiplied) to straight RGBA.
                let alpha = from[3];
                let unpremultiply = |c: u8| {
                    if alpha == 0 || alpha == 255 {
                        c
                    } else {
                        ((c as u32 * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8
                    }
                };
                to[0] = unpremultiply(from[2]);
                to[1] = unpremultiply(from[1]);
                to[2] = unpremultiply(from[0]);
                to[3] = alpha;
            }
        }
        devices.context.Unmap(&staging, 0);
        Ok((width, height, pixels))
    }
}

// ---------------------------------------------------------------- PrintWindow

fn print_window(hwnd: HWND) -> Option<Pixels> {
    unsafe {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
            ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindow};
        #[link(name = "user32")]
        extern "system" {
            fn PrintWindow(hwnd: HWND, hdc: HDC, flags: u32) -> i32;
        }
        /// Includes content drawn by the graphics card.
        const PW_RENDERFULLCONTENT: u32 = 2;

        if !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
            && GetWindowRect(hwnd, &mut rect).is_err()
        {
            return None;
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 100 || height <= 100 || width > 7680 || height > 4320 {
            return None;
        }
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(memory, bitmap.into());
        let mut result = None;
        if PrintWindow(hwnd, memory, PW_RENDERFULLCONTENT) != 0 {
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            if GetDIBits(
                memory,
                bitmap,
                0,
                height as u32,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut info,
                DIB_RGB_COLORS,
            ) != 0
            {
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                    // GDI leaves alpha undefined.
                    pixel[3] = 255;
                }
                result = Some((width as u32, height as u32, pixels));
            }
        }
        SelectObject(memory, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        result
    }
}

// ---------------------------------------------------------------- the picture

/// Whether a picture shows something: not (almost) entirely see-through, and
/// not all black.
pub fn looks_real(rgba: &[u8]) -> bool {
    let count = rgba.len() / 4;
    if count == 0 {
        return false;
    }
    let step = (count / 4096).max(1);
    let (mut seen, mut visible, mut brightest) = (0usize, 0usize, 0u32);
    for index in (0..count).step_by(step) {
        let pixel = &rgba[index * 4..index * 4 + 4];
        seen += 1;
        if pixel[3] < 16 {
            continue;
        }
        visible += 1;
        let light = (pixel[0] as u32 * 3 + pixel[1] as u32 * 6 + pixel[2] as u32) / 10;
        brightest = brightest.max(light);
    }
    visible * 20 >= seen && brightest > 12
}

fn encode(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    max_width: u32,
    max_height: u32,
) -> Option<String> {
    use base64::Engine;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut image = image::RgbaImage::from_raw(width, height, pixels)?;
        if image.width() > max_width || image.height() > max_height {
            image = image::DynamicImage::ImageRgba8(image)
                .resize(max_width, max_height, image::imageops::FilterType::Triangle)
                .into_rgba8();
        }
        let mut buffer = std::io::Cursor::new(Vec::new());
        image::write_buffer_with_format(
            &mut buffer,
            &image,
            image.width(),
            image.height(),
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .ok()?;
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buffer.into_inner())
        ))
    }))
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::looks_real;

    #[test]
    fn see_through_and_black_pictures_are_not_kept() {
        let clear = vec![0u8; 64 * 64 * 4];
        assert!(!looks_real(&clear));
        let black: Vec<u8> = (0..64 * 64).flat_map(|_| [0, 0, 0, 255]).collect();
        assert!(!looks_real(&black));
        let white: Vec<u8> = (0..64 * 64).flat_map(|_| [255, 255, 255, 255]).collect();
        assert!(looks_real(&white));
        // A dark page with a little text is a real picture.
        let mut page = black.clone();
        for index in (0..64 * 64).step_by(7) {
            page[index * 4..index * 4 + 3].copy_from_slice(&[200, 200, 200]);
        }
        assert!(looks_real(&page));
        // Mostly see-through with a sliver drawn: an app that hasn't drawn yet.
        let mut sliver = clear.clone();
        for index in 0..64 {
            sliver[index * 4..index * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
        assert!(!looks_real(&sliver));
    }
}
