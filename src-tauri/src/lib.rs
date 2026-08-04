mod autostart;
mod detector;
mod dialog;
mod display;
mod icon;
mod launcher;
mod model;
mod process;
mod profile;
mod recent;
mod runtime;
mod steam;

use model::{
    AppSettings, ApplicationIcon, ApplicationIconRequest, CloseBehavior, DetectLaunchRequest,
    DisplayInfo, DisplayMode, GameProfile, MinimizeBehavior, PendingRestore, RecentApplication,
    RuntimeStatus, SteamGame,
};
use runtime::RuntimeManager;
use tauri::{
    Emitter, Manager, State,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[tauri::command]
fn enumerate_displays() -> Result<Vec<DisplayInfo>, String> {
    display::enumerate_displays()
}

#[tauri::command]
fn enumerate_display_modes(device_name: String) -> Result<Vec<DisplayMode>, String> {
    display::enumerate_modes(&device_name)
}

#[tauri::command]
fn get_settings() -> Result<AppSettings, String> {
    profile::load_settings()
}

#[tauri::command]
fn set_default_display_mode(
    mode: DisplayMode,
    runtime: State<'_, RuntimeManager>,
) -> Result<AppSettings, String> {
    let status = runtime.status();
    if status.phase != "idle" && status.phase != "error" {
        return Err("游戏运行期间不能修改默认分辨率".to_string());
    }

    let supported = display::enumerate_modes(&mode.device_name)?;
    let selected = supported
        .into_iter()
        .find(|item| {
            item.width == mode.width
                && item.height == mode.height
                && item.refresh_rate == mode.refresh_rate
        })
        .ok_or_else(|| "所选显示模式已不可用，请刷新后重试".to_string())?;

    display::switch_mode_persistent(&selected)?;
    let current = display::current_mode(&selected.device_name)?;
    if current.width != selected.width
        || current.height != selected.height
        || current.refresh_rate != selected.refresh_rate
    {
        return Err(format!("显示模式验证失败，当前为 {}", current.label()));
    }
    profile::save_default_display_mode(selected)
}

#[tauri::command]
fn set_close_behavior(
    close_behavior: CloseBehavior,
    confirm_close_behavior: Option<bool>,
    minimize_behavior: Option<MinimizeBehavior>,
) -> Result<AppSettings, String> {
    profile::save_close_behavior(close_behavior, confirm_close_behavior, minimize_behavior)
}

#[tauri::command]
fn get_auto_start_enabled() -> Result<bool, String> {
    autostart::is_enabled()
}

#[tauri::command]
fn set_auto_start_enabled(enabled: bool) -> Result<bool, String> {
    autostart::set_enabled(enabled)
}

#[tauri::command]
fn list_profiles() -> Result<Vec<GameProfile>, String> {
    profile::list()
}

#[tauri::command]
fn save_profile(profile: GameProfile) -> Result<GameProfile, String> {
    profile::save(profile)
}

#[tauri::command]
fn delete_profile(id: String) -> Result<(), String> {
    profile::delete(&id)
}

#[tauri::command]
fn select_executable() -> Option<String> {
    dialog::select_executable()
}

#[tauri::command]
fn select_shortcut() -> Option<String> {
    dialog::select_shortcut()
}

#[tauri::command]
fn list_steam_games() -> Result<Vec<SteamGame>, String> {
    steam::list_installed_games()
}

#[tauri::command]
fn list_recent_apps() -> Vec<RecentApplication> {
    recent::list()
}

#[tauri::command]
async fn get_application_icon(request: ApplicationIconRequest) -> Option<ApplicationIcon> {
    tauri::async_runtime::spawn_blocking(move || icon::load(&request))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
fn open_launch_source(request: DetectLaunchRequest) -> Result<(), String> {
    launcher::launch_request(&request)
}

#[tauri::command]
async fn detect_game_process(
    request: DetectLaunchRequest,
) -> Result<Vec<RecentApplication>, String> {
    tauri::async_runtime::spawn_blocking(move || detector::detect(&request))
        .await
        .map_err(|error| format!("进程侦测任务失败：{error}"))?
}

#[tauri::command]
fn launch_profile(
    id: String,
    app: tauri::AppHandle,
    runtime: State<'_, RuntimeManager>,
) -> Result<(), String> {
    let game = profile::find(&id)?;
    runtime.start(game, app)
}

#[tauri::command]
fn get_runtime_status(runtime: State<'_, RuntimeManager>) -> RuntimeStatus {
    runtime.status()
}

#[tauri::command]
fn get_pending_restore() -> Result<Option<PendingRestore>, String> {
    profile::load_pending()
}

#[tauri::command]
fn restore_display(
    app: tauri::AppHandle,
    runtime: State<'_, RuntimeManager>,
) -> Result<(), String> {
    runtime.restore_now(&app)
}

#[tauri::command]
fn cancel_launch_wait(
    app: tauri::AppHandle,
    runtime: State<'_, RuntimeManager>,
) -> Result<(), String> {
    runtime.cancel_wait(&app)
}

#[tauri::command]
fn win_minimize(window: tauri::Window) -> Result<(), String> {
    match profile::load_settings()?.minimize_behavior {
        MinimizeBehavior::Taskbar => window
            .minimize()
            .map_err(|error| format!("最小化主窗口失败：{error}")),
        MinimizeBehavior::HideToTray => window
            .hide()
            .map_err(|error| format!("隐藏主窗口失败：{error}")),
    }
}

#[tauri::command]
fn win_toggle_maximize(window: tauri::Window) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
fn win_hide(window: tauri::Window) -> Result<(), String> {
    window
        .hide()
        .map_err(|error| format!("隐藏主窗口失败：{error}"))
}

#[tauri::command]
fn win_close(app: tauri::AppHandle) {
    app.exit(0);
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "tray-open", "打开 Resolution Hub", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let exit = MenuItem::with_id(app, "tray-exit", "退出程序", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &separator, &exit])?;

    let mut tray = TrayIconBuilder::with_id("resolution-hub")
        .menu(&menu)
        .tooltip("Resolution Hub")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-open" => show_main_window(app),
            "tray-exit" => {
                show_main_window(app);
                let _ = app.emit("tray-exit-requested", ());
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeManager::default())
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.emit("window-close-requested", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            enumerate_displays,
            enumerate_display_modes,
            get_settings,
            set_default_display_mode,
            set_close_behavior,
            get_auto_start_enabled,
            set_auto_start_enabled,
            list_profiles,
            save_profile,
            delete_profile,
            select_executable,
            select_shortcut,
            list_steam_games,
            list_recent_apps,
            get_application_icon,
            open_launch_source,
            detect_game_process,
            launch_profile,
            get_runtime_status,
            get_pending_restore,
            restore_display,
            cancel_launch_wait,
            win_minimize,
            win_toggle_maximize,
            win_hide,
            win_close,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Resolution Hub 失败");
}
