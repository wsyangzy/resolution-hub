use std::thread;

use tauri::{
    App, AppHandle, Emitter, Manager, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::{
    model::{AppSettings, RuntimeStatus},
    profile,
    runtime::RuntimeManager,
};

const TRAY_ID: &str = "resolution-hub";
const MENU_STATUS: &str = "tray-status";
const MENU_OPEN: &str = "tray-open";
const MENU_AUTO_SWITCH: &str = "tray-auto-switch";
const MENU_GAME_PREFIX: &str = "tray-game:";
const MENU_GAME_EMPTY: &str = "tray-game-empty";
const MENU_RESTORE: &str = "tray-restore";
const MENU_EXIT: &str = "tray-exit";
const MAX_TRAY_GAMES: usize = 5;
const GAME_INSERT_INDEX: usize = 4;

pub struct TrayState {
    menu: Menu<Wry>,
    status_item: MenuItem<Wry>,
    auto_switch_item: CheckMenuItem<Wry>,
    restore_item: MenuItem<Wry>,
}

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(app, MENU_STATUS, "状态：就绪", false, None::<&str>)?;
    let open_item = MenuItem::with_id(app, MENU_OPEN, "打开 Resolution Hub", true, None::<&str>)?;
    let auto_switch_enabled = profile::load_settings()
        .map(|settings| settings.auto_switch_enabled)
        .unwrap_or(false);
    let auto_switch_item = CheckMenuItem::with_id(
        app,
        MENU_AUTO_SWITCH,
        "自动接管外部游戏",
        true,
        auto_switch_enabled,
        None::<&str>,
    )?;
    let games_separator = PredefinedMenuItem::separator(app)?;
    let actions_separator = PredefinedMenuItem::separator(app)?;
    let restore_item = MenuItem::with_id(
        app,
        MENU_RESTORE,
        "恢复桌面显示模式",
        matches!(profile::load_pending(), Ok(Some(_))),
        None::<&str>,
    )?;
    let exit_item = MenuItem::with_id(app, MENU_EXIT, "退出程序", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &open_item,
            &auto_switch_item,
            &games_separator,
            &actions_separator,
            &restore_item,
            &exit_item,
        ],
    )?;
    populate_games(app, &menu)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("Resolution Hub")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                MENU_OPEN => show_main_window(app),
                MENU_AUTO_SWITCH => update_auto_switch_from_tray(app),
                MENU_RESTORE => restore_from_tray(app.clone()),
                MENU_EXIT => {
                    show_main_window(app);
                    let _ = app.emit("tray-exit-requested", ());
                }
                _ if id.starts_with(MENU_GAME_PREFIX) => {
                    launch_game_from_tray(app.clone(), id[MENU_GAME_PREFIX.len()..].to_string());
                }
                _ => {}
            }
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
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    app.manage(TrayState {
        menu,
        status_item,
        auto_switch_item,
        restore_item,
    });
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn set_auto_switch_enabled(app: &AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let settings = profile::save_auto_switch_enabled(enabled)?;
    if let Some(tray) = app.try_state::<TrayState>() {
        let _ = tray.auto_switch_item.set_checked(enabled);
    }
    crate::auto_monitor::wake(app);
    let _ = app.emit("settings-changed", settings.clone());
    Ok(settings)
}

fn update_auto_switch_from_tray(app: &AppHandle) {
    let Some(tray) = app.try_state::<TrayState>() else {
        return;
    };
    let requested = tray.auto_switch_item.is_checked().unwrap_or(false);
    if let Err(error) = set_auto_switch_enabled(app, requested) {
        if let Ok(settings) = profile::load_settings() {
            let _ = tray
                .auto_switch_item
                .set_checked(settings.auto_switch_enabled);
        }
        let _ = app.emit(
            "tray-action-error",
            format!("更新自动接管设置失败：{error}"),
        );
    }
}

pub fn update_status(app: &AppHandle, status: &RuntimeStatus) {
    let Some(tray) = app.try_state::<TrayState>() else {
        return;
    };
    let message = status.message.replace('&', "&&");
    let _ = tray.status_item.set_text(format!("状态：{message}"));
    let session_available = matches!(status.phase.as_str(), "idle" | "error");
    set_game_items_enabled(&tray.menu, session_available);
    let restore_available = status.phase != "restoring"
        && (status.can_restore || matches!(profile::load_pending(), Ok(Some(_))));
    let _ = tray.restore_item.set_enabled(restore_available);
}

pub fn refresh_games(app: &AppHandle) {
    let Some(tray) = app.try_state::<TrayState>() else {
        return;
    };
    remove_game_items(&tray.menu);
    let _ = populate_games(app, &tray.menu);
    let status = app.state::<RuntimeManager>().status();
    set_game_items_enabled(
        &tray.menu,
        matches!(status.phase.as_str(), "idle" | "error"),
    );
}

fn populate_games<M: Manager<Wry>>(manager: &M, menu: &Menu<Wry>) -> tauri::Result<()> {
    let games = profile::most_used(MAX_TRAY_GAMES).unwrap_or_default();
    if games.is_empty() {
        let empty = MenuItem::with_id(
            manager,
            MENU_GAME_EMPTY,
            "尚未添加游戏",
            false,
            None::<&str>,
        )?;
        menu.insert(&empty, GAME_INSERT_INDEX)?;
        return Ok(());
    }

    for (index, game) in games.iter().enumerate() {
        let name = game.name.replace('&', "&&");
        let item = MenuItem::with_id(
            manager,
            format!("{MENU_GAME_PREFIX}{}", game.id),
            format!("{}. {name}", index + 1),
            true,
            None::<&str>,
        )?;
        menu.insert(&item, GAME_INSERT_INDEX + index)?;
    }
    Ok(())
}

fn remove_game_items(menu: &Menu<Wry>) {
    for item in menu.items().unwrap_or_default() {
        let id = item.id().as_ref();
        if (id.starts_with(MENU_GAME_PREFIX) || id == MENU_GAME_EMPTY)
            && let Some(item) = item.as_menuitem()
        {
            let _ = menu.remove(item);
        }
    }
}

fn set_game_items_enabled(menu: &Menu<Wry>, enabled: bool) {
    for item in menu.items().unwrap_or_default() {
        if item.id().as_ref().starts_with(MENU_GAME_PREFIX)
            && let Some(item) = item.as_menuitem()
        {
            let _ = item.set_enabled(enabled);
        }
    }
}

fn restore_from_tray(app: AppHandle) {
    thread::spawn(move || {
        if let Err(error) = app.state::<RuntimeManager>().restore_now(&app) {
            show_main_window(&app);
            let _ = app.emit(
                "tray-action-error",
                format!("恢复桌面显示模式失败：{error}"),
            );
        }
    });
}

fn launch_game_from_tray(app: AppHandle, profile_id: String) {
    thread::spawn(move || {
        let result = profile::find(&profile_id)
            .and_then(|game| app.state::<RuntimeManager>().start(game, app.clone()));
        if let Err(error) = result {
            show_main_window(&app);
            let _ = app.emit("tray-action-error", error);
        }
    });
}
