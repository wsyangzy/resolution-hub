use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
            IsWindowVisible,
        },
    },
    core::BOOL,
};

use crate::{model::RecentApplication, process, profile};

static HISTORY: OnceLock<Mutex<Vec<RecentApplication>>> = OnceLock::new();

pub fn list() -> Vec<RecentApplication> {
    refresh();
    persist();
    let mut applications = history()
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    applications.retain(|item| !should_ignore(&item.process_name, Path::new(&item.executable)));
    applications.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    applications.truncate(40);
    applications
}

pub fn scan_visible() -> Vec<RecentApplication> {
    let mut windows = Vec::<(u32, String)>::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_window),
            LPARAM((&mut windows as *mut Vec<(u32, String)>) as isize),
        );
    }

    let timestamp = now();
    let mut applications = HashMap::new();
    for (pid, title) in windows {
        let Some(executable) = process::executable_path(pid) else {
            continue;
        };
        let Some(process_name) = executable
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if should_ignore(&process_name, &executable) {
            continue;
        }

        applications
            .entry(executable.to_string_lossy().to_lowercase())
            .or_insert_with(|| RecentApplication {
                pid,
                name: clean_title(&title, &process_name),
                executable: executable.to_string_lossy().into_owned(),
                process_name,
                last_seen: timestamp,
            });
    }
    applications.into_values().collect()
}

fn refresh() {
    let visible = scan_visible();
    let Ok(mut items) = history().lock() else {
        return;
    };

    for application in visible {
        if let Some(existing) = items.iter_mut().find(|item| {
            item.executable
                .eq_ignore_ascii_case(&application.executable)
        }) {
            *existing = application;
        } else {
            items.push(application);
        }
    }
    items.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    items.truncate(40);
}

fn history() -> &'static Mutex<Vec<RecentApplication>> {
    HISTORY.get_or_init(|| Mutex::new(load()))
}

fn load() -> Vec<RecentApplication> {
    let Ok(path) = history_path() else {
        return Vec::new();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn persist() {
    let Ok(items) = history().lock() else {
        return;
    };
    let Ok(content) = serde_json::to_string_pretty(&*items) else {
        return;
    };
    if let Ok(path) = history_path() {
        let _ = fs::write(path, content);
    }
}

fn history_path() -> Result<std::path::PathBuf, String> {
    Ok(profile::app_dir()?.join("recent-apps.json"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn clean_title(title: &str, process_name: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        process_name.trim_end_matches(".exe").to_string()
    } else {
        title.to_string()
    }
}

fn should_ignore(process_name: &str, executable: &Path) -> bool {
    const IGNORED: &[&str] = &[
        "applicationframehost.exe",
        "explorer.exe",
        "resolution-hub.exe",
        "resolution-hub-native-ui.exe",
        "searchhost.exe",
        "shellexperiencehost.exe",
        "startmenuexperiencehost.exe",
        "textinputhost.exe",
    ];
    IGNORED
        .iter()
        .any(|ignored| process_name.eq_ignore_ascii_case(ignored))
        || executable
            .to_string_lossy()
            .to_lowercase()
            .contains("\\windows\\systemapps\\")
}

unsafe extern "system" fn enum_window(window: HWND, parameter: LPARAM) -> BOOL {
    unsafe {
        if !IsWindowVisible(window).as_bool() {
            return true.into();
        }
        let length = GetWindowTextLengthW(window);
        if length <= 0 {
            return true.into();
        }

        let mut title = vec![0u16; length as usize + 1];
        if GetWindowTextW(window, &mut title) <= 0 {
            return true.into();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == 0 {
            return true.into();
        }

        let windows = &mut *(parameter.0 as *mut Vec<(u32, String)>);
        windows.push((pid, String::from_utf16_lossy(&title[..length as usize])));
        true.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_sources_exclude_both_hub_windows_but_keep_games() {
        assert!(should_ignore(
            "Resolution-Hub-Native-UI.exe",
            Path::new(r"C:\Apps\resolution-hub-native-ui.exe")
        ));
        assert!(should_ignore(
            "resolution-hub.exe",
            Path::new(r"C:\Apps\resolution-hub.exe")
        ));
        assert!(!should_ignore("game.exe", Path::new(r"C:\Games\game.exe")));
    }
}
