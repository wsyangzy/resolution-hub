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

const FOCUS_POLL_INTERVAL_MS: u32 = 250;
const FOCUS_CHANGE_DEBOUNCE: Duration = Duration::from_millis(750);
const DISPLAY_SWITCH_COOLDOWN: Duration = Duration::from_secs(1);

struct SessionDisplayModes {
    desktop: DisplayMode,
    target: DisplayMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FocusAction {
    RestoreDesktop,
    ApplyTarget,
}

struct FocusMonitor {
    target_active: bool,
    seen_game_foreground: bool,
    candidate: Option<(bool, Instant)>,
    ignore_until: Instant,
}

impl FocusMonitor {
    fn new(game_foreground: Option<bool>, assume_game_seen: bool, now: Instant) -> Self {
        Self {
            target_active: true,
            seen_game_foreground: assume_game_seen || game_foreground == Some(true),
            candidate: None,
            ignore_until: now + DISPLAY_SWITCH_COOLDOWN,
        }
    }

    fn observe(&mut self, game_foreground: Option<bool>, now: Instant) -> Option<FocusAction> {
        if game_foreground == Some(true) {
            self.seen_game_foreground = true;
        }
        if now < self.ignore_until {
            self.candidate = None;
            return None;
        }

        let desired_target = match game_foreground {
            Some(true) => true,
            Some(false) if self.seen_game_foreground => false,
            _ => {
                self.candidate = None;
                return None;
            }
        };
        if desired_target == self.target_active {
            self.candidate = None;
            return None;
        }

        match self.candidate {
            Some((candidate, since))
                if candidate == desired_target
                    && now.saturating_duration_since(since) >= FOCUS_CHANGE_DEBOUNCE =>
            {
                Some(if desired_target {
                    FocusAction::ApplyTarget
                } else {
                    FocusAction::RestoreDesktop
                })
            }
            Some((candidate, _)) if candidate == desired_target => None,
            _ => {
                self.candidate = Some((desired_target, now));
                None
            }
        }
    }

    fn mark_switched(&mut self, target_active: bool, now: Instant) {
        self.target_active = target_active;
        self.candidate = None;
        self.ignore_until = now + DISPLAY_SWITCH_COOLDOWN;
    }
}

#[derive(Clone)]
pub struct RuntimeManager {
    status: Arc<Mutex<RuntimeStatus>>,
    generation: Arc<AtomicU64>,
    display_lock: Arc<Mutex<()>>,
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
            display_lock: Arc::new(Mutex::new(())),
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
        let restore_on_focus_loss = profile::load_settings()?.restore_on_focus_loss;
        let generation = {
            let mut status = self
                .status
                .lock()
                .map_err(|_| "无法锁定运行状态".to_string())?;
            if status.phase != "idle" && status.phase != "error" {
                return Err("已有游戏正在接管显示模式".to_string());
            }

            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
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
        self.emit(&app);

        let manager = self.clone();
        thread::spawn(move || {
            if let Err(error) =
                manager.run_session(&profile, restore_on_focus_loss, generation, &app)
            {
                manager.handle_session_error(error, generation, &app);
            }
        });
        Ok(())
    }

    pub fn restore_now(&self, app: &AppHandle) -> Result<(), String> {
        self.generation.fetch_add(1, Ordering::SeqCst);
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

        self.restore_pending_now()?;
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
        restore_on_focus_loss: bool,
        generation: u64,
        app: &AppHandle,
    ) -> Result<(), String> {
        let mut game_pid = process::find_pid(&game.process_name);
        let already_running = game_pid.is_some();
        let switch_timing = game.effective_switch_timing();

        let mut session_modes = None;
        if already_running {
            self.update(
                generation,
                "switching",
                "检测到游戏已在运行，正在接管显示模式",
                false,
                false,
                app,
            );
            session_modes = self.switch_to_target(game, generation)?;
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
                session_modes = self.switch_to_target(game, generation)?;
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
            launcher::launch(game)?;
            let waiting_with_restore = matches!(switch_timing, SwitchTiming::BeforeLaunch);
            self.update(
                generation,
                "waiting",
                if matches!(switch_timing, SwitchTiming::OnProcessDetected) {
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
                thread::sleep(Duration::from_millis(250));
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
                session_modes = self.switch_to_target(game, generation)?;
            }
        }

        if !self.is_current(generation) {
            return Ok(());
        }
        let session_modes = session_modes.ok_or_else(|| "无法确定游戏显示模式".to_string())?;

        self.update(
            generation,
            "running",
            if restore_on_focus_loss {
                "游戏运行中，切到后台时将恢复桌面分辨率"
            } else {
                "游戏运行中，退出后将自动恢复"
            },
            true,
            false,
            app,
        );

        let mut focus_monitor = restore_on_focus_loss.then(|| {
            let initial_foreground = game_pid
                .and_then(|pid| process::foreground_pid().map(|foreground| foreground == pid));
            FocusMonitor::new(initial_foreground, already_running, Instant::now())
        });
        'monitoring: loop {
            let mut use_polling_fallback = false;
            while let Some(pid) = game_pid {
                let exited = if let Some(monitor) = focus_monitor.as_mut() {
                    match process::ProcessWaiter::open(pid) {
                        Some(waiter) => loop {
                            match waiter.wait(FOCUS_POLL_INTERVAL_MS) {
                                Some(true) => break true,
                                Some(false) => {
                                    if !self.is_current(generation) {
                                        return Ok(());
                                    }
                                    self.handle_focus_change(
                                        pid,
                                        &session_modes,
                                        monitor,
                                        generation,
                                        app,
                                    )?;
                                }
                                None => break false,
                            }
                        },
                        None => false,
                    }
                } else {
                    process::wait_for_exit(pid)
                };
                if !exited {
                    use_polling_fallback = true;
                    break;
                }
                if !self.is_current(generation) {
                    return Ok(());
                }
                game_pid = process::find_pid(&game.process_name);
            }

            while use_polling_fallback {
                if !self.is_current(generation) {
                    return Ok(());
                }
                let Some(pid) = process::find_pid(&game.process_name) else {
                    break;
                };
                if let Some(monitor) = focus_monitor.as_mut() {
                    self.handle_focus_change(pid, &session_modes, monitor, generation, app)?;
                }
                thread::sleep(Duration::from_secs(1));
            }

            if let Some(pid) = self.wait_for_exit_confirmation(game, generation) {
                game_pid = Some(pid);
                continue 'monitoring;
            }
            break 'monitoring;
        }

        self.update(
            generation,
            "restoring",
            "游戏已退出，正在恢复桌面",
            true,
            false,
            app,
        );
        if !self.is_current(generation) {
            return Ok(());
        }
        if !self.restore_pending_for_generation(generation)? {
            return Ok(());
        }
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

    fn wait_for_exit_confirmation(&self, game: &GameProfile, generation: u64) -> Option<u32> {
        let confirmation_ms = game
            .restore_delay_ms
            .clamp(u64::from(FOCUS_POLL_INTERVAL_MS), 30_000);
        let deadline = Instant::now() + Duration::from_millis(confirmation_ms);
        loop {
            if !self.is_current(generation) {
                return None;
            }
            if let Some(pid) = process::find_pid(&game.process_name) {
                return Some(pid);
            }

            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            thread::sleep(
                Duration::from_millis(u64::from(FOCUS_POLL_INTERVAL_MS)).min(deadline - now),
            );
        }
    }

    fn switch_to_target(
        &self,
        game: &GameProfile,
        generation: u64,
    ) -> Result<Option<SessionDisplayModes>, String> {
        if !self.is_current(generation) {
            return Ok(None);
        }

        let current = display::current_mode(&game.display_device)?;
        let desktop = profile::default_display_mode(&game.display_device)?
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
        let target = DisplayMode {
            device_name: game.display_device.clone(),
            width: game.width,
            height: game.height,
            refresh_rate: game.refresh_rate,
            bits_per_pixel: 32,
        };
        let modes = SessionDisplayModes { desktop, target };

        let _display = self
            .display_lock
            .lock()
            .map_err(|_| "无法锁定显示模式切换".to_string())?;
        if !self.is_current(generation) {
            return Ok(None);
        }
        profile::save_pending(&PendingRestore {
            profile_name: game.name.clone(),
            mode: modes.desktop.clone(),
        })?;
        switch_mode_and_verify(&modes.target)?;
        Ok(Some(modes))
    }

    fn handle_focus_change(
        &self,
        game_pid: u32,
        modes: &SessionDisplayModes,
        monitor: &mut FocusMonitor,
        generation: u64,
        app: &AppHandle,
    ) -> Result<(), String> {
        let game_foreground = process::foreground_pid().map(|foreground| foreground == game_pid);
        let Some(action) = monitor.observe(game_foreground, Instant::now()) else {
            return Ok(());
        };

        let (target, pending_message, complete_message, target_active) = match action {
            FocusAction::RestoreDesktop => (
                &modes.desktop,
                "游戏已切到后台，正在恢复桌面分辨率",
                "游戏在后台，桌面分辨率已恢复",
                false,
            ),
            FocusAction::ApplyTarget => (
                &modes.target,
                "游戏已返回前台，正在重新应用游戏分辨率",
                "游戏运行中，切到后台时将恢复桌面分辨率",
                true,
            ),
        };
        self.update(generation, "running", pending_message, true, false, app);
        if !self.switch_session_mode(target, generation)? {
            return Ok(());
        }
        monitor.mark_switched(target_active, Instant::now());
        self.update(generation, "running", complete_message, true, false, app);
        Ok(())
    }

    fn switch_session_mode(&self, target: &DisplayMode, generation: u64) -> Result<bool, String> {
        let _display = self
            .display_lock
            .lock()
            .map_err(|_| "无法锁定显示模式切换".to_string())?;
        if !self.is_current(generation) {
            return Ok(false);
        }
        switch_mode_and_verify(target)?;
        Ok(true)
    }

    fn restore_pending_now(&self) -> Result<(), String> {
        let _display = self
            .display_lock
            .lock()
            .map_err(|_| "无法锁定显示模式恢复".to_string())?;
        restore_pending_unlocked()
    }

    fn restore_pending_for_generation(&self, generation: u64) -> Result<bool, String> {
        let _display = self
            .display_lock
            .lock()
            .map_err(|_| "无法锁定显示模式恢复".to_string())?;
        if !self.is_current(generation) {
            return Ok(false);
        }
        restore_pending_unlocked()?;
        Ok(true)
    }

    fn handle_session_error(&self, error: String, generation: u64, app: &AppHandle) {
        if !self.is_current(generation) {
            return;
        }

        let had_pending = matches!(profile::load_pending(), Ok(Some(_)));
        let restore_result = if had_pending {
            self.restore_pending_for_generation(generation).map(|_| ())
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

    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
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
        let _ = app.emit("runtime-status", status);
    }

    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("runtime-status", self.status());
    }
}

fn switch_mode_and_verify(target: &DisplayMode) -> Result<(), String> {
    display::switch_mode(target)?;
    thread::sleep(Duration::from_millis(650));

    let current = display::current_mode(&target.device_name)?;
    if current.width != target.width
        || current.height != target.height
        || current.refresh_rate != target.refresh_rate
    {
        return Err(format!("显示模式验证失败，当前为 {}", current.label()));
    }
    Ok(())
}

fn restore_pending_unlocked() -> Result<(), String> {
    let Some(pending) = profile::load_pending()? else {
        return Ok(());
    };
    switch_mode_and_verify(&pending.mode)?;
    profile::clear_pending()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{FocusAction, FocusMonitor};

    #[test]
    fn focus_monitor_requires_stable_known_transitions() {
        let start = Instant::now();
        let mut monitor = FocusMonitor::new(None, false, start);

        assert_eq!(
            monitor.observe(Some(false), start + Duration::from_secs(2)),
            None
        );
        assert_eq!(
            monitor.observe(Some(true), start + Duration::from_secs(3)),
            None
        );
        assert_eq!(
            monitor.observe(Some(false), start + Duration::from_secs(4)),
            None
        );
        assert_eq!(
            monitor.observe(Some(true), start + Duration::from_millis(4_500)),
            None
        );
        assert_eq!(
            monitor.observe(Some(false), start + Duration::from_secs(5)),
            None
        );
        assert_eq!(
            monitor.observe(Some(false), start + Duration::from_millis(5_750)),
            Some(FocusAction::RestoreDesktop)
        );

        monitor.mark_switched(false, start + Duration::from_millis(5_750));
        assert_eq!(
            monitor.observe(Some(true), start + Duration::from_secs(7)),
            None
        );
        assert_eq!(
            monitor.observe(None, start + Duration::from_millis(7_500)),
            None
        );
        assert_eq!(
            monitor.observe(Some(true), start + Duration::from_secs(8)),
            None
        );
        assert_eq!(
            monitor.observe(Some(true), start + Duration::from_millis(8_750)),
            Some(FocusAction::ApplyTarget)
        );

        let mut already_running = FocusMonitor::new(Some(false), true, start);
        assert_eq!(
            already_running.observe(Some(false), start + Duration::from_secs(1)),
            None
        );
        assert_eq!(
            already_running.observe(Some(false), start + Duration::from_millis(1_750)),
            Some(FocusAction::RestoreDesktop)
        );
    }
}
