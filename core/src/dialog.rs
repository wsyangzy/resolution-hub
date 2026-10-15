use std::mem::size_of;

use windows::{
    Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    },
    core::{PCWSTR, PWSTR},
};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn select_executable() -> Option<String> {
    select_file(
        "可执行文件 (*.exe)\0*.exe\0所有文件 (*.*)\0*.*\0",
        "选择游戏可执行文件",
    )
}

pub fn select_shortcut() -> Option<String> {
    select_file(
        "Windows 快捷方式 (*.lnk;*.url)\0*.lnk;*.url\0所有文件 (*.*)\0*.*\0",
        "选择启动器或游戏快捷方式",
    )
}

fn select_file(filter_text: &str, title_text: &str) -> Option<String> {
    let mut file_buffer = vec![0u16; 32_768];
    let filter = wide(filter_text);
    let title = wide(title_text);

    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_buffer.as_mut_ptr()),
        nMaxFile: file_buffer.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };

    if !unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        return None;
    }

    let end = file_buffer
        .iter()
        .position(|&character| character == 0)
        .unwrap_or(file_buffer.len());
    Some(String::from_utf16_lossy(&file_buffer[..end]))
}
