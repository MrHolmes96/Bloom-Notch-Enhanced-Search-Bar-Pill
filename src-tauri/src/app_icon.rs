//! App icons read straight from a program's own icon resource, at the largest
//! size it ships.
//!
//! The shell's large-icon lookup asks for 256 px. A program whose icon stops
//! at 128 px (RuneLite) or 48 px then comes back as a small picture in a big
//! empty square, or blurry, and window icons are usually only 32 px. Reading
//! the icon group from the file and taking its biggest picture gives the
//! sharpest version the program has, which the dock then draws at the same
//! size as every other icon.
//!
//! The picture is taken exactly as the program stores it: a PNG as it is, a
//! 32-bit bitmap with its own transparency converted pixel for pixel. Going
//! through a Windows icon handle instead can darken the see-through parts of
//! a picture (a glow, soft edges), which made icons like Razer's look dull.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    EnumResourceNamesW, FindResourceW, LoadLibraryExW, LoadResource, LockResource, SizeofResource,
    LOAD_LIBRARY_AS_DATAFILE, LOAD_LIBRARY_AS_IMAGE_RESOURCE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, DestroyIcon, LR_DEFAULTCOLOR, RT_GROUP_ICON, RT_ICON,
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A resource's name: a number or a string.
enum Name {
    Id(u16),
    Text(Vec<u16>),
}

impl Name {
    fn as_pcwstr(&self) -> PCWSTR {
        match self {
            Name::Id(id) => PCWSTR(*id as usize as *const u16),
            Name::Text(text) => PCWSTR(text.as_ptr()),
        }
    }
}

unsafe extern "system" fn collect_name(
    _module: HMODULE,
    _kind: PCWSTR,
    name: PCWSTR,
    param: isize,
) -> windows::core::BOOL {
    let names = &mut *(param as *mut Vec<Name>);
    if (name.0 as usize) >> 16 == 0 {
        names.push(Name::Id(name.0 as usize as u16));
    } else {
        let mut text: Vec<u16> = name.as_wide().to_vec();
        text.push(0);
        names.push(Name::Text(text));
    }
    true.into()
}

/// The icon group the shell means by `index`: the n-th group for 0 and up,
/// the group with id `-index` below 0.
unsafe fn group_name(module: HMODULE, index: i32) -> Option<Name> {
    if index < 0 {
        return Some(Name::Id(index.unsigned_abs() as u16));
    }
    let mut names: Vec<Name> = Vec::new();
    let _ = EnumResourceNamesW(
        Some(module),
        RT_GROUP_ICON,
        Some(collect_name),
        &mut names as *mut Vec<Name> as isize,
    );
    let index = index as usize;
    if index < names.len() {
        Some(names.swap_remove(index))
    } else {
        None
    }
}

unsafe fn resource_bytes(module: HMODULE, name: PCWSTR, kind: PCWSTR) -> Option<&'static [u8]> {
    let resource = FindResourceW(Some(module), name, kind);
    if resource.is_invalid() {
        return None;
    }
    let size = SizeofResource(Some(module), resource) as usize;
    let data = LoadResource(Some(module), resource).ok()?;
    let pointer = LockResource(data) as *const u8;
    if pointer.is_null() || size == 0 {
        return None;
    }
    Some(std::slice::from_raw_parts(pointer, size))
}

/// The biggest picture in an icon group (`GRPICONDIR`): its id and size.
/// Among equal sizes the one with the most colours wins.
fn best_entry(group: &[u8]) -> Option<(u16, u32)> {
    if group.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([group[4], group[5]]) as usize;
    let mut best: Option<(u16, u32, u16)> = None;
    for index in 0..count {
        let at = 6 + index * 14;
        let entry = group.get(at..at + 14)?;
        let width = if entry[0] == 0 { 256 } else { entry[0] as u32 };
        let height = if entry[1] == 0 { 256 } else { entry[1] as u32 };
        let size = width.max(height);
        let bits = u16::from_le_bytes([entry[6], entry[7]]);
        let id = u16::from_le_bytes([entry[12], entry[13]]);
        if best.is_none_or(|(_, best_size, best_bits)| {
            size > best_size || (size == best_size && bits > best_bits)
        }) {
            best = Some((id, size, bits));
        }
    }
    best.map(|(id, size, _)| (id, size))
}

fn png_data_uri(png: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// One icon picture as stored in a program, as a PNG data URI with its colours
/// untouched: a PNG picture as it is, a 32-bit bitmap with transparency
/// converted directly. `None` for older pictures (fewer colours, or a mask
/// for transparency), which Windows converts.
pub fn stored_picture(bits: &[u8]) -> Option<String> {
    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bits.starts_with(PNG_SIGNATURE) {
        return Some(png_data_uri(bits));
    }
    let rgba = bitmap_pixels(bits)?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png_data_uri(&png))
}

/// The pixels of a 32-bit icon bitmap (BITMAPINFOHEADER, then the colour rows
/// from the bottom up, then the mask), top row first. `None` unless it is
/// uncompressed 32-bit with transparency of its own.
fn bitmap_pixels(bits: &[u8]) -> Option<image::RgbaImage> {
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bits.get(at..at + 4)?.try_into().ok()?))
    };
    let header = u32_at(0)? as usize;
    let width = u32_at(4)? as i32;
    let height = u32_at(8)? as i32 / 2;
    let bit_count = u16::from_le_bytes(bits.get(14..16)?.try_into().ok()?);
    let compression = u32_at(16)?;
    if header < 40 || bit_count != 32 || compression != 0 || width <= 0 || height <= 0 {
        return None;
    }
    let (width, height) = (width as usize, height as usize);
    if width > 1024 || height > 1024 {
        return None;
    }
    let row = width * 4;
    let colours = bits.get(header..header + row * height)?;
    let mut rgba = Vec::with_capacity(row * height);
    for y in (0..height).rev() {
        for pixel in colours[y * row..(y + 1) * row].chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    // A picture without any transparency of its own uses the mask instead.
    if rgba.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return None;
    }
    image::RgbaImage::from_raw(width as u32, height as u32, rgba)
}

/// The largest picture of a program's or library's icon as a PNG data URI,
/// when that picture is at least `min_size` px. `index` is the icon's index
/// as shortcuts and the shell give it (0 for a program's own icon).
pub fn resource_icon(file: &str, index: i32, min_size: u32) -> Option<String> {
    let path = wide(file);
    unsafe {
        let module = LoadLibraryExW(
            PCWSTR(path.as_ptr()),
            None,
            LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE,
        )
        .ok()?;
        let result = (|| {
            let group = group_name(module, index)?;
            let entries = resource_bytes(module, group.as_pcwstr(), RT_GROUP_ICON)?;
            let (id, size) = best_entry(entries)?;
            if size < min_size {
                return None;
            }
            let bits = resource_bytes(module, PCWSTR(id as usize as *const u16), RT_ICON)?;
            if let Some(png) = stored_picture(bits) {
                return Some(png);
            }
            let icon = CreateIconFromResourceEx(
                bits,
                true,
                0x0003_0000,
                size as i32,
                size as i32,
                LR_DEFAULTCOLOR,
            )
            .ok()?;
            let png = crate::utils::icon_to_base64(icon);
            let _ = DestroyIcon(icon);
            png
        })();
        let _ = FreeLibrary(module);
        result
    }
}

/// The program or library file and index a shortcut takes its icon from:
/// its own icon setting when that is a program or library, else its target
/// program's icon.
pub fn shortcut_resource(path: &str) -> Option<(String, i32)> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    let is_binary = |file: &str| {
        let lower = file.to_lowercase();
        (lower.ends_with(".exe") || lower.ends_with(".dll")) && std::path::Path::new(file).is_file()
    };
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        let wide_path = wide(path);
        file.Load(PCWSTR(wide_path.as_ptr()), STGM(0)).ok()?;
        let mut buffer = [0u16; 1024];
        let mut index = 0i32;
        if link.GetIconLocation(&mut buffer, &mut index).is_ok() {
            let location = String::from_utf16_lossy(&buffer)
                .trim_matches(char::from(0))
                .trim()
                .to_string();
            if !location.is_empty() {
                let location = crate::utils::expand_env_vars(&location);
                // An icon file or a picture is handled by the caller.
                return is_binary(&location).then_some((location, index));
            }
        }
        let mut target = [0u16; 1024];
        link.GetPath(&mut target, std::ptr::null_mut(), 0).ok()?;
        let target = String::from_utf16_lossy(&target)
            .trim_matches(char::from(0))
            .trim()
            .to_string();
        is_binary(&target).then_some((target, 0))
    }
}

/// Programs whose windows belong to other apps (Store apps, consoles,
/// management snap-ins), so their own icon is not the window's.
pub fn hosts_other_apps(path: &str) -> bool {
    let name = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    matches!(
        name.as_str(),
        "applicationframehost.exe"
            | "mmc.exe"
            | "rundll32.exe"
            | "dllhost.exe"
            | "svchost.exe"
            | "conhost.exe"
            | "openconsole.exe"
            | "wscript.exe"
            | "cscript.exe"
            | "mshta.exe"
            | "python.exe"
            | "pythonw.exe"
            | "java.exe"
            | "javaw.exe"
            | "jp2launcher.exe"
            | "electron.exe"
            | "node.exe"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_pictures_keep_their_colours() {
        // A PNG picture is passed through as it is.
        let png = b"\x89PNG\r\n\x1a\nrest";
        assert!(stored_picture(png)
            .unwrap()
            .starts_with("data:image/png;base64,iVBORw0KGg"));

        // A 2 x 1 32-bit bitmap: a half see-through green and an opaque red,
        // stored blue-green-red-alpha, bottom row first, then the mask.
        let mut bmp = vec![0u8; 40];
        bmp[0] = 40;
        bmp[4] = 2;
        bmp[8] = 2; // twice the height: colours and mask
        bmp[12] = 1;
        bmp[14] = 32;
        bmp.extend_from_slice(&[0x2C, 0xD6, 0x44, 0x80, 0x00, 0x00, 0xFF, 0xFF]);
        bmp.extend_from_slice(&[0; 4]);
        let pixels = bitmap_pixels(&bmp).unwrap();
        assert_eq!(pixels.get_pixel(0, 0).0, [0x44, 0xD6, 0x2C, 0x80]);
        assert_eq!(pixels.get_pixel(1, 0).0, [0xFF, 0x00, 0x00, 0xFF]);
        assert!(stored_picture(&bmp).is_some());

        // No transparency of its own: left to Windows (it uses the mask).
        let mut masked = bmp.clone();
        masked[43] = 0;
        masked[47] = 0;
        assert!(bitmap_pixels(&masked).is_none());

        // Fewer colours: left to Windows.
        let mut paletted = bmp.clone();
        paletted[14] = 8;
        assert!(stored_picture(&paletted).is_none());
    }

    fn group(entries: &[(u8, u16, u16)]) -> Vec<u8> {
        let mut bytes = vec![0, 0, 1, 0, entries.len() as u8, 0];
        for (size, bits, id) in entries {
            bytes.extend_from_slice(&[*size, *size, 0, 0, 1, 0]);
            bytes.extend_from_slice(&bits.to_le_bytes());
            bytes.extend_from_slice(&1000u32.to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn picks_the_biggest_picture() {
        // RuneLite's icon: up to 128 px.
        let runelite = group(&[
            (16, 32, 1),
            (24, 32, 2),
            (32, 32, 3),
            (48, 32, 4),
            (128, 32, 5),
        ]);
        assert_eq!(best_entry(&runelite), Some((5, 128)));
        // 0 means 256; more colours win at the same size.
        let modern = group(&[(48, 8, 1), (48, 32, 2), (0, 32, 3)]);
        assert_eq!(best_entry(&modern), Some((3, 256)));
        let small = group(&[(32, 4, 7), (32, 32, 8)]);
        assert_eq!(best_entry(&small), Some((8, 32)));
        assert_eq!(best_entry(&[0, 0, 1]), None);
    }

    #[test]
    fn knows_host_programs() {
        assert!(hosts_other_apps(
            "C:\\Windows\\System32\\ApplicationFrameHost.exe"
        ));
        assert!(hosts_other_apps("C:\\Windows\\System32\\mmc.exe"));
        assert!(!hosts_other_apps(
            "C:\\Users\\k\\AppData\\Local\\RuneLite\\RuneLite.exe"
        ));
    }
}
