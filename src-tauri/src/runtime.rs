use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Emitter};

use crate::{
    display, launcher,
    model::{DisplayMode, GameProfile, PendingRestore, RuntimeStatus, SwitchTiming},
    process, profile,
};

const DISPLAY_MODE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(3);
const DISPLAY_MODE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const DISPLAY_MODE_STABLE_DURATION: Duration = Duration::from_millis(500);
const DISPLAY_MODE_SWITCH_ATTEMPTS: usize = 2;
const GAME_DISPLAY_STABILIZATION_WINDOW: Duration = Duration::from_secs(15);
const GAME_DISPLAY_STABILIZATION_INTERVAL: Duration = Duration::from_secs(1);
const PROCESS_DETECTION_INTERVAL: Duration = Duration::from_millis(500);
const PROCESS_HANDOFF_GRACE_PERIOD: Duration = Duration::from_secs(3);
const PROCESS_FALLBACK_INTERVAL: Duration = Duration::from_secs(2);
const CANCELLABLE_SLEEP_SLICE: Duration = Duration::from_millis(100);

#[derive(Clone)]
pub struct RuntimeManager {
    status: Arc<Mutex<RuntimeStatus>>,
    generation: Arc<AtomicU64>,
    automatic_generation: Arc<AtomicU64>,
    display_operation: Arc<Mutex<()>>,
}

impl Default for RuntimeManager {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(RuntimeStatus {
                phase: "idle".to_string(),
                profile_id: None,
                profile_name: None,
                message: "就绪".to_string(),
                can_restore: false,
                can_cancel: false,
            })),
            generation: Arc::new(AtomicU64::new(0)),
            automatic_generation: Arc::new(AtomicU64::new(0)),
            display_operation: Arc::new(Mutex::new(())),
        }
    }
}

impl RuntimeManager {
    pub fn status(&self) -> RuntimeStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| RuntimeStatus {
                phase: "error".to_string(),
                profile_id: None,
                profile_name: None,
                message: "运行状态锁已损坏".to_string(),
                can_restore: true,
                can_cancel: false,
            })
    }

    pub fn start(&self, profile: GameProfile, app: AppHandle) -> Result<(), String> {
        let generation = {
            let mut status = self
                .status
                .lock()
                .map_err(|_| "无法锁定运行状态".to_string())?;
            if status.phase != "idle" && status.phase != "error" {
                return Err("已有游戏正在接管显示模式".to_string());
            }
            if profile::load_pending()?.is_some() {
                return Err("检测到尚未恢复的桌面显示模式，请先点击立即恢复".to_string());
            }
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            self.automatic_generation.store(0, Ordering::SeqCst);
            *status = RuntimeStatus {
                phase: "preparing".to_string(),
                profile_id: Some(profile.id.clone()),
                profile_name: Some(profile.name.clone()),
                message: "正在准备启动游戏".to_string(),
                can_restore: false,
                can_cancel: false,
            };
            generation
        };
        let _ = profile::record_launch(&profile.id);
        crate::tray::refresh_games(&app);
        self.emit(&app);

        let manager = self.clone();
        thread::spawn(move || {
            if let Err(error) = manager.run_session(&profile, generation, &app) {
                manager.handle_session_error(error, generation, &app);
            }
        });
        Ok(())
    }

    pub fn start_automatic(
        &self,
        game: &GameProfile,
        pid: u32,
        app: &AppHandle,
    ) -> Result<bool, String> {
        if !process::is_pid_running(pid) {
            return Ok(false);
        }
        let generation = {
            let mut status = self
                .status
                .lock()
                .map_err(|_| "无法锁定运行状态".to_string())?;
            if !matches!(status.phase.as_str(), "idle" | "error") {
                return Ok(false);
            }
            if profile::load_pending()?.is_some() {
                return Ok(false);
            }
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            self.automatic_generation
                .store(generation, Ordering::SeqCst);
            *status = RuntimeStatus {
                phase: "switching".to_string(),
                profile_id: Some(game.id.clone()),
                profile_name: Some(game.name.clone()),
                message: format!("检测到 {}，正在自动接管显示模式", game.name),
                can_restore: false,
                can_cancel: false,
            };
            generation
        };
        self.emit(app);

        if let Err(error) = self.switch_to_target(game, generation) {
            self.handle_automatic_error(error.clone(), generation, app);
            return Err(error);
        }
        if !self.is_automatic_generation(generation) {
            return Ok(false);
        }
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "running".to_string(),
                profile_id: Some(game.id.clone()),
                profile_name: Some(game.name.clone()),
                message: format!("已自动接管 {}，退出后将恢复桌面", game.name),
                can_restore: true,
                can_cancel: false,
            },
            app,
        );
        Ok(true)
    }

    pub fn switch_automatic(
        &self,
        game: &GameProfile,
        pid: u32,
        app: &AppHandle,
    ) -> Result<bool, String> {
        if !process::is_pid_running(pid) {
            return Ok(false);
        }
        let generation = self.automatic_generation.load(Ordering::SeqCst);
        if !self.is_automatic_generation(generation) {
            return Ok(false);
        }
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "switching".to_string(),
                profile_id: Some(game.id.clone()),
                profile_name: Some(game.name.clone()),
                message: format!("正在将自动接管切换到 {}", game.name),
                can_restore: true,
                can_cancel: false,
            },
            app,
        );

        if let Err(error) = self.apply_target(game, generation) {
            self.handle_automatic_error(error.clone(), generation, app);
            return Err(error);
        }
        if !self.is_automatic_generation(generation) {
            return Ok(false);
        }
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "running".to_string(),
                profile_id: Some(game.id.clone()),
                profile_name: Some(game.name.clone()),
                message: format!("已自动接管 {}，退出后将恢复桌面", game.name),
                can_restore: true,
                can_cancel: false,
            },
            app,
        );
        Ok(true)
    }

    pub fn ensure_automatic_target(
        &self,
        game: &GameProfile,
        app: &AppHandle,
    ) -> Result<bool, String> {
        let generation = self.automatic_generation.load(Ordering::SeqCst);
        if !self.is_automatic_generation(generation) {
            return Ok(false);
        }
        let target = target_mode(game);
        if verify_display_mode(&target).is_ok() {
            return Ok(true);
        }
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "switching".to_string(),
                profile_id: Some(game.id.clone()),
                profile_name: Some(game.name.clone()),
                message: "游戏覆盖了显示模式，正在重新应用".to_string(),
                can_restore: true,
                can_cancel: false,
            },
            app,
        );
        if let Err(error) = self.apply_target(game, generation) {
            self.handle_automatic_error(error.clone(), generation, app);
            return Err(error);
        }
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "running".to_string(),
                profile_id: Some(game.id.clone()),
                profile_name: Some(game.name.clone()),
                message: format!("已自动接管 {}，退出后将恢复桌面", game.name),
                can_restore: true,
                can_cancel: false,
            },
            app,
        );
        Ok(self.is_automatic_generation(generation))
    }

    pub fn finish_automatic(&self, restore_delay_ms: u64, app: &AppHandle) -> Result<(), String> {
        let generation = self.automatic_generation.load(Ordering::SeqCst);
        if !self.is_automatic_generation(generation) {
            return Ok(());
        }
        let current = self.status();
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "restoring".to_string(),
                profile_id: current.profile_id,
                profile_name: current.profile_name,
                message: "自动监控到游戏已退出，正在恢复桌面".to_string(),
                can_restore: true,
                can_cancel: false,
            },
            app,
        );
        if !self.sleep_while_current(
            generation,
            Duration::from_millis(restore_delay_ms.clamp(0, 30_000)),
        ) || !self.is_automatic_generation(generation)
        {
            return Ok(());
        }

        if let Err(error) = self.restore_pending_serialized() {
            let _ = self.automatic_generation.compare_exchange(
                generation,
                0,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            self.set_status(
                generation,
                RuntimeStatus {
                    phase: "error".to_string(),
                    profile_id: None,
                    profile_name: None,
                    message: format!("自动恢复桌面显示模式失败：{error}"),
                    can_restore: true,
                    can_cancel: false,
                },
                app,
            );
            return Err(error);
        }
        let _ = self.automatic_generation.compare_exchange(
            generation,
            0,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "idle".to_string(),
                profile_id: None,
                profile_name: None,
                message: "外部游戏已退出，桌面显示模式已恢复".to_string(),
                can_restore: false,
                can_cancel: false,
            },
            app,
        );
        Ok(())
    }

    pub fn automatic_active(&self) -> bool {
        let generation = self.automatic_generation.load(Ordering::SeqCst);
        self.is_automatic_generation(generation)
    }

    pub fn restore_now(&self, app: &AppHandle) -> Result<(), String> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.automatic_generation.store(0, Ordering::SeqCst);
        self.set_status_unchecked(
            RuntimeStatus {
                phase: "restoring".to_string(),
                profile_id: self.status().profile_id,
                profile_name: self.status().profile_name,
                message: "正在恢复桌面显示模式".to_string(),
                can_restore: true,
                can_cancel: false,
            },
            app,
        );

        if let Err(error) = self.restore_pending_serialized() {
            self.set_status_unchecked(
                RuntimeStatus {
                    phase: "error".to_string(),
                    profile_id: None,
                    profile_name: None,
                    message: format!("恢复桌面显示模式失败：{error}"),
                    can_restore: true,
                    can_cancel: false,
                },
                app,
            );
            return Err(error);
        }
        self.set_status_unchecked(
            RuntimeStatus {
                phase: "idle".to_string(),
                profile_id: None,
                profile_name: None,
                message: "桌面显示模式已恢复".to_string(),
                can_restore: false,
                can_cancel: false,
            },
            app,
        );
        Ok(())
    }

    pub fn cancel_wait(&self, app: &AppHandle) -> Result<(), String> {
        let status = self.status();
        if !status.can_cancel {
            return Err("当前没有可取消的游戏启动等待".to_string());
        }

        self.generation.fetch_add(1, Ordering::SeqCst);
        self.set_status_unchecked(
            RuntimeStatus {
                phase: "idle".to_string(),
                profile_id: None,
                profile_name: None,
                message: "已取消等待，启动器可以继续保持打开".to_string(),
                can_restore: false,
                can_cancel: false,
            },
            app,
        );
        Ok(())
    }

    fn run_session(
        &self,
        game: &GameProfile,
        generation: u64,
        app: &AppHandle,
    ) -> Result<(), String> {
        let mut game_pid = process::find_pid(&game.process_name);
        let already_running = game_pid.is_some();
        let switch_timing = game.effective_switch_timing();

        if already_running {
            self.update(
                generation,
                "switching",
                "检测到游戏已在运行，正在接管显示模式",
                false,
                false,
                app,
            );
            self.switch_to_target(game, generation)?;
        } else {
            if matches!(switch_timing, SwitchTiming::BeforeLaunch) {
                self.update(
                    generation,
                    "switching",
                    "正在记录并切换显示模式",
                    false,
                    false,
                    app,
                );
                self.switch_to_target(game, generation)?;
                self.update(
                    generation,
                    "launching",
                    "显示模式切换成功，正在启动游戏",
                    true,
                    false,
                    app,
                );
            } else {
                self.update(
                    generation,
                    "launching",
                    "正在打开游戏启动器",
                    false,
                    false,
                    app,
                );
            }

            if !self.is_current(generation) {
                return Ok(());
            }
            let launch_command_sent = launcher::launch(game)?;
            let reuse_launcher = !launch_command_sent;
            let waiting_with_restore = matches!(switch_timing, SwitchTiming::BeforeLaunch);
            self.update(
                generation,
                "waiting",
                if reuse_launcher {
                    "检测到已打开的启动器，请直接从其中启动游戏"
                } else if matches!(switch_timing, SwitchTiming::OnProcessDetected) {
                    "启动器已打开，等待登录、更新或游戏进程"
                } else {
                    "游戏命令已发送，正在等待进程"
                },
                waiting_with_restore,
                !waiting_with_restore,
                app,
            );

            let timeout_sec = game.launch_timeout_sec.clamp(30, 3_600);
            let wait_deadline = Instant::now() + Duration::from_secs(timeout_sec);
            while game_pid.is_none() {
                if !self.is_current(generation) {
                    return Ok(());
                }
                if Instant::now() >= wait_deadline {
                    return Err(format!(
                        "{timeout_sec} 秒内未检测到进程 {}",
                        game.process_name
                    ));
                }
                game_pid = process::find_pid(&game.process_name);
                if game_pid.is_some() {
                    break;
                }
                thread::sleep(PROCESS_DETECTION_INTERVAL);
            }

            if matches!(switch_timing, SwitchTiming::OnProcessDetected) {
                self.update(
                    generation,
                    "switching",
                    "已检测到游戏进程，正在切换显示模式",
                    false,
                    false,
                    app,
                );
                self.switch_to_target(game, generation)?;
            }
        }

        self.update(
            generation,
            "running",
            "游戏运行中，退出后将自动恢复",
            true,
            false,
            app,
        );

        if !self.stabilize_target_mode(game, generation, app)? {
            return Ok(());
        }
        let mut use_polling_fallback = false;
        while let Some(pid) = game_pid {
            let Some(waiter) = process::ProcessExitWaiter::open(pid) else {
                use_polling_fallback = true;
                break;
            };
            loop {
                if !self.is_current(generation) {
                    return Ok(());
                }
                match waiter.wait_timeout(1_000) {
                    Some(true) => break,
                    Some(false) => continue,
                    None => {
                        use_polling_fallback = true;
                        break;
                    }
                }
            }
            if use_polling_fallback {
                break;
            }
            game_pid = self.find_process_during_grace_period(
                &game.process_name,
                generation,
                PROCESS_HANDOFF_GRACE_PERIOD,
            );
        }
        while use_polling_fallback && process::is_running(&game.process_name) != Some(false) {
            if !self.sleep_while_current(generation, PROCESS_FALLBACK_INTERVAL) {
                return Ok(());
            }
        }

        self.update(
            generation,
            "restoring",
            "游戏已退出，正在恢复桌面",
            true,
            false,
            app,
        );
        if !self.sleep_while_current(
            generation,
            Duration::from_millis(game.restore_delay_ms.clamp(0, 30_000)),
        ) {
            return Ok(());
        }

        if !self.is_current(generation) {
            return Ok(());
        }
        self.restore_pending_serialized()?;
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "idle".to_string(),
                profile_id: None,
                profile_name: None,
                message: "游戏已退出，桌面显示模式已恢复".to_string(),
                can_restore: false,
                can_cancel: false,
            },
            app,
        );
        Ok(())
    }

    fn switch_to_target(&self, game: &GameProfile, generation: u64) -> Result<(), String> {
        let _display_operation = self
            .display_operation
            .lock()
            .map_err(|_| "显示操作锁已损坏".to_string())?;
        if !self.is_current(generation) {
            return Ok(());
        }

        let current = display::current_mode(&game.display_device)?;
        let original = profile::default_display_mode(&game.display_device)?
            .filter(|configured| {
                display::enumerate_modes(&game.display_device)
                    .map(|modes| {
                        modes.iter().any(|mode| {
                            mode.width == configured.width
                                && mode.height == configured.height
                                && mode.refresh_rate == configured.refresh_rate
                        })
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(current);
        profile::save_pending(&PendingRestore {
            profile_name: game.name.clone(),
            mode: original,
        })?;

        if !self.is_current(generation) {
            return Ok(());
        }
        let target = DisplayMode {
            device_name: game.display_device.clone(),
            width: game.width,
            height: game.height,
            refresh_rate: game.refresh_rate,
            bits_per_pixel: 32,
        };
        switch_mode_verified(&target, || self.is_current(generation))?;
        Ok(())
    }

    fn apply_target(&self, game: &GameProfile, generation: u64) -> Result<(), String> {
        let _display_operation = self
            .display_operation
            .lock()
            .map_err(|_| "显示操作锁已损坏".to_string())?;
        if !self.is_current(generation) {
            return Ok(());
        }
        let target = target_mode(game);
        switch_mode_verified(&target, || self.is_current(generation))?;
        Ok(())
    }

    fn stabilize_target_mode(
        &self,
        game: &GameProfile,
        generation: u64,
        app: &AppHandle,
    ) -> Result<bool, String> {
        let deadline = Instant::now() + GAME_DISPLAY_STABILIZATION_WINDOW;
        let target = DisplayMode {
            device_name: game.display_device.clone(),
            width: game.width,
            height: game.height,
            refresh_rate: game.refresh_rate,
            bits_per_pixel: 32,
        };
        while Instant::now() < deadline {
            if !self.sleep_while_current(generation, GAME_DISPLAY_STABILIZATION_INTERVAL) {
                return Ok(false);
            }
            if process::is_running(&game.process_name) == Some(false) {
                return Ok(true);
            }
            if verify_display_mode(&target).is_err() {
                self.update(
                    generation,
                    "switching",
                    "游戏覆盖了显示模式，正在重新应用",
                    true,
                    false,
                    app,
                );
                let _display_operation = self
                    .display_operation
                    .lock()
                    .map_err(|_| "显示操作锁已损坏".to_string())?;
                if !self.is_current(generation) {
                    return Ok(false);
                }
                switch_mode_verified(&target, || self.is_current(generation))?;
                self.update(
                    generation,
                    "running",
                    "游戏运行中，退出后将自动恢复",
                    true,
                    false,
                    app,
                );
            }
        }
        Ok(true)
    }
    fn restore_pending_serialized(&self) -> Result<(), String> {
        let _display_operation = self
            .display_operation
            .lock()
            .map_err(|_| "显示操作锁已损坏".to_string())?;
        restore_pending()
    }

    fn handle_session_error(&self, error: String, generation: u64, app: &AppHandle) {
        if !self.is_current(generation) {
            return;
        }

        let had_pending = matches!(profile::load_pending(), Ok(Some(_)));
        let restore_result = if had_pending {
            self.restore_pending_serialized()
        } else {
            Ok(())
        };
        let can_restore = had_pending && restore_result.is_err();
        let message = match (had_pending, restore_result) {
            (false, _) => error,
            (true, Ok(())) => format!("{error}；已恢复桌面显示模式"),
            (true, Err(restore_error)) => format!("{error}；自动恢复失败：{restore_error}"),
        };
        self.set_status(
            generation,
            RuntimeStatus {
                phase: "error".to_string(),
                profile_id: None,
                profile_name: None,
                message,
                can_restore,
                can_cancel: false,
            },
            app,
        );
    }

    fn handle_automatic_error(&self, error: String, generation: u64, app: &AppHandle) {
        if !self.is_automatic_generation(generation) {
            return;
        }
        let _ = self.automatic_generation.compare_exchange(
            generation,
            0,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        self.handle_session_error(format!("自动接管失败：{error}"), generation, app);
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    fn is_automatic_generation(&self, generation: u64) -> bool {
        generation != 0
            && self.is_current(generation)
            && self.automatic_generation.load(Ordering::SeqCst) == generation
    }

    fn sleep_while_current(&self, generation: u64, duration: Duration) -> bool {
        let deadline = Instant::now() + duration;
        while self.is_current(generation) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return true;
            }
            thread::sleep(remaining.min(CANCELLABLE_SLEEP_SLICE));
        }
        false
    }

    fn find_process_during_grace_period(
        &self,
        process_name: &str,
        generation: u64,
        duration: Duration,
    ) -> Option<u32> {
        let deadline = Instant::now() + duration;
        loop {
            if !self.is_current(generation) {
                return None;
            }
            if let Some(pid) = process::find_pid(process_name) {
                return Some(pid);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            thread::sleep(remaining.min(PROCESS_DETECTION_INTERVAL));
        }
    }
    fn update(
        &self,
        generation: u64,
        phase: &str,
        message: &str,
        can_restore: bool,
        can_cancel: bool,
        app: &AppHandle,
    ) {
        let current = self.status();
        self.set_status(
            generation,
            RuntimeStatus {
                phase: phase.to_string(),
                profile_id: current.profile_id,
                profile_name: current.profile_name,
                message: message.to_string(),
                can_restore,
                can_cancel,
            },
            app,
        );
    }

    fn set_status(&self, generation: u64, status: RuntimeStatus, app: &AppHandle) {
        if self.is_current(generation) {
            self.set_status_unchecked(status, app);
        }
    }

    fn set_status_unchecked(&self, status: RuntimeStatus, app: &AppHandle) {
        if let Ok(mut current) = self.status.lock() {
            *current = status.clone();
        }
        crate::tray::update_status(app, &status);
        let _ = app.emit("runtime-status", status);
    }

    fn emit(&self, app: &AppHandle) {
        let status = self.status();
        crate::tray::update_status(app, &status);
        let _ = app.emit("runtime-status", status);
    }
}

fn target_mode(game: &GameProfile) -> DisplayMode {
    DisplayMode {
        device_name: game.display_device.clone(),
        width: game.width,
        height: game.height,
        refresh_rate: game.refresh_rate,
        bits_per_pixel: 32,
    }
}

fn display_mode_matches(target: &DisplayMode, current: &DisplayMode) -> bool {
    current.width == target.width
        && current.height == target.height
        && current.refresh_rate == target.refresh_rate
        && current.bits_per_pixel == target.bits_per_pixel
}

fn verify_display_mode(target: &DisplayMode) -> Result<(), String> {
    let current = display::current_mode(&target.device_name)?;
    if display_mode_matches(target, &current) {
        Ok(())
    } else {
        Err(format!("显示模式验证失败，当前为 {}", current.label()))
    }
}

fn wait_for_display_mode(
    target: &DisplayMode,
    should_continue: &impl Fn() -> bool,
) -> Result<bool, String> {
    let deadline = Instant::now() + DISPLAY_MODE_CONFIRM_TIMEOUT;
    let mut matching_since = None;
    loop {
        if !should_continue() {
            return Ok(false);
        }
        match verify_display_mode(target) {
            Ok(()) => {
                let first_match = matching_since.get_or_insert_with(Instant::now);
                if first_match.elapsed() >= DISPLAY_MODE_STABLE_DURATION {
                    return Ok(true);
                }
            }
            Err(error) => {
                matching_since = None;
                if Instant::now() >= deadline {
                    return Err(error);
                }
            }
        }
        thread::sleep(DISPLAY_MODE_POLL_INTERVAL);
    }
}

fn switch_mode_verified(
    target: &DisplayMode,
    should_continue: impl Fn() -> bool,
) -> Result<bool, String> {
    if verify_display_mode(target).is_ok() {
        return Ok(true);
    }
    let mut last_error = None;
    for attempt in 0..DISPLAY_MODE_SWITCH_ATTEMPTS {
        if !should_continue() {
            return Ok(false);
        }
        match display::switch_mode(target) {
            Ok(()) => match wait_for_display_mode(target, &should_continue) {
                Ok(true) => return Ok(true),
                Ok(false) => return Ok(false),
                Err(error) => last_error = Some(error),
            },
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < DISPLAY_MODE_SWITCH_ATTEMPTS {
            thread::sleep(DISPLAY_MODE_POLL_INTERVAL);
        }
    }
    Err(last_error.unwrap_or_else(|| "显示模式切换未完成".to_string()))
}

fn restore_pending() -> Result<(), String> {
    let Some(pending) = profile::load_pending()? else {
        return Ok(());
    };
    switch_mode_verified(&pending.mode, || true)?;
    profile::clear_pending()
}

#[cfg(test)]
mod tests {
    use super::display_mode_matches;
    use crate::model::DisplayMode;

    fn mode(width: u32, height: u32, refresh_rate: u32) -> DisplayMode {
        DisplayMode {
            device_name: "DISPLAY1".to_string(),
            width,
            height,
            refresh_rate,
            bits_per_pixel: 32,
        }
    }

    #[test]
    fn display_confirmation_requires_the_complete_target_mode() {
        let target = mode(1920, 1080, 144);
        assert!(display_mode_matches(&target, &mode(1920, 1080, 144)));
        assert!(!display_mode_matches(&target, &mode(1920, 1080, 60)));
        assert!(!display_mode_matches(&target, &mode(2560, 1440, 144)));
    }
}
