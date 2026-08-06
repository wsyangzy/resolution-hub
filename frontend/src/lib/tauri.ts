import { getVersion as getAppVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

import type {
  ApplicationIcon,
  ApplicationIconRequest,
  AppSettings,
  CloseBehavior,
  DetectLaunchRequest,
  DisplayInfo,
  DisplayMode,
  GameProfile,
  MinimizeBehavior,
  PendingRestore,
  RecentApplication,
  RuntimeStatus,
  SteamGame,
} from "../types";

const WINDOW_SIZE_STORAGE_KEY = "resolution-hub-window-size";
const MIN_WINDOW_WIDTH = 760;
const MIN_WINDOW_HEIGHT = 560;
const MAX_WINDOW_DIMENSION = 10_000;

interface StoredWindowSize {
  width: number;
  height: number;
}

function readStoredWindowSize(): StoredWindowSize | null {
  try {
    const value = JSON.parse(window.localStorage.getItem(WINDOW_SIZE_STORAGE_KEY) ?? "null");
    if (
      Number.isInteger(value?.width)
      && Number.isInteger(value?.height)
      && value.width >= MIN_WINDOW_WIDTH
      && value.height >= MIN_WINDOW_HEIGHT
      && value.width <= MAX_WINDOW_DIMENSION
      && value.height <= MAX_WINDOW_DIMENSION
    ) {
      return value;
    }
  } catch {
    // Ignore unavailable or invalid local storage and use the configured default size.
  }
  return null;
}

function storeWindowSize() {
  const width = window.innerWidth;
  const height = window.innerHeight;
  if (width < MIN_WINDOW_WIDTH || height < MIN_WINDOW_HEIGHT) {
    return;
  }
  try {
    window.localStorage.setItem(WINDOW_SIZE_STORAGE_KEY, JSON.stringify({ width, height }));
  } catch {
    // Window persistence must never block normal window operation.
  }
}

export const tauriApi = {
  initializeWindowSizePersistence: async () => {
    const stored = readStoredWindowSize();
    if (stored) {
      try {
        const appWindow = getCurrentWindow();
        await appWindow.setSize(new LogicalSize(stored.width, stored.height));
        await appWindow.center();
      } catch {
        // Browser-only development and denied window permissions keep the default size.
      }
    }
    window.addEventListener("resize", storeWindowSize);
    return () => window.removeEventListener("resize", storeWindowSize);
  },
  enumerateDisplays: () => invoke<DisplayInfo[]>("enumerate_displays"),
  enumerateDisplayModes: (deviceName: string) =>
    invoke<DisplayMode[]>("enumerate_display_modes", { deviceName }),
  getSettings: () => invoke<AppSettings>("get_settings"),
  setDefaultDisplayMode: (mode: DisplayMode) =>
    invoke<AppSettings>("set_default_display_mode", { mode }),
  setCloseBehavior: (
    closeBehavior: CloseBehavior,
    confirmCloseBehavior: boolean,
    minimizeBehavior: MinimizeBehavior,
  ) => invoke<AppSettings>("set_close_behavior", {
    closeBehavior,
    confirmCloseBehavior,
    minimizeBehavior,
  }),
  setRestoreOnFocusLoss: (enabled: boolean) =>
    invoke<AppSettings>("set_restore_on_focus_loss", { enabled }),
  getAutoStartEnabled: () => invoke<boolean>("get_auto_start_enabled"),
  setAutoStartEnabled: (enabled: boolean) =>
    invoke<boolean>("set_auto_start_enabled", { enabled }),
  listProfiles: () => invoke<GameProfile[]>("list_profiles"),
  saveProfile: (profile: GameProfile) =>
    invoke<GameProfile>("save_profile", { profile }),
  deleteProfile: (id: string) => invoke<void>("delete_profile", { id }),
  selectExecutable: () => invoke<string | null>("select_executable"),
  selectShortcut: () => invoke<string | null>("select_shortcut"),
  listSteamGames: () => invoke<SteamGame[]>("list_steam_games"),
  listRecentApps: () => invoke<RecentApplication[]>("list_recent_apps"),
  getApplicationIcon: (request: ApplicationIconRequest) =>
    invoke<ApplicationIcon | null>("get_application_icon", { request }),
  openLaunchSource: (request: DetectLaunchRequest) =>
    invoke<void>("open_launch_source", { request }),
  detectGameProcess: (request: DetectLaunchRequest) =>
    invoke<RecentApplication[]>("detect_game_process", { request }),
  launchProfile: (id: string) => invoke<void>("launch_profile", { id }),
  getRuntimeStatus: () => invoke<RuntimeStatus>("get_runtime_status"),
  getPendingRestore: () =>
    invoke<PendingRestore | null>("get_pending_restore"),
  restoreDisplay: () => invoke<void>("restore_display"),
  cancelLaunchWait: () => invoke<void>("cancel_launch_wait"),
  minimize: () => invoke<void>("win_minimize"),
  toggleMaximize: () => invoke<void>("win_toggle_maximize"),
  hide: () => invoke<void>("win_hide"),
  close: () => invoke<void>("win_close"),
  getVersion: () => getAppVersion(),
  listenRuntime: (handler: (status: RuntimeStatus) => void) =>
    listen<RuntimeStatus>("runtime-status", ({ payload }) => handler(payload)),
  listenWindowCloseRequested: (handler: () => void) =>
    listen<void>("window-close-requested", handler),
  listenTrayExitRequested: (handler: () => void) =>
    listen<void>("tray-exit-requested", handler),
};
