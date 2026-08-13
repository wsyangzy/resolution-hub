use std::{fs, path::Path, process::Command};

use windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::PCWSTR,
};

use crate::{
    icon,
    model::{DetectLaunchRequest, GameProfile, LaunchType},
    process,
};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn launch(profile: &GameProfile) -> Result<bool, String> {
    match profile.launch_type {
        LaunchType::Executable => launch_executable(profile).map(|_| true),
        LaunchType::Steam => launch_steam(&profile.steam_app_id).map(|_| true),
        LaunchType::Shortcut => {
            if is_launcher_running(&profile.shortcut_path, &profile.launcher_process_name) {
                Ok(false)
            } else {
                launch_shortcut(&profile.shortcut_path).map(|_| true)
            }
        }
    }
}

pub fn launch_request(request: &DetectLaunchRequest) -> Result<bool, String> {
    match request.launch_type {
        LaunchType::Executable => launch_executable_parts(
            request.executable.trim(),
            request.arguments.trim(),
            request.working_directory.trim(),
        )
        .map(|_| true),
        LaunchType::Steam => launch_steam(&request.steam_app_id).map(|_| true),
        LaunchType::Shortcut => {
            if is_launcher_running(&request.shortcut_path, &request.launcher_process_name) {
                Ok(false)
            } else {
                launch_shortcut(&request.shortcut_path).map(|_| true)
            }
        }
    }
}

fn is_launcher_running(shortcut_path: &str, configured_process_name: &str) -> bool {
    let candidates = launcher_process_candidates(shortcut_path, configured_process_name);
    process::is_any_running(candidates.iter().map(String::as_str))
}

fn launcher_process_candidates(shortcut_path: &str, configured_process_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    push_unique_process_name(&mut candidates, configured_process_name);

    let shortcut = Path::new(shortcut_path.trim());
    if !shortcut
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
    {
        return candidates;
    }
    let Some(target) = icon::shortcut_target(shortcut) else {
        return candidates;
    };
    launcher_process_candidates_from_target(&target, &mut candidates);
    candidates
}

fn launcher_process_candidates_from_target(target: &Path, candidates: &mut Vec<String>) {
    if let Some(name) = target.file_name().and_then(|name| name.to_str()) {
        push_unique_process_name(candidates, name);
    }
    if let Some(directory) = target.parent() {
        collect_launcher_executables(directory, 1, candidates);
    }
}

fn collect_launcher_executables(directory: &Path, depth: usize, candidates: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() && depth > 0 {
            collect_launcher_executables(&path, depth - 1, candidates);
            continue;
        }
        if !file_type.is_file() || !is_launcher_executable(&path) {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            push_unique_process_name(candidates, name);
        }
    }
}

fn is_launcher_executable(path: &Path) -> bool {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return false;
    }
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_lowercase();
    name.contains("launcher") || name.contains("client") || name.contains("登录器")
}

fn push_unique_process_name(candidates: &mut Vec<String>, process_name: &str) {
    let process_name = process_name.trim().trim_matches('"');
    if process_name.is_empty()
        || candidates
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(process_name))
    {
        return;
    }
    candidates.push(process_name.to_string());
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
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{is_launcher_running, launcher_process_candidates_from_target, parse_arguments};

    #[test]
    fn parses_quoted_game_arguments() {
        assert_eq!(
            parse_arguments(r#"-novid -path "D:\Game Data" -fullscreen"#),
            vec!["-novid", "-path", r#"D:\Game Data"#, "-fullscreen"]
        );
    }

    #[test]
    fn reuses_an_explicitly_configured_running_launcher() {
        let executable = std::env::current_exe().expect("current executable should exist");
        let process_name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .expect("current executable should have a file name");

        assert!(is_launcher_running("", process_name));
    }

    #[test]
    fn discovers_a_persistent_launcher_below_a_bootstrap_target() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("resolution-hub-launcher-{suffix}"));
        let launcher_dir = root.join("Launcher");
        fs::create_dir_all(&launcher_dir).expect("temporary launcher directory should exist");
        let target = root.join("aclos-launcher.exe");
        fs::write(&target, []).expect("bootstrap fixture should exist");
        fs::write(launcher_dir.join("无畏契约登录器.exe"), [])
            .expect("persistent launcher fixture should exist");
        fs::write(root.join("crashpad_handler.exe"), []).expect("helper fixture should exist");

        let mut candidates = Vec::new();
        launcher_process_candidates_from_target(&target, &mut candidates);

        assert!(candidates.iter().any(|name| name == "aclos-launcher.exe"));
        assert!(candidates.iter().any(|name| name == "无畏契约登录器.exe"));
        assert!(!candidates.iter().any(|name| name == "crashpad_handler.exe"));
        fs::remove_dir_all(root).expect("temporary launcher directory should be removable");
    }
}
