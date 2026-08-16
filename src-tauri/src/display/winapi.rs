use std::{
    collections::{HashMap, HashSet},
    mem::size_of,
};

use windows::{
    Win32::{
        Devices::Display::{
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
            QueryDisplayConfig,
        },
        Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
        Graphics::Gdi::{
            CDS_RESET, CDS_TEST, CDS_TYPE, CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW,
            DISP_CHANGE_SUCCESSFUL, DISPLAY_DEVICE_ACTIVE, DISPLAY_DEVICE_PRIMARY_DEVICE,
            DISPLAY_DEVICEW, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH,
            ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, ENUM_REGISTRY_SETTINGS,
            EnumDisplayDevicesW, EnumDisplaySettingsW,
        },
    },
    core::PCWSTR,
};

use crate::model::{DisplayInfo, DisplayMode};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn utf16_array(value: &[u16]) -> String {
    let end = value.iter().position(|&c| c == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

fn read_mode(device_name: &str, index: ENUM_DISPLAY_SETTINGS_MODE) -> Option<DEVMODEW> {
    let device = wide(device_name);
    let mut mode = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };

    let ok = unsafe { EnumDisplaySettingsW(PCWSTR(device.as_ptr()), index, &mut mode) };
    ok.as_bool().then_some(mode)
}

fn to_mode(device_name: &str, mode: &DEVMODEW) -> DisplayMode {
    DisplayMode {
        device_name: device_name.to_string(),
        width: mode.dmPelsWidth,
        height: mode.dmPelsHeight,
        refresh_rate: mode.dmDisplayFrequency,
        bits_per_pixel: mode.dmBitsPerPel,
    }
}

fn monitor_name(adapter_name: &str) -> Option<String> {
    let adapter = wide(adapter_name);
    let mut monitor = DISPLAY_DEVICEW {
        cb: size_of::<DISPLAY_DEVICEW>() as u32,
        ..Default::default()
    };
    let ok = unsafe { EnumDisplayDevicesW(PCWSTR(adapter.as_ptr()), 0, &mut monitor, 0) };
    if !ok.as_bool() {
        return None;
    }

    let name = utf16_array(&monitor.DeviceString);
    (!name.trim().is_empty()).then_some(name)
}

fn physical_monitor_names() -> HashMap<String, String> {
    for _ in 0..3 {
        let mut path_count = 0;
        let mut mode_count = 0;
        let size_result = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };
        if size_result != ERROR_SUCCESS {
            return HashMap::new();
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        let query_result = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if query_result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if query_result != ERROR_SUCCESS {
            return HashMap::new();
        }

        paths.truncate(path_count as usize);
        let mut names = HashMap::new();
        for path in paths {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };
            let source_result = unsafe { DisplayConfigGetDeviceInfo(&mut source.header as *mut _) };
            if source_result != 0 {
                continue;
            }

            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                },
                ..Default::default()
            };
            let target_result = unsafe { DisplayConfigGetDeviceInfo(&mut target.header as *mut _) };
            if target_result != 0 {
                continue;
            }

            let source_name = utf16_array(&source.viewGdiDeviceName);
            let friendly_name = utf16_array(&target.monitorFriendlyDeviceName);
            if !source_name.trim().is_empty() && !friendly_name.trim().is_empty() {
                names.insert(source_name.to_ascii_lowercase(), friendly_name);
            }
        }
        return names;
    }

    HashMap::new()
}

pub fn current_mode(device_name: &str) -> Result<DisplayMode, String> {
    read_mode(device_name, ENUM_CURRENT_SETTINGS)
        .map(|mode| to_mode(device_name, &mode))
        .ok_or_else(|| format!("无法读取显示器 {device_name} 的当前模式"))
}

pub fn registry_mode(device_name: &str) -> Result<DisplayMode, String> {
    read_mode(device_name, ENUM_REGISTRY_SETTINGS)
        .map(|mode| to_mode(device_name, &mode))
        .ok_or_else(|| format!("无法读取显示器 {device_name} 的持久显示模式"))
}

pub fn mode_matches(actual: &DisplayMode, expected: &DisplayMode) -> bool {
    actual.device_name == expected.device_name
        && actual.width == expected.width
        && actual.height == expected.height
        && actual.refresh_rate == expected.refresh_rate
        && actual.bits_per_pixel == expected.bits_per_pixel
}

pub fn verify_persistent_mode(expected: &DisplayMode) -> Result<(), String> {
    let current = current_mode(&expected.device_name)?;
    if !mode_matches(&current, expected) {
        return Err(format!(
            "当前显示模式验证失败，期望 {}，实际 {}",
            expected.label(),
            current.label()
        ));
    }

    let persisted = registry_mode(&expected.device_name)?;
    if !mode_matches(&persisted, expected) {
        return Err(format!(
            "持久显示模式验证失败，期望 {}，实际 {}",
            expected.label(),
            persisted.label()
        ));
    }
    Ok(())
}

pub fn enumerate_displays() -> Result<Vec<DisplayInfo>, String> {
    let mut displays = Vec::new();
    let physical_names = physical_monitor_names();
    let mut index = 0;

    loop {
        let mut device = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };

        let ok = unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) };
        if !ok.as_bool() {
            break;
        }
        index += 1;

        if device.StateFlags.0 & DISPLAY_DEVICE_ACTIVE.0 == 0 {
            continue;
        }

        let device_name = utf16_array(&device.DeviceName);
        let friendly_name = physical_names
            .get(&device_name.to_ascii_lowercase())
            .cloned()
            .or_else(|| monitor_name(&device_name))
            .unwrap_or_else(|| utf16_array(&device.DeviceString));
        let current_mode = current_mode(&device_name)?;

        displays.push(DisplayInfo {
            device_name,
            friendly_name,
            is_primary: device.StateFlags.0 & DISPLAY_DEVICE_PRIMARY_DEVICE.0 != 0,
            current_mode,
        });
    }

    if displays.is_empty() {
        Err("Windows 没有返回任何活动显示器".to_string())
    } else {
        displays.sort_by_key(|display| !display.is_primary);
        Ok(displays)
    }
}

/// 只枚举当前活动显示器的设备名。自动接管每次轮询都要用它来筛掉目标显示器
/// 已经不存在的配置，所以刻意不读 friendly name 和当前模式。
/// 返回空集合表示这次枚举不可信，调用方不得据此认为“没有任何显示器”。
pub fn active_device_names() -> HashSet<String> {
    let mut names = HashSet::new();
    let mut index = 0;

    loop {
        let mut device = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };

        let ok = unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) };
        if !ok.as_bool() {
            break;
        }
        index += 1;

        if device.StateFlags.0 & DISPLAY_DEVICE_ACTIVE.0 == 0 {
            continue;
        }
        let device_name = utf16_array(&device.DeviceName);
        if !device_name.trim().is_empty() {
            names.insert(device_name);
        }
    }

    names
}

pub fn enumerate_modes(device_name: &str) -> Result<Vec<DisplayMode>, String> {
    let mut modes = Vec::new();
    let mut seen = HashSet::new();
    let mut index = 0;

    while let Some(mode) = read_mode(device_name, ENUM_DISPLAY_SETTINGS_MODE(index)) {
        index += 1;
        if mode.dmBitsPerPel < 32 || mode.dmDisplayFrequency <= 1 {
            continue;
        }

        let key = (mode.dmPelsWidth, mode.dmPelsHeight, mode.dmDisplayFrequency);
        if seen.insert(key) {
            modes.push(to_mode(device_name, &mode));
        }
    }

    modes.sort_by(|a, b| {
        (b.width * b.height)
            .cmp(&(a.width * a.height))
            .then_with(|| b.width.cmp(&a.width))
            .then_with(|| b.height.cmp(&a.height))
            .then_with(|| b.refresh_rate.cmp(&a.refresh_rate))
    });

    if modes.is_empty() {
        Err(format!("显示器 {device_name} 没有可用的 32 位显示模式"))
    } else {
        Ok(modes)
    }
}

fn switch_mode_with_flags(target: &DisplayMode, flags: CDS_TYPE) -> Result<(), String> {
    let mut index = 0;
    let mut selected = None;

    while let Some(mode) = read_mode(&target.device_name, ENUM_DISPLAY_SETTINGS_MODE(index)) {
        index += 1;
        if mode.dmPelsWidth == target.width
            && mode.dmPelsHeight == target.height
            && mode.dmDisplayFrequency == target.refresh_rate
            && mode.dmBitsPerPel == target.bits_per_pixel
        {
            selected = Some(mode);
            break;
        }
    }

    let mut mode = selected.ok_or_else(|| {
        format!(
            "显示器不支持 {}x{} @ {} Hz",
            target.width, target.height, target.refresh_rate
        )
    })?;
    mode.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_BITSPERPEL;

    let device = wide(&target.device_name);
    let test_result = unsafe {
        ChangeDisplaySettingsExW(PCWSTR(device.as_ptr()), Some(&mode), None, CDS_TEST, None)
    };
    if test_result != DISP_CHANGE_SUCCESSFUL {
        return Err(format!(
            "Windows 拒绝测试显示模式，错误码 {}",
            test_result.0
        ));
    }

    let result = unsafe {
        ChangeDisplaySettingsExW(PCWSTR(device.as_ptr()), Some(&mode), None, flags, None)
    };
    if result == DISP_CHANGE_SUCCESSFUL {
        Ok(())
    } else {
        Err(format!("切换显示模式失败，错误码 {}", result.0))
    }
}

pub fn switch_mode(target: &DisplayMode) -> Result<(), String> {
    switch_mode_with_flags(target, CDS_TYPE(0))
}

pub fn switch_mode_persistent(target: &DisplayMode) -> Result<(), String> {
    switch_mode_with_flags(target, CDS_UPDATEREGISTRY | CDS_RESET)
}

#[cfg(test)]
mod tests {
    use super::mode_matches;
    use crate::model::DisplayMode;

    fn mode(device_name: &str, width: u32, height: u32, refresh_rate: u32) -> DisplayMode {
        DisplayMode {
            device_name: device_name.to_string(),
            width,
            height,
            refresh_rate,
            bits_per_pixel: 32,
        }
    }

    #[test]
    fn persistent_mode_match_requires_all_display_properties() {
        let expected = mode(r"\\.\DISPLAY1", 2560, 1440, 280);
        assert!(mode_matches(&expected, &expected));
        assert!(!mode_matches(
            &mode(r"\\.\DISPLAY2", 2560, 1440, 280),
            &expected,
        ));
        assert!(!mode_matches(
            &mode(r"\\.\DISPLAY1", 2304, 1440, 280),
            &expected,
        ));
        assert!(!mode_matches(
            &mode(r"\\.\DISPLAY1", 2560, 1440, 240),
            &expected,
        ));
        let mut wrong_depth = expected.clone();
        wrong_depth.bits_per_pixel = 24;
        assert!(!mode_matches(&wrong_depth, &expected));
    }
}
