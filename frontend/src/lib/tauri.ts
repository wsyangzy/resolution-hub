import { getVersion as getAppVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { currentMonitor, getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
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
const WORK_AREA_SETTLE_ATTEMPTS = 8;
const WORK_AREA_SETTLE_INTERVAL_MS = 120;

interface StoredWindowSize {
  width: number;
  height: number;
}

type AppWindow = ReturnType<typeof getCurrentWindow>;

let runtimeWindowSizePersistenceSuspended = false;
let transientWindowSizePersistenceLocks = 0;
let nativeSizeEventVersion = 0;

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

function isWindowSizePersistenceSuspended() {
  return runtimeWindowSizePersistenceSuspended || transientWindowSizePersistenceLocks > 0;
}

function storeWindowSize() {
  if (isWindowSizePersistenceSuspended()) {
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
  persistNativeWindowSizeAtScale(physicalSize, scaleFactor, version);
}

function persistNativeWindowSizeAtScale(
  physicalSize: PhysicalSize,
  scaleFactor: number,
  version: number,
) {
  if (isWindowSizePersistenceSuspended() || version !== nativeSizeEventVersion) {
    return;
  }
  writeStoredWindowSize(physicalSize.toLogical(scaleFactor));
}

function handleNativeWindowResize(appWindow: AppWindow, physicalSize: PhysicalSize) {
  if (isWindowSizePersistenceSuspended()) {
    return;
  }
  const version = ++nativeSizeEventVersion;
  void persistNativeWindowSize(appWindow, physicalSize, version).catch(() => undefined);
}

function handleNativeWindowScaleChange(physicalSize: PhysicalSize, scaleFactor: number) {
  if (isWindowSizePersistenceSuspended()) {
    return;
  }
  const version = ++nativeSizeEventVersion;
  persistNativeWindowSizeAtScale(physicalSize, scaleFactor, version);
}

function setWindowSizePersistenceSuspended(suspended: boolean) {
  runtimeWindowSizePersistenceSuspended = suspended;
  nativeSizeEventVersion += 1;
}

function acquireWindowSizePersistenceLock() {
  transientWindowSizePersistenceLocks += 1;
  nativeSizeEventVersion += 1;
  return () => {
    transientWindowSizePersistenceLocks = Math.max(0, transientWindowSizePersistenceLocks - 1);
    nativeSizeEventVersion += 1;
  };
}

async function getLogicalWorkAreaLimit(): Promise<StoredWindowSize | null> {
  try {
    const monitor = await currentMonitor();
    if (!monitor) {
      return null;
    }
    const workArea = monitor.workArea.size.toLogical(monitor.scaleFactor);
    return {
      width: Math.max(1, Math.floor(workArea.width - WINDOW_FRAME_TOLERANCE)),
      height: Math.max(1, Math.floor(workArea.height - WINDOW_FRAME_TOLERANCE)),
    };
  } catch {
    return null;
  }
}

/// 显示模式切换后 Windows 需要一小段时间才会广播新的工作区尺寸。在它稳定之前
/// 归一化窗口，会把窗口按旧分辨率的工作区夹小，随后解除持久化暂停时又把这个
/// 错误尺寸写进 localStorage，导致恢复桌面分辨率后布局再也回不到原状。
async function waitForStableWorkArea() {
  let previous: StoredWindowSize | null = null;
  for (let attempt = 0; attempt < WORK_AREA_SETTLE_ATTEMPTS; attempt += 1) {
    const current = await getLogicalWorkAreaLimit();
    if (
      current
      && previous
      && current.width === previous.width
      && current.height === previous.height
    ) {
      return;
    }
    previous = current;
    await new Promise((resolve) => {
      window.setTimeout(resolve, WORK_AREA_SETTLE_INTERVAL_MS);
    });
  }
}

async function ensureWindowSize(
  appWindow: AppWindow = getCurrentWindow(),
  options: { settleDisplay?: boolean } = {},
) {
  if (options.settleDisplay) {
    await waitForStableWorkArea();
  }
  const workAreaLimit = await getLogicalWorkAreaLimit();
  const minimumWidth = workAreaLimit
    ? Math.min(MIN_WINDOW_WIDTH, workAreaLimit.width)
    : MIN_WINDOW_WIDTH;
  const minimumHeight = workAreaLimit
    ? Math.min(MIN_WINDOW_HEIGHT, workAreaLimit.height)
    : MIN_WINDOW_HEIGHT;
  try {
    await appWindow.setSizeConstraints({
      minWidth: minimumWidth,
      minHeight: minimumHeight,
    });
  } catch {
    // The static Tauri window constraints remain as a fallback.
  }

  const stored = readStoredWindowSize();
  let requested = stored;
  if (!requested) {
    const scaleFactor = await appWindow.scaleFactor();
    requested = (await appWindow.innerSize()).toLogical(scaleFactor);
  }

  const target = {
    width: Math.min(
      clampWindowDimension(
        requested.width,
        stored ? MIN_STORED_WINDOW_WIDTH : minimumWidth,
      ),
      workAreaLimit?.width ?? MAX_WINDOW_DIMENSION,
    ),
    height: Math.min(
      clampWindowDimension(
        requested.height,
        stored ? MIN_STORED_WINDOW_HEIGHT : minimumHeight,
      ),
      workAreaLimit?.height ?? MAX_WINDOW_DIMENSION,
    ),
  };
  await appWindow.setSize(new LogicalSize(target.width, target.height));
  if (target.width < requested.width || target.height < requested.height) {
    await appWindow.center().catch(() => undefined);
  }
  if (!stored && !isWindowSizePersistenceSuspended()) {
    writeStoredWindowSize(target);
  }
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
    let releaseNativeScaleChange: (() => void) | undefined;
    const nativeWindow = appWindow;
    if (nativeWindow) {
      try {
        releaseNativeResize = await nativeWindow.onResized(({ payload }) => {
          handleNativeWindowResize(nativeWindow, payload);
        });
      } catch {
        // Fall back to the browser resize event when native events are unavailable.
      }
      try {
        releaseNativeScaleChange = await nativeWindow.onScaleChanged(({ payload }) => {
          handleNativeWindowScaleChange(payload.size, payload.scaleFactor);
        });
      } catch {
        // Resize events still preserve the window size on runtimes without scale events.
      }
    }
    if (!releaseNativeResize) {
      window.addEventListener("resize", storeWindowSize);
    }
    return () => {
      releaseNativeResize?.();
      releaseNativeScaleChange?.();
      if (!releaseNativeResize) {
        window.removeEventListener("resize", storeWindowSize);
      }
    };
  },
  ensureWindowSize,
  settleWindowSizeAfterDisplayChange: () =>
    ensureWindowSize(getCurrentWindow(), { settleDisplay: true }),
  setWindowSizePersistenceSuspended,
  enumerateDisplays: () => invoke<DisplayInfo[]>("enumerate_displays"),
  enumerateDisplayModes: (deviceName: string) =>
    invoke<DisplayMode[]>("enumerate_display_modes", { deviceName }),
  getSettings: () => invoke<AppSettings>("get_settings"),
  setDefaultDisplayMode: async (mode: DisplayMode) => {
    const releasePersistenceLock = acquireWindowSizePersistenceLock();
    try {
      const settings = await invoke<AppSettings>("set_default_display_mode", { mode });
      await ensureWindowSize().catch(() => undefined);
      return settings;
    } finally {
      releasePersistenceLock();
    }
  },
  setCloseBehavior: (
    closeBehavior: CloseBehavior,
    confirmCloseBehavior: boolean,
    minimizeBehavior: MinimizeBehavior,
  ) => invoke<AppSettings>("set_close_behavior", {
    closeBehavior,
    confirmCloseBehavior,
    minimizeBehavior,
  }),
  openProjectHomepage: () => invoke<void>("open_project_homepage"),
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
    invoke<boolean>("open_launch_source", { request }),
  detectGameProcess: (request: DetectLaunchRequest) =>
    invoke<RecentApplication[]>("detect_game_process", { request }),
  launchProfile: async (id: string) => {
    const releasePersistenceLock = acquireWindowSizePersistenceLock();
    try {
      await invoke<void>("launch_profile", { id });
    } finally {
      releasePersistenceLock();
    }
  },
  getRuntimeStatus: () => invoke<RuntimeStatus>("get_runtime_status"),
  getPendingRestore: () =>
    invoke<PendingRestore | null>("get_pending_restore"),
  restoreDisplay: () => invoke<void>("restore_display"),
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
  listenTrayActionError: (handler: (message: string) => void) =>
    listen<string>("tray-action-error", ({ payload }) => handler(payload)),
};
