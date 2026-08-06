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
                INFINITE, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
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

pub(crate) struct ProcessWaiter {
    handle: HANDLE,
}

impl ProcessWaiter {
    pub(crate) fn open(pid: u32) -> Option<Self> {
        if pid == 0 {
            return None;
        }
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok()?;
        Some(Self { handle })
    }

    pub(crate) fn wait(&self, timeout_ms: u32) -> Option<bool> {
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

impl Drop for ProcessWaiter {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
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

pub fn wait_for_exit(pid: u32) -> bool {
    ProcessWaiter::open(pid).and_then(|waiter| waiter.wait(INFINITE)) == Some(true)
}

pub fn foreground_pid() -> Option<u32> {
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() {
        return None;
    }

    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) } == 0 || pid == 0 {
        None
    } else {
        Some(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcessWaiter, find_pid};

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
        let waiter = ProcessWaiter::open(std::process::id()).expect("current process should open");
        assert_eq!(waiter.wait(0), Some(false));
    }
}
