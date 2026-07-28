use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayMode {
    pub device_name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32,
    pub bits_per_pixel: u32,
}

impl DisplayMode {
    pub fn label(&self) -> String {
        format!("{}x{} @ {} Hz", self.width, self.height, self.refresh_rate)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub device_name: String,
    pub friendly_name: String,
    pub is_primary: bool,
    pub current_mode: DisplayMode,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub default_display_modes: Vec<DisplayMode>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchType {
    Executable,
    Steam,
    Shortcut,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SwitchTiming {
    BeforeLaunch,
    OnProcessDetected,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameProfile {
    pub id: String,
    pub name: String,
    pub launch_type: LaunchType,
    pub executable: String,
    pub arguments: String,
    #[serde(default)]
    pub working_directory: String,
    pub steam_app_id: String,
    #[serde(default)]
    pub shortcut_path: String,
    #[serde(default)]
    pub launcher_provider: String,
    pub process_name: String,
    #[serde(default)]
    pub switch_timing: Option<SwitchTiming>,
    #[serde(default = "default_launch_timeout_sec")]
    pub launch_timeout_sec: u64,
    pub display_device: String,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32,
    pub restore_delay_ms: u64,
}

impl GameProfile {
    pub fn normalize(&mut self) {
        if self.switch_timing.is_none() {
            self.switch_timing = Some(match self.launch_type {
                LaunchType::Executable => SwitchTiming::BeforeLaunch,
                LaunchType::Steam | LaunchType::Shortcut => SwitchTiming::OnProcessDetected,
            });
        }
        if self.launch_timeout_sec == 0 {
            self.launch_timeout_sec = default_launch_timeout_sec();
        }
    }

    pub fn effective_switch_timing(&self) -> SwitchTiming {
        self.switch_timing.unwrap_or(match self.launch_type {
            LaunchType::Executable => SwitchTiming::BeforeLaunch,
            LaunchType::Steam | LaunchType::Shortcut => SwitchTiming::OnProcessDetected,
        })
    }
}

const fn default_launch_timeout_sec() -> u64 {
    600
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamGame {
    pub app_id: String,
    pub name: String,
    pub install_dir: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentApplication {
    #[serde(default)]
    pub pid: u32,
    pub name: String,
    pub executable: String,
    pub process_name: String,
    pub last_seen: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectLaunchRequest {
    pub launch_type: LaunchType,
    pub executable: String,
    pub arguments: String,
    #[serde(default)]
    pub working_directory: String,
    pub steam_app_id: String,
    #[serde(default)]
    pub shortcut_path: String,
    #[serde(default = "default_true")]
    pub launch_before_detect: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRestore {
    pub profile_name: String,
    pub mode: DisplayMode,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub phase: String,
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub message: String,
    pub can_restore: bool,
    pub can_cancel: bool,
}

#[cfg(test)]
mod tests {
    use super::{GameProfile, LaunchType, SwitchTiming};

    #[test]
    fn launch_type_uses_frontend_names() {
        assert_eq!(
            serde_json::to_string(&LaunchType::Executable).unwrap(),
            r#""executable""#
        );
        assert_eq!(
            serde_json::to_string(&LaunchType::Steam).unwrap(),
            r#""steam""#
        );
        assert_eq!(
            serde_json::to_string(&LaunchType::Shortcut).unwrap(),
            r#""shortcut""#
        );
    }

    #[test]
    fn old_steam_profile_uses_process_detection() {
        let mut profile: GameProfile = serde_json::from_str(
            r#"{
                "id":"1",
                "name":"Old Steam Game",
                "launchType":"steam",
                "executable":"",
                "arguments":"",
                "steamAppId":"730",
                "processName":"game.exe",
                "displayDevice":"DISPLAY1",
                "width":1920,
                "height":1080,
                "refreshRate":144,
                "restoreDelayMs":2000
            }"#,
        )
        .unwrap();

        profile.normalize();
        assert!(matches!(
            profile.switch_timing,
            Some(SwitchTiming::OnProcessDetected)
        ));
        assert_eq!(profile.launch_timeout_sec, 600);
    }
}
