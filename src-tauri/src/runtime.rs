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
/// 抢回显示模式的次数上限。每次 `ChangeDisplaySettingsExW` 都是一次全系统显示驱动重置，
/// 游戏侧对应一次 DXGI swapchain 重建。游戏坚持要自己的模式时（虚幻引擎会按
/// `GameUserSettings` 反复应用分辨率）无上限地抢就变成每秒一次的拉锯，把游戏主线程连同
/// 挂在它 tick 上的网络收发一起反复阻塞，表现为丢包持续上涨。抢不过就认输：让游戏用它
/// 自己的模式，比两败俱伤好。
const GAME_DISPLAY_REAPPLY_LIMIT: usize = 2;
/// 窗口贴合的观察窗口。进程出现只代表 exe 被加载，反作弊初始化、引擎启动、着色器编译
/// 走完之后全屏窗口才创建，这个时刻常常远晚于显示模式稳定窗口，所以必须留足时间。
const GAME_WINDOW_FIT_WINDOW: Duration = Duration::from_secs(180);
/// 贴合的次数上限。跨进程 `SetWindowPos` 会同步等待游戏窗口的消息循环，同样是打断，
/// 和游戏来回改窗口尺寸时同样会拉锯。
const GAME_WINDOW_FIT_LIMIT: usize = 6;
/// 贴合过之后还要连续这么多次扫描都没有窗口越界，才认为窗口已经稳定。虚幻引擎在登录、
/// 着色器编译完成之后会再走一次分辨率应用，把窗口改回配置里的尺寸；贴合成功一次就收手
/// 的话那次改回没人处理，画面只剩左上角一块。
const GAME_WINDOW_FIT_SETTLED_SCANS: usize = 5;
const PROCESS_DETECTION_INTERVAL: Duration = Duration::from_millis(500);
const PROCESS_HANDOFF_GRACE_PERIOD: Duration = Duration::from_secs(3);
const PROCESS_FALLBACK_INTERVAL: Duration = Duration::from_secs(2);
const CANCELLABLE_SLEEP_SLICE: Duration = Duration::from_millis(100);

#[derive(Clone)]
pub struct RuntimeManager {
    status: Arc<Mutex<RuntimeStatus>>,
    generation: Arc<AtomicU64>,
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
            })),
            generation: Arc::new(AtomicU64::new(0)),
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
            *status = RuntimeStatus {
                phase: "preparing".to_string(),
                profile_id: Some(profile.id.clone()),
                profile_name: Some(profile.name.clone()),
                message: "正在准备启动游戏".to_string(),
                can_restore: false,
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

    /// 结束当前会话并恢复桌面显示模式。守候阶段没有恢复点时恢复本身是空操作，
    /// 所以这也是「退出专注页」唯一需要的动作。
    pub fn restore_now(&self, app: &AppHandle) -> Result<(), String> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.set_status_unchecked(
            RuntimeStatus {
                phase: "restoring".to_string(),
                profile_id: self.status().profile_id,
                profile_name: self.status().profile_name,
                message: "正在恢复桌面显示模式".to_string(),
                can_restore: true,
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
            },
            app,
        );
        Ok(())
    }

    /// 守护式会话：专注页存在期间会话一直有效，游戏可以反复启停。每次游戏退出都恢复
    /// 桌面，每次重新出现都再接管一次，直到用户结束会话（`restore_now` 递增 generation）。
    fn run_session(
        &self,
        game: &GameProfile,
        generation: u64,
        app: &AppHandle,
    ) -> Result<(), String> {
        let Some(mut game_pid) = self.start_first_round(game, generation, app)? else {
            return Ok(());
        };

        loop {
            self.update(
                generation,
                "running",
                "游戏运行中，退出后将自动恢复",
                true,
                app,
            );
            if !self.stabilize_target_mode(game, generation, app)? {
                return Ok(());
            }
            if !self.wait_for_exit(game, game_pid, generation)? {
                return Ok(());
            }

            self.update(
                generation,
                "restoring",
                "游戏已退出，正在恢复桌面",
                true,
                app,
            );
            if !self.sleep_while_current(
                generation,
                Duration::from_millis(game.restore_delay_ms.clamp(0, 30_000)),
            ) || !self.is_current(generation)
            {
                return Ok(());
            }
            self.restore_pending_serialized()?;

            // 回到守候：不再发启动命令，也不再超时，等用户从启动器或桌面把游戏拉起来。
            self.update(
                generation,
                "waiting",
                "桌面已恢复，正在等待游戏再次启动",
                false,
                app,
            );
            let Some(next_pid) = self.wait_for_process(game, generation, None)? else {
                return Ok(());
            };
            self.update(
                generation,
                "switching",
                "已检测到游戏进程，正在切换显示模式",
                false,
                app,
            );
            self.switch_to_target(game, generation)?;
            if !self.is_current(generation) {
                return Ok(());
            }
            game_pid = next_pid;
        }
    }

    /// 首轮由本应用发起启动，是整个会话里唯一能在游戏进程出现之前就切好显示模式的时机
    /// （`beforeLaunch`）。返回 `Ok(None)` 表示会话已被取代。
    fn start_first_round(
        &self,
        game: &GameProfile,
        generation: u64,
        app: &AppHandle,
    ) -> Result<Option<u32>, String> {
        if let Some(pid) = process::find_pid(&game.process_name) {
            self.update(
                generation,
                "switching",
                "检测到游戏已在运行，正在接管显示模式",
                false,
                app,
            );
            self.switch_to_target(game, generation)?;
            return Ok(self.is_current(generation).then_some(pid));
        }

        let switch_timing = game.effective_switch_timing();
        if matches!(switch_timing, SwitchTiming::BeforeLaunch) {
            self.update(
                generation,
                "switching",
                "正在记录并切换显示模式",
                false,
                app,
            );
            self.switch_to_target(game, generation)?;
            self.update(
                generation,
                "launching",
                "显示模式切换成功，正在启动游戏",
                true,
                app,
            );
        } else {
            self.update(generation, "launching", "正在打开游戏启动器", false, app);
        }

        if !self.is_current(generation) {
            return Ok(None);
        }
        let launch_command_sent = launcher::launch(game)?;
        self.update(
            generation,
            "waiting",
            if !launch_command_sent {
                "检测到已打开的启动器，请直接从其中启动游戏"
            } else if matches!(switch_timing, SwitchTiming::OnProcessDetected) {
                "启动器已打开，等待登录、更新或游戏进程"
            } else {
                "游戏命令已发送，正在等待进程"
            },
            matches!(switch_timing, SwitchTiming::BeforeLaunch),
            app,
        );

        let timeout_sec = game.launch_timeout_sec.clamp(30, 3_600);
        let deadline = Instant::now() + Duration::from_secs(timeout_sec);
        let Some(pid) = self.wait_for_process(game, generation, Some(deadline))? else {
            return Ok(None);
        };

        if matches!(switch_timing, SwitchTiming::OnProcessDetected) {
            self.update(
                generation,
                "switching",
                "已检测到游戏进程，正在切换显示模式",
                false,
                app,
            );
            self.switch_to_target(game, generation)?;
            if !self.is_current(generation) {
                return Ok(None);
            }
        }
        Ok(Some(pid))
    }

    /// 等目标进程出现。`deadline` 为 `None` 表示守候阶段，一直等到用户结束会话。
    /// `Ok(None)` 表示会话已被取代。
    fn wait_for_process(
        &self,
        game: &GameProfile,
        generation: u64,
        deadline: Option<Instant>,
    ) -> Result<Option<u32>, String> {
        loop {
            if !self.is_current(generation) {
                return Ok(None);
            }
            if let Some(pid) = process::find_pid(&game.process_name) {
                return Ok(Some(pid));
            }
            if let Some(deadline) = deadline
                && Instant::now() >= deadline
            {
                return Err(format!(
                    "{} 秒内未检测到进程 {}",
                    game.launch_timeout_sec.clamp(30, 3_600),
                    game.process_name
                ));
            }
            thread::sleep(PROCESS_DETECTION_INTERVAL);
        }
    }

    /// 等游戏进程退出，`Ok(false)` 表示会话已被取代。
    fn wait_for_exit(&self, game: &GameProfile, pid: u32, generation: u64) -> Result<bool, String> {
        let mut game_pid = Some(pid);
        let mut use_polling_fallback = false;
        while let Some(pid) = game_pid {
            let Some(waiter) = process::ProcessExitWaiter::open(pid) else {
                use_polling_fallback = true;
                break;
            };
            loop {
                if !self.is_current(generation) {
                    return Ok(false);
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
                return Ok(false);
            }
        }
        Ok(true)
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
        profile::save_pending(&PendingRestore {
            profile_name: game.name.clone(),
            mode: desktop_mode_for_restore(&game.display_device, current),
        })?;

        if !self.is_current(generation) {
            return Ok(());
        }
        switch_mode_verified(&target_mode(game), || self.is_current(generation))?;
        Ok(())
    }

    /// 游戏启动初期的看护：抢回被游戏覆盖的显示模式，并把按旧分辨率创建的窗口贴合回
    /// 目标显示器。两件事的时间尺度差得远，各有各的截止时间；两件事也都是对游戏进程的
    /// 打断，所以还各有各的次数上限——无上限地做会和游戏拉锯到把它卡出丢包。
    fn stabilize_target_mode(
        &self,
        game: &GameProfile,
        generation: u64,
        app: &AppHandle,
    ) -> Result<bool, String> {
        let started = Instant::now();
        let mode_deadline = started + GAME_DISPLAY_STABILIZATION_WINDOW;
        let fit_deadline = started + GAME_WINDOW_FIT_WINDOW;
        let target = target_mode(game);
        let mut reapplied = 0;
        let mut fitted = 0;
        let mut settled_scans = 0;

        loop {
            if !self.sleep_while_current(generation, GAME_DISPLAY_STABILIZATION_INTERVAL) {
                return Ok(false);
            }
            let now = Instant::now();
            let watch_mode = now < mode_deadline && reapplied < GAME_DISPLAY_REAPPLY_LIMIT;
            let watch_fit = now < fit_deadline
                && fitted < GAME_WINDOW_FIT_LIMIT
                && settled_scans < GAME_WINDOW_FIT_SETTLED_SCANS;
            if !watch_mode && !watch_fit {
                return Ok(true);
            }
            // 一次快照同时拿到存活状态和 pid：外层 None 说明快照不可信，不能当成游戏已退出。
            let pids = match process::lookup_pids(&game.process_name) {
                Some(pids) if pids.is_empty() => return Ok(true),
                Some(pids) => pids,
                None => Vec::new(),
            };

            if watch_mode && verify_display_mode(&target).is_err() {
                reapplied += 1;
                if !self.reapply_target_mode(&target, generation, app)? {
                    return Ok(false);
                }
                // 重新应用会再造一次错配，窗口要重新观察。
                settled_scans = 0;
            }

            // 游戏窗口早于本次切换创建时不会自己缩小，桌面只显示画面左上角一块。贴合一次
            // 不算完：引擎初始化跑完还会再应用一次自己的分辨率，所以要盯到连续若干次扫描
            // 都没有窗口越界为止。
            if watch_fit {
                if pids
                    .iter()
                    .any(|&pid| display::fit_windows_to_desktop(pid, &game.display_device))
                {
                    fitted += 1;
                    settled_scans = 0;
                } else if fitted > 0 {
                    settled_scans += 1;
                }
            }
        }
    }

    /// 抢回一次被游戏覆盖的显示模式。`Ok(false)` 表示会话已被取代。抢回失败只更新状态
    /// 文案，不向上传播：游戏已经在跑了，中止会话会顺带把桌面切回桌面模式，让游戏画面
    /// 彻底错乱，比没抢到目标模式糟得多。
    fn reapply_target_mode(
        &self,
        target: &DisplayMode,
        generation: u64,
        app: &AppHandle,
    ) -> Result<bool, String> {
        self.update(
            generation,
            "switching",
            "游戏覆盖了显示模式，正在重新应用",
            true,
            app,
        );
        let outcome = {
            let _display_operation = self
                .display_operation
                .lock()
                .map_err(|_| "显示操作锁已损坏".to_string())?;
            if !self.is_current(generation) {
                return Ok(false);
            }
            switch_mode_verified(target, || self.is_current(generation))
        };
        match outcome {
            Ok(false) => return Ok(false),
            Ok(true) => self.update(
                generation,
                "running",
                "游戏运行中，退出后将自动恢复",
                true,
                app,
            ),
            Err(_) => self.update(
                generation,
                "running",
                "游戏坚持使用自己的显示模式，已保留它的设置",
                true,
                app,
            ),
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
            },
            app,
        );
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
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

/// 恢复点里的桌面模式。按可靠性排序：用户为这台显示器配置的默认模式 → 注册表里的桌面
/// 持久模式 → 当前实际模式。
///
/// 当前模式只能垫底：`onProcessDetected` 时序下它是在游戏进程已经出现之后读到的，游戏
/// 完全可能抢先改过它，把它写进恢复点会让桌面永久停在游戏的分辨率上。注册表模式不受
/// 临时切换影响，所以比它可信。
fn desktop_mode_for_restore(device_name: &str, current: DisplayMode) -> DisplayMode {
    let supported = display::enumerate_modes(device_name).unwrap_or_default();
    let candidates = [
        profile::default_display_mode(device_name).ok().flatten(),
        display::registry_mode(device_name).ok(),
    ];
    pick_supported(candidates.into_iter().flatten(), &supported).unwrap_or(current)
}

/// 取第一个仍受显示器支持的候选。恢复点挑错了桌面就回不到用户的分辨率，所以每个候选
/// 都要先确认还在 `enumerate_modes` 里。
fn pick_supported(
    candidates: impl IntoIterator<Item = DisplayMode>,
    supported: &[DisplayMode],
) -> Option<DisplayMode> {
    candidates.into_iter().find(|candidate| {
        supported.iter().any(|mode| {
            mode.width == candidate.width
                && mode.height == candidate.height
                && mode.refresh_rate == candidate.refresh_rate
        })
    })
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
    use super::{display_mode_matches, pick_supported};
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

    #[test]
    fn restore_candidates_are_tried_in_order_and_must_still_be_supported() {
        let supported = [mode(2560, 1600, 300), mode(1580, 1080, 260)];

        // 首选可用时直接取它。
        assert_eq!(
            pick_supported([mode(2560, 1600, 300), mode(1580, 1080, 260)], &supported),
            Some(mode(2560, 1600, 300)),
        );
        // 首选已不受支持（例如换了显示器）时才退到下一个候选。
        assert_eq!(
            pick_supported([mode(3840, 2160, 60), mode(1580, 1080, 260)], &supported),
            Some(mode(1580, 1080, 260)),
        );
        // 刷新率不同也算不受支持，避免把桌面恢复成另一个刷新率。
        assert_eq!(pick_supported([mode(2560, 1600, 240)], &supported), None,);
        // 全部不可用时交给调用方兜底。
        assert_eq!(pick_supported([], &supported), None);
    }
}
