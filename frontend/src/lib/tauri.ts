import { getVersion as getAppVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import type { PhysicalSize } from "@tauri-apps/api/dpi";

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
// Windows WM_SIZE excludes the invisible resize frame of undecorated windows.
const WINDOW_FRAME_TOLERANCE = 32;
const MIN_STORED_WINDOW_WIDTH = MIN_WINDOW_WIDTH - WINDOW_FRAME_TOLERANCE;
const MIN_STORED_WINDOW_HEIGHT = MIN_WINDOW_HEIGHT - WINDOW_FRAME_TOLERANCE;
const MAX_WINDOW_DIMENSION = 10_000;

interface StoredWindowSize {
  width: number;
  height: number;
}

type AppWindow = ReturnType<typeof getCurrentWindow>;

let windowSizePersistenceSuspended = false;
let nativeResizeVersion = 0;

function readStoredWindowSize(): StoredWindowSize | null {
  try {
    const value = JSON.parse(window.localStorage.getItem(WINDOW_SIZE_STORAGE_KEY) ?? "null");
    if (
      Number.isInteger(value?.width)
      && Number.isInteger(value?.height)
      && value.width >= MIN_STORED_WINDOW_WIDTH
      && value.height >= MIN_STORED_WINDOW_HEIGHT
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

function clampWindowDimension(value: number, minimum: number) {
  if (!Number.isFinite(value)) {
    return minimum;
  }
  return Math.min(MAX_WINDOW_DIMENSION, Math.max(minimum, Math.round(value)));
}

function writeStoredWindowSize(size: { width: number; height: number }) {
  if (
    !Number.isFinite(size.width)
    || !Number.isFinite(size.height)
    || size.width < MIN_STORED_WINDOW_WIDTH
    || size.height < MIN_STORED_WINDOW_HEIGHT
  ) {
    return;
  }

  const value = {
    width: clampWindowDimension(size.width, MIN_STORED_WINDOW_WIDTH),
    height: clampWindowDimension(size.height, MIN_STORED_WINDOW_HEIGHT),
  };
  try {
    window.localStorage.setItem(WINDOW_SIZE_STORAGE_KEY, JSON.stringify(value));
  } catch {
    // Window persistence must never block normal window operation.
  }
}

function storeWindowSize() {
  if (windowSizePersistenceSuspended) {
    return;
  }
  writeStoredWindowSize({ width: window.innerWidth, height: window.innerHeight });
}

async function persistNativeWindowSize(
  appWindow: AppWindow,
  physicalSize: PhysicalSize,
  version: number,
) {
  const scaleFactor = await appWindow.scaleFactor();
  if (windowSizePersistenceSuspended || version !== nativeResizeVersion) {
    return;
  }
  writeStoredWindowSize(physicalSize.toLogical(scaleFactor));
}

function handleNativeWindowResize(appWindow: AppWindow, physicalSize: PhysicalSize) {
  if (windowSizePersistenceSuspended) {
    return;
  }
  const version = ++nativeResizeVersion;
  void persistNativeWindowSize(appWindow, physicalSize, version).catch(() => undefined);
}

function setWindowSizePersistenceSuspended(suspended: boolean) {
  windowSizePersistenceSuspended = suspended;
  nativeResizeVersion += 1;
}

async function ensureWindowSize(appWindow: AppWindow = getCurrentWindow()) {
  try {
    await appWindow.setSizeConstraints({
      minWidth: MIN_WINDOW_WIDTH,
      minHeight: MIN_WINDOW_HEIGHT,
    });
  } catch {
    // The static Tauri window constraints remain as a fallback.
  }

  const stored = readStoredWindowSize();
  if (stored) {
    await appWindow.setSize(new LogicalSize(stored.width, stored.height));
    return;
  }

  const scaleFactor = await appWindow.scaleFactor();
  const currentSize = (await appWindow.innerSize()).toLogical(scaleFactor);
  const target = {
    width: clampWindowDimension(currentSize.width, MIN_WINDOW_WIDTH),
    height: clampWindowDimension(currentSize.height, MIN_WINDOW_HEIGHT),
  };
  await appWindow.setSize(new LogicalSize(target.width, target.height));
}

export const tauriApi = {
  initializeWindowSizePersistence: async () => {
    const stored = readStoredWindowSize();
    let appWindow: AppWindow | undefined;
    try {
      appWindow = getCurrentWindow();
      await ensureWindowSize(appWindow);
      if (stored) {
        await appWindow.center();
      }
    } catch {
      // Browser-only development and denied window permissions keep the default size.
    }

    let releaseNativeResize: (() => void) | undefined;
    const nativeWindow = appWindow;
    if (nativeWindow) {
      try {
        releaseNativeResize = await nativeWindow.onResized(({ payload }) => {
          handleNativeWindowResize(nativeWindow, payload);
        });
      } catch {
        // Fall back to the browser resize event when native events are unavailable.
      }
    }
    if (!releaseNativeResize) {
      window.addEventListener("resize", storeWindowSize);
    }
    return () => {
      releaseNativeResize?.();
      if (!releaseNativeResize) {
        window.removeEventListener("resize", storeWindowSize);
      }
    };
  },
  ensureWindowSize,
  setWindowSizePersistenceSuspended,
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
