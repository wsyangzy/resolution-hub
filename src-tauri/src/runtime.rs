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

#[derive(Clone)]
pub struct RuntimeManager {
    status: Arc<Mutex<RuntimeStatus>>,
    generation: Arc<AtomicU64>,
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
            if let Err(error) = manager.run_session(&profile, generation, &app) {
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

        restore_pending()?;
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
        let already_running = process::is_running(&game.process_name);
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
            while !process::is_running(&game.process_name) {
                if !self.is_current(generation) {
                    return Ok(());
                }
                if Instant::now() >= wait_deadline {
                    return Err(format!(
                        "{timeout_sec} 秒内未检测到进程 {}",
                        game.process_name
                    ));
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

        while process::is_running(&game.process_name) {
            if !self.is_current(generation) {
                return Ok(());
            }
            thread::sleep(Duration::from_secs(1));
        }

        self.update(
            generation,
            "restoring",
            "游戏已退出，正在恢复桌面",
            true,
            false,
            app,
        );
        thread::sleep(Duration::from_millis(
            game.restore_delay_ms.clamp(0, 30_000),
        ));

        if !self.is_current(generation) {
            return Ok(());
        }
        restore_pending()?;
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

        let target = DisplayMode {
            device_name: game.display_device.clone(),
            width: game.width,
            height: game.height,
            refresh_rate: game.refresh_rate,
            bits_per_pixel: 32,
        };
        display::switch_mode(&target)?;
        thread::sleep(Duration::from_millis(650));

        if !self.is_current(generation) {
            return Ok(());
        }
        let current = display::current_mode(&game.display_device)?;
        if current.width != target.width
            || current.height != target.height
            || current.refresh_rate != target.refresh_rate
        {
            return Err(format!("显示模式验证失败，当前为 {}", current.label()));
        }
        Ok(())
    }

    fn handle_session_error(&self, error: String, generation: u64, app: &AppHandle) {
        if !self.is_current(generation) {
            return;
        }

        let had_pending = matches!(profile::load_pending(), Ok(Some(_)));
        let restore_result = if had_pending {
            restore_pending()
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

fn restore_pending() -> Result<(), String> {
    let Some(pending) = profile::load_pending()? else {
        return Ok(());
    };
    display::switch_mode(&pending.mode)?;
    profile::clear_pending()
}
