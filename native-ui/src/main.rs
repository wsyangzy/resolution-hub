#![windows_subsystem = "windows"]

mod chrome;
mod editor;
mod icons;
mod performance;
mod placement;
mod preferences;
mod presentation;
mod runtime_reader;
mod state;

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use editor::{DetectionAction, DetectionResult, FileChoice};
use preferences::ViewMode;
use resolution_hub_core::{
    detector, dialog, display, launcher,
    model::{GameProfile, LaunchType, SwitchTiming},
    profile, recent,
    runtime::{RuntimeManager, StatusSink},
    steam,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use state::{AppState, Operation, Panel, Recovery};

slint::include_modules!();

#[derive(Clone)]
struct Controller {
    window: slint::Weak<AppWindow>,
    panel: slint::Weak<AppWindow>,
    native_ready: Arc<AtomicBool>,
    panel_transition: Arc<AtomicU64>,
    geometry_pending: Arc<AtomicBool>,
    runtime_refresh_pending: Arc<AtomicBool>,
    runtime_action_pending: Arc<AtomicBool>,
    runtime_reader: runtime_reader::Reader,
    icons: icons::Loader,
    preferences: preferences::Writer,
    state: Arc<Mutex<AppState>>,
    manager: RuntimeManager,
}

impl Controller {
    fn toggle_panel(&self, kind: i32) {
        let panel = match kind {
            1 => Panel::Library,
            2 => Panel::Details,
            3 => Panel::Status,
            7 => Panel::Appearance,
            _ => return,
        };
        if let Ok(mut state) = self.state.lock() {
            state.toggle_panel(panel);
        }
        self.render();
        if panel == Panel::Library
            && let Some(window) = self.window.upgrade()
            && window.get_window_mode()
        {
            window.invoke_focus_content();
            chrome::focus(&window);
        }
    }

    fn set_view_mode(&self, window_mode: bool) {
        let mode = if window_mode {
            ViewMode::Window
        } else {
            ViewMode::Toolbar
        };
        let changed = if let Ok(mut state) = self.state.lock() {
            if !state.can_open_panel() {
                return;
            }
            let changed = state.set_view_mode(mode);
            state.dismiss_panel();
            changed
        } else {
            return;
        };
        // 先同步 Slint 的尺寸约束，再调整 Win32 窗口，只在模式变化时重置默认尺寸。
        self.render();
        if let Some(window) = self.window.upgrade() {
            if changed {
                window.window().set_maximized(false);
                if let Err(error) = chrome::constrain_main(&window, true) {
                    self.window_error(error);
                }
            }
            window.invoke_focus_content();
            chrome::focus(&window);
        }
        if let Err(error) = self.preferences.save(mode) {
            if let Ok(mut state) = self.state.lock() {
                state.appearance_notice = error;
            }
            self.render();
        }
    }

    fn schedule_geometry(&self) {
        if !self.native_ready.load(Ordering::Acquire)
            || self.geometry_pending.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let controller = self.clone();
        slint::Timer::single_shot(Duration::from_millis(16), move || {
            controller.geometry_pending.store(false, Ordering::Release);
            controller.reposition_windows();
        });
    }

    fn toggle_editor(&self) {
        if self.state.lock().is_ok_and(|state| state.editor.is_some()) {
            self.dismiss_panel();
        } else {
            self.open_editor();
        }
    }

    fn dismiss_panel(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.dismiss_panel();
        }
        self.render();
        if self
            .state
            .lock()
            .is_ok_and(|state| state.visible_panel() == Panel::Hidden)
            && let Some(window) = self.window.upgrade()
        {
            chrome::focus(&window);
        }
    }

    fn native_event(&self, event: chrome::NativeEvent) {
        if !self.native_ready.load(Ordering::Acquire) {
            return;
        }
        let controller = self.clone();
        match event {
            chrome::NativeEvent::Deactivated => {
                // 先让 Windows 完成窗口间的焦点转移，点击工具按钮不会被误判为点击外部。
                slint::Timer::single_shot(Duration::from_millis(120), move || {
                    if let (Some(window), Some(panel)) =
                        (controller.window.upgrade(), controller.panel.upgrade())
                        && !chrome::has_focus(&window)
                        && !chrome::has_focus(&panel)
                    {
                        if let Ok(mut state) = controller.state.lock() {
                            state.dismiss_inactive_panel();
                        }
                        controller.render();
                    }
                });
            }
            chrome::NativeEvent::GeometryChanged => {
                self.schedule_geometry();
            }
            chrome::NativeEvent::PanelPainted(frame) => {
                let generation = self.panel_transition.load(Ordering::Acquire);
                // 绘制通知中不能直接改 UI 属性，等当前绘制结束后提交进入状态。
                self.post(move |controller| controller.reveal_panel(generation, frame));
            }
        }
    }

    fn window_error(&self, error: String) {
        eprintln!("窗口操作失败：{error}");
        if let Ok(mut state) = self.state.lock() {
            state.notice = Some(error.clone());
        }
        for weak in [&self.window, &self.panel] {
            if let Some(window) = weak.upgrade() {
                window.set_status_error(true);
                window.set_status_title("窗口操作失败".into());
                window.set_status_text(error.clone().into());
            }
        }
    }

    fn reposition_windows(&self) {
        let _timing = performance::measure("window-layout");
        let (Some(window), Some(panel)) = (self.window.upgrade(), self.panel.upgrade()) else {
            return;
        };
        if let Err(error) = chrome::constrain_main(&window, false) {
            self.window_error(error);
        }
        let presentation = self.state.lock().ok().map(|state| state.visible_panel());
        if let Some(kind) = presentation
            && let Err(error) = chrome::position_panel(&window, &panel, kind)
        {
            self.window_error(error);
        }
    }

    fn sync_panel(&self, kind: Panel) {
        let _timing = performance::measure("panel-sync");
        // 详情和状态共享一个面板，运行时状态变化不应重播同一内容的展开动画。
        let kind = if kind == Panel::Details {
            Panel::Status
        } else {
            kind
        };
        if !self.native_ready.load(Ordering::Acquire) {
            return;
        }
        let (Some(window), Some(panel)) = (self.window.upgrade(), self.panel.upgrade()) else {
            return;
        };
        if kind == Panel::Hidden {
            if panel.get_panel_shown() {
                let generation = self.panel_transition.fetch_add(1, Ordering::AcqRel) + 1;
                let transition = self.panel_transition.clone();
                panel.set_panel_shown(false);
                let delay = panel.get_panel_exit_duration().max(0) as u64;
                let weak = panel.as_weak();
                slint::Timer::single_shot(Duration::from_millis(delay), move || {
                    if transition.load(Ordering::Acquire) == generation
                        && let Some(panel) = weak.upgrade()
                        && !panel.get_panel_shown()
                        && let Err(error) = panel.hide()
                    {
                        eprintln!("无法收起功能面板：{error}");
                    }
                });
            }
            return;
        }
        let changed = !panel.get_panel_shown() || panel.get_panel_kind() != kind as i32;
        let generation = if changed {
            // 每次展开（包括从其他内容切换）先提交动画起点。
            // show() 后立即设为 true 会在首次绘制前合并掉透明帧。
            panel.set_panel_animate(false);
            panel.set_panel_revealed(false);
            let _ = panel.get_panel_progress();
            self.panel_transition.fetch_add(1, Ordering::AcqRel) + 1
        } else {
            self.panel_transition.load(Ordering::Acquire)
        };
        panel.set_panel_kind(kind as i32);
        if changed && let Err(error) = chrome::position_panel(&window, &panel, kind) {
            self.window_error(error);
        }
        panel.set_panel_shown(true);
        if changed {
            self.show_prepared_panel(generation, 0);
        }
    }

    fn show_prepared_panel(&self, generation: u64, attempt: u32) {
        if self.panel_transition.load(Ordering::Acquire) != generation {
            return;
        }
        let Some(panel) = self.panel.upgrade() else {
            return;
        };
        if !panel.get_panel_shown() || panel.get_panel_revealed() {
            return;
        }
        if chrome::panel_frame(&panel).is_none() {
            // 先让 Resized 与 DPI 事件更新窗口；不要把初始化尺寸的 HWND 映射到桌面。
            if attempt < 60 {
                let controller = self.clone();
                slint::Timer::single_shot(Duration::from_millis(16), move || {
                    controller.show_prepared_panel(generation, attempt + 1);
                });
            } else {
                self.window_error("功能面板尺寸未就绪，请收起后重新打开".into());
            }
            return;
        }
        if !panel.window().is_visible()
            && let Err(error) = panel.show()
        {
            self.window_error(format!("无法展开功能面板：{error}"));
            return;
        }
        chrome::focus(&panel);
        if panel.get_frame_notifier_ready() {
            panel.window().request_redraw();
            return;
        }
        self.reveal_panel_without_notifier(generation, 0);
    }

    fn reveal_panel_without_notifier(&self, generation: u64, attempt: u32) {
        // 无绘制通知时才用有上限的计时回退；无动画也必须先等待目标尺寸。
        let controller = self.clone();
        slint::Timer::single_shot(Duration::from_millis(16), move || {
            if controller.panel_transition.load(Ordering::Acquire) != generation {
                return;
            }
            let Some(panel) = controller.panel.upgrade() else {
                return;
            };
            if !panel.get_panel_shown() || panel.get_panel_revealed() {
                return;
            }
            if let Some(frame) = chrome::panel_frame(&panel) {
                controller.reveal_panel(generation, frame);
            } else if attempt < 60 {
                controller.reveal_panel_without_notifier(generation, attempt + 1);
            } else {
                controller.window_error("功能面板尺寸未就绪，请收起后重新打开".into());
            }
        });
    }

    fn reveal_panel(&self, generation: u64, frame: chrome::PanelFrame) {
        if self.panel_transition.load(Ordering::Acquire) == generation
            && let Some(panel) = self.panel.upgrade()
            && panel.get_panel_shown()
            && !panel.get_panel_revealed()
            && chrome::panel_frame(&panel) == Some(frame)
        {
            panel.set_panel_animate(true);
            panel.set_panel_revealed(true);
            panel.invoke_focus_content();
        }
    }

    fn read_runtime(&self, action: Box<dyn FnOnce(&Self) + Send>) {
        let timing = performance::measure("runtime-round-trip");
        let controller = self.clone();
        if let Err(error) = self.runtime_reader.read(move |snapshot| {
            controller.post(move |controller| {
                let _timing = timing;
                // 读取完成到 UI 回调之间仍可能发生恢复或取消，旧快照不能覆盖新状态。
                let revision = controller.runtime_reader.revision();
                if !controller.runtime_reader.is_current(&snapshot) {
                    controller.read_runtime(action);
                    return;
                }
                if let Ok(mut state) = controller.state.lock() {
                    state.runtime = snapshot.runtime;
                    if let Some(pending) = snapshot.pending {
                        state.recovery = Recovery::from_result(pending);
                    }
                }
                action(controller);
                // 事件可在上面的检查与清除合并标记之间到达，不能漏掉最后一次变化。
                if controller.runtime_reader.revision() != revision {
                    controller.queue_runtime_refresh();
                }
            });
        }) {
            self.runtime_refresh_pending.store(false, Ordering::Release);
            self.runtime_action_pending.store(false, Ordering::Release);
            self.post(move |controller| controller.window_error(error));
        }
    }

    fn queue_runtime_refresh(&self) {
        if self.runtime_refresh_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        self.read_runtime(Box::new(|controller| {
            controller
                .runtime_refresh_pending
                .store(false, Ordering::Release);
            controller.render();
        }));
    }

    fn with_runtime(&self, action: fn(&Self)) {
        // 磁盘慢时连续点击只排队一次，避免积压的启动/保存意图在稍后重复执行。
        if self.runtime_action_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        self.read_runtime(Box::new(move |controller| {
            controller
                .runtime_action_pending
                .store(false, Ordering::Release);
            action(controller);
        }));
    }

    fn status_sink(&self) -> StatusSink {
        let controller = self.clone();
        Arc::new(move |_| {
            controller.runtime_reader.invalidate();
            controller.queue_runtime_refresh();
        })
    }

    fn post(&self, action: impl FnOnce(&Self) + Send + 'static) {
        let controller = self.clone();
        if let Err(error) = self
            .window
            .upgrade_in_event_loop(move |_| action(&controller))
        {
            eprintln!("无法更新原生界面：{error}");
        }
    }

    fn run_task(&self, name: &'static str, task: impl FnOnce(Self) + Send + 'static) {
        let controller = self.clone();
        if let Err(error) = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let _timing = performance::measure(name);
                task(controller);
            })
        {
            if let Ok(mut state) = self.state.lock() {
                state.fail_operation(format!("无法启动后台任务：{error}"));
            }
            self.render();
        }
    }

    fn open_editor(&self) {
        self.with_runtime(Self::open_editor_ready);
    }

    fn open_editor_ready(&self) {
        if !self
            .state
            .lock()
            .is_ok_and(|mut state| state.begin_editor_load())
        {
            return;
        }
        self.render();
        self.run_task("native-editor-load", |controller| {
            let catalog = editor::EditorCatalog::load();
            controller.post(move |controller| {
                if let Ok(mut state) = controller.state.lock() {
                    state.operation = Operation::Idle;
                    if let Some(editor) = state.editor.as_mut() {
                        editor.load_catalog(catalog);
                    }
                }
                controller.render();
            });
        });
    }

    fn update_editor(&self, draft: NewGameDraft) {
        if let Ok(mut state) = self.state.lock()
            && state.can_edit()
            && let Some(editor) = state.editor.as_mut()
        {
            editor.update(draft);
        }
        self.render_editor();
    }

    fn choose_editor_file(&self, choice: FileChoice) {
        if !self
            .state
            .lock()
            .is_ok_and(|mut state| state.begin_file_choice())
        {
            return;
        }
        self.render();
        self.run_task("native-editor-file", move |controller| {
            let path = match choice {
                FileChoice::Shortcut { .. } => dialog::select_shortcut(),
                FileChoice::Executable { .. } => dialog::select_executable(),
            };
            controller.post(move |controller| {
                if let Ok(mut state) = controller.state.lock() {
                    state.operation = Operation::Idle;
                    if let Some(path) = path
                        && let Some(editor) = state.editor.as_mut()
                    {
                        editor.apply_file(path, choice);
                    }
                }
                controller.render();
            });
        });
    }

    fn refresh_editor_sources(&self) {
        let kind = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.begin_source_refresh());
        let Some(kind) = kind else { return };
        self.render();
        self.run_task("native-editor-sources", move |controller| {
            let games = (kind == editor::STEAM).then(steam::list_installed_games);
            let apps = (kind == editor::RECENT).then(recent::list);
            controller.post(move |controller| {
                if let Ok(mut state) = controller.state.lock() {
                    state.operation = Operation::Idle;
                    if let Some(editor) = state.editor.as_mut()
                        && let Some(catalog) = editor.catalog.as_mut()
                    {
                        editor.catalog_revision = editor.catalog_revision.wrapping_add(1);
                        if let Some(games) = games {
                            catalog.set_steam_games(games);
                        }
                        if let Some(apps) = apps {
                            catalog.recent_apps = apps;
                        }
                    }
                }
                controller.render();
            });
        });
    }

    fn detect_editor_process(&self) {
        self.with_runtime(Self::detect_editor_process_ready);
    }

    fn detect_editor_process_ready(&self) {
        let task = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.begin_detection());
        self.render();
        let Some(task) = task else { return };
        self.run_task("native-editor-detect", move |controller| {
            let result = if task.cancelled.load(Ordering::Acquire) {
                Err("已取消游戏进程识别".into())
            } else {
                match task.action {
                    DetectionAction::OpenLauncher(request) => {
                        launcher::launch_request(&request).map(DetectionResult::LauncherOpened)
                    }
                    DetectionAction::Detect(request) => {
                        detector::detect_with_cancel(&request, || {
                            task.cancelled.load(Ordering::Acquire)
                        })
                        .map(DetectionResult::Candidates)
                    }
                }
            };
            controller.post(move |controller| {
                if let Ok(mut state) = controller.state.lock() {
                    state.finish_detection(task.generation, result);
                }
                controller.render();
            });
        });
    }

    fn save_editor(&self) {
        self.with_runtime(Self::save_editor_ready);
    }

    fn save_editor_ready(&self) {
        let profile = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.begin_profile_save());
        self.render();
        let Some(profile) = profile else { return };
        self.run_task("native-editor-save", move |controller| {
            let result = editor::save(profile);
            controller.post(move |controller| {
                if let Ok(mut state) = controller.state.lock() {
                    state.finish_profile_save(result);
                }
                controller.render();
            });
        });
    }

    fn refresh(&self) {
        self.with_runtime(Self::refresh_ready);
    }

    fn refresh_ready(&self) {
        if !self
            .state
            .lock()
            .is_ok_and(|mut state| state.begin_refresh())
        {
            return;
        }
        self.render();
        self.run_task("native-refresh", |controller| {
            // 三项读取彼此独立；配置或显示器读取失败不能遮住恢复入口。
            let pending = profile::load_pending();
            let profiles = profile::list();
            let display = display::enumerate_displays().and_then(|displays| {
                displays
                    .iter()
                    .find(|display| display.is_primary)
                    .or_else(|| displays.first())
                    .map(|display| display.friendly_name.clone())
                    .ok_or_else(|| "Windows 没有返回活动显示器".to_string())
            });
            let runtime = controller.manager.status();
            controller.post(move |controller| {
                if let Ok(mut state) = controller.state.lock() {
                    state.operation = Operation::Idle;
                    state.runtime = runtime;
                    state.recovery = Recovery::from_result(pending);
                    state.replace_profiles(profiles);
                    match display {
                        Ok(display) => state.display_summary = display,
                        Err(error) => {
                            state.display_summary = "显示器读取失败".to_string();
                            if state.notice.is_none() {
                                state.notice = Some(error);
                            }
                        }
                    }
                }
                controller.render();
            });
        });
    }

    fn launch(&self) {
        self.with_runtime(Self::launch_ready);
    }

    fn launch_ready(&self) {
        let profile = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.begin_launch());
        let Some(profile) = profile else {
            self.render();
            return;
        };
        self.render();
        self.run_task("native-launch", move |controller| {
            let result = controller
                .manager
                .start_with_sink(profile, controller.status_sink());
            controller.read_runtime(Box::new(move |controller| {
                if let Ok(mut state) = controller.state.lock() {
                    state.operation = Operation::Idle;
                    state.notice = result.err();
                }
                controller.render();
            }));
        });
    }

    fn restore(&self, close_after: bool) {
        if !self
            .state
            .lock()
            .is_ok_and(|mut state| state.begin_restore(close_after))
        {
            return;
        }
        self.render();
        self.run_task("native-restore", move |controller| {
            let result = controller
                .manager
                .restore_now_with_sink(controller.status_sink());
            let pending = profile::load_pending();
            let runtime = controller.manager.status();
            controller.post(move |controller| {
                let may_close = controller
                    .state
                    .lock()
                    .is_ok_and(|mut state| state.finish_restore(result, pending, runtime));
                controller.render();
                if may_close && let Err(error) = slint::quit_event_loop() {
                    if let Ok(mut state) = controller.state.lock() {
                        state.operation = Operation::Idle;
                        state.notice = Some(format!("桌面已恢复，但退出失败：{error}"));
                    }
                    controller.render();
                }
            });
        });
    }

    fn request_close(&self) {
        self.read_runtime(Box::new(Self::request_close_ready));
    }

    fn request_close_ready(&self) {
        let restore_without_prompt = self
            .state
            .lock()
            .is_ok_and(|mut state| state.request_close());
        self.render();
        if restore_without_prompt {
            self.restore(true);
        }
    }

    fn render(&self) {
        let _timing = performance::measure("ui-render");
        let (Some(window), Some(panel)) = (self.window.upgrade(), self.panel.upgrade()) else {
            return;
        };
        let Ok(state) = self.state.lock() else {
            for window in [&window, &panel] {
                window.set_status_text("界面状态不可用，请重新打开应用以恢复桌面".into());
                window.set_launch_enabled(false);
                window.set_browse_enabled(false);
                window.set_restore_enabled(false);
                window.set_panels_enabled(false);
            }
            return;
        };
        let kind = state.visible_panel();
        Self::render_window(&window, &state);
        if kind != Panel::Hidden {
            Self::render_window(&panel, &state);
        } else {
            // 关闭输入与确认内容，避免隐藏表单保留光标/过渡状态；列表模型无需重新绑定。
            panel.set_editor_active(false);
            panel.set_editor_discard_confirmation(false);
            panel.set_close_confirmation(false);
        }
        drop(state);
        self.sync_panel(kind);
    }

    // 输入、搜索和来源选择只影响表单，不重新同步主窗口或改变原生窗口状态。
    fn render_editor(&self) {
        let _timing = performance::measure("editor-render");
        if let Some(panel) = self.panel.upgrade()
            && let Ok(state) = self.state.lock()
            && state.visible_panel() == Panel::Editor
        {
            Self::render_window(&panel, &state);
        }
    }

    fn render_window(window: &AppWindow, state: &AppState) {
        window.set_window_mode(state.view_mode == ViewMode::Window);
        window.set_appearance_notice(state.appearance_notice.clone().into());
        window.set_active_panel(state.visible_panel() as i32);
        window.set_panels_enabled(state.can_open_panel());
        let selected = state.selected_profile();
        presentation::profiles(window, state);
        window.set_selected_index(state.selected_index().map_or(-1, |index| index as i32));
        window.set_has_profile(selected.is_some());
        window.set_profile_name(
            selected
                .map_or("尚未添加游戏", |profile| profile.name.as_str())
                .into(),
        );
        window.set_profile_resolution(
            selected
                .map(|profile| format!("{} × {}", profile.width, profile.height))
                .unwrap_or_default()
                .into(),
        );
        window.set_profile_refresh(
            selected
                .map(|profile| profile.refresh_rate.to_string())
                .unwrap_or_default()
                .into(),
        );
        window.set_profile_source(
            selected
                .map(|profile| launch_type_label(profile.launch_type))
                .unwrap_or_default()
                .into(),
        );
        if window.get_is_panel() && matches!(state.visible_panel(), Panel::Details | Panel::Status)
        {
            let details = selected.map(profile_details).unwrap_or_default();
            if let Some(model) = updated_model(&window.get_profile_details(), details) {
                window.set_profile_details(model);
            }
        }
        window.set_display_summary(state.display_summary.clone().into());
        window.set_launch_enabled(state.can_launch());
        window.set_browse_enabled(state.can_browse());
        window.set_add_enabled(state.can_add() || state.editor.is_some());
        window.set_editor_active(state.editor.is_some());
        window.set_editor_busy(state.operation != Operation::Idle);
        window.set_editor_discard_confirmation(state.discard_confirmation);
        if window.get_is_panel()
            && let Some(editor) = state.editor.as_ref()
        {
            if window.get_editor_draft() != editor.draft {
                window.set_editor_draft(editor.draft.clone());
            }
            window.set_editor_loaded(editor.catalog.is_some());
            window.set_editor_dirty(editor.dirty());
            window.set_editor_detecting(matches!(
                state.operation,
                Operation::OpeningLauncher
                    | Operation::DetectingProcess
                    | Operation::CancellingDetection
            ));
            window.set_editor_can_cancel_detection(state.operation == Operation::DetectingProcess);
            window.set_editor_launcher_ready(editor.launcher_ready);
            window.set_editor_detect_status(editor.detect_status.clone().into());
            window.set_editor_error(editor.message().into());
            window.set_editor_busy_text(
                match state.operation {
                    Operation::LoadingEditor => "正在读取启动来源与显示器…",
                    Operation::LoadingSources => "正在刷新启动来源…",
                    Operation::OpeningLauncher => "正在打开启动器…",
                    Operation::DetectingProcess => "正在识别，可点击取消识别或按 Escape",
                    Operation::CancellingDetection => "正在停止识别，请稍候…",
                    Operation::ChoosingFile => "请在文件选择窗口中选择文件…",
                    Operation::SavingProfile => "正在保存游戏配置…",
                    _ => "",
                }
                .into(),
            );
            presentation::editor_models(window, editor);
            window.set_editor_source_selected(editor.selected_source.is_some());
            window.set_editor_source_detail(
                editor
                    .selected_source
                    .as_ref()
                    .map(|source| source.detail.clone())
                    .unwrap_or_default()
                    .into(),
            );
            window.set_editor_source_kind_label(
                editor
                    .selected_source
                    .as_ref()
                    .map(|source| source.kind.clone())
                    .unwrap_or_default()
                    .into(),
            );
            if window.get_editor_steam_search() != editor.steam_search.as_str() {
                window.set_editor_steam_search(editor.steam_search.clone().into());
            }
        }
        window.set_session_active(state.session_active());
        window.set_restore_visible(
            state.needs_restore()
                || matches!(state.operation, Operation::Restoring | Operation::Closing),
        );
        window.set_restore_enabled(state.can_restore());
        window.set_restore_label(
            match state.operation {
                Operation::Restoring | Operation::Closing => "正在恢复…",
                _ if state.session_active() => "结束会话",
                _ if matches!(state.recovery, Recovery::Unavailable(_)) => "重试恢复",
                _ => "立即恢复",
            }
            .into(),
        );
        window.set_status_title(
            match state.operation {
                Operation::Refreshing => "正在读取配置",
                Operation::LoadingEditor
                | Operation::LoadingSources
                | Operation::ChoosingFile
                | Operation::OpeningLauncher
                | Operation::DetectingProcess
                | Operation::CancellingDetection
                | Operation::SavingProfile => "添加游戏",
                Operation::Starting => "正在准备启动",
                Operation::Restoring => "正在恢复桌面",
                Operation::Closing => "恢复后退出",
                Operation::Idle if state.notice.is_some() || state.runtime.phase == "error" => {
                    "需要处理"
                }
                Operation::Idle if !state.session_active() && state.needs_restore() => "需要恢复",
                Operation::Idle => phase_label(&state.runtime.phase),
            }
            .into(),
        );
        window.set_status_text(
            state
                .notice
                .clone()
                .unwrap_or_else(|| match state.operation {
                    Operation::Refreshing => "正在读取游戏配置并检查桌面恢复状态".to_string(),
                    Operation::LoadingEditor
                    | Operation::LoadingSources
                    | Operation::ChoosingFile
                    | Operation::OpeningLauncher
                    | Operation::DetectingProcess
                    | Operation::CancellingDetection
                    | Operation::SavingProfile => "正在添加游戏配置".to_string(),
                    Operation::Starting => "正在准备所选游戏的守候会话".to_string(),
                    Operation::Restoring => "正在结束会话并恢复桌面显示模式".to_string(),
                    Operation::Closing => "正在恢复桌面，完成后将自动退出".to_string(),
                    Operation::Idle => state.runtime.message.clone(),
                })
                .into(),
        );
        window.set_status_error(state.notice.is_some() || state.runtime.phase == "error");
        window.set_status_phase(
            match state.operation {
                Operation::Refreshing => "loading",
                Operation::LoadingEditor
                | Operation::LoadingSources
                | Operation::ChoosingFile
                | Operation::OpeningLauncher
                | Operation::DetectingProcess
                | Operation::CancellingDetection
                | Operation::SavingProfile => "idle",
                Operation::Starting => "preparing",
                Operation::Restoring | Operation::Closing => "restoring",
                Operation::Idle => &state.runtime.phase,
            }
            .into(),
        );
        let recovery_message = match &state.recovery {
            Recovery::Unchecked => "正在检查上次会话的桌面恢复状态".to_string(),
            Recovery::Pending(pending) => format!(
                "“{}”的桌面模式尚未恢复。恢复目标：{}。请先恢复，再启动新的会话。",
                pending.profile_name,
                pending.mode.label()
            ),
            Recovery::Unavailable(error) => {
                format!("无法确认桌面恢复状态：{error}。请重试恢复或刷新配置。")
            }
            Recovery::Clear if state.runtime.can_restore => {
                "桌面模式尚未确认恢复，请先点击立即恢复。".to_string()
            }
            Recovery::Clear => String::new(),
        };
        window.set_recovery_visible(!state.session_active() && !recovery_message.is_empty());
        window.set_recovery_message(recovery_message.into());
        window.set_close_ready(state.operation == Operation::Idle);
        window.set_close_message(
            if state.operation == Operation::CancellingDetection {
                "正在取消进程识别，停止后即可确认退出。已打开的游戏或启动器会继续运行。"
            } else if state.operation != Operation::Idle {
                "正在处理当前操作，请等待完成后再确认退出。"
            } else if state.editor.as_ref().is_some_and(editor::EditorState::dirty) {
                "新增游戏尚未保存。退出将放弃本次填写的内容；如有需要恢复的桌面模式，会先恢复再关闭。"
            } else if state.needs_restore() {
                "退出前会结束会话并恢复桌面显示模式。游戏进程会继续运行；恢复失败时将保留此窗口。"
            } else {
                "会话已结束，确认退出 Resolution Hub？"
            }
            .into(),
        );
        window.set_close_confirmation(state.close_confirmation);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    performance::initialize()?;
    let factory = chrome::select_backend()?;
    let window = factory.create(false)?;
    let panel = factory.create(true)?;
    let manager = RuntimeManager::default();
    let mut state = AppState::new(manager.status());
    match preferences::load() {
        Ok(preferences) => state.view_mode = preferences.view_mode,
        Err(error) => state.appearance_notice = error,
    }
    let state = Arc::new(Mutex::new(state));
    let preference_state = state.clone();
    let preference_windows = [window.as_weak(), panel.as_weak()];
    let preferences = preferences::Writer::start(move |mode, result| {
        let state = preference_state.clone();
        let windows = preference_windows.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let Ok(mut state) = state.lock() else { return };
            if state.view_mode != mode {
                return;
            }
            state.appearance_notice = result
                .err()
                .map(|error| format!("布局已切换，但未能记住选择：{error}"))
                .unwrap_or_default();
            let notice = state.appearance_notice.clone();
            drop(state);
            for window in windows.iter().filter_map(slint::Weak::upgrade) {
                window.set_appearance_notice(notice.clone().into());
            }
        });
    })?;
    let icons = icons::Loader::start(&window, &panel)?;
    let controller = Controller {
        window: window.as_weak(),
        panel: panel.as_weak(),
        native_ready: Arc::new(AtomicBool::new(false)),
        panel_transition: Arc::new(AtomicU64::new(0)),
        geometry_pending: Arc::new(AtomicBool::new(false)),
        runtime_refresh_pending: Arc::new(AtomicBool::new(false)),
        runtime_action_pending: Arc::new(AtomicBool::new(false)),
        runtime_reader: runtime_reader::Reader::start(manager.clone())?,
        icons,
        preferences,
        state,
        manager: manager.clone(),
    };
    connect_controller(&window, &controller, false);
    connect_controller(&panel, &controller, true);
    let setup = controller.clone();
    slint::spawn_local(async move {
        if let (Some(window), Some(panel)) = (setup.window.upgrade(), setup.panel.upgrade()) {
            let result = chrome::initialize(&window, &panel).await;
            setup.native_ready.store(true, Ordering::Release);
            if let Err(error) = result {
                setup.window_error(error);
            }
            setup.render();
        }
    })?;

    controller.refresh();
    let run_result = window.run();
    if let Ok(mut state) = controller.state.lock() {
        state.cancel_detection();
    }
    // 事件循环异常退出也要使旧 worker 失效并尝试恢复，失败时由运行时保留恢复点。
    if run_result.is_err()
        || !matches!(manager.status().phase.as_str(), "idle" | "error")
        || manager.status().can_restore
        || !matches!(profile::load_pending(), Ok(None))
    {
        manager.restore_now_with_sink(Arc::new(|_| {}))?;
    }
    run_result?;
    controller.preferences.flush();
    Ok(())
}

fn connect_controller(window: &AppWindow, controller: &Controller, is_panel: bool) {
    let icons = controller.icons.clone();
    window.on_icon_requested(move |id| icons.request(id));
    let layout = controller.clone();
    window.on_view_mode_requested(move |window_mode| layout.set_view_mode(window_mode));
    let geometry = controller.clone();
    window.on_panel_size_requested(move || geometry.schedule_geometry());
    let refresh = controller.clone();
    window.on_refresh_requested(move || refresh.refresh());
    let add = controller.clone();
    window.on_add_requested(move || add.toggle_editor());
    let retry_editor = controller.clone();
    window.on_editor_reload_requested(move || retry_editor.open_editor());
    let edit = controller.clone();
    window.on_editor_draft_changed(move |draft| edit.update_editor(draft));
    let pick_executable = controller.clone();
    window.on_editor_executable_requested(move |replace_name| {
        pick_executable.choose_editor_file(FileChoice::Executable { replace_name });
    });
    let pick_shortcut = controller.clone();
    window.on_editor_shortcut_requested(move |riot_preset| {
        pick_shortcut.choose_editor_file(FileChoice::Shortcut { riot_preset });
    });
    let pick_source = controller.clone();
    window.on_editor_source_chosen(move |index| {
        if let Ok(index) = usize::try_from(index)
            && let Ok(mut state) = pick_source.state.lock()
            && state.can_edit()
            && let Some(editor) = state.editor.as_mut()
        {
            editor.select_source(index);
        }
        pick_source.render_editor();
    });
    let search = controller.clone();
    window.on_editor_search_changed(move |query| {
        if let Ok(mut state) = search.state.lock()
            && state.can_edit()
            && let Some(editor) = state.editor.as_mut()
        {
            editor.steam_search = query.to_string();
        }
        search.render_editor();
    });
    let refresh_sources = controller.clone();
    window.on_editor_refresh_sources(move || refresh_sources.refresh_editor_sources());
    let detect = controller.clone();
    window.on_editor_detect_requested(move || detect.detect_editor_process());
    let cancel_detection = controller.clone();
    window.on_editor_cancel_detection(move || {
        if let Ok(mut state) = cancel_detection.state.lock() {
            state.cancel_detection();
        }
        cancel_detection.render();
    });
    let candidate = controller.clone();
    window.on_editor_candidate_selected(move |index| {
        if let Ok(index) = usize::try_from(index)
            && let Ok(mut state) = candidate.state.lock()
            && state.can_edit()
            && let Some(editor) = state.editor.as_mut()
        {
            editor.select_candidate(index);
        }
        candidate.render_editor();
    });
    let save_editor = controller.clone();
    window.on_editor_save_requested(move || save_editor.save_editor());
    let cancel_editor = controller.clone();
    window.on_editor_cancel_requested(move || {
        if let Ok(mut state) = cancel_editor.state.lock() {
            state.cancel_editor();
        }
        cancel_editor.render();
    });
    let discard_editor = controller.clone();
    window.on_editor_discard_confirmed(move || {
        if let Ok(mut state) = discard_editor.state.lock() {
            state.discard_editor();
        }
        discard_editor.render();
    });
    let keep_editor = controller.clone();
    window.on_editor_discard_cancelled(move || {
        if let Ok(mut state) = keep_editor.state.lock() {
            state.discard_confirmation = false;
        }
        keep_editor.render();
    });
    let launch = controller.clone();
    window.on_launch_requested(move || launch.launch());
    let selection = controller.clone();
    window.on_profile_selected(move |index| {
        if let Ok(index) = usize::try_from(index)
            && let Ok(mut state) = selection.state.lock()
        {
            state.select_profile(index);
        }
        selection.render();
    });
    let restore = controller.clone();
    window.on_restore_requested(move || restore.restore(false));
    let close = controller.clone();
    window.window().on_close_requested(move || {
        if is_panel {
            close.dismiss_panel();
        } else {
            close.request_close();
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
    let confirm = controller.clone();
    window.on_close_confirmed(move || confirm.restore(true));
    let cancel = controller.clone();
    window.on_close_cancelled(move || {
        if let Ok(mut state) = cancel.state.lock() {
            state.close_confirmation = false;
        }
        cancel.render();
    });

    let toggle = controller.clone();
    window.on_panel_requested(move |kind| toggle.toggle_panel(kind));
    let dismiss = controller.clone();
    window.on_panel_dismiss_requested(move || dismiss.dismiss_panel());
    let activate = controller.clone();
    window.on_profile_activated(move |index| {
        if let Ok(index) = usize::try_from(index)
            && let Ok(mut state) = activate.state.lock()
        {
            state.activate_profile(index);
        }
        activate.render();
        if let Some(window) = activate.window.upgrade() {
            chrome::focus(&window);
        }
    });
    let details = controller.clone();
    window.on_profile_details_requested(move |index| {
        if let Ok(index) = usize::try_from(index)
            && let Ok(mut state) = details.state.lock()
        {
            state.show_details(index);
        }
        details.render();
    });
    let events = controller.clone();
    chrome::connect(window, move |event| events.native_event(event));
}

fn updated_model<T: Clone + PartialEq + 'static>(
    current: &ModelRc<T>,
    entries: Vec<T>,
) -> Option<ModelRc<T>> {
    (current.row_count() != entries.len()
        || entries
            .iter()
            .enumerate()
            .any(|(index, value)| current.row_data(index).as_ref() != Some(value)))
    .then(|| ModelRc::new(VecModel::from(entries)))
}

fn profile_details(profile: &GameProfile) -> Vec<DetailEntry> {
    let mut entries = vec![
        (
            "启动来源",
            match profile.launch_type {
                LaunchType::Steam => format!("Steam App ID {}", profile.steam_app_id),
                LaunchType::Shortcut => profile.shortcut_path.clone(),
                LaunchType::Executable => profile.executable.clone(),
            },
        ),
        ("游戏主进程", profile.process_name.clone()),
        ("目标显示器", profile.display_device.clone()),
        (
            "切换时机",
            match profile.effective_switch_timing() {
                SwitchTiming::BeforeLaunch => "启动命令之前",
                SwitchTiming::OnProcessDetected => "检测到游戏进程时",
            }
            .into(),
        ),
        (
            "等待游戏启动",
            format!("{} 分钟", profile.launch_timeout_sec as f64 / 60.0),
        ),
        (
            "退出后恢复延迟",
            format!("{} 秒", profile.restore_delay_ms as f64 / 1000.0),
        ),
    ];
    if matches!(profile.launch_type, LaunchType::Shortcut) {
        entries.insert(
            1,
            (
                "启动器类型",
                if profile.launcher_provider.is_empty() {
                    "启动器".into()
                } else {
                    profile.launcher_provider.clone()
                },
            ),
        );
        entries.insert(
            3,
            (
                "启动器进程",
                if profile.launcher_process_name.is_empty() {
                    "未指定".into()
                } else {
                    profile.launcher_process_name.clone()
                },
            ),
        );
    }
    if !profile.arguments.is_empty() {
        entries.push(("启动参数", profile.arguments.clone()));
    }
    if !profile.working_directory.is_empty() {
        entries.push(("工作目录", profile.working_directory.clone()));
    }
    entries
        .into_iter()
        .map(|(label, value)| DetailEntry {
            label: label.into(),
            value: value.into(),
        })
        .collect()
}

fn launch_type_label(launch_type: LaunchType) -> &'static str {
    match launch_type {
        LaunchType::Executable => "本地 EXE",
        LaunchType::Steam => "Steam",
        LaunchType::Shortcut => "启动器",
    }
}

fn phase_label(phase: &str) -> &'static str {
    match phase {
        "preparing" => "准备启动",
        "switching" => "切换显示模式",
        "launching" => "正在启动",
        "waiting" => "守候中",
        "running" => "游戏运行中",
        "restoring" => "正在恢复桌面",
        "error" => "需要处理",
        _ => "就绪",
    }
}
