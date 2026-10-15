use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use resolution_hub_core::model::{GameProfile, PendingRestore, RuntimeStatus};

use crate::editor::{DetectionAction, DetectionResult, EditorState};
use crate::preferences::ViewMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Operation {
    #[default]
    Idle,
    Refreshing,
    LoadingEditor,
    LoadingSources,
    ChoosingFile,
    OpeningLauncher,
    DetectingProcess,
    CancellingDetection,
    SavingProfile,
    Starting,
    Restoring,
    Closing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum Panel {
    #[default]
    Hidden,
    Library,
    Details,
    Status,
    Editor,
    DiscardConfirmation,
    CloseConfirmation,
    Appearance,
}

impl Panel {
    pub fn dismiss_on_deactivate(self) -> bool {
        matches!(
            self,
            Self::Library | Self::Details | Self::Status | Self::Appearance
        )
    }
}

pub struct DetectionTask {
    pub generation: u64,
    pub cancelled: Arc<AtomicBool>,
    pub action: DetectionAction,
}

struct ActiveDetection {
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Default)]
pub enum Recovery {
    #[default]
    Unchecked,
    Clear,
    Pending(PendingRestore),
    Unavailable(String),
}

impl Recovery {
    pub fn from_result(result: Result<Option<PendingRestore>, String>) -> Self {
        match result {
            Ok(Some(pending)) => Self::Pending(pending),
            Ok(None) => Self::Clear,
            Err(error) => Self::Unavailable(error),
        }
    }

    pub fn needs_restore(&self) -> bool {
        !matches!(self, Self::Clear)
    }
}

pub struct AppState {
    pub view_mode: ViewMode,
    pub appearance_notice: String,
    pub profiles_revision: u64,
    pub profiles: Vec<GameProfile>,
    selected_id: Option<String>,
    pub runtime: RuntimeStatus,
    pub recovery: Recovery,
    pub operation: Operation,
    pub close_confirmation: bool,
    pub notice: Option<String>,
    pub display_summary: String,
    pub editor: Option<EditorState>,
    pub discard_confirmation: bool,
    panel: Panel,
    profiles_loaded: bool,
    detection_generation: u64,
    detection: Option<ActiveDetection>,
}

impl AppState {
    pub fn new(runtime: RuntimeStatus) -> Self {
        Self {
            view_mode: ViewMode::Toolbar,
            appearance_notice: String::new(),
            profiles_revision: 0,
            profiles: Vec::new(),
            selected_id: None,
            runtime,
            recovery: Recovery::Unchecked,
            operation: Operation::Idle,
            close_confirmation: false,
            notice: None,
            display_summary: "正在读取显示器".to_string(),
            editor: None,
            discard_confirmation: false,
            panel: Panel::Hidden,
            profiles_loaded: false,
            detection_generation: 0,
            detection: None,
        }
    }

    pub fn session_active(&self) -> bool {
        !matches!(self.runtime.phase.as_str(), "idle" | "error")
    }

    pub fn visible_panel(&self) -> Panel {
        if self.close_confirmation {
            Panel::CloseConfirmation
        } else if self.discard_confirmation {
            Panel::DiscardConfirmation
        } else if self.editor.is_some() {
            Panel::Editor
        } else {
            self.panel
        }
    }

    pub fn can_open_panel(&self) -> bool {
        self.editor.is_none()
            && !self.close_confirmation
            && !self.discard_confirmation
            && self.operation != Operation::Closing
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) -> bool {
        if !self.can_open_panel() || self.view_mode == mode {
            return false;
        }
        self.view_mode = mode;
        self.panel = Panel::Hidden;
        true
    }

    pub fn toggle_panel(&mut self, panel: Panel) {
        if !self.can_open_panel()
            || !panel.dismiss_on_deactivate()
            || (panel == Panel::Details && self.selected_profile().is_none())
        {
            return;
        }
        let same_content = self.panel == panel
            || (matches!(self.panel, Panel::Details | Panel::Status)
                && matches!(panel, Panel::Details | Panel::Status));
        self.panel = if same_content {
            Panel::Hidden
        } else if self.view_mode == ViewMode::Window && panel == Panel::Library {
            Panel::Hidden
        } else {
            panel
        };
    }

    pub fn show_details(&mut self, index: usize) {
        if self.can_open_panel() {
            self.select_profile(index);
            if self.selected_index() == Some(index) {
                self.panel = Panel::Details;
            }
        }
    }

    pub fn activate_profile(&mut self, index: usize) {
        if self.can_browse() && index < self.profiles.len() {
            self.select_profile(index);
            self.panel = Panel::Hidden;
        }
    }

    pub fn dismiss_panel(&mut self) {
        if self.operation == Operation::Closing {
            return;
        }
        if self.close_confirmation {
            self.close_confirmation = false;
        } else if self.discard_confirmation {
            self.discard_confirmation = false;
        } else if self.editor.is_some() {
            if self.operation == Operation::DetectingProcess {
                self.cancel_detection();
            } else {
                self.cancel_editor();
            }
        } else {
            self.panel = Panel::Hidden;
        }
    }

    pub fn dismiss_inactive_panel(&mut self) {
        // 登录启动器、选择文件和确认退出时，失焦不能隐藏草稿或跳过保护。
        if self.visible_panel().dismiss_on_deactivate() && self.operation != Operation::Closing {
            self.panel = Panel::Hidden;
        }
    }

    pub fn can_browse(&self) -> bool {
        self.operation == Operation::Idle
            && !self.session_active()
            && !self.close_confirmation
            && self.editor.is_none()
            && !self.discard_confirmation
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.profiles
            .iter()
            .position(|profile| Some(&profile.id) == self.selected_id.as_ref())
    }

    pub fn selected_profile(&self) -> Option<&GameProfile> {
        self.selected_index().map(|index| &self.profiles[index])
    }

    pub fn select_profile(&mut self, index: usize) {
        if self.can_browse()
            && let Some(profile) = self.profiles.get(index)
        {
            self.selected_id = Some(profile.id.clone());
        }
    }

    pub fn replace_profiles(&mut self, result: Result<Vec<GameProfile>, String>) {
        match result {
            Ok(profiles) => {
                self.profiles_revision = self.profiles_revision.wrapping_add(1);
                self.profiles = profiles;
                self.profiles_loaded = true;
                if self.selected_index().is_none() {
                    self.selected_id = self.profiles.first().map(|profile| profile.id.clone());
                }
            }
            Err(error) => {
                // 保留上次读到的列表供查看，但不允许用读取失败后的旧数据启动。
                self.profiles_loaded = false;
                self.notice = Some(error);
            }
        }
    }

    pub fn can_launch(&self) -> bool {
        self.can_browse()
            && self.profiles_loaded
            && self.selected_profile().is_some()
            && !self.recovery.needs_restore()
            && !self.runtime.can_restore
    }

    pub fn needs_restore(&self) -> bool {
        self.session_active() || self.runtime.can_restore || self.recovery.needs_restore()
    }

    pub fn can_restore(&self) -> bool {
        self.operation == Operation::Idle
            && !self.close_confirmation
            && self.editor.is_none()
            && self.needs_restore()
    }

    pub fn can_add(&self) -> bool {
        self.can_browse() && self.profiles_loaded && !self.needs_restore()
    }

    pub fn can_edit(&self) -> bool {
        self.operation == Operation::Idle
            && self.editor.is_some()
            && !self.close_confirmation
            && !self.discard_confirmation
    }

    pub fn begin_editor_load(&mut self) -> bool {
        if !self.can_add() && !self.can_edit() {
            return false;
        }
        let editor = self.editor.get_or_insert_with(EditorState::default);
        editor.error = None;
        self.operation = Operation::LoadingEditor;
        self.panel = Panel::Hidden;
        true
    }

    pub fn cancel_editor(&mut self) {
        if !self.can_edit() {
            return;
        }
        if self.editor.as_ref().is_some_and(EditorState::dirty) {
            self.discard_confirmation = true;
        } else {
            self.editor = None;
        }
    }

    pub fn discard_editor(&mut self) {
        if self.operation == Operation::Idle && self.discard_confirmation {
            self.editor = None;
            self.discard_confirmation = false;
        }
    }

    pub fn begin_file_choice(&mut self) -> bool {
        if !self.can_edit() {
            return false;
        }
        self.operation = Operation::ChoosingFile;
        true
    }

    pub fn begin_source_refresh(&mut self) -> Option<i32> {
        if !self.can_edit() {
            return None;
        }
        let kind = self.editor.as_ref()?.draft.source_kind;
        if !matches!(kind, crate::editor::STEAM | crate::editor::RECENT) {
            return None;
        }
        self.operation = Operation::LoadingSources;
        Some(kind)
    }

    pub fn begin_detection(&mut self) -> Option<DetectionTask> {
        if !self.can_edit() {
            return None;
        }
        if self.needs_restore() {
            self.editor.as_mut()?.error = Some("请先返回主界面恢复桌面，再识别游戏进程".into());
            return None;
        }
        let editor = self.editor.as_mut()?;
        let action = match editor.detection_action() {
            Ok(action) => action,
            Err(error) => {
                editor.error = Some(error);
                return None;
            }
        };
        editor.error = None;
        editor.candidates.clear();
        self.operation = match &action {
            DetectionAction::OpenLauncher(_) => {
                editor.detect_status = "正在打开启动器…".into();
                Operation::OpeningLauncher
            }
            DetectionAction::Detect(request) => {
                editor.detect_status = if request.launch_before_detect {
                    "正在启动游戏并等待新的游戏进程（最多 60 秒），识别期间不切换分辨率。"
                } else {
                    "正在等待游戏进程（最多 120 秒），请现在从启动器进入游戏。"
                }
                .into();
                Operation::DetectingProcess
            }
        };
        self.detection_generation += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.detection = Some(ActiveDetection {
            generation: self.detection_generation,
            cancelled: cancelled.clone(),
        });
        Some(DetectionTask {
            generation: self.detection_generation,
            cancelled,
            action,
        })
    }

    pub fn cancel_detection(&mut self) {
        if let Some(detection) = &self.detection {
            detection.cancelled.store(true, Ordering::Release);
            self.operation = Operation::CancellingDetection;
            if let Some(editor) = self.editor.as_mut() {
                editor.detect_status = "正在停止识别…".into();
            }
        }
    }

    pub fn finish_detection(&mut self, generation: u64, result: Result<DetectionResult, String>) {
        let Some(detection) = &self.detection else {
            return;
        };
        // 旧回调不能覆盖取消后的输入或下一次识别的状态。
        if detection.generation != generation {
            return;
        }
        let cancelled = detection.cancelled.load(Ordering::Acquire);
        self.detection = None;
        self.operation = Operation::Idle;
        if let Some(editor) = self.editor.as_mut() {
            if cancelled {
                editor.detect_status = "已取消识别，可手动填写主进程或重新识别。".into();
                editor.candidates.clear();
            } else {
                editor.finish_detection(result);
            }
        }
    }

    pub fn fail_operation(&mut self, message: String) {
        if let Some(detection) = self.detection.take() {
            detection.cancelled.store(true, Ordering::Release);
        }
        self.operation = Operation::Idle;
        if let Some(editor) = self.editor.as_mut() {
            editor.detect_status.clear();
            editor.error = Some(message);
        } else {
            self.notice = Some(message);
        }
    }

    pub fn begin_profile_save(&mut self) -> Option<GameProfile> {
        if !self.can_edit() {
            return None;
        }
        if self.needs_restore() {
            self.editor.as_mut()?.error = Some("请先返回主界面恢复桌面，再添加游戏".into());
            return None;
        }
        match self.editor.as_ref()?.to_profile() {
            Ok(profile) => {
                self.operation = Operation::SavingProfile;
                self.editor.as_mut()?.error = None;
                Some(profile)
            }
            Err(error) => {
                self.editor.as_mut()?.error = Some(error);
                None
            }
        }
    }

    pub fn finish_profile_save(&mut self, result: Result<GameProfile, String>) {
        self.operation = Operation::Idle;
        match result {
            Ok(profile) => {
                self.profiles_revision = self.profiles_revision.wrapping_add(1);
                self.selected_id = Some(profile.id.clone());
                if let Some(index) = self.profiles.iter().position(|item| item.id == profile.id) {
                    self.profiles[index] = profile;
                } else {
                    self.profiles.push(profile);
                }
                self.profiles_loaded = true;
                self.editor = None;
                self.discard_confirmation = false;
                self.notice = None;
            }
            Err(error) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.error = Some(error);
                }
            }
        }
    }

    pub fn begin_refresh(&mut self) -> bool {
        if !self.can_browse() {
            return false;
        }
        self.operation = Operation::Refreshing;
        self.notice = None;
        true
    }

    pub fn begin_launch(&mut self) -> Option<GameProfile> {
        if !self.can_launch() {
            return None;
        }
        let profile = self.selected_profile()?.clone();
        self.operation = Operation::Starting;
        self.panel = Panel::Status;
        self.notice = None;
        Some(profile)
    }

    /// true 表示无需确认，可以进入后台恢复；不能据此直接退出进程。
    pub fn request_close(&mut self) -> bool {
        if self.operation == Operation::Closing {
            return false;
        }
        self.cancel_detection();
        self.discard_confirmation = false;
        if self.operation != Operation::Idle
            || self.needs_restore()
            || self.editor.as_ref().is_some_and(EditorState::dirty)
        {
            self.close_confirmation = true;
            false
        } else {
            true
        }
    }

    pub fn begin_restore(&mut self, close_after: bool) -> bool {
        if self.operation != Operation::Idle || (!close_after && !self.can_restore()) {
            return false;
        }
        self.operation = if close_after {
            Operation::Closing
        } else {
            Operation::Restoring
        };
        self.close_confirmation = false;
        self.panel = Panel::Status;
        if close_after {
            self.editor = None;
            self.discard_confirmation = false;
        }
        self.notice = None;
        true
    }

    /// 只有恢复调用成功、恢复点已清除且会话已结束，才允许完成关闭。
    pub fn finish_restore(
        &mut self,
        result: Result<(), String>,
        pending: Result<Option<PendingRestore>, String>,
        runtime: RuntimeStatus,
    ) -> bool {
        let closing = self.operation == Operation::Closing;
        self.operation = Operation::Idle;
        self.runtime = runtime;
        self.recovery = Recovery::from_result(pending);

        let error = match result {
            Err(error) => Some(error),
            Ok(()) if self.needs_restore() => Some("恢复尚未确认完成，请重试恢复".to_string()),
            Ok(()) => None,
        };
        if let Some(error) = error {
            self.notice = Some(format!("恢复失败，窗口已保留：{error}"));
            self.close_confirmation = false;
            if !self.recovery.needs_restore() {
                self.recovery = Recovery::Unavailable(error);
            }
            return false;
        }
        self.notice = None;
        if closing {
            // quit_event_loop 的退出请求异步处理，期间仍不能接受新的启动操作。
            self.operation = Operation::Closing;
        }
        closing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{DisplayChoice, EditorCatalog, FileChoice};
    use resolution_hub_core::model::{DisplayInfo, DisplayMode, LaunchType};

    fn status(phase: &str, can_restore: bool) -> RuntimeStatus {
        RuntimeStatus {
            phase: phase.to_string(),
            can_restore,
            ..Default::default()
        }
    }

    fn profile(id: &str) -> GameProfile {
        GameProfile {
            id: id.to_string(),
            name: format!("游戏 {id}"),
            launch_type: LaunchType::Executable,
            executable: "game.exe".to_string(),
            arguments: String::new(),
            working_directory: String::new(),
            steam_app_id: String::new(),
            shortcut_path: String::new(),
            launcher_provider: String::new(),
            launcher_process_name: String::new(),
            process_name: "game.exe".to_string(),
            switch_timing: None,
            launch_timeout_sec: 600,
            display_device: "display".to_string(),
            width: 1920,
            height: 1080,
            refresh_rate: 60,
            restore_delay_ms: 0,
        }
    }

    fn pending() -> PendingRestore {
        PendingRestore {
            profile_name: "游戏".to_string(),
            mode: DisplayMode {
                device_name: "display".to_string(),
                width: 2560,
                height: 1440,
                refresh_rate: 60,
                bits_per_pixel: 32,
            },
        }
    }

    fn ready() -> AppState {
        let mut state = AppState::new(status("idle", false));
        state.replace_profiles(Ok(vec![profile("a"), profile("b")]));
        state.recovery = Recovery::Clear;
        state
    }

    fn editing() -> AppState {
        let mut state = ready();
        assert!(state.begin_editor_load());
        let mode = pending().mode;
        state
            .editor
            .as_mut()
            .unwrap()
            .load_catalog(Ok(EditorCatalog {
                displays: vec![DisplayChoice {
                    info: DisplayInfo {
                        device_name: mode.device_name.clone(),
                        friendly_name: "显示器".into(),
                        current_mode: mode.clone(),
                        is_primary: true,
                    },
                    modes: vec![mode],
                    error: None,
                }],
                ..Default::default()
            }));
        state.operation = Operation::Idle;
        state
    }

    #[test]
    fn toolbar_starts_collapsed_and_profile_activation_collapses_library() {
        let mut state = ready();
        assert_eq!(state.visible_panel(), Panel::Hidden);
        state.toggle_panel(Panel::Library);
        assert_eq!(state.visible_panel(), Panel::Library);
        state.select_profile(1);
        assert_eq!(state.visible_panel(), Panel::Library);
        state.activate_profile(1);
        assert_eq!(state.selected_index(), Some(1));
        assert_eq!(state.visible_panel(), Panel::Hidden);
        state.toggle_panel(Panel::Status);
        state.toggle_panel(Panel::Status);
        assert_eq!(state.visible_panel(), Panel::Hidden);
    }

    #[test]
    fn status_entry_toggles_the_same_details_panel_without_changing_the_session() {
        for mode in [ViewMode::Toolbar, ViewMode::Window] {
            let mut state = ready();
            state.set_view_mode(mode);
            state.select_profile(1);
            state.runtime = status("running", true);
            state.recovery = Recovery::Pending(pending());
            state.show_details(1);
            assert_eq!(state.visible_panel(), Panel::Details);
            state.toggle_panel(Panel::Status);
            assert_eq!(state.visible_panel(), Panel::Hidden);
            state.toggle_panel(Panel::Status);
            assert_eq!(state.visible_panel(), Panel::Status);
            state.toggle_panel(Panel::Details);
            assert_eq!(state.visible_panel(), Panel::Hidden);
            assert_eq!(state.selected_index(), Some(1));
            assert_eq!(state.runtime.phase, "running");
            assert!(matches!(state.recovery, Recovery::Pending(_)));
            assert!(state.can_restore());
        }
    }

    #[test]
    fn combined_panel_keeps_recovery_accessible_when_the_game_library_is_empty() {
        let mut state = ready();
        state.replace_profiles(Ok(Vec::new()));
        state.recovery = Recovery::Pending(pending());
        assert!(state.selected_profile().is_none());
        state.toggle_panel(Panel::Status);
        assert_eq!(state.visible_panel(), Panel::Status);
        assert!(!state.can_launch());
        assert!(state.can_restore());
        assert!(state.begin_restore(false));
        assert_eq!(state.operation, Operation::Restoring);
        assert!(matches!(state.recovery, Recovery::Pending(_)));
    }

    #[test]
    fn changing_layout_preserves_selection_and_active_session_recovery() {
        let mut state = ready();
        state.select_profile(1);
        state.runtime = status("running", true);
        state.recovery = Recovery::Pending(pending());
        state.toggle_panel(Panel::Appearance);
        assert!(state.set_view_mode(ViewMode::Window));
        assert_eq!(state.selected_index(), Some(1));
        assert_eq!(state.visible_panel(), Panel::Hidden);
        assert_eq!(state.runtime.phase, "running");
        assert!(state.runtime.can_restore);
        assert!(matches!(state.recovery, Recovery::Pending(_)));
        assert!(state.can_restore());
        assert!(!state.can_launch());
        assert!(state.set_view_mode(ViewMode::Toolbar));
        assert_eq!(state.selected_index(), Some(1));
        assert!(state.session_active());
    }

    #[test]
    fn layout_changes_cannot_discard_a_form_or_bypass_close_confirmation() {
        let mut state = editing();
        state.editor.as_mut().unwrap().draft.name = "尚未保存的游戏".into();
        assert!(!state.set_view_mode(ViewMode::Window));
        assert_eq!(state.editor.as_ref().unwrap().draft.name, "尚未保存的游戏");
        assert_eq!(state.visible_panel(), Panel::Editor);
        state.request_close();
        assert_eq!(state.visible_panel(), Panel::CloseConfirmation);
        assert!(!state.set_view_mode(ViewMode::Window));
        assert_eq!(state.view_mode, ViewMode::Toolbar);
        assert!(state.close_confirmation);
    }

    #[test]
    fn window_layout_uses_the_embedded_library_and_retains_other_panels() {
        let mut state = ready();
        assert!(state.set_view_mode(ViewMode::Window));
        state.toggle_panel(Panel::Library);
        assert_eq!(state.visible_panel(), Panel::Hidden);
        state.activate_profile(1);
        assert_eq!(state.selected_index(), Some(1));
        state.toggle_panel(Panel::Details);
        assert_eq!(state.visible_panel(), Panel::Details);
        state.toggle_panel(Panel::Library);
        assert_eq!(state.visible_panel(), Panel::Hidden);
        state.toggle_panel(Panel::Appearance);
        state.dismiss_inactive_panel();
        assert_eq!(state.visible_panel(), Panel::Hidden);
    }

    #[test]
    fn outside_click_only_dismisses_browsing_panels() {
        let mut state = ready();
        for panel in [Panel::Library, Panel::Details, Panel::Status] {
            state.toggle_panel(panel);
            state.dismiss_inactive_panel();
            assert_eq!(state.visible_panel(), Panel::Hidden);
        }
        assert!(state.begin_editor_load());
        state.dismiss_inactive_panel();
        assert_eq!(state.visible_panel(), Panel::Editor);
        assert!(!state.request_close());
        state.dismiss_inactive_panel();
        assert_eq!(state.visible_panel(), Panel::CloseConfirmation);
    }

    #[test]
    fn panel_close_preserves_dirty_draft_and_requires_explicit_discard() {
        let mut state = editing();
        state.editor.as_mut().unwrap().draft.name = "未保存的游戏".into();
        state.dismiss_panel();
        assert_eq!(state.visible_panel(), Panel::DiscardConfirmation);
        state.toggle_panel(Panel::Library);
        state.dismiss_inactive_panel();
        assert_eq!(state.visible_panel(), Panel::DiscardConfirmation);
        state.dismiss_panel();
        assert_eq!(state.visible_panel(), Panel::Editor);
        assert_eq!(state.editor.as_ref().unwrap().draft.name, "未保存的游戏");
        state.dismiss_panel();
        state.discard_editor();
        assert_eq!(state.visible_panel(), Panel::Hidden);
    }

    #[test]
    fn busy_editor_cannot_be_dismissed_or_replaced_by_a_tool() {
        for operation in [
            Operation::ChoosingFile,
            Operation::SavingProfile,
            Operation::LoadingSources,
            Operation::OpeningLauncher,
        ] {
            let mut state = editing();
            state.operation = operation;
            state.dismiss_panel();
            state.toggle_panel(Panel::Status);
            assert_eq!(state.visible_panel(), Panel::Editor);
            assert_eq!(state.operation, operation);
        }
    }

    #[test]
    fn closing_detection_panel_cancels_wait_but_keeps_form_until_worker_returns() {
        let (mut state, task) = detecting_state();
        state.dismiss_inactive_panel();
        assert!(!task.cancelled.load(Ordering::Acquire));
        state.dismiss_panel();
        assert!(task.cancelled.load(Ordering::Acquire));
        assert_eq!(state.operation, Operation::CancellingDetection);
        assert_eq!(state.visible_panel(), Panel::Editor);
        state.dismiss_panel();
        state.finish_detection(task.generation, Ok(detected_process()));
        assert_eq!(state.visible_panel(), Panel::Editor);
        assert!(state.editor.as_ref().unwrap().draft.process_name.is_empty());
    }

    #[test]
    fn session_detail_and_close_panels_keep_runtime_guards() {
        let mut state = ready();
        assert!(state.begin_launch().is_some());
        assert_eq!(state.visible_panel(), Panel::Status);
        state.operation = Operation::Idle;
        state.runtime = status("waiting", false);
        state.show_details(1);
        assert_eq!(state.selected_index(), Some(0));
        assert_eq!(state.visible_panel(), Panel::Status);
        state.show_details(0);
        assert_eq!(state.visible_panel(), Panel::Details);
        assert!(!state.request_close());
        state.dismiss_panel();
        assert_eq!(state.visible_panel(), Panel::Details);
        assert!(state.session_active());
        assert!(!state.request_close());
        assert!(state.begin_restore(true));
        state.dismiss_inactive_panel();
        assert_eq!(state.visible_panel(), Panel::Status);
        assert!(!state.finish_restore(
            Err("模拟恢复失败".into()),
            Ok(Some(pending())),
            status("error", true)
        ));
        assert_eq!(state.visible_panel(), Panel::Status);
        assert!(state.can_restore());
    }

    #[test]
    fn empty_library_can_add_and_pristine_editor_cancels_without_discard_prompt() {
        let mut state = editing();
        assert!(!state.can_browse());
        assert!(!state.can_launch());
        assert!(!state.can_restore());
        assert!(!state.begin_refresh());
        state.cancel_editor();
        assert!(state.editor.is_none());
        assert!(!state.discard_confirmation);
        state.replace_profiles(Ok(Vec::new()));
        assert!(state.can_add());
        assert!(state.begin_editor_load());
        assert!(!state.begin_editor_load());
    }

    #[test]
    fn dirty_editor_requires_confirmation_and_return_keeps_the_draft() {
        let mut state = editing();
        state.editor.as_mut().unwrap().apply_file(
            "game.exe".into(),
            FileChoice::Executable { replace_name: true },
        );
        state.cancel_editor();
        assert!(state.discard_confirmation);
        assert!(!state.can_edit());
        state.discard_confirmation = false;
        assert_eq!(state.editor.as_ref().unwrap().draft.executable, "game.exe");
        state.cancel_editor();
        state.discard_editor();
        assert!(state.editor.is_none());
        assert_eq!(state.profiles.len(), 2);
        assert_eq!(state.selected_profile().unwrap().id, "a");
    }

    #[test]
    fn save_failure_keeps_input_and_close_waits_until_the_save_finishes() {
        let mut state = editing();
        state.editor.as_mut().unwrap().apply_file(
            "game.exe".into(),
            FileChoice::Executable { replace_name: true },
        );
        assert!(state.begin_profile_save().is_some());
        assert!(state.begin_profile_save().is_none());
        state.cancel_editor();
        assert!(!state.discard_confirmation);
        assert!(!state.request_close());
        assert!(state.close_confirmation);
        assert!(!state.begin_restore(true));
        state.finish_profile_save(Err("磁盘写入失败".into()));
        assert_eq!(state.editor.as_ref().unwrap().draft.executable, "game.exe");
        assert_eq!(state.editor.as_ref().unwrap().message(), "磁盘写入失败");
        state.close_confirmation = false;
        assert!(state.begin_profile_save().is_some());
    }

    #[test]
    fn saved_game_is_selected_without_launching_or_replacing_existing_games() {
        let mut state = editing();
        state.editor.as_mut().unwrap().apply_file(
            "game.exe".into(),
            FileChoice::Executable { replace_name: true },
        );
        let mut saved = state.begin_profile_save().unwrap();
        saved.id = "new-game".into();
        state.finish_profile_save(Ok(saved));
        assert!(state.editor.is_none());
        assert_eq!(state.profiles.len(), 3);
        assert_eq!(state.profiles[0].id, "a");
        assert_eq!(state.profiles[1].id, "b");
        assert_eq!(state.selected_profile().unwrap().id, "new-game");
        assert_eq!(state.operation, Operation::Idle);
        assert!(!state.session_active());
        assert!(state.can_launch());
    }

    #[test]
    fn application_close_with_unsaved_game_still_uses_protected_restore_flow() {
        let mut state = editing();
        state.editor.as_mut().unwrap().apply_file(
            "game.exe".into(),
            FileChoice::Executable { replace_name: true },
        );
        assert!(!state.request_close());
        assert!(state.close_confirmation);
        state.close_confirmation = false;
        assert!(state.editor.as_ref().unwrap().dirty());
        assert!(!state.request_close());
        assert!(state.begin_restore(true));
        assert!(state.editor.is_none());
        assert!(!state.finish_restore(
            Err("桌面恢复失败".into()),
            Ok(Some(pending())),
            status("error", true),
        ));
        assert!(state.can_restore());
        assert!(!state.can_launch());
    }

    #[test]
    fn active_session_recovery_and_unreadable_profiles_block_new_game_operations() {
        let mut state = ready();
        state.runtime = status("waiting", false);
        assert!(!state.begin_editor_load());
        state.runtime = status("idle", false);
        state.recovery = Recovery::Pending(pending());
        assert!(!state.begin_editor_load());
        state.recovery = Recovery::Clear;
        state.replace_profiles(Err("配置读取失败".into()));
        assert!(!state.begin_editor_load());
    }

    #[test]
    fn selection_survives_reordering_and_launches_the_selected_id() {
        let mut state = ready();
        state.select_profile(1);
        state.replace_profiles(Ok(vec![profile("b"), profile("a")]));
        assert_eq!(state.selected_index(), Some(0));
        assert_eq!(state.begin_launch().unwrap().id, "b");
        assert!(!state.can_browse());
        assert!(state.begin_launch().is_none());
    }

    #[test]
    fn deleted_selection_falls_back_and_empty_library_cannot_launch() {
        let mut state = ready();
        state.select_profile(1);
        state.replace_profiles(Ok(vec![profile("a")]));
        assert_eq!(state.selected_profile().unwrap().id, "a");
        state.replace_profiles(Ok(Vec::new()));
        assert!(state.selected_profile().is_none());
        assert!(state.begin_launch().is_none());
    }

    #[test]
    fn unchecked_existing_and_unreadable_recovery_all_block_launch() {
        for recovery in [
            Recovery::Unchecked,
            Recovery::Pending(pending()),
            Recovery::Unavailable("无法读取恢复点".to_string()),
        ] {
            let mut state = ready();
            state.recovery = recovery;
            assert!(state.begin_launch().is_none());
            assert!(state.can_restore());
            assert!(!state.request_close());
            assert!(state.close_confirmation);
        }
    }

    #[test]
    fn profile_read_failure_keeps_recovery_available() {
        let mut state = ready();
        state.replace_profiles(Err("配置损坏".to_string()));
        state.recovery = Recovery::Pending(pending());
        assert!(!state.can_launch());
        assert!(state.can_restore());
        assert!(state.begin_refresh());
    }

    #[test]
    fn waiting_without_a_restore_point_is_still_an_active_session() {
        let mut state = ready();
        state.runtime = status("waiting", false);
        assert!(!state.can_launch());
        assert!(state.can_restore());
        state.select_profile(1);
        assert_eq!(state.selected_profile().unwrap().id, "a");
        assert!(!state.begin_refresh());
        assert!(!state.request_close());
        state.close_confirmation = false;
        assert!(state.session_active());
    }

    #[test]
    fn close_during_start_waits_for_start_completion_before_restoring() {
        let mut state = ready();
        state.begin_launch().unwrap();
        assert!(!state.request_close());
        assert!(state.close_confirmation);
        assert!(!state.begin_restore(true));
        state.operation = Operation::Idle;
        state.runtime = status("switching", false);
        assert!(state.begin_restore(true));
        assert!(!state.begin_restore(true));
    }

    #[test]
    fn failed_close_retains_recovery_and_only_success_allows_exit() {
        let mut state = ready();
        state.runtime = status("running", true);
        assert!(!state.request_close());
        assert!(state.begin_restore(true));
        assert!(!state.finish_restore(
            Err("显示器不可用".to_string()),
            Ok(Some(pending())),
            status("error", true),
        ));
        assert!(state.can_restore());
        assert!(!state.can_launch());
        assert!(state.begin_restore(true));
        assert!(state.finish_restore(Ok(()), Ok(None), status("idle", false)));
    }

    #[test]
    fn successful_call_with_remaining_or_unreadable_recovery_cannot_exit() {
        for recovery in [Ok(Some(pending())), Err("恢复点读取失败".to_string())] {
            let mut state = ready();
            assert!(state.begin_restore(true));
            assert!(!state.finish_restore(Ok(()), recovery, status("idle", false)));
            assert!(state.can_restore());
        }
    }

    #[test]
    fn manual_restore_stays_open_and_idle_close_still_checks_recovery() {
        let mut state = ready();
        assert!(state.request_close());
        state.runtime = status("waiting", false);
        assert!(state.begin_restore(false));
        assert!(!state.finish_restore(Ok(()), Ok(None), status("idle", false)));
        assert!(state.can_launch());
        assert!(state.begin_restore(true));
        assert!(state.finish_restore(Ok(()), Ok(None), status("idle", false)));
    }

    #[test]
    fn successful_close_blocks_queued_launches_until_the_event_loop_exits() {
        let mut state = ready();
        assert!(state.begin_restore(true));
        assert!(state.finish_restore(Ok(()), Ok(None), status("idle", false)));
        assert!(!state.can_launch());
        assert!(!state.can_browse());
        assert!(state.begin_launch().is_none());
        assert!(!state.request_close());
        assert!(!state.close_confirmation);
    }

    fn detecting_state() -> (AppState, DetectionTask) {
        let mut state = editing();
        let editor = state.editor.as_mut().unwrap();
        editor.apply_file(
            "game.exe".into(),
            FileChoice::Executable { replace_name: true },
        );
        editor.draft.process_name = "".into();
        let task = state.begin_detection().unwrap();
        (state, task)
    }

    fn detected_process() -> DetectionResult {
        DetectionResult::Candidates(vec![resolution_hub_core::model::RecentApplication {
            pid: 123,
            name: "实际游戏".into(),
            executable: r"C:\Games\actual-game.exe".into(),
            process_name: "actual-game.exe".into(),
            last_seen: 1,
        }])
    }

    #[test]
    fn detection_blocks_conflicting_actions_and_accepts_the_result_without_launching_a_session() {
        let (mut state, task) = detecting_state();
        assert_eq!(state.operation, Operation::DetectingProcess);
        assert!(!state.can_edit());
        assert!(state.begin_profile_save().is_none());
        assert!(!state.begin_file_choice());
        assert!(state.begin_detection().is_none());
        assert!(state.begin_source_refresh().is_none());
        state.finish_detection(task.generation, Ok(detected_process()));
        assert_eq!(
            state.editor.as_ref().unwrap().draft.process_name,
            "actual-game.exe"
        );
        assert_eq!(state.operation, Operation::Idle);
        assert!(!state.session_active());
        assert!(state.can_edit());
    }

    #[test]
    fn cancelled_and_outdated_detection_results_do_not_overwrite_the_draft_or_new_operation() {
        let (mut state, first) = detecting_state();
        state.cancel_detection();
        assert!(first.cancelled.load(Ordering::Acquire));
        assert_eq!(state.operation, Operation::CancellingDetection);
        assert!(!state.can_edit());
        state.finish_detection(first.generation, Ok(detected_process()));
        assert!(state.editor.as_ref().unwrap().draft.process_name.is_empty());
        let second = state.begin_detection().unwrap();
        assert!(second.generation > first.generation);
        state.finish_detection(first.generation, Ok(detected_process()));
        assert_eq!(state.operation, Operation::DetectingProcess);
        assert!(state.editor.as_ref().unwrap().draft.process_name.is_empty());
        state.finish_detection(second.generation, Ok(detected_process()));
        assert!(state.can_edit());
    }

    #[test]
    fn close_during_detection_cancels_the_worker_and_waits_before_allowing_exit() {
        let (mut state, task) = detecting_state();
        assert!(!state.request_close());
        assert!(state.close_confirmation);
        assert!(task.cancelled.load(Ordering::Acquire));
        assert!(!state.begin_restore(true));
        state.finish_detection(task.generation, Ok(detected_process()));
        assert_eq!(state.operation, Operation::Idle);
        assert!(state.close_confirmation);
        assert!(state.editor.as_ref().unwrap().draft.process_name.is_empty());
        state.close_confirmation = false;
        assert!(state.can_edit());
        assert!(state.editor.as_ref().unwrap().dirty());
    }

    #[test]
    fn failed_detection_worker_releases_the_form_and_invalidates_late_results() {
        let (mut state, task) = detecting_state();
        state.fail_operation("线程创建失败".into());
        assert!(task.cancelled.load(Ordering::Acquire));
        assert!(state.can_edit());
        state.finish_detection(task.generation, Ok(detected_process()));
        assert!(state.editor.as_ref().unwrap().draft.process_name.is_empty());
        assert_eq!(state.editor.as_ref().unwrap().message(), "线程创建失败");
    }

    #[test]
    fn launcher_opening_and_scanning_are_distinct_operations() {
        let mut state = editing();
        state.editor.as_mut().unwrap().apply_file(
            "launcher.lnk".into(),
            FileChoice::Shortcut { riot_preset: false },
        );
        let opening = state.begin_detection().unwrap();
        assert_eq!(state.operation, Operation::OpeningLauncher);
        assert!(matches!(opening.action, DetectionAction::OpenLauncher(_)));
        state.finish_detection(
            opening.generation,
            Ok(DetectionResult::LauncherOpened(false)),
        );
        let scanning = state.begin_detection().unwrap();
        let DetectionAction::Detect(request) = scanning.action else {
            panic!("must scan without opening again")
        };
        assert!(!request.launch_before_detect);
        assert_eq!(state.operation, Operation::DetectingProcess);
    }

    #[test]
    fn missing_source_and_pending_restore_block_detection_before_any_background_work() {
        let mut state = editing();
        assert!(state.begin_detection().is_none());
        assert_eq!(state.operation, Operation::Idle);
        state.editor.as_mut().unwrap().apply_file(
            "game.exe".into(),
            FileChoice::Executable { replace_name: true },
        );
        state.recovery = Recovery::Pending(pending());
        assert!(state.begin_detection().is_none());
        assert!(
            state
                .editor
                .as_ref()
                .unwrap()
                .message()
                .contains("恢复桌面")
        );
    }
}
