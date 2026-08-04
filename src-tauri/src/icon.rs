use std::{
    ffi::c_void,
    fs,
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use windows::{
    Win32::{
        Foundation::MAX_PATH,
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, HGDIOBJ, SelectObject,
        },
        Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, IPersistFile, STGM_READ,
        },
        UI::{
            Shell::{
                IShellLinkW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW,
                SLGP_UNCPRIORITY, ShellLink,
            },
            WindowsAndMessaging::{
                DI_NORMAL, DestroyIcon, DrawIconEx, HICON, PrivateExtractIconsW,
            },
        },
    },
    core::{Interface, PCWSTR},
};

use crate::{
    model::{ApplicationIcon, ApplicationIconRequest},
    steam,
};

const EMBEDDED_ICON_SIZE: i32 = 64;
const SHELL_ICON_SIZE: i32 = 32;

pub fn load(request: &ApplicationIconRequest) -> Option<ApplicationIcon> {
    for path in [&request.shortcut_path, &request.executable] {
        let path = path.trim();
        if !path.is_empty() {
            for (icon_path, icon_index) in icon_candidates(Path::new(path)) {
                if let Some(icon) =
                    embedded_icon(&icon_path, icon_index).or_else(|| shell_icon(&icon_path))
                {
                    return Some(icon);
                }
            }
        }
    }

    let steam_app_id = request.steam_app_id.trim();
    if let Some(executable) = steam::executable_path(steam_app_id, request.process_name.trim())
        && let Some(icon) = embedded_icon(&executable, 0).or_else(|| shell_icon(&executable))
    {
        return Some(icon);
    }

    steam::cached_icon(steam_app_id).map(|(mime_type, data)| ApplicationIcon {
        kind: "encoded".to_string(),
        mime_type,
        width: 0,
        height: 0,
        data,
    })
}

fn icon_candidates(path: &Path) -> Vec<(PathBuf, i32)> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("url") => {
            let resolved = resolve_url_icon(path);
            (resolved != path)
                .then_some((resolved, 0))
                .into_iter()
                .collect()
        }
        Some(extension) if extension.eq_ignore_ascii_case("lnk") => {
            resolve_lnk_icon_candidates(path)
        }
        _ => vec![(path.to_path_buf(), 0)],
    }
}

fn resolve_url_icon(path: &Path) -> PathBuf {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("url"))
    {
        return path.to_path_buf();
    }

    fs::read_to_string(path)
        .ok()
        .and_then(|content| {
            content.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("IconFile=")
                    .map(|value| PathBuf::from(value.trim().trim_matches('"')))
            })
        })
        .filter(|icon| icon.is_file())
        .unwrap_or_else(|| path.to_path_buf())
}

fn resolve_lnk_icon_candidates(path: &Path) -> Vec<(PathBuf, i32)> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = resolve_lnk_icon_candidates_inner(path).unwrap_or_default();
        if initialized {
            CoUninitialize();
        }
        result
    }
}

fn resolve_lnk_icon_candidates_inner(path: &Path) -> Option<Vec<(PathBuf, i32)>> {
    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let persist_file: IPersistFile = shell_link.cast().ok()?;
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe { persist_file.Load(PCWSTR(wide_path.as_ptr()), STGM_READ) }.ok()?;

    let mut candidates = Vec::new();
    let mut icon_path = vec![0u16; MAX_PATH as usize];
    let mut icon_index = 0;
    if unsafe { shell_link.GetIconLocation(&mut icon_path, &mut icon_index) }.is_ok()
        && let Some(icon_path) = string_from_wide_buffer(&icon_path)
            .map(PathBuf::from)
            .filter(|icon_path| icon_path.is_file())
    {
        candidates.push((icon_path, icon_index));
    }

    let mut target = vec![0u16; MAX_PATH as usize];
    if unsafe { shell_link.GetPath(&mut target, std::ptr::null_mut(), SLGP_UNCPRIORITY.0 as u32) }
        .is_ok()
        && let Some(target) = string_from_wide_buffer(&target)
            .map(PathBuf::from)
            .filter(|target| target.is_file())
    {
        candidates.push((target, 0));
    }

    Some(candidates)
}

fn string_from_wide_buffer(buffer: &[u16]) -> Option<String> {
    let length = buffer.iter().position(|character| *character == 0)?;
    if length == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(buffer.get(..length)?))
}

fn shell_icon(path: &Path) -> Option<ApplicationIcon> {
    if !path.is_file() {
        return None;
    }

    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut file_info = SHFILEINFOW::default();
    let loaded = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide_path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut file_info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    if loaded == 0 || file_info.hIcon.is_invalid() {
        if !file_info.hIcon.is_invalid() {
            unsafe {
                let _ = DestroyIcon(file_info.hIcon);
            }
        }
        return None;
    }

    let pixels = render_icon(file_info.hIcon, SHELL_ICON_SIZE);
    unsafe {
        let _ = DestroyIcon(file_info.hIcon);
    }
    pixels.map(|data| ApplicationIcon {
        kind: "rgba".to_string(),
        mime_type: String::new(),
        width: SHELL_ICON_SIZE as u32,
        height: SHELL_ICON_SIZE as u32,
        data,
    })
}

fn embedded_icon(path: &Path, icon_index: i32) -> Option<ApplicationIcon> {
    if !path.is_file() {
        return None;
    }

    let wide_path = fixed_wide_path(path)?;
    let mut icons = [HICON::default()];
    let extracted = unsafe {
        PrivateExtractIconsW(
            &wide_path,
            icon_index,
            EMBEDDED_ICON_SIZE,
            EMBEDDED_ICON_SIZE,
            Some(&mut icons),
            None,
            0,
        )
    };
    if extracted == 0 || extracted == u32::MAX || icons[0].is_invalid() {
        if !icons[0].is_invalid() {
            unsafe {
                let _ = DestroyIcon(icons[0]);
            }
        }
        return None;
    }

    let pixels = render_icon(icons[0], EMBEDDED_ICON_SIZE);
    unsafe {
        let _ = DestroyIcon(icons[0]);
    }
    pixels.map(|data| ApplicationIcon {
        kind: "rgba".to_string(),
        mime_type: String::new(),
        width: EMBEDDED_ICON_SIZE as u32,
        height: EMBEDDED_ICON_SIZE as u32,
        data,
    })
}

fn fixed_wide_path(path: &Path) -> Option<[u16; MAX_PATH as usize]> {
    let encoded: Vec<u16> = path.as_os_str().encode_wide().collect();
    if encoded.len() >= MAX_PATH as usize {
        return None;
    }
    let mut wide_path = [0; MAX_PATH as usize];
    wide_path[..encoded.len()].copy_from_slice(&encoded);
    Some(wide_path)
}

fn render_icon(icon: HICON, size: i32) -> Option<Vec<u8>> {
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }

        let mut bitmap_info = BITMAPINFO::default();
        bitmap_info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap =
            match CreateDIBSection(Some(dc), &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(bitmap) => bitmap,
                Err(_) => {
                    let _ = DeleteDC(dc);
                    return None;
                }
            };

        let previous = SelectObject(dc, HGDIOBJ(bitmap.0));
        let drawn = DrawIconEx(dc, 0, 0, icon, size, size, 0, None, DI_NORMAL).is_ok();
        let data = if drawn && !bits.is_null() {
            let length = (size * size * 4) as usize;
            Some(bgra_to_rgba(std::slice::from_raw_parts(
                bits.cast::<u8>(),
                length,
            )))
        } else {
            None
        };

        let _ = SelectObject(dc, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(dc);
        data
    }
}

fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let has_alpha = bgra.chunks_exact(4).any(|pixel| pixel[3] != 0);
    let mut rgba = Vec::with_capacity(bgra.len());
    for pixel in bgra.chunks_exact(4) {
        let alpha = if has_alpha {
            pixel[3]
        } else if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
            u8::MAX
        } else {
            0
        };
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], alpha]);
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::{EMBEDDED_ICON_SIZE, bgra_to_rgba, icon_candidates, load, string_from_wide_buffer};
    use crate::model::ApplicationIconRequest;

    #[test]
    fn converts_bgra_pixels_to_rgba() {
        assert_eq!(bgra_to_rgba(&[10, 20, 30, 255]), [30, 20, 10, 255]);
    }

    #[test]
    fn reads_a_null_terminated_wide_string() {
        assert_eq!(
            string_from_wide_buffer(&[
                't' as u16, 'e' as u16, 's' as u16, 't' as u16, 0, 'x' as u16
            ])
            .as_deref(),
            Some("test")
        );
        assert_eq!(string_from_wide_buffer(&[0, 'x' as u16]), None);
    }

    #[test]
    fn does_not_fall_back_to_a_shortcut_overlay() {
        assert!(icon_candidates(std::path::Path::new("C:\\missing-shortcut.lnk")).is_empty());
    }

    #[test]
    fn extracts_a_high_resolution_executable_icon_after_a_missing_shortcut() {
        let executable = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap())
            .join("System32")
            .join("notepad.exe");
        let icon = load(&ApplicationIconRequest {
            executable: executable.to_string_lossy().into_owned(),
            shortcut_path: "C:\\missing-shortcut.lnk".to_string(),
            steam_app_id: String::new(),
            process_name: String::new(),
        })
        .expect("the executable should provide a fallback icon");
        assert_eq!(icon.width, EMBEDDED_ICON_SIZE as u32);
        assert_eq!(icon.height, EMBEDDED_ICON_SIZE as u32);
        assert_eq!(
            icon.data.len(),
            (EMBEDDED_ICON_SIZE * EMBEDDED_ICON_SIZE * 4) as usize
        );
        assert!(icon.data.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }
}
