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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseBehavior {
    #[default]
    Ask,
    HideToTray,
    Exit,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MinimizeBehavior {
    #[default]
    Taskbar,
    HideToTray,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_display_modes: Vec<DisplayMode>,
    pub minimize_behavior: MinimizeBehavior,
    pub close_behavior: CloseBehavior,
    pub confirm_close_behavior: bool,
    pub auto_switch_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_display_modes: Vec::new(),
            minimize_behavior: MinimizeBehavior::Taskbar,
            close_behavior: CloseBehavior::HideToTray,
            confirm_close_behavior: true,
            auto_switch_enabled: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAppSettings {
    #[serde(default)]
    default_display_modes: Vec<DisplayMode>,
    #[serde(default)]
    minimize_behavior: MinimizeBehavior,
    #[serde(default)]
    close_behavior: CloseBehavior,
    #[serde(default)]
    confirm_close_behavior: Option<bool>,
    #[serde(default)]
    auto_switch_enabled: bool,
}

impl<'de> Deserialize<'de> for AppSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let stored = StoredAppSettings::deserialize(deserializer)?;
        let legacy_ask = stored.close_behavior == CloseBehavior::Ask;
        Ok(Self {
            default_display_modes: stored.default_display_modes,
            minimize_behavior: stored.minimize_behavior,
            close_behavior: if legacy_ask {
                CloseBehavior::HideToTray
            } else {
                stored.close_behavior
            },
            confirm_close_behavior: stored.confirm_close_behavior.unwrap_or(legacy_ask),
            auto_switch_enabled: stored.auto_switch_enabled,
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationIconRequest {
    #[serde(default)]
    pub executable: String,
    #[serde(default)]
    pub shortcut_path: String,
    #[serde(default)]
    pub steam_app_id: String,
    #[serde(default)]
    pub process_name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationIcon {
    pub kind: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
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
    #[serde(default)]
    pub launcher_process_name: String,
    pub process_name: String,
    #[serde(default = "default_true")]
    pub auto_switch: bool,
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
    #[serde(default)]
    pub launcher_process_name: String,
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
    use super::{
        AppSettings, CloseBehavior, GameProfile, LaunchType, MinimizeBehavior, SwitchTiming,
    };

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
        assert!(profile.launcher_process_name.is_empty());
        assert!(profile.auto_switch);
    }

    #[test]
    fn old_launcher_profile_keeps_the_optional_process_empty() {
        let mut profile: GameProfile = serde_json::from_str(
            r#"{
                "id":"1",
                "name":"VALORANT",
                "launchType":"shortcut",
                "executable":"",
                "arguments":"",
                "steamAppId":"",
                "shortcutPath":"C:\\VALORANT.lnk",
                "launcherProvider":"Riot Client",
                "processName":"VALORANT-Win64-Shipping.exe",
                "displayDevice":"DISPLAY1",
                "width":1920,
                "height":1080,
                "refreshRate":144,
                "restoreDelayMs":2000
            }"#,
        )
        .unwrap();

        profile.normalize();

        assert!(profile.launcher_process_name.is_empty());
    }
    #[test]
    fn old_settings_keep_close_behavior_compatibility() {
        let unconfigured: AppSettings =
            serde_json::from_str(r#"{"defaultDisplayModes":[]}"#).unwrap();
        let configured: AppSettings =
            serde_json::from_str(r#"{"defaultDisplayModes":[],"closeBehavior":"exit"}"#).unwrap();
        let current: AppSettings = serde_json::from_str(
            r#"{"defaultDisplayModes":[],"closeBehavior":"hideToTray","confirmCloseBehavior":true,"autoSwitchEnabled":true}"#,
        )
        .unwrap();

        assert_eq!(unconfigured.close_behavior, CloseBehavior::HideToTray);
        assert_eq!(unconfigured.minimize_behavior, MinimizeBehavior::Taskbar);
        assert!(unconfigured.confirm_close_behavior);
        assert!(!unconfigured.auto_switch_enabled);
        assert_eq!(configured.close_behavior, CloseBehavior::Exit);
        assert_eq!(configured.minimize_behavior, MinimizeBehavior::Taskbar);
        assert!(!configured.confirm_close_behavior);
        assert_eq!(current.close_behavior, CloseBehavior::HideToTray);
        assert_eq!(current.minimize_behavior, MinimizeBehavior::Taskbar);
        assert!(current.confirm_close_behavior);
        assert!(current.auto_switch_enabled);
        assert_eq!(
            serde_json::to_string(&CloseBehavior::HideToTray).unwrap(),
            r#""hideToTray""#
        );
        assert_eq!(
            serde_json::to_string(&CloseBehavior::Exit).unwrap(),
            r#""exit""#
        );
    }
}
