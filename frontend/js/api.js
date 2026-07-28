/* global window */

window.ResolutionAPI = {
    enumerateDisplays() {
        return window.__TAURI__.core.invoke("enumerate_displays");
    },
    enumerateDisplayModes(deviceName) {
        return window.__TAURI__.core.invoke("enumerate_display_modes", { deviceName });
    },
    getSettings() {
        return window.__TAURI__.core.invoke("get_settings");
    },
    setDefaultDisplayMode(mode) {
        return window.__TAURI__.core.invoke("set_default_display_mode", { mode });
    },
    getAutoStartEnabled() {
        return window.__TAURI__.core.invoke("get_auto_start_enabled");
    },
    setAutoStartEnabled(enabled) {
        return window.__TAURI__.core.invoke("set_auto_start_enabled", { enabled });
    },
    listProfiles() {
        return window.__TAURI__.core.invoke("list_profiles");
    },
    saveProfile(profile) {
        return window.__TAURI__.core.invoke("save_profile", { profile });
    },
    deleteProfile(id) {
        return window.__TAURI__.core.invoke("delete_profile", { id });
    },
    selectExecutable() {
        return window.__TAURI__.core.invoke("select_executable");
    },
    selectShortcut() {
        return window.__TAURI__.core.invoke("select_shortcut");
    },
    listSteamGames() {
        return window.__TAURI__.core.invoke("list_steam_games");
    },
    listRecentApps() {
        return window.__TAURI__.core.invoke("list_recent_apps");
    },
    openLaunchSource(request) {
        return window.__TAURI__.core.invoke("open_launch_source", { request });
    },
    detectGameProcess(request) {
        return window.__TAURI__.core.invoke("detect_game_process", { request });
    },
    launchProfile(id) {
        return window.__TAURI__.core.invoke("launch_profile", { id });
    },
    getRuntimeStatus() {
        return window.__TAURI__.core.invoke("get_runtime_status");
    },
    getPendingRestore() {
        return window.__TAURI__.core.invoke("get_pending_restore");
    },
    restoreDisplay() {
        return window.__TAURI__.core.invoke("restore_display");
    },
    cancelLaunchWait() {
        return window.__TAURI__.core.invoke("cancel_launch_wait");
    },
    minimize() {
        return window.__TAURI__.core.invoke("win_minimize");
    },
    toggleMaximize() {
        return window.__TAURI__.core.invoke("win_toggle_maximize");
    },
    close() {
        return window.__TAURI__.core.invoke("win_close");
    },
    listenRuntime(handler) {
        return window.__TAURI__.event.listen("runtime-status", (event) => handler(event.payload));
    },
};
