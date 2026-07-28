use std::io::ErrorKind;

use winreg::{RegKey, enums::HKEY_CURRENT_USER};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Resolution Hub";

fn current_command() -> Result<String, String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("无法定位当前程序：{error}"))?;
    Ok(format!("\"{}\"", executable.display()))
}

pub fn is_enabled() -> Result<bool, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey(RUN_KEY) {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("无法读取 Windows 启动项：{error}")),
    };
    let command: String = match key.get_value(VALUE_NAME) {
        Ok(command) => command,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("无法读取开机自启状态：{error}")),
    };

    Ok(command.eq_ignore_ascii_case(&current_command()?))
}

pub fn set_enabled(enabled: bool) -> Result<bool, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(RUN_KEY)
        .map_err(|error| format!("无法打开 Windows 启动项：{error}"))?;

    if enabled {
        key.set_value(VALUE_NAME, &current_command()?)
            .map_err(|error| format!("无法启用开机自启：{error}"))?;
    } else if let Err(error) = key.delete_value(VALUE_NAME)
        && error.kind() != ErrorKind::NotFound
    {
        return Err(format!("无法关闭开机自启：{error}"));
    }

    is_enabled()
}
