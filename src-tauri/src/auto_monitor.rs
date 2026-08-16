use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};

use crate::{model::GameProfile, process, profile, runtime::RuntimeManager};

const IDLE_SCAN_INTERVAL: Duration = Duration::from_secs(3);
const ACTIVE_SCAN_INTERVAL: Duration = Duration::from_secs(1);
const DISABLED_WAIT_INTERVAL: Duration = Duration::from_secs(60 * 60);
const PROCESS_HANDOFF_GRACE: Duration = Duration::from_secs(3);
const OWNER_SWITCH_DEBOUNCE: Duration = Duration::from_millis(800);
const TARGET_STABILIZATION_WINDOW: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct AutoMonitor {
    shared: Arc<MonitorShared>,
}

struct MonitorShared {
    state: Mutex<MonitorState>,
    wake: Condvar,
    started: AtomicBool,
    paused: AtomicBool,
    stopped: AtomicBool,
}

#[derive(Default)]
struct MonitorState {
    session: Option<AutoSession>,
    suppressed_profile_ids: HashSet<String>,
    active_profile_ids: HashSet<String>,
    pending_owner: Option<(String, Instant)>,
    wake_requested: bool,
}

#[derive(Clone)]
struct AutoSession {
    owner: GameProfile,
    missing_since: Option<Instant>,
    stabilize_until: Instant,
}

#[derive(Clone)]
struct ActiveGame {
    profile: GameProfile,
    pids: Vec<u32>,
}

impl Default for AutoMonitor {
    fn default() -> Self {
        Self {
            shared: Arc::new(MonitorShared {
                state: Mutex::new(MonitorState::default()),
                wake: Condvar::new(),
                started: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
            }),
        }
    }
}

pub fn start(app: AppHandle) {
    if let Some(monitor) = app.try_state::<AutoMonitor>() {
        monitor.start_worker(app.clone());
    }
}

pub fn wake(app: &AppHandle) {
    let Some(monitor) = app.try_state::<AutoMonitor>() else {
        return;
    };
    if let Ok(mut state) = monitor.shared.state.lock() {
        state.wake_requested = true;
        monitor.shared.wake.notify_one();
    }
}

pub fn pause(app: &AppHandle) {
    let Some(monitor) = app.try_state::<AutoMonitor>() else {
        return;
    };
    monitor.shared.paused.store(true, Ordering::SeqCst);

    // Locking after the flag change waits for any in-flight tick to finish.
    if let Ok(mut state) = monitor.shared.state.lock() {
        state.wake_requested = true;
        monitor.shared.wake.notify_all();
    }
}

pub fn resume(app: &AppHandle) {
    let Some(monitor) = app.try_state::<AutoMonitor>() else {
        return;
    };
    monitor.shared.paused.store(false, Ordering::SeqCst);
    wake(app);
}

pub fn stop(app: &AppHandle) {
    let Some(monitor) = app.try_state::<AutoMonitor>() else {
        return;
    };
    monitor.shared.stopped.store(true, Ordering::SeqCst);

    // Taking this lock is a barrier for any in-flight tick and its display side effects.
    if let Ok(mut state) = monitor.shared.state.lock() {
        state.wake_requested = true;
        state.pending_owner = None;
        monitor.shared.wake.notify_all();
    }
}

impl AutoMonitor {
    fn start_worker(&self, app: AppHandle) {
        if self
            .shared
            .started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let monitor = self.clone();
        thread::spawn(move || {
            while !monitor.shared.stopped.load(Ordering::SeqCst) {
                let wait = monitor.tick(&app);
                if monitor.shared.stopped.load(Ordering::SeqCst) {
                    break;
                }

                let Ok(state) = monitor.shared.state.lock() else {
                    break;
                };
                let Ok((mut state, _)) =
                    monitor
                        .shared
                        .wake
                        .wait_timeout_while(state, wait, |state| {
                            !state.wake_requested && !monitor.shared.stopped.load(Ordering::SeqCst)
                        })
                else {
                    break;
                };
                state.wake_requested = false;
            }
        });
    }

    fn tick(&self, app: &AppHandle) -> Duration {
        let Ok(settings) = profile::load_settings() else {
            return IDLE_SCAN_INTERVAL;
        };
        let runtime = app.state::<RuntimeManager>();

        if !settings.auto_switch_enabled {
            let Ok(mut state) = self.shared.state.lock() else {
                return IDLE_SCAN_INTERVAL;
            };
            if self.shared.stopped.load(Ordering::SeqCst)
                || self.shared.paused.load(Ordering::SeqCst)
            {
                return DISABLED_WAIT_INTERVAL;
            }
            if runtime.automatic_active() {
                let _ = runtime.finish_automatic(0, app);
            }
            state.session = None;
            state.suppressed_profile_ids.clear();
            state.active_profile_ids.clear();
            state.pending_owner = None;
            return DISABLED_WAIT_INTERVAL;
        }

        let Ok(profiles) = profile::list() else {
            return IDLE_SCAN_INTERVAL;
        };
        let Some(running) = process::running_by_name() else {
            return ACTIVE_SCAN_INTERVAL;
        };
        // 空集合说明显示器枚举不可信，此时不能把所有配置都当成“目标显示器已消失”。
        let active_displays = crate::display::active_device_names();
        if active_displays.is_empty() {
            return IDLE_SCAN_INTERVAL;
        }
        let active = active_games(profiles, &running, &active_displays);
        let active_ids: HashSet<String> =
            active.iter().map(|game| game.profile.id.clone()).collect();

        let Ok(mut state) = self.shared.state.lock() else {
            return IDLE_SCAN_INTERVAL;
        };
        if self.shared.stopped.load(Ordering::SeqCst) || self.shared.paused.load(Ordering::SeqCst) {
            return DISABLED_WAIT_INTERVAL;
        }

        state
            .suppressed_profile_ids
            .retain(|profile_id| active_ids.contains(profile_id));

        if state.session.is_some() && !runtime.automatic_active() {
            state.session = None;
            state.pending_owner = None;
            state
                .suppressed_profile_ids
                .extend(active_ids.iter().cloned());
        }

        let status = runtime.status();
        if state.session.is_none() && !matches!(status.phase.as_str(), "idle" | "error") {
            state.active_profile_ids = active_ids;
            return scan_interval(&active);
        }

        let mut usage_changed = false;
        for game in &active {
            if !state.active_profile_ids.contains(&game.profile.id)
                && profile::record_launch(&game.profile.id).is_ok()
            {
                usage_changed = true;
            }
        }
        state.active_profile_ids = active_ids.clone();
        if usage_changed {
            crate::tray::refresh_games(app);
        }

        let eligible: Vec<ActiveGame> = active
            .iter()
            .filter(|game| !state.suppressed_profile_ids.contains(&game.profile.id))
            .cloned()
            .collect();
        let foreground_pid = process::foreground_process_id();
        let current_owner = state
            .session
            .as_ref()
            .map(|session| session.owner.id.as_str());
        let candidate = select_owner(&eligible, foreground_pid, current_owner).cloned();

        if let Some(session) = state.session.clone() {
            let current = active
                .iter()
                .find(|game| game.profile.id == session.owner.id);

            if let Some(current) = current {
                if let Some(next) = candidate.as_ref() {
                    let owner_changed = next.profile.id != session.owner.id;
                    let target_changed = !session.matches_profile(&next.profile);
                    if owner_changed {
                        if !owner_switch_ready(
                            &mut state.pending_owner,
                            &next.profile.id,
                            Instant::now(),
                        ) {
                            return ACTIVE_SCAN_INTERVAL;
                        }
                        self.switch_session(&mut state, next, &runtime, app);
                        return ACTIVE_SCAN_INTERVAL;
                    }
                    state.pending_owner = None;
                    if target_changed {
                        self.switch_session(&mut state, next, &runtime, app);
                        return ACTIVE_SCAN_INTERVAL;
                    }
                } else {
                    state.pending_owner = None;
                }

                if let Some(active_session) = state.session.as_mut() {
                    active_session.missing_since = None;
                }
                if Instant::now() < session.stabilize_until
                    && runtime
                        .ensure_automatic_target(&current.profile, app)
                        .is_err()
                {
                    state.session = None;
                    state.pending_owner = None;
                    // 只抑制失败的这份配置，同一游戏为其他显示器准备的配置仍应有机会。
                    state
                        .suppressed_profile_ids
                        .insert(current.profile.id.clone());
                }
                return ACTIVE_SCAN_INTERVAL;
            }

            state.pending_owner = None;
            if let Some(next) = candidate.as_ref() {
                self.switch_session(&mut state, next, &runtime, app);
                return ACTIVE_SCAN_INTERVAL;
            }

            let missing_since = state
                .session
                .as_mut()
                .and_then(|session| {
                    if session.missing_since.is_none() {
                        session.missing_since = Some(Instant::now());
                    }
                    session.missing_since
                })
                .unwrap_or_else(Instant::now);
            let restore_wait = PROCESS_HANDOFF_GRACE
                + Duration::from_millis(session.owner.restore_delay_ms.clamp(0, 30_000));
            if missing_since.elapsed() < restore_wait {
                return ACTIVE_SCAN_INTERVAL;
            }
            if runtime.finish_automatic(0, app).is_err() {
                state
                    .suppressed_profile_ids
                    .extend(active_ids.iter().cloned());
            }
            state.session = None;
            return IDLE_SCAN_INTERVAL;
        }

        let Some(candidate) = candidate else {
            return scan_interval(&active);
        };
        let Some(&pid) = candidate.pids.first() else {
            return IDLE_SCAN_INTERVAL;
        };
        match runtime.start_automatic(&candidate.profile, pid, app) {
            Ok(true) => {
                state.session = Some(AutoSession::new(candidate.profile));
                ACTIVE_SCAN_INTERVAL
            }
            Ok(false) => IDLE_SCAN_INTERVAL,
            Err(_) => {
                // 只抑制失败的这份配置，否则同一游戏里真正可用的那份也会被一起封杀。
                state
                    .suppressed_profile_ids
                    .insert(candidate.profile.id.clone());
                IDLE_SCAN_INTERVAL
            }
        }
    }

    fn switch_session(
        &self,
        state: &mut MonitorState,
        next: &ActiveGame,
        runtime: &RuntimeManager,
        app: &AppHandle,
    ) {
        let Some(&pid) = next.pids.first() else {
            return;
        };
        let same_display = state
            .session
            .as_ref()
            .is_some_and(|session| session.owner.display_device == next.profile.display_device);
        let switched = if same_display {
            runtime.switch_automatic(&next.profile, pid, app)
        } else {
            runtime
                .finish_automatic(0, app)
                .and_then(|()| runtime.start_automatic(&next.profile, pid, app))
        };
        match switched {
            Ok(true) => {
                state.session = Some(AutoSession::new(next.profile.clone()));
                state.pending_owner = None;
            }
            Ok(false) => {
                if !runtime.automatic_active() {
                    state.session = None;
                }
            }
            Err(_) => {
                state.session = None;
                state.pending_owner = None;
                // 只抑制切换失败的这份配置，同一游戏的其他配置仍应有机会。
                state.suppressed_profile_ids.insert(next.profile.id.clone());
            }
        }
    }
}

impl AutoSession {
    fn new(owner: GameProfile) -> Self {
        Self {
            owner,
            missing_since: None,
            stabilize_until: Instant::now() + TARGET_STABILIZATION_WINDOW,
        }
    }

    fn matches_profile(&self, profile: &GameProfile) -> bool {
        self.owner.id == profile.id
            && self.owner.display_device == profile.display_device
            && self.owner.width == profile.width
            && self.owner.height == profile.height
            && self.owner.refresh_rate == profile.refresh_rate
    }
}

fn active_games(
    profiles: Vec<GameProfile>,
    running: &HashMap<String, Vec<u32>>,
    active_displays: &HashSet<String>,
) -> Vec<ActiveGame> {
    profiles
        .into_iter()
        .filter(|profile| profile.auto_switch)
        // 同一个游戏可以为不同显示器场景各存一份配置。目标显示器当前不存在的那份
        // 必然切换失败，这里直接剔除，让接管落到真正适用于当前显示器环境的配置上。
        .filter(|profile| active_displays.contains(profile.display_device.trim()))
        .filter_map(|profile| {
            let process_name = profile
                .process_name
                .trim()
                .trim_matches('"')
                .to_ascii_lowercase();
            running
                .get(&process_name)
                .filter(|pids| !pids.is_empty())
                .map(|pids| ActiveGame {
                    profile,
                    pids: pids.clone(),
                })
        })
        .collect()
}

fn select_owner<'a>(
    active: &'a [ActiveGame],
    foreground_pid: Option<u32>,
    current_owner: Option<&str>,
) -> Option<&'a ActiveGame> {
    if let Some(pid) = foreground_pid
        && let Some(game) = active.iter().find(|game| game.pids.contains(&pid))
    {
        return Some(game);
    }
    if let Some(owner) = current_owner
        && let Some(game) = active.iter().find(|game| game.profile.id == owner)
    {
        return Some(game);
    }
    active.first()
}

fn owner_switch_ready(
    pending_owner: &mut Option<(String, Instant)>,
    profile_id: &str,
    now: Instant,
) -> bool {
    match pending_owner.as_ref() {
        Some((pending_id, since)) if pending_id == profile_id => {
            if now.duration_since(*since) >= OWNER_SWITCH_DEBOUNCE {
                *pending_owner = None;
                true
            } else {
                false
            }
        }
        _ => {
            *pending_owner = Some((profile_id.to_string(), now));
            false
        }
    }
}

fn scan_interval(active: &[ActiveGame]) -> Duration {
    if active.is_empty() {
        IDLE_SCAN_INTERVAL
    } else {
        ACTIVE_SCAN_INTERVAL
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        time::Instant,
    };

    use super::{
        ActiveGame, OWNER_SWITCH_DEBOUNCE, active_games, owner_switch_ready, select_owner,
    };
    use crate::model::GameProfile;

    fn profile(id: &str, process_name: &str, auto_switch: bool) -> GameProfile {
        profile_on(id, process_name, auto_switch, "DISPLAY1")
    }

    fn profile_on(
        id: &str,
        process_name: &str,
        auto_switch: bool,
        display_device: &str,
    ) -> GameProfile {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": id,
            "launchType": "executable",
            "executable": "C:\\game.exe",
            "arguments": "",
            "steamAppId": "",
            "processName": process_name,
            "autoSwitch": auto_switch,
            "switchTiming": "beforeLaunch",
            "launchTimeoutSec": 600,
            "displayDevice": display_device,
            "width": 1920,
            "height": 1080,
            "refreshRate": 60,
            "restoreDelayMs": 2000
        }))
        .expect("profile should deserialize")
    }

    fn displays(names: &[&str]) -> HashSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    fn active(id: &str, pid: u32) -> ActiveGame {
        ActiveGame {
            profile: profile(id, &format!("{id}.exe"), true),
            pids: vec![pid],
        }
    }

    #[test]
    fn active_games_use_one_lightweight_snapshot_and_profile_opt_in() {
        let profiles = vec![
            profile("enabled", "\"Game.exe\"", true),
            profile("disabled", "Other.exe", false),
        ];
        let running = HashMap::from([
            ("game.exe".to_string(), vec![42, 43]),
            ("other.exe".to_string(), vec![99]),
        ]);

        let active = active_games(profiles, &running, &displays(&["DISPLAY1"]));
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].profile.id, "enabled");
        assert_eq!(active[0].pids, vec![42, 43]);
    }

    #[test]
    fn duplicate_profiles_are_disambiguated_by_the_active_display() {
        // 同一个进程名、两份配置，分别面向内屏和外接屏。
        let profiles = vec![
            profile_on("external", "Game.exe", true, "DISPLAY2"),
            profile_on("builtin", "Game.exe", true, "DISPLAY1"),
        ];
        let running = HashMap::from([("game.exe".to_string(), vec![42])]);

        // 拔掉外接屏：只剩内屏那份，不会再选中必然失败的 DISPLAY2 配置。
        let only_builtin = active_games(profiles.clone(), &running, &displays(&["DISPLAY1"]));
        assert_eq!(only_builtin.len(), 1);
        assert_eq!(only_builtin[0].profile.id, "builtin");

        // 接上外接屏：两份都可用，退回列表顺序，取靠前那份。
        let both = active_games(profiles, &running, &displays(&["DISPLAY1", "DISPLAY2"]));
        assert_eq!(both.len(), 2);
        assert_eq!(both[0].profile.id, "external");
    }

    #[test]
    fn foreground_game_wins_over_current_owner() {
        let active = vec![active("a", 10), active("b", 20)];
        assert_eq!(
            select_owner(&active, Some(20), Some("a")).map(|game| game.profile.id.as_str()),
            Some("b")
        );
    }

    #[test]
    fn non_game_foreground_keeps_current_owner() {
        let active = vec![active("a", 10), active("b", 20)];
        assert_eq!(
            select_owner(&active, Some(30), Some("a")).map(|game| game.profile.id.as_str()),
            Some("a")
        );
    }

    #[test]
    fn owner_switch_requires_a_stable_foreground_candidate() {
        let started = Instant::now();
        let mut pending = None;
        assert!(!owner_switch_ready(&mut pending, "b", started));
        assert!(!owner_switch_ready(
            &mut pending,
            "b",
            started + OWNER_SWITCH_DEBOUNCE / 2,
        ));
        assert!(owner_switch_ready(
            &mut pending,
            "b",
            started + OWNER_SWITCH_DEBOUNCE,
        ));
    }
}
