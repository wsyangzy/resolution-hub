use std::{path::Path, process::Command};

use windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::PCWSTR,
};

use crate::model::{DetectLaunchRequest, GameProfile, LaunchType};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn launch(profile: &GameProfile) -> Result<(), String> {
    match profile.launch_type {
        LaunchType::Executable => launch_executable(profile),
        LaunchType::Steam => launch_steam(&profile.steam_app_id),
        LaunchType::Shortcut => launch_shortcut(&profile.shortcut_path),
    }
}

pub fn launch_request(request: &DetectLaunchRequest) -> Result<(), String> {
    match request.launch_type {
        LaunchType::Executable => launch_executable_parts(
            request.executable.trim(),
            request.arguments.trim(),
            request.working_directory.trim(),
        ),
        LaunchType::Steam => launch_steam(&request.steam_app_id),
        LaunchType::Shortcut => launch_shortcut(&request.shortcut_path),
    }
}

fn launch_executable(profile: &GameProfile) -> Result<(), String> {
    launch_executable_parts(
        &profile.executable,
        &profile.arguments,
        &profile.working_directory,
    )
}

fn launch_executable_parts(
    executable: &str,
    arguments: &str,
    working_directory: &str,
) -> Result<(), String> {
    let executable = Path::new(executable.trim());
    if !executable.is_file() {
        return Err("游戏 EXE 路径不存在".to_string());
    }

    let mut command = Command::new(executable);
    let configured_directory = Path::new(working_directory.trim());
    if !working_directory.trim().is_empty() && configured_directory.is_dir() {
        command.current_dir(configured_directory);
    } else if let Some(parent) = executable.parent() {
        command.current_dir(parent);
    }
    command.args(parse_arguments(arguments));
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动游戏失败：{e}"))
}

fn launch_steam(app_id: &str) -> Result<(), String> {
    let app_id = app_id.trim();
    if app_id.is_empty() || !app_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Steam App ID 必须是数字".to_string());
    }

    let target = wide(&format!("steam://rungameid/{app_id}"));
    shell_execute(&target, PCWSTR::null(), PCWSTR::null(), "无法调用 Steam")
}

fn launch_shortcut(shortcut_path: &str) -> Result<(), String> {
    let path = Path::new(shortcut_path.trim());
    if !path.is_file() {
        return Err("启动器或游戏快捷方式不存在".to_string());
    }

    let target = wide(shortcut_path.trim());
    shell_execute(
        &target,
        PCWSTR::null(),
        PCWSTR::null(),
        "无法打开启动器快捷方式",
    )
}

fn shell_execute(
    target: &[u16],
    parameters: PCWSTR,
    working_directory: PCWSTR,
    error_message: &str,
) -> Result<(), String> {
    let operation = wide("open");
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            parameters,
            working_directory,
            SW_SHOWNORMAL,
        )
    };

    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!(
            "{error_message}，ShellExecute 错误码 {}",
            result.0 as isize
        ))
    }
}

fn parse_arguments(arguments: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for character in arguments.chars() {
        match character {
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted && !current.is_empty() => {
                result.push(std::mem::take(&mut current));
            }
            ' ' | '\t' if !quoted => {}
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::parse_arguments;

    #[test]
    fn parses_quoted_game_arguments() {
        assert_eq!(
            parse_arguments(r#"-novid -path "D:\Game Data" -fullscreen"#),
            vec!["-novid", "-path", r#"D:\Game Data"#, "-fullscreen"]
        );
    }
}
