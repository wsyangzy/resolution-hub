use std::{mem::size_of, path::PathBuf};

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
    },
    core::PWSTR,
};

#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub process_name: String,
    pub executable: Option<PathBuf>,
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
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return processes;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
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
        }
        let _ = CloseHandle(snapshot);
    }
    processes
}

pub fn find_pid(process_name: &str) -> Option<u32> {
    let target = process_name.trim().trim_matches('"');
    if target.is_empty() {
        return None;
    }

    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return None;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut found = None;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if utf16_array(&entry.szExeFile).eq_ignore_ascii_case(target) {
                    found = Some(entry.th32ProcessID);
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
        found
    }
}

pub fn is_running(process_name: &str) -> bool {
    find_pid(process_name).is_some()
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

    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
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
        }
        let _ = CloseHandle(snapshot);
        found
    }
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
    use super::{ProcessExitWaiter, find_pid};

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
    fn current_process_is_not_signaled() {
        let waiter = ProcessExitWaiter::open(std::process::id())
            .expect("current process should be waitable");
        assert_eq!(waiter.wait_timeout(0), Some(false));
    }
}
