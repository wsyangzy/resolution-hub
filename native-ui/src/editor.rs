use std::path::Path;

use resolution_hub_lib::{
    display,
    model::{
        ApplicationIconRequest, DetectLaunchRequest, DisplayInfo, DisplayMode, GameProfile,
        LaunchType, RecentApplication, SteamGame, SwitchTiming,
    },
    profile, recent, steam,
};

use crate::{NewGameDraft, SourceEntry};

// 顺序与原版 ProfileEditor 的来源标签一致；这是表单状态，不写入 JSON。
pub const STEAM: i32 = 0;
pub const SHORTCUT: i32 = 1;
pub const EXECUTABLE: i32 = 2;
pub const RECENT: i32 = 3;
pub const VALORANT_PROCESS: &str = "VALORANT-Win64-Shipping.exe";

pub struct DisplayChoice {
    pub info: DisplayInfo,
    pub modes: Vec<DisplayMode>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct EditorCatalog {
    pub displays: Vec<DisplayChoice>,
    pub steam_games: Vec<SteamGame>,
    pub steam_notice: String,
    pub recent_apps: Vec<RecentApplication>,
}

impl EditorCatalog {
    pub fn load() -> Result<Self, String> {
        let displays = display::enumerate_displays()?;
        if displays.is_empty() {
            return Err("没有可用的显示器，请连接显示器后重新读取".into());
        }
        let displays = displays
            .into_iter()
            .map(|info| {
                let (modes, error) = match display::enumerate_modes(&info.device_name) {
                    Ok(modes) => (modes, None),
                    Err(error) => (Vec::new(), Some(error)),
                };
                DisplayChoice { info, modes, error }
            })
            .collect();
        let mut catalog = Self {
            displays,
            // 与原版相同：打开新增表单时刷新最近应用历史。
            recent_apps: recent::list(),
            ..Default::default()
        };
        catalog.set_steam_games(steam::list_installed_games());
        Ok(catalog)
    }

    pub fn set_steam_games(&mut self, result: Result<Vec<SteamGame>, String>) {
        match result {
            Ok(games) => {
                self.steam_games = games;
                self.steam_notice.clear();
            }
            Err(error) => {
                self.steam_games.clear();
                self.steam_notice = format!("读取 Steam 游戏库失败：{error}");
            }
        }
    }

    fn default_mode_index(&self, display_index: i32) -> i32 {
        let Some(choice) = self.displays.get(display_index as usize) else {
            return -1;
        };
        choice
            .modes
            .iter()
            .position(|mode| {
                mode.width == choice.info.current_mode.width
                    && mode.height == choice.info.current_mode.height
                    && mode.refresh_rate == choice.info.current_mode.refresh_rate
            })
            .or_else(|| (!choice.modes.is_empty()).then_some(0))
            .map_or(-1, |index| index as i32)
    }
}

#[derive(Clone, Copy)]
pub enum FileChoice {
    Executable { replace_name: bool },
    Shortcut { riot_preset: bool },
}

pub enum DetectionAction {
    OpenLauncher(DetectLaunchRequest),
    Detect(DetectLaunchRequest),
}

pub enum DetectionResult {
    LauncherOpened(bool),
    Candidates(Vec<RecentApplication>),
}

pub struct SelectedSource {
    pub key: String,
    pub detail: String,
    pub kind: String,
}

pub struct EditorState {
    pub catalog_revision: u64,
    pub draft: NewGameDraft,
    initial: NewGameDraft,
    pub catalog: Option<EditorCatalog>,
    pub error: Option<String>,
    pub steam_search: String,
    pub selected_source: Option<SelectedSource>,
    pub launcher_ready: bool,
    pub detect_status: String,
    pub candidates: Vec<RecentApplication>,
}

impl Default for EditorState {
    fn default() -> Self {
        let draft = NewGameDraft {
            source_kind: STEAM,
            switch_timing: 1,
            wait_minutes: "10".into(),
            restore_seconds: "2".into(),
            display_index: -1,
            mode_index: -1,
            ..Default::default()
        };
        Self {
            catalog_revision: 0,
            initial: draft.clone(),
            draft,
            catalog: None,
            error: None,
            steam_search: String::new(),
            selected_source: None,
            launcher_ready: false,
            detect_status: String::new(),
            candidates: Vec::new(),
        }
    }
}

impl EditorState {
    pub fn dirty(&self) -> bool {
        self.draft != self.initial
    }

    pub fn load_catalog(&mut self, result: Result<EditorCatalog, String>) {
        self.catalog_revision = self.catalog_revision.wrapping_add(1);
        match result {
            Ok(catalog) => {
                let first_load = self.catalog.is_none();
                let previous = self.selected_display().map(|choice| {
                    (
                        choice.info.device_name.clone(),
                        choice.modes.get(self.draft.mode_index as usize).cloned(),
                    )
                });
                self.draft.display_index = catalog
                    .displays
                    .iter()
                    .position(|choice| {
                        previous
                            .as_ref()
                            .is_some_and(|(device, _)| device == &choice.info.device_name)
                    })
                    .or_else(|| {
                        catalog
                            .displays
                            .iter()
                            .position(|choice| choice.info.is_primary)
                    })
                    .unwrap_or(0) as i32;
                self.draft.mode_index = previous
                    .as_ref()
                    .and_then(|(_, mode)| mode.as_ref())
                    .and_then(|mode| {
                        catalog
                            .displays
                            .get(self.draft.display_index as usize)?
                            .modes
                            .iter()
                            .position(|candidate| candidate == mode)
                    })
                    .map_or_else(
                        || catalog.default_mode_index(self.draft.display_index),
                        |index| index as i32,
                    );
                self.catalog = Some(catalog);
                if first_load {
                    self.initial.display_index = self.draft.display_index;
                    self.initial.mode_index = self.draft.mode_index;
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn update(&mut self, mut draft: NewGameDraft) {
        if !(STEAM..=RECENT).contains(&draft.source_kind) {
            return;
        }
        if draft.source_kind != self.draft.source_kind {
            draft.switch_timing = if matches!(draft.source_kind, EXECUTABLE | RECENT) {
                0
            } else {
                1
            };
            self.selected_source = None;
            self.reset_detection();
        } else if draft.steam_app_id != self.draft.steam_app_id
            || draft.executable != self.draft.executable
            || draft.shortcut_path != self.draft.shortcut_path
        {
            if draft.source_kind == STEAM && !self.draft.steam_app_id.is_empty() {
                draft.process_name = "".into();
            }
            self.selected_source = None;
            self.reset_detection();
        } else if draft.launcher_process_name != self.draft.launcher_process_name {
            self.reset_detection();
        }
        if draft.display_index != self.draft.display_index {
            draft.mode_index = self.catalog.as_ref().map_or(-1, |catalog| {
                catalog.default_mode_index(draft.display_index)
            });
        }
        self.draft = draft;
        self.error = None;
    }

    fn reset_detection(&mut self) {
        self.launcher_ready = false;
        self.detect_status.clear();
        self.candidates.clear();
    }

    pub fn source_entries(&self) -> Vec<SourceEntry> {
        let Some(catalog) = &self.catalog else {
            return Vec::new();
        };
        let selected_key = self
            .selected_source
            .as_ref()
            .map(|source| source.key.as_str());
        match self.draft.source_kind {
            STEAM => {
                let query = self.steam_search.trim().to_lowercase();
                catalog
                    .steam_games
                    .iter()
                    .enumerate()
                    .filter(|(_, game)| {
                        game.name.to_lowercase().contains(&query) || game.app_id.contains(&query)
                    })
                    .map(|(index, game)| SourceEntry {
                        index: index as i32,
                        title: game.name.clone().into(),
                        detail: game.install_dir.clone().into(),
                        meta: game.app_id.clone().into(),
                        selected: selected_key == Some(format!("steam:{}", game.app_id).as_str()),
                        ..Default::default()
                    })
                    .collect()
            }
            RECENT => catalog
                .recent_apps
                .iter()
                .enumerate()
                .map(|(index, app)| SourceEntry {
                    index: index as i32,
                    title: app.name.clone().into(),
                    detail: app.executable.clone().into(),
                    meta: app.process_name.clone().into(),
                    selected: selected_key
                        == Some(format!("recent:{}", app.executable.to_lowercase()).as_str()),
                    ..Default::default()
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn source_notice(&self) -> String {
        let Some(catalog) = &self.catalog else {
            return "正在读取启动来源…".into();
        };
        if self.draft.source_kind == STEAM {
            if !catalog.steam_notice.is_empty() {
                return catalog.steam_notice.clone();
            }
            if catalog.steam_games.is_empty() {
                return "没有发现已安装的 Steam 游戏，可刷新或在高级设置中填写 App ID。".into();
            }
            let query = self.steam_search.trim().to_lowercase();
            if !catalog.steam_games.iter().any(|game| {
                game.name.to_lowercase().contains(&query) || game.app_id.contains(&query)
            }) {
                return "没有匹配的 Steam 游戏".into();
            }
        } else if self.draft.source_kind == RECENT && catalog.recent_apps.is_empty() {
            return "暂时没有最近活动的应用，请先打开游戏后刷新。".into();
        }
        String::new()
    }

    pub fn source_icon_request(&self, index: usize) -> Option<ApplicationIconRequest> {
        let catalog = self.catalog.as_ref()?;
        match self.draft.source_kind {
            STEAM => Some(ApplicationIconRequest {
                steam_app_id: catalog.steam_games.get(index)?.app_id.clone(),
                ..Default::default()
            }),
            RECENT => Some(ApplicationIconRequest {
                executable: catalog.recent_apps.get(index)?.executable.clone(),
                ..Default::default()
            }),
            _ => None,
        }
    }

    pub fn select_source(&mut self, index: usize) {
        match self.draft.source_kind {
            STEAM => {
                let Some(game) = self
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.steam_games.get(index))
                    .cloned()
                else {
                    return;
                };
                self.draft.name = game.name.into();
                self.draft.steam_app_id = game.app_id.clone().into();
                self.draft.executable = "".into();
                self.draft.shortcut_path = "".into();
                self.draft.process_name = "".into();
                self.draft.switch_timing = 1;
                self.selected_source = Some(SelectedSource {
                    key: format!("steam:{}", game.app_id),
                    detail: format!("{} · App ID {}", game.install_dir, game.app_id),
                    kind: "Steam".into(),
                });
            }
            RECENT => {
                let Some(app) = self
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.recent_apps.get(index))
                    .cloned()
                else {
                    return;
                };
                self.draft.name = app.name.into();
                self.draft.executable = app.executable.clone().into();
                self.draft.process_name = app.process_name.into();
                self.draft.steam_app_id = "".into();
                self.draft.shortcut_path = "".into();
                self.draft.switch_timing = 0;
                self.selected_source = Some(SelectedSource {
                    key: format!("recent:{}", app.executable.to_lowercase()),
                    detail: app.executable,
                    kind: "最近运行".into(),
                });
            }
            _ => return,
        }
        self.draft.arguments = "".into();
        self.draft.working_directory = "".into();
        self.draft.launcher_provider = "".into();
        self.draft.launcher_process_name = "".into();
        self.reset_detection();
        self.error = None;
    }

    pub fn apply_file(&mut self, path: String, choice: FileChoice) {
        match choice {
            FileChoice::Executable { replace_name } => {
                self.draft.source_kind = EXECUTABLE;
                if replace_name || self.draft.name.trim().is_empty() {
                    self.draft.name = file_stem(&path).into();
                }
                self.draft.executable = path.clone().into();
                self.draft.process_name = file_name(&path).into();
                self.draft.steam_app_id = "".into();
                self.draft.shortcut_path = "".into();
                self.draft.launcher_provider = "".into();
                self.draft.switch_timing = 0;
                self.selected_source = Some(SelectedSource {
                    key: format!("executable:{}", path.to_lowercase()),
                    detail: path,
                    kind: "EXE".into(),
                });
            }
            FileChoice::Shortcut { riot_preset } => {
                self.draft.source_kind = SHORTCUT;
                self.draft.name = if riot_preset {
                    "无畏契约".into()
                } else {
                    file_stem(&path).into()
                };
                self.draft.shortcut_path = path.clone().into();
                self.draft.executable = "".into();
                self.draft.steam_app_id = "".into();
                self.draft.arguments = "".into();
                self.draft.working_directory = "".into();
                self.draft.launcher_provider = if riot_preset {
                    "Riot Client"
                } else {
                    "启动器"
                }
                .into();
                // 普通快捷方式可能指向登录器，不能据此推断游戏主进程。
                self.draft.process_name = if riot_preset {
                    VALORANT_PROCESS.into()
                } else {
                    "".into()
                };
                if riot_preset {
                    self.draft.wait_minutes = "10".into();
                }
                self.draft.switch_timing = 1;
                self.selected_source = Some(SelectedSource {
                    key: format!("shortcut:{}", path.to_lowercase()),
                    detail: path,
                    kind: if riot_preset {
                        "Riot Client"
                    } else {
                        "启动器"
                    }
                    .into(),
                });
            }
        }
        self.draft.launcher_process_name = "".into();
        self.reset_detection();
        self.error = None;
    }

    pub fn detection_action(&self) -> Result<DetectionAction, String> {
        let launch_type = self.launch_type()?;
        let request = DetectLaunchRequest {
            launch_type,
            executable: clean_path(&self.draft.executable),
            arguments: self.draft.arguments.trim().into(),
            working_directory: clean_path(&self.draft.working_directory),
            steam_app_id: self.draft.steam_app_id.trim().into(),
            shortcut_path: clean_path(&self.draft.shortcut_path),
            launcher_process_name: file_name(&clean_path(&self.draft.launcher_process_name)),
            launch_before_detect: !matches!(launch_type, LaunchType::Shortcut),
        };
        if matches!(launch_type, LaunchType::Shortcut) && !self.launcher_ready {
            Ok(DetectionAction::OpenLauncher(request))
        } else {
            Ok(DetectionAction::Detect(request))
        }
    }

    pub fn finish_detection(&mut self, result: Result<DetectionResult, String>) {
        match result {
            Ok(DetectionResult::LauncherOpened(launched)) => {
                self.launcher_ready = true;
                self.detect_status = if launched {
                    "启动器已打开。完成登录和更新后，点击“开始识别”，再从启动器进入游戏。"
                } else {
                    "检测到启动器已在运行。点击“开始识别”，再从现有启动器进入游戏。"
                }
                .into();
            }
            Ok(DetectionResult::Candidates(apps)) if apps.len() == 1 => {
                self.draft.process_name = apps[0].process_name.clone().into();
                self.detect_status = format!("已检测到 {}", self.draft.process_name);
            }
            Ok(DetectionResult::Candidates(apps)) => {
                self.detect_status = if apps.is_empty() {
                    "未检测到新的游戏进程，请确认游戏已启动后重试。"
                } else {
                    "检测到多个候选进程，请选择实际游戏的主进程。"
                }
                .into();
                self.candidates = apps;
            }
            Err(error) => {
                self.detect_status.clear();
                self.error = Some(format!("识别游戏进程失败：{error}"));
            }
        }
    }

    pub fn select_candidate(&mut self, index: usize) {
        if let Some(app) = self.candidates.get(index) {
            self.draft.process_name = app.process_name.clone().into();
            self.detect_status = format!("已选择 {}", app.process_name);
            self.candidates.clear();
            self.error = None;
        }
    }

    fn launch_type(&self) -> Result<LaunchType, String> {
        match self.draft.source_kind {
            STEAM
                if self
                    .draft
                    .steam_app_id
                    .trim()
                    .parse::<u32>()
                    .is_ok_and(|id| id > 0)
                    && self
                        .draft
                        .steam_app_id
                        .trim()
                        .bytes()
                        .all(|b| b.is_ascii_digit()) =>
            {
                Ok(LaunchType::Steam)
            }
            SHORTCUT
                if has_extension(&clean_path(&self.draft.shortcut_path), "lnk")
                    || has_extension(&clean_path(&self.draft.shortcut_path), "url") =>
            {
                Ok(LaunchType::Shortcut)
            }
            EXECUTABLE | RECENT if has_extension(&clean_path(&self.draft.executable), "exe") => {
                Ok(LaunchType::Executable)
            }
            STEAM => Err("请先选择 Steam 游戏，或在高级设置中填写有效的数字 App ID".into()),
            SHORTCUT => Err("请先选择启动器或游戏的 .lnk / .url 快捷方式".into()),
            EXECUTABLE | RECENT => Err("请先选择游戏 EXE 或最近运行的游戏".into()),
            _ => Err("请选择有效的启动来源".into()),
        }
    }

    pub fn selected_display(&self) -> Option<&DisplayChoice> {
        self.catalog
            .as_ref()?
            .displays
            .get(self.draft.display_index as usize)
    }

    pub fn message(&self) -> String {
        self.error
            .as_ref()
            .or_else(|| {
                self.selected_display()
                    .and_then(|choice| choice.error.as_ref())
            })
            .cloned()
            .unwrap_or_default()
    }

    pub fn to_profile(&self) -> Result<GameProfile, String> {
        let draft = &self.draft;
        if draft.name.trim().is_empty() {
            return Err("请填写游戏名称".into());
        }
        let launch_type = self.launch_type()?;
        let process_name = file_name(&clean_path(&draft.process_name));
        if !has_extension(&process_name, "exe") {
            return Err("请填写游戏主进程名（含 .exe），或使用进程识别".into());
        }
        let choice = self.selected_display().ok_or("请选择目标显示器")?;
        let mode = choice
            .modes
            .get(draft.mode_index as usize)
            .ok_or("请选择该显示器支持的显示模式")?;
        let launch_timeout_sec = parse_duration(
            &draft.wait_minutes,
            1.0,
            60.0,
            60.0,
            "等待时间需为 1～60 分钟",
        )?;
        let restore_delay_ms = parse_duration(
            &draft.restore_seconds,
            0.0,
            30.0,
            1000.0,
            "恢复延迟需为 0～30 秒",
        )?;
        let switch_timing = match draft.switch_timing {
            0 => Some(SwitchTiming::BeforeLaunch),
            1 => Some(SwitchTiming::OnProcessDetected),
            _ => return Err("请选择有效的切换时机".into()),
        };
        let shortcut = matches!(launch_type, LaunchType::Shortcut);
        let executable = matches!(launch_type, LaunchType::Executable);
        let launcher_process_name = if shortcut {
            let name = file_name(&clean_path(&draft.launcher_process_name));
            if !name.is_empty() && !has_extension(&name, "exe") {
                return Err("启动器进程名需包含 .exe，或留空自动解析快捷方式".into());
            }
            name
        } else {
            String::new()
        };
        Ok(GameProfile {
            id: String::new(),
            name: draft.name.trim().into(),
            launch_type,
            executable: if executable {
                clean_path(&draft.executable)
            } else {
                String::new()
            },
            steam_app_id: if matches!(launch_type, LaunchType::Steam) {
                draft.steam_app_id.trim().into()
            } else {
                String::new()
            },
            shortcut_path: if shortcut {
                clean_path(&draft.shortcut_path)
            } else {
                String::new()
            },
            launcher_provider: if shortcut {
                if draft.launcher_provider == "Riot Client" {
                    "Riot Client"
                } else {
                    "启动器"
                }
                .into()
            } else {
                String::new()
            },
            launcher_process_name,
            process_name,
            arguments: if executable {
                draft.arguments.trim().into()
            } else {
                String::new()
            },
            working_directory: if executable {
                clean_path(&draft.working_directory)
            } else {
                String::new()
            },
            switch_timing,
            launch_timeout_sec,
            display_device: choice.info.device_name.clone(),
            width: mode.width,
            height: mode.height,
            refresh_rate: mode.refresh_rate,
            restore_delay_ms,
        })
    }
}

pub fn save(profile: GameProfile) -> Result<GameProfile, String> {
    if !profile.working_directory.is_empty() && !Path::new(&profile.working_directory).is_dir() {
        return Err("工作目录不存在，请重新选择或留空".into());
    }
    // 保存只写配置，不启动游戏、不切换显示模式。
    let modes = display::enumerate_modes(&profile.display_device)?;
    if !mode_is_supported(&profile, &modes) {
        return Err("所选显示模式已不可用，请重新读取显示器并选择模式".into());
    }
    profile::save(profile)
}

fn mode_is_supported(profile: &GameProfile, modes: &[DisplayMode]) -> bool {
    modes.iter().any(|mode| {
        mode.device_name == profile.display_device
            && mode.width == profile.width
            && mode.height == profile.height
            && mode.refresh_rate == profile.refresh_rate
            && mode.bits_per_pixel >= 32
    })
}

fn parse_duration(
    value: &str,
    minimum: f64,
    maximum: f64,
    multiplier: f64,
    message: &str,
) -> Result<u64, String> {
    let value = value
        .trim()
        .parse::<f64>()
        .map_err(|_| message.to_string())?;
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(message.into());
    }
    Ok((value * multiplier).round() as u64)
}

fn clean_path(value: &str) -> String {
    value.trim().trim_matches('"').trim().into()
}
fn file_name(value: &str) -> String {
    Path::new(value)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
fn file_stem(value: &str) -> String {
    Path::new(value)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
fn has_extension(value: &str, extension: &str) -> bool {
    Path::new(value)
        .extension()
        .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(device: &str, width: u32, refresh_rate: u32) -> DisplayMode {
        DisplayMode {
            device_name: device.into(),
            width,
            height: 1080,
            refresh_rate,
            bits_per_pixel: 32,
        }
    }

    fn application(name: &str) -> RecentApplication {
        RecentApplication {
            pid: 42,
            name: name.into(),
            executable: format!(r"C:\Games\{name}.exe"),
            process_name: format!("{name}.exe"),
            last_seen: 1,
        }
    }

    fn catalog() -> EditorCatalog {
        let primary = mode("primary", 1920, 144);
        let secondary = mode("secondary", 2560, 60);
        EditorCatalog {
            displays: vec![
                DisplayChoice {
                    info: DisplayInfo {
                        device_name: "primary".into(),
                        friendly_name: "主显示器".into(),
                        is_primary: true,
                        current_mode: primary.clone(),
                    },
                    modes: vec![primary, mode("primary", 1600, 120)],
                    error: None,
                },
                DisplayChoice {
                    info: DisplayInfo {
                        device_name: "secondary".into(),
                        friendly_name: "副显示器".into(),
                        is_primary: false,
                        current_mode: secondary.clone(),
                    },
                    modes: vec![secondary],
                    error: None,
                },
            ],
            steam_games: vec![
                SteamGame {
                    app_id: "100".into(),
                    name: "游戏 A".into(),
                    install_dir: "A".into(),
                },
                SteamGame {
                    app_id: "200".into(),
                    name: "游戏 B".into(),
                    install_dir: "B".into(),
                },
            ],
            recent_apps: vec![application("game")],
            ..Default::default()
        }
    }

    fn ready() -> EditorState {
        let mut editor = EditorState::default();
        editor.load_catalog(Ok(catalog()));
        editor
    }

    fn executable() -> EditorState {
        let mut editor = ready();
        editor.apply_file(
            r"C:\My Games\game.exe".into(),
            FileChoice::Executable { replace_name: true },
        );
        editor
    }

    #[test]
    fn new_editor_defaults_to_steam_without_dirtying_the_draft() {
        let editor = ready();
        assert_eq!(editor.draft.source_kind, STEAM);
        assert_eq!(editor.draft.switch_timing, 1);
        assert!(!editor.dirty());
    }

    #[test]
    fn executable_selection_fills_the_original_defaults() {
        let profile = executable().to_profile().unwrap();
        assert!(profile.id.is_empty());
        assert_eq!(profile.name, "game");
        assert_eq!(profile.process_name, "game.exe");
        assert_eq!(profile.executable, r"C:\My Games\game.exe");
        assert!(profile.steam_app_id.is_empty() && profile.shortcut_path.is_empty());
        assert!(matches!(
            profile.switch_timing,
            Some(SwitchTiming::BeforeLaunch)
        ));
        assert_eq!(
            (profile.launch_timeout_sec, profile.restore_delay_ms),
            (600, 2000)
        );
        assert_eq!((profile.width, profile.refresh_rate), (1920, 144));
    }

    #[test]
    fn valorant_preset_sets_provider_main_process_and_waiting_policy() {
        let mut editor = executable();
        editor.draft.wait_minutes = "3".into();
        editor.apply_file(
            r"C:\Games\VALORANT.lnk".into(),
            FileChoice::Shortcut { riot_preset: true },
        );
        let profile = editor.to_profile().unwrap();
        assert_eq!(profile.name, "无畏契约");
        assert_eq!(profile.launcher_provider, "Riot Client");
        assert_eq!(profile.process_name, VALORANT_PROCESS);
        assert_eq!(profile.launch_timeout_sec, 600);
        assert!(matches!(
            profile.switch_timing,
            Some(SwitchTiming::OnProcessDetected)
        ));
        assert!(profile.launcher_process_name.is_empty());
        assert!(profile.executable.is_empty());
    }

    #[test]
    fn other_launcher_clears_a_previous_game_process_and_keeps_reuse_separate() {
        let mut editor = executable();
        editor.apply_file(
            r"C:\Games\Launcher.url".into(),
            FileChoice::Shortcut { riot_preset: false },
        );
        assert!(editor.draft.process_name.is_empty());
        assert!(editor.to_profile().unwrap_err().contains("主进程"));
        editor.draft.process_name = "actual-game.exe".into();
        editor.draft.launcher_process_name = " Launcher.exe ".into();
        let profile = editor.to_profile().unwrap();
        assert_eq!(profile.process_name, "actual-game.exe");
        assert_eq!(profile.launcher_process_name, "Launcher.exe");
        assert_eq!(profile.launcher_provider, "启动器");
    }

    #[test]
    fn steam_search_preserves_catalog_indices_and_clears_the_previous_game_process() {
        let mut editor = ready();
        editor.select_source(0);
        editor.draft.process_name = "a.exe".into();
        editor.steam_search = "200".into();
        let entries = editor.source_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].index, 1);
        editor.select_source(entries[0].index as usize);
        assert_eq!(editor.draft.name, "游戏 B");
        assert_eq!(editor.draft.steam_app_id, "200");
        assert!(editor.draft.process_name.is_empty());
        let DetectionAction::Detect(request) = editor.detection_action().unwrap() else {
            panic!("Steam should launch and detect")
        };
        assert!(request.launch_before_detect);
        assert_eq!(request.steam_app_id, "200");
    }

    #[test]
    fn recent_application_is_saved_as_an_executable_profile() {
        let mut editor = ready();
        let mut draft = editor.draft.clone();
        draft.source_kind = RECENT;
        editor.update(draft);
        editor.select_source(0);
        let profile = editor.to_profile().unwrap();
        assert!(matches!(profile.launch_type, LaunchType::Executable));
        assert_eq!(profile.executable, r"C:\Games\game.exe");
        assert_eq!(profile.process_name, "game.exe");
        assert!(matches!(
            profile.switch_timing,
            Some(SwitchTiming::BeforeLaunch)
        ));
    }

    #[test]
    fn launcher_detection_requires_opening_then_detecting_without_relaunch() {
        let mut editor = ready();
        editor.apply_file(
            r"C:\Games\Launcher.lnk".into(),
            FileChoice::Shortcut { riot_preset: false },
        );
        assert!(matches!(
            editor.detection_action().unwrap(),
            DetectionAction::OpenLauncher(_)
        ));
        editor.finish_detection(Ok(DetectionResult::LauncherOpened(false)));
        assert!(editor.detect_status.contains("已在运行"));
        let DetectionAction::Detect(request) = editor.detection_action().unwrap() else {
            panic!("launcher should already be ready")
        };
        assert!(!request.launch_before_detect);
        editor.finish_detection(Ok(DetectionResult::Candidates(vec![
            application("launcher"),
            application("actual-game"),
        ])));
        assert!(editor.draft.process_name.is_empty());
        editor.select_candidate(1);
        assert_eq!(editor.draft.process_name, "actual-game.exe");
        assert!(editor.candidates.is_empty());
    }

    #[test]
    fn a_single_detected_process_is_applied_and_a_new_source_invalidates_readiness() {
        let mut editor = ready();
        editor.apply_file(
            r"C:\Games\Launcher.lnk".into(),
            FileChoice::Shortcut { riot_preset: false },
        );
        editor.finish_detection(Ok(DetectionResult::LauncherOpened(true)));
        editor.finish_detection(Ok(DetectionResult::Candidates(vec![application(
            "actual-game",
        )])));
        assert_eq!(editor.draft.process_name, "actual-game.exe");
        let mut draft = editor.draft.clone();
        draft.shortcut_path = r"C:\Games\Another.lnk".into();
        editor.update(draft);
        assert!(!editor.launcher_ready);
        assert!(editor.detect_status.is_empty());
        assert!(matches!(
            editor.detection_action().unwrap(),
            DetectionAction::OpenLauncher(_)
        ));
    }

    #[test]
    fn source_changes_restore_recommended_timing_and_preserve_user_name_on_advanced_browse() {
        let mut editor = executable();
        editor.draft.name = "自定义名称".into();
        editor.apply_file(
            r"C:\Games\another.exe".into(),
            FileChoice::Executable {
                replace_name: false,
            },
        );
        assert_eq!(editor.draft.name, "自定义名称");
        let mut draft = editor.draft.clone();
        draft.source_kind = SHORTCUT;
        editor.update(draft);
        assert_eq!(editor.draft.switch_timing, 1);
        assert!(editor.selected_source.is_none());
    }

    #[test]
    fn durations_convert_units_and_reject_invalid_values() {
        let mut editor = executable();
        editor.draft.wait_minutes = "1.5".into();
        editor.draft.restore_seconds = "0.25".into();
        let profile = editor.to_profile().unwrap();
        assert_eq!(
            (profile.launch_timeout_sec, profile.restore_delay_ms),
            (90, 250)
        );
        for value in ["", "NaN", "inf", "0", "61"] {
            editor.draft.wait_minutes = value.into();
            assert!(editor.to_profile().is_err(), "{value}");
        }
        editor.draft.wait_minutes = "60".into();
        for value in ["", "NaN", "-1", "31"] {
            editor.draft.restore_seconds = value.into();
            assert!(editor.to_profile().is_err(), "{value}");
        }
    }

    #[test]
    fn display_refresh_preserves_a_supported_mode_and_rejects_a_stale_mode() {
        let mut editor = executable();
        editor.draft.mode_index = 1;
        let profile = editor.to_profile().unwrap();
        editor.load_catalog(Ok(catalog()));
        assert_eq!(editor.draft.mode_index, 1);
        assert!(mode_is_supported(&profile, &catalog().displays[0].modes));
        assert!(!mode_is_supported(&profile, &catalog().displays[1].modes));
        let mut draft = editor.draft.clone();
        draft.display_index = 1;
        editor.update(draft);
        assert_eq!(editor.draft.mode_index, 0);
        assert_eq!(editor.to_profile().unwrap().display_device, "secondary");
        editor.draft.mode_index = 999;
        assert!(editor.to_profile().is_err());
    }

    #[test]
    fn malformed_sources_are_rejected_before_detection_or_save() {
        let mut editor = ready();
        editor.draft.name = "game".into();
        for value in ["", "0", "-3", "abc", "4294967296"] {
            editor.draft.steam_app_id = value.into();
            assert!(editor.detection_action().is_err());
            assert!(editor.to_profile().is_err());
        }
        editor.draft.source_kind = SHORTCUT;
        editor.draft.shortcut_path = "launcher.exe".into();
        assert!(editor.detection_action().is_err());
    }
}
