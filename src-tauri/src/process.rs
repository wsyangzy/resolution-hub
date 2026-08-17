use std::{collections::HashMap, mem::size_of, path::PathBuf, thread, time::Duration};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
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

/// 返回 `None` 表示这次没能拿到可信的进程快照，调用方不得把它当成“没有进程在运行”。
pub fn running_by_name() -> Option<HashMap<String, Vec<u32>>> {
    let mut running: HashMap<String, Vec<u32>> = HashMap::new();
    let (snapshot, mut entry) = open_first_entry()?;
    unsafe {
        loop {
            let process_name = utf16_array(&entry.szExeFile).to_ascii_lowercase();
            if !process_name.is_empty() {
                running
                    .entry(process_name)
                    .or_default()
                    .push(entry.th32ProcessID);
            }
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(snapshot);
    }
    Some(running)
}

pub fn foreground_process_id() -> Option<u32> {
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        (pid != 0).then_some(pid)
    }
}

/// 外层 `None` 表示这次没能拿到可信的进程快照；内层 `None` 表示快照可信但没有这个进程。
pub fn lookup_pid(process_name: &str) -> Option<Option<u32>> {
    let target = process_name.trim().trim_matches('"');
    if target.is_empty() {
        return Some(None);
    }

    let (snapshot, mut entry) = open_first_entry()?;
    unsafe {
        let mut found = None;
        loop {
            if utf16_array(&entry.szExeFile).eq_ignore_ascii_case(target) {
                found = Some(entry.th32ProcessID);
                break;
            }
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }

        let _ = CloseHandle(snapshot);
        Some(found)
    }
}

pub fn find_pid(process_name: &str) -> Option<u32> {
    lookup_pid(process_name).flatten()
}

/// 返回 `None` 表示这次没能拿到可信的进程快照。调用方不得把它当成“进程已退出”，
/// 否则会在系统繁忙时提前恢复桌面显示模式并把控制权交还给自动监控，形成切换震荡。
pub fn is_running(process_name: &str) -> Option<bool> {
    lookup_pid(process_name).map(|pid| pid.is_some())
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

pub fn is_pid_running(pid: u32) -> bool {
    ProcessExitWaiter::open(pid).is_some_and(|waiter| waiter.wait_timeout(0) == Some(false))
}
pub struct ProcessExitWaiter {
    handle: HANDLE,
}

impl ProcessExitWaiter {
    pub fn open(pid: u32) -> Option<Self> {
        if pid == 0 {
            return None;
        }
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok()?;
        Some(Self { handle })
    }

    pub fn wait_timeout(&self, timeout_ms: u32) -> Option<bool> {
        let result = unsafe { WaitForSingleObject(self.handle, timeout_ms) };
        if result == WAIT_OBJECT_0 {
            Some(true)
        } else if result == WAIT_TIMEOUT {
            Some(false)
        } else {
            None
        }
    }
}

impl Drop for ProcessExitWaiter {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcessExitWaiter, find_pid, is_running, running_by_name};

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
    fn lightweight_snapshot_groups_process_ids_by_name() {
        let executable = std::env::current_exe().expect("current executable should exist");
        let process_name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .expect("current executable should have a file name")
            .to_ascii_lowercase();

        // 快照成功必须返回 Some；把失败退化成空表会让调用方误判游戏已退出。
        let running =
            running_by_name().expect("process snapshot should be retried until it succeeds");
        assert!(
            running
                .get(&process_name)
                .is_some_and(|pids| pids.contains(&std::process::id()))
        );
    }

    #[test]
    fn a_missing_process_is_distinguishable_from_a_failed_snapshot() {
        let executable = std::env::current_exe().expect("current executable should exist");
        let process_name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .expect("current executable should have a file name");

        // Some(false) 才代表“快照可信且进程确实不在”。调用方必须能把它和
        // None（快照不可信）区分开，否则会在系统繁忙时误判游戏已退出。
        assert_eq!(is_running(process_name), Some(true));
        assert_eq!(
            is_running("resolution-hub-no-such-process.exe"),
            Some(false)
        );
    }

    #[test]
    fn current_process_is_not_signaled() {
        let waiter = ProcessExitWaiter::open(std::process::id())
            .expect("current process should be waitable");
        assert_eq!(waiter.wait_timeout(0), Some(false));
    }
}
