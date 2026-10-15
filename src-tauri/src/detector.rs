use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    launcher,
    model::{DetectLaunchRequest, LaunchType, RecentApplication},
    process, recent,
};

pub fn detect(request: &DetectLaunchRequest) -> Result<Vec<RecentApplication>, String> {
    detect_with_cancel(request, || false)
}

/// 原生编辑器可停止识别；取消只结束扫描，不终止用户启动的游戏或启动器。
pub fn detect_with_cancel(
    request: &DetectLaunchRequest,
    is_cancelled: impl Fn() -> bool,
) -> Result<Vec<RecentApplication>, String> {
    let check_cancelled = || {
        if is_cancelled() {
            Err("已取消游戏进程识别".to_string())
        } else {
            Ok(())
        }
    };
    check_cancelled()?;
    let baseline: HashSet<u32> = process::snapshot()
        .into_iter()
        .map(|process| process.pid)
        .collect();
    if request.launch_before_detect {
        check_cancelled()?;
        launcher::launch_request(request).map(|_| ())?;
    }

    let timeout_sec = if matches!(request.launch_type, LaunchType::Shortcut) {
        120
    } else {
        60
    };
    let deadline = Instant::now() + Duration::from_secs(timeout_sec);
    let mut candidates = HashMap::<String, RecentApplication>::new();
    let mut visible_since = None;

    while Instant::now() < deadline {
        check_cancelled()?;
        for application in recent::scan_visible() {
            check_cancelled()?;
            if baseline.contains(&application.pid) || ignored(&application.process_name) {
                continue;
            }
            candidates.insert(application.process_name.to_lowercase(), application);
        }

        if !candidates.is_empty() {
            let first_seen = visible_since.get_or_insert_with(Instant::now);
            if first_seen.elapsed() >= Duration::from_secs(2) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(750));
    }

    check_cancelled()?;
    if candidates.is_empty() {
        for process in process::snapshot() {
            check_cancelled()?;
            if baseline.contains(&process.pid) || ignored(&process.process_name) {
                continue;
            }
            let Some(executable) = process.executable else {
                continue;
            };
            candidates.insert(
                process.process_name.to_lowercase(),
                RecentApplication {
                    pid: process.pid,
                    name: process.process_name.trim_end_matches(".exe").to_string(),
                    executable: executable.to_string_lossy().into_owned(),
                    process_name: process.process_name,
                    last_seen: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|duration| duration.as_secs())
                        .unwrap_or(0),
                },
            );
        }
    }

    check_cancelled()?;
    let mut result: Vec<_> = candidates.into_values().collect();
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    if result.is_empty() {
        Err(format!("{timeout_sec} 秒内没有检测到新的游戏进程"))
    } else {
        Ok(result)
    }
}

fn ignored(process_name: &str) -> bool {
    const IGNORED: &[&str] = &[
        "cmd.exe",
        "conhost.exe",
        "agent.exe",
        "battle.net.exe",
        "crashreportclient.exe",
        "crashpad_handler.exe",
        "eabackgroundservice.exe",
        "eadesktop.exe",
        "ealauncher.exe",
        "ealocalhostsvc.exe",
        "easyanticheat.exe",
        "easyanticheat_eos.exe",
        "epicgameslauncher.exe",
        "epicwebhelper.exe",
        "gameoverlayui.exe",
        "galaxyclient helper.exe",
        "galaxyclient.exe",
        "launcherpatcher.exe",
        "rockstar games launcher.exe",
        "riotclientcrashhandler.exe",
        "riotclientservices.exe",
        "riotclientux.exe",
        "riotclientuxrender.exe",
        "socialclubhelper.exe",
        "steam.exe",
        "steamerrorreporter.exe",
        "steamservice.exe",
        "steamwebhelper.exe",
        "unrealcefsubprocess.exe",
        "ubisoftconnect.exe",
        "upc.exe",
        "uplaywebcore.exe",
        "valorant.exe",
        "vgc.exe",
        "vgm.exe",
    ];
    IGNORED
        .iter()
        .any(|ignored| process_name.eq_ignore_ascii_case(ignored))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_detection_never_opens_the_launch_source() {
        let request = DetectLaunchRequest {
            launch_type: LaunchType::Executable,
            executable: "must-not-launch.exe".into(),
            arguments: String::new(),
            working_directory: String::new(),
            steam_app_id: String::new(),
            shortcut_path: String::new(),
            launcher_process_name: String::new(),
            launch_before_detect: true,
        };
        assert_eq!(
            detect_with_cancel(&request, || true).unwrap_err(),
            "已取消游戏进程识别"
        );
    }

    #[test]
    fn ignores_common_launcher_and_bootstrap_processes() {
        assert!(ignored("RiotClientServices.exe"));
        assert!(ignored("EpicGamesLauncher.exe"));
        assert!(ignored("Battle.net.exe"));
        assert!(ignored("VALORANT.exe"));
        assert!(!ignored("VALORANT-Win64-Shipping.exe"));
    }
}
