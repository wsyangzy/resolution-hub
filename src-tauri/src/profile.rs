use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::model::{
    AppSettings, CloseBehavior, DisplayMode, GameProfile, LaunchType, MinimizeBehavior,
    PendingRestore,
};

pub(crate) fn app_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位 APPDATA 目录".to_string())?;
    let dir = base.join("resolution-hub");
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录：{e}"))?;
    Ok(dir)
}

fn profiles_path() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("profiles.json"))
}

fn pending_path() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("pending-restore.json"))
}

fn settings_path() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("settings.json"))
}

fn usage_path() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("profile-usage.json"))
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileUsage {
    launch_count: u64,
    last_launched_at: u64,
}

pub fn list() -> Result<Vec<GameProfile>, String> {
    let path = profiles_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path).map_err(|e| format!("读取配置失败：{e}"))?;
    let mut profiles: Vec<GameProfile> =
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败：{e}"))?;
    profiles.iter_mut().for_each(GameProfile::normalize);
    Ok(profiles)
}

pub fn save(mut profile: GameProfile) -> Result<GameProfile, String> {
    profile.normalize();
    validate(&profile)?;
    let mut profiles = list()?;

    if profile.id.trim().is_empty() {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("系统时间异常：{e}"))?
            .as_millis();
        profile.id = format!("game-{millis}");
    }

    match profiles.iter().position(|item| item.id == profile.id) {
        Some(index) => profiles[index] = profile.clone(),
        None => profiles.push(profile.clone()),
    }

    write_profiles(&profiles)?;
    Ok(profile)
}

pub fn delete(id: &str) -> Result<(), String> {
    let mut profiles = list()?;
    profiles.retain(|item| item.id != id);
    write_profiles(&profiles)
}

pub fn find(id: &str) -> Result<GameProfile, String> {
    list()?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "找不到游戏配置".to_string())
}

pub fn record_launch(id: &str) -> Result<(), String> {
    let mut usage = load_usage()?;
    let item = usage.entry(id.to_string()).or_default();
    item.launch_count = item.launch_count.saturating_add(1);
    item.last_launched_at = current_timestamp()?;
    write_usage(&usage)
}

pub fn most_used(limit: usize) -> Result<Vec<GameProfile>, String> {
    let profiles = list()?;
    let usage = load_usage().unwrap_or_default();
    Ok(rank_profiles(profiles, &usage, limit))
}

fn rank_profiles(
    profiles: Vec<GameProfile>,
    usage: &HashMap<String, ProfileUsage>,
    limit: usize,
) -> Vec<GameProfile> {
    let mut indexed: Vec<_> = profiles.into_iter().enumerate().collect();
    indexed.sort_by(|(left_index, left), (right_index, right)| {
        let left_usage = usage.get(&left.id).cloned().unwrap_or_default();
        let right_usage = usage.get(&right.id).cloned().unwrap_or_default();
        right_usage
            .launch_count
            .cmp(&left_usage.launch_count)
            .then_with(|| {
                right_usage
                    .last_launched_at
                    .cmp(&left_usage.last_launched_at)
            })
            .then_with(|| left_index.cmp(right_index))
    });
    indexed
        .into_iter()
        .take(limit)
        .map(|(_, profile)| profile)
        .collect()
}

fn load_usage() -> Result<HashMap<String, ProfileUsage>, String> {
    let path = usage_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content =
        fs::read_to_string(path).map_err(|error| format!("读取游戏使用记录失败：{error}"))?;
    serde_json::from_str(&content).map_err(|error| format!("解析游戏使用记录失败：{error}"))
}

fn write_usage(usage: &HashMap<String, ProfileUsage>) -> Result<(), String> {
    let content = serde_json::to_string_pretty(usage)
        .map_err(|error| format!("序列化游戏使用记录失败：{error}"))?;
    fs::write(usage_path()?, content).map_err(|error| format!("保存游戏使用记录失败：{error}"))
}

fn current_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("系统时间异常：{error}"))
}

fn write_profiles(profiles: &[GameProfile]) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(profiles).map_err(|e| format!("序列化配置失败：{e}"))?;
    fs::write(profiles_path()?, content).map_err(|e| format!("保存配置失败：{e}"))
}

fn validate(profile: &GameProfile) -> Result<(), String> {
    if profile.name.trim().is_empty() {
        return Err("游戏名称不能为空".to_string());
    }
    if profile.process_name.trim().is_empty() {
        return Err("监控进程名不能为空".to_string());
    }
    if profile.display_device.trim().is_empty()
        || profile.width == 0
        || profile.height == 0
        || profile.refresh_rate == 0
    {
        return Err("请选择有效的目标显示模式".to_string());
    }
    match &profile.launch_type {
        LaunchType::Executable if !std::path::Path::new(profile.executable.trim()).is_file() => {
            return Err("请选择有效的游戏 EXE".to_string());
        }
        LaunchType::Steam
            if profile.steam_app_id.trim().is_empty()
                || !profile
                    .steam_app_id
                    .trim()
                    .chars()
                    .all(|character| character.is_ascii_digit()) =>
        {
            return Err("Steam App ID 必须是数字".to_string());
        }
        LaunchType::Shortcut if !std::path::Path::new(profile.shortcut_path.trim()).is_file() => {
            return Err("请选择有效的启动器或游戏快捷方式".to_string());
        }
        _ => {}
    }
    if !(30..=3_600).contains(&profile.launch_timeout_sec) {
        return Err("等待游戏启动时间必须在 30 到 3600 秒之间".to_string());
    }
    Ok(())
}

pub fn save_pending(pending: &PendingRestore) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(pending).map_err(|e| format!("保存恢复点失败：{e}"))?;
    fs::write(pending_path()?, content).map_err(|e| format!("写入恢复点失败：{e}"))
}

pub fn load_pending() -> Result<Option<PendingRestore>, String> {
    let path = pending_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|e| format!("读取恢复点失败：{e}"))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|e| format!("解析恢复点失败：{e}"))
}

pub fn clear_pending() -> Result<(), String> {
    let path = pending_path()?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("清理恢复点失败：{e}"))?;
    }
    Ok(())
}

pub fn load_settings() -> Result<AppSettings, String> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(path).map_err(|e| format!("读取设置失败：{e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("解析设置失败：{e}"))
}

fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(settings).map_err(|e| format!("序列化设置失败：{e}"))?;
    fs::write(settings_path()?, content).map_err(|e| format!("保存设置失败：{e}"))
}

pub fn save_default_display_mode(mode: DisplayMode) -> Result<AppSettings, String> {
    let mut settings = load_settings()?;
    settings
        .default_display_modes
        .retain(|item| item.device_name != mode.device_name);
    settings.default_display_modes.push(mode);

    save_settings(&settings)?;
    Ok(settings)
}

pub fn save_close_behavior(
    close_behavior: CloseBehavior,
    confirm_close_behavior: Option<bool>,
    minimize_behavior: Option<MinimizeBehavior>,
) -> Result<AppSettings, String> {
    let mut settings = load_settings()?;
    settings.close_behavior = match close_behavior {
        CloseBehavior::Ask => CloseBehavior::HideToTray,
        behavior => behavior,
    };
    if let Some(confirm) = confirm_close_behavior {
        settings.confirm_close_behavior = confirm;
    }
    if let Some(minimize) = minimize_behavior {
        settings.minimize_behavior = minimize;
    }
    save_settings(&settings)?;
    Ok(settings)
}

pub fn default_display_mode(device_name: &str) -> Result<Option<DisplayMode>, String> {
    Ok(load_settings()?
        .default_display_modes
        .into_iter()
        .find(|mode| mode.device_name == device_name))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{ProfileUsage, rank_profiles};
    use crate::model::GameProfile;

    fn profile(id: &str) -> GameProfile {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": id,
            "launchType": "steam",
            "executable": "",
            "arguments": "",
            "steamAppId": "123",
            "processName": "game.exe",
            "switchTiming": "onProcessDetected",
            "launchTimeoutSec": 600,
            "displayDevice": "DISPLAY1",
            "width": 1920,
            "height": 1080,
            "refreshRate": 144,
            "restoreDelayMs": 2000
        }))
        .expect("profile should deserialize")
    }

    #[test]
    fn ranks_profiles_by_count_then_recent_launch_then_original_order() {
        let profiles = vec![profile("a"), profile("b"), profile("c"), profile("d")];
        let usage = HashMap::from([
            (
                "a".to_string(),
                ProfileUsage {
                    launch_count: 2,
                    last_launched_at: 100,
                },
            ),
            (
                "b".to_string(),
                ProfileUsage {
                    launch_count: 2,
                    last_launched_at: 200,
                },
            ),
            (
                "c".to_string(),
                ProfileUsage {
                    launch_count: 1,
                    last_launched_at: 300,
                },
            ),
        ]);

        let ranked = rank_profiles(profiles, &usage, 3);
        assert_eq!(
            ranked
                .iter()
                .map(|profile| profile.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );
    }
}
