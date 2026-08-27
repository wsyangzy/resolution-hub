export type LaunchType = "executable" | "steam" | "shortcut";

export type SwitchTiming = "beforeLaunch" | "onProcessDetected";

export type RuntimePhase =
  | "idle"
  | "preparing"
  | "switching"
  | "launching"
  | "waiting"
  | "running"
  | "restoring"
  | "error";

export type CloseBehavior = "hideToTray" | "exit";

export type MinimizeBehavior = "taskbar" | "hideToTray";

export interface DisplayMode {
  deviceName: string;
  width: number;
  height: number;
  refreshRate: number;
  bitsPerPixel: number;
}

export interface DisplayInfo {
  deviceName: string;
  friendlyName: string;
  isPrimary: boolean;
  currentMode: DisplayMode;
}

export interface AppSettings {
  defaultDisplayModes: DisplayMode[];
  minimizeBehavior: MinimizeBehavior;
  closeBehavior: CloseBehavior;
  confirmCloseBehavior: boolean;
}

export interface ApplicationIconRequest {
  executable?: string;
  shortcutPath?: string;
  steamAppId?: string;
  processName?: string;
}

export interface ApplicationIcon {
  kind: "encoded" | "rgba";
  mimeType: string;
  width: number;
  height: number;
  data: number[];
}

export interface GameProfile {
  id: string;
  name: string;
  launchType: LaunchType;
  executable: string;
  arguments: string;
  workingDirectory: string;
  steamAppId: string;
  shortcutPath: string;
  launcherProvider: string;
  launcherProcessName: string;
  processName: string;
  switchTiming: SwitchTiming | null;
  launchTimeoutSec: number;
  displayDevice: string;
  width: number;
  height: number;
  refreshRate: number;
  restoreDelayMs: number;
}

export interface SteamGame {
  appId: string;
  name: string;
  installDir: string;
}

export interface RecentApplication {
  pid: number;
  name: string;
  executable: string;
  processName: string;
  lastSeen: number;
}

export interface DetectLaunchRequest {
  launchType: LaunchType;
  executable: string;
  arguments: string;
  workingDirectory: string;
  steamAppId: string;
  shortcutPath: string;
  launcherProcessName: string;
  launchBeforeDetect: boolean;
}

export interface PendingRestore {
  profileName: string;
  mode: DisplayMode;
}

export interface RuntimeStatus {
  phase: RuntimePhase;
  profileId: string | null;
  profileName: string | null;
  message: string;
  canRestore: boolean;
}
