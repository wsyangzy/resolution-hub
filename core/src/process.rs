use std::{mem::size_of, path::PathBuf, thread, time::Duration};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::PWSTR,
};

#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub process_name: String,
    pub executable: Option<PathBuf>,
}

const SNAPSHOT_ATTEMPTS: usize = 5;
const SNAPSHOT_RETRY_DELAY: Duration = Duration::from_millis(20);

/// 进程表在枚举期间发生变动时 `CreateToolhelp32Snapshot` 会返回 ERROR_BAD_LENGTH，
/// 系统繁忙（例如游戏运行）时尤其频繁。必须重试，否则调用方会把瞬时失败
/// 当成“系统里没有这个进程”，进而误判游戏已退出。
fn create_process_snapshot() -> Option<HANDLE> {
    for attempt in 0..SNAPSHOT_ATTEMPTS {
        if let Ok(snapshot) = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
            return Some(snapshot);
        }
        if attempt + 1 < SNAPSHOT_ATTEMPTS {
            thread::sleep(SNAPSHOT_RETRY_DELAY);
        }
    }
    None
}

/// 取得一份可枚举的进程快照，`Process32FirstW` 失败同样视为快照失败。
fn open_first_entry() -> Option<(HANDLE, PROCESSENTRY32W)> {
    let snapshot = create_process_snapshot()?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_err() {
        unsafe {
            let _ = CloseHandle(snapshot);
        }
        return None;
    }
    Some((snapshot, entry))
}

fn utf16_array(value: &[u16]) -> String {
    let end = value.iter().position(|&c| c == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

pub fn executable_path(pid: u32) -> Option<PathBuf> {
    if pid == 0 {
        return None;
    }

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 32_768];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        result
            .ok()
            .map(|_| PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize])))
    }
}

pub fn snapshot() -> Vec<ProcessInfo> {
    let mut processes = Vec::new();
    let Some((snapshot, mut entry)) = open_first_entry() else {
        return processes;
    };
    unsafe {
        loop {
            let pid = entry.th32ProcessID;
            processes.push(ProcessInfo {
                pid,
                process_name: utf16_array(&entry.szExeFile),
                executable: executable_path(pid),
            });
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(snapshot);
    }
    processes
}

/// 外层 `None` 表示这次没能拿到可信的进程快照；空 `Vec` 表示快照可信但没有这个进程。
/// 同名进程可能不止一个（多进程架构的游戏），窗口未必挂在第一个上，所以全部返回。
pub fn lookup_pids(process_name: &str) -> Option<Vec<u32>> {
    let target = process_name.trim().trim_matches('"');
    if target.is_empty() {
        return Some(Vec::new());
    }

    let (snapshot, mut entry) = open_first_entry()?;
    unsafe {
        let mut found = Vec::new();
        loop {
            if utf16_array(&entry.szExeFile).eq_ignore_ascii_case(target) {
                found.push(entry.th32ProcessID);
            }
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }

        let _ = CloseHandle(snapshot);
        Some(found)
    }
}

/// 外层 `None` 表示这次没能拿到可信的进程快照；内层 `None` 表示快照可信但没有这个进程。
pub fn lookup_pid(process_name: &str) -> Option<Option<u32>> {
    lookup_pids(process_name).map(|pids| pids.first().copied())
}

pub fn find_pid(process_name: &str) -> Option<u32> {
    lookup_pid(process_name).flatten()
}

pub fn is_any_running<'a>(process_names: impl IntoIterator<Item = &'a str>) -> bool {
    let targets: Vec<String> = process_names
        .into_iter()
        .map(|name| name.trim().trim_matches('"').to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    if targets.is_empty() {
        return false;
    }

    let Some((snapshot, mut entry)) = open_first_entry() else {
        return false;
    };
    unsafe {
        let mut found = false;
        loop {
            let running = utf16_array(&entry.szExeFile).to_ascii_lowercase();
            if targets.iter().any(|target| target == &running) {
                found = true;
                break;
            }
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

#[cfg(test)]
mod tests {
    use super::{find_pid, lookup_pids};

    #[test]
    fn finds_the_current_process() {
        let executable = std::env::current_exe().expect("current executable should exist");
        let process_name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .expect("current executable should have a file name");

        assert!(find_pid(process_name).is_some());
    }

    #[test]
    fn lookup_returns_every_process_id_sharing_the_name() {
        let executable = std::env::current_exe().expect("current executable should exist");
        let process_name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .expect("current executable should have a file name");

        // 快照成功必须返回 Some；把失败退化成空表会让调用方误判游戏已退出。
        // 窗口未必挂在第一个同名进程上，所以贴合逻辑依赖这里返回全部 pid。
        let pids = lookup_pids(process_name)
            .expect("process snapshot should be retried until it succeeds");
        assert!(pids.contains(&std::process::id()));
        assert_eq!(
            lookup_pids("resolution-hub-no-such-process.exe"),
            Some(Vec::new())
        );
    }
}
