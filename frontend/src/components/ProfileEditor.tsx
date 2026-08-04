import {
  ArrowLeft,
  Check,
  Clock3,
  FolderOpen,
  Gamepad2,
  History,
  Link2,
  Monitor,
  Radar,
  RefreshCw,
  Save,
  Search,
  Shield,
  SquareTerminal,
} from "lucide-react";
import {
  type FormEvent,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";

import { tauriApi } from "../lib/tauri";
import type {
  DetectLaunchRequest,
  DisplayInfo,
  DisplayMode,
  GameProfile,
  LaunchType,
  RecentApplication,
  SteamGame,
  SwitchTiming,
} from "../types";
import { AppIcon } from "./AppIcon";
import {
  Accordion,
  SelectControl,
  SlidingTabs,
  Tooltip,
  type SelectOption,
  type TabOption,
} from "./ui";

type SourceKind = "steam" | "shortcut" | "executable" | "recent";
type ToastTone = "error" | "success" | "info";

interface SelectedSource {
  key: string;
  name: string;
  detail: string;
  type: string;
}

interface DraftState {
  id: string;
  source: SourceKind;
  launchType: LaunchType;
  name: string;
  executable: string;
  arguments: string;
  workingDirectory: string;
  steamAppId: string;
  shortcutPath: string;
  launcherProvider: string;
  processName: string;
  switchTiming: SwitchTiming;
  launchTimeoutMinutes: string;
  displayDevice: string;
  modeKey: string;
  restoreDelaySeconds: string;
  selectedSource: SelectedSource | null;
  dirty: boolean;
}

type DraftAction =
  | { type: "reset"; state: DraftState }
  | { type: "patch"; patch: Partial<DraftState>; dirty?: boolean }
  | { type: "saved" };

export interface ProfileEditorProps {
  profile: GameProfile | null;
  profiles: GameProfile[];
  displays: DisplayInfo[];
  busy: boolean;
  onSave: (profile: GameProfile) => Promise<unknown> | unknown;
  onBack: () => void;
  onDirtyChange: (dirty: boolean) => void;
  onToast: (message: string, tone?: ToastTone) => void;
}

const sourceTabs: Array<TabOption<SourceKind>> = [
  { value: "steam", label: "Steam", icon: <Gamepad2 size={15} /> },
  { value: "shortcut", label: "启动器", icon: <Link2 size={15} /> },
  { value: "executable", label: "本地 EXE", icon: <SquareTerminal size={15} /> },
  { value: "recent", label: "最近运行", icon: <History size={15} /> },
];

const VALORANT_PROCESS_NAME = "VALORANT-Win64-Shipping.exe";

const switchTimingOptions: Array<SelectOption<SwitchTiming>> = [
  { value: "beforeLaunch", label: "启动命令之前" },
  { value: "onProcessDetected", label: "检测到游戏进程时" },
];

function modeKey(mode: Pick<DisplayMode, "width" | "height" | "refreshRate">) {
  return `${mode.width}x${mode.height}@${mode.refreshRate}`;
}

function initialDraft(profile: GameProfile | null, displays: DisplayInfo[]): DraftState {
  const launchType = profile?.launchType ?? "steam";
  const source: SourceKind = launchType === "executable" ? "executable" : launchType;
  const display = displays.find((item) => item.deviceName === profile?.displayDevice)
    ?? displays.find((item) => item.isPrimary)
    ?? displays[0];
  const selectedMode = profile
    ? { width: profile.width, height: profile.height, refreshRate: profile.refreshRate }
    : display?.currentMode;
  const selectedSource = profile
    ? {
        key: launchType === "steam"
          ? `steam:${profile.steamAppId}`
          : launchType === "shortcut"
            ? `shortcut:${profile.shortcutPath.toLowerCase()}`
            : `executable:${profile.executable.toLowerCase()}`,
        name: profile.name,
        detail: launchType === "steam"
          ? `Steam App ID ${profile.steamAppId}`
          : launchType === "shortcut"
            ? profile.shortcutPath
            : profile.executable,
        type: launchType === "steam"
          ? "STEAM"
          : launchType === "shortcut"
            ? (profile.launcherProvider === "Riot Client" ? "RIOT" : "启动器")
            : "EXE",
      }
    : null;

  return {
    id: profile?.id ?? "",
    source,
    launchType,
    name: profile?.name ?? "",
    executable: profile?.executable ?? "",
    arguments: profile?.arguments ?? "",
    workingDirectory: profile?.workingDirectory ?? "",
    steamAppId: profile?.steamAppId ?? "",
    shortcutPath: profile?.shortcutPath ?? "",
    launcherProvider: profile?.launcherProvider ?? "",
    processName: profile?.processName ?? "",
    switchTiming: profile?.switchTiming
      ?? (launchType === "executable" ? "beforeLaunch" : "onProcessDetected"),
    launchTimeoutMinutes: String((profile?.launchTimeoutSec ?? 600) / 60),
    displayDevice: profile?.displayDevice ?? display?.deviceName ?? "",
    modeKey: selectedMode ? modeKey(selectedMode) : "",
    restoreDelaySeconds: String((profile?.restoreDelayMs ?? 2000) / 1000),
    selectedSource,
    dirty: false,
  };
}

function draftReducer(state: DraftState, action: DraftAction): DraftState {
  switch (action.type) {
    case "reset":
      return action.state;
    case "patch":
      return {
        ...state,
        ...action.patch,
        dirty: action.dirty ?? true,
      };
    case "saved":
      return { ...state, dirty: false };
  }
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  const message = String(error ?? "").trim();
  return message || "发生未知错误";
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? "";
}

export function ProfileEditor({
  profile,
  profiles,
  displays,
  busy,
  onSave,
  onBack,
  onDirtyChange,
  onToast,
}: ProfileEditorProps) {
  const [draft, dispatch] = useReducer(draftReducer, initialDraft(profile, displays));
  const [modes, setModes] = useState<DisplayMode[]>([]);
  const [modesLoading, setModesLoading] = useState(false);
  const [modesError, setModesError] = useState<string | null>(null);
  const [modesReload, setModesReload] = useState(0);
  const [steamGames, setSteamGames] = useState<SteamGame[]>([]);
  const [steamSearch, setSteamSearch] = useState("");
  const [steamLoading, setSteamLoading] = useState(true);
  const [steamError, setSteamError] = useState<string | null>(null);
  const [recentApps, setRecentApps] = useState<RecentApplication[]>([]);
  const [recentLoading, setRecentLoading] = useState(true);
  const [recentError, setRecentError] = useState<string | null>(null);
  const [launcherReady, setLauncherReady] = useState(false);
  const [detecting, setDetecting] = useState(false);
  const [detectStatus, setDetectStatus] = useState<string | null>(null);
  const [candidates, setCandidates] = useState<RecentApplication[]>([]);
  const [saving, setSaving] = useState(false);
  const mountedRef = useRef(false);
  const modesRequestRef = useRef(0);
  const detectRequestRef = useRef(0);
  const profileKeyRef = useRef(profile?.id ?? "__new__");

  const patch = useCallback((next: Partial<DraftState>, dirty = true) => {
    dispatch({ type: "patch", patch: next, dirty });
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      modesRequestRef.current += 1;
      detectRequestRef.current += 1;
    };
  }, []);

  useEffect(() => {
    const profileKey = profile?.id ?? "__new__";
    if (profileKeyRef.current === profileKey) {
      return;
    }
    profileKeyRef.current = profileKey;
    dispatch({ type: "reset", state: initialDraft(profile, displays) });
    setLauncherReady(false);
    setDetectStatus(null);
    setCandidates([]);
  }, [displays, profile]);

  useEffect(() => {
    if (draft.displayDevice || displays.length === 0) {
      return;
    }
    const display = displays.find((item) => item.isPrimary) ?? displays[0];
    if (display) {
      patch({ displayDevice: display.deviceName, modeKey: modeKey(display.currentMode) }, false);
    }
  }, [displays, draft.displayDevice, patch]);

  useEffect(() => {
    onDirtyChange(draft.dirty);
  }, [draft.dirty, onDirtyChange]);

  useEffect(() => {
    const deviceName = draft.displayDevice;
    if (!deviceName) {
      setModes([]);
      setModesError(null);
      return;
    }

    const request = ++modesRequestRef.current;
    setModesLoading(true);
    setModesError(null);
    void tauriApi.enumerateDisplayModes(deviceName).then((availableModes) => {
      if (!mountedRef.current || request !== modesRequestRef.current) {
        return;
      }
      setModes(availableModes);
      const selectedExists = availableModes.some((mode) => modeKey(mode) === draft.modeKey);
      if (!selectedExists) {
        const current = displays.find((display) => display.deviceName === deviceName)?.currentMode;
        const fallback = availableModes.find((mode) => current && modeKey(mode) === modeKey(current))
          ?? availableModes[0];
        patch({ modeKey: fallback ? modeKey(fallback) : "" }, false);
      }
    }).catch((error: unknown) => {
      if (!mountedRef.current || request !== modesRequestRef.current) {
        return;
      }
      setModes([]);
      setModesError(`读取显示模式失败：${errorMessage(error)}`);
    }).finally(() => {
      if (mountedRef.current && request === modesRequestRef.current) {
        setModesLoading(false);
      }
    });
  }, [displays, draft.displayDevice, modesReload, patch]);

  const loadSteamGames = useCallback(async () => {
    setSteamLoading(true);
    setSteamError(null);
    try {
      const games = await tauriApi.listSteamGames();
      if (mountedRef.current) {
        setSteamGames(games);
      }
    } catch (error) {
      if (mountedRef.current) {
        setSteamError(`读取 Steam 游戏库失败：${errorMessage(error)}`);
      }
    } finally {
      if (mountedRef.current) {
        setSteamLoading(false);
      }
    }
  }, []);

  const loadRecentApps = useCallback(async () => {
    setRecentLoading(true);
    setRecentError(null);
    try {
      const applications = await tauriApi.listRecentApps();
      if (mountedRef.current) {
        setRecentApps(applications);
      }
    } catch (error) {
      if (mountedRef.current) {
        setRecentError(`读取最近应用失败：${errorMessage(error)}`);
      }
    } finally {
      if (mountedRef.current) {
        setRecentLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    void loadSteamGames();
    void loadRecentApps();
  }, [loadRecentApps, loadSteamGames]);

  const filteredSteamGames = useMemo(() => {
    const query = steamSearch.trim().toLowerCase();
    if (!query) {
      return steamGames;
    }
    return steamGames.filter((game) =>
      game.name.toLowerCase().includes(query) || game.appId.includes(query),
    );
  }, [steamGames, steamSearch]);

  const locked = busy || saving || detecting;

  const changeSource = (source: SourceKind) => {
    if (locked || source === draft.source) {
      return;
    }
    const launchType: LaunchType = source === "steam"
      ? "steam"
      : source === "shortcut"
        ? "shortcut"
        : "executable";
    patch({
      source,
      launchType,
      switchTiming: launchType === "executable" ? "beforeLaunch" : "onProcessDetected",
      selectedSource: null,
    });
    setLauncherReady(false);
    setDetectStatus(null);
    setCandidates([]);
  };

  const selectSteamGame = (game: SteamGame) => {
    patch({
      source: "steam",
      launchType: "steam",
      name: game.name,
      steamAppId: game.appId,
      executable: "",
      arguments: "",
      workingDirectory: "",
      shortcutPath: "",
      launcherProvider: "",
      processName: "",
      switchTiming: "onProcessDetected",
      selectedSource: {
        key: `steam:${game.appId}`,
        name: game.name,
        detail: `${game.installDir} · App ID ${game.appId}`,
        type: "STEAM",
      },
    });
    setLauncherReady(false);
    setDetectStatus(null);
    setCandidates([]);
  };

  const selectRecentApplication = (application: RecentApplication) => {
    patch({
      source: "recent",
      launchType: "executable",
      name: application.name,
      executable: application.executable,
      arguments: "",
      workingDirectory: "",
      steamAppId: "",
      shortcutPath: "",
      launcherProvider: "",
      processName: application.processName,
      switchTiming: "beforeLaunch",
      selectedSource: {
        key: `recent:${application.executable.toLowerCase()}`,
        name: application.name,
        detail: application.executable,
        type: "最近应用",
      },
    });
    setLauncherReady(false);
    setDetectStatus(null);
    setCandidates([]);
  };

  const browseExecutable = async (replaceName: boolean) => {
    try {
      const path = await tauriApi.selectExecutable();
      if (!path || !mountedRef.current) {
        return;
      }
      const filename = fileName(path);
      const name = filename.replace(/\.exe$/i, "");
      patch({
        source: "executable",
        launchType: "executable",
        executable: path,
        steamAppId: "",
        shortcutPath: "",
        launcherProvider: "",
        processName: filename,
        name: replaceName || !draft.name ? name : draft.name,
        switchTiming: "beforeLaunch",
        selectedSource: {
          key: `executable:${path.toLowerCase()}`,
          name: replaceName || !draft.name ? name : draft.name,
          detail: path,
          type: "EXE",
        },
      });
      setLauncherReady(false);
      setDetectStatus(null);
      setCandidates([]);
    } catch (error) {
      onToast(`选择游戏 EXE 失败：${errorMessage(error)}`, "error");
    }
  };

  const browseShortcut = async (riotPreset: boolean) => {
    try {
      const path = await tauriApi.selectShortcut();
      if (!path || !mountedRef.current) {
        return;
      }
      const shortcutName = fileName(path).replace(/\.(lnk|url)$/i, "");
      const name = riotPreset ? "无畏契约" : shortcutName;
      patch({
        source: "shortcut",
        launchType: "shortcut",
        shortcutPath: path,
        executable: "",
        arguments: "",
        workingDirectory: "",
        steamAppId: "",
        launcherProvider: riotPreset ? "Riot Client" : "启动器",
        name,
        processName: riotPreset ? VALORANT_PROCESS_NAME : draft.processName,
        launchTimeoutMinutes: riotPreset ? "10" : draft.launchTimeoutMinutes,
        switchTiming: "onProcessDetected",
        selectedSource: {
          key: `shortcut:${path.toLowerCase()}`,
          name,
          detail: path,
          type: riotPreset ? "RIOT" : "启动器",
        },
      });
      setLauncherReady(false);
      setDetectStatus(null);
      setCandidates([]);
    } catch (error) {
      onToast(`选择快捷方式失败：${errorMessage(error)}`, "error");
    }
  };

  const detectProcess = async () => {
    const request: DetectLaunchRequest = {
      launchType: draft.launchType,
      executable: draft.executable.trim(),
      arguments: draft.arguments.trim(),
      workingDirectory: draft.workingDirectory.trim(),
      steamAppId: draft.steamAppId.trim(),
      shortcutPath: draft.shortcutPath.trim(),
      launchBeforeDetect: draft.launchType !== "shortcut",
    };
    if (request.launchType === "steam" && !request.steamAppId) {
      onToast("请先从 Steam 游戏库选择游戏", "error");
      return;
    }
    if (request.launchType === "executable" && !request.executable) {
      onToast("请先选择游戏 EXE", "error");
      return;
    }
    if (request.launchType === "shortcut" && !request.shortcutPath) {
      onToast("请先选择启动器或游戏快捷方式", "error");
      return;
    }

    const requestId = ++detectRequestRef.current;
    setDetecting(true);
    setCandidates([]);
    try {
      if (request.launchType === "shortcut" && !launcherReady) {
        setDetectStatus("正在打开启动器...");
        await tauriApi.openLaunchSource(request);
        if (!mountedRef.current || requestId !== detectRequestRef.current) {
          return;
        }
        setLauncherReady(true);
        setDetectStatus("启动器已打开。完成登录和更新后，点击“开始识别”，再从启动器进入游戏。");
        return;
      }

      setDetectStatus(request.launchType === "shortcut"
        ? "正在等待游戏进程，请现在从启动器进入游戏..."
        : "游戏已启动，正在等待新的可见进程...");
      const applications = await tauriApi.detectGameProcess(request);
      if (!mountedRef.current || requestId !== detectRequestRef.current) {
        return;
      }
      if (applications.length === 1 && applications[0]) {
        patch({ processName: applications[0].processName });
        setDetectStatus(`已检测到 ${applications[0].processName}`);
      } else if (applications.length > 1) {
        setCandidates(applications);
        setDetectStatus("检测到多个候选进程，请选择游戏主进程");
      } else {
        setDetectStatus("未检测到新的可见进程，请确认游戏已经启动后重试。");
      }
    } catch (error) {
      if (mountedRef.current && requestId === detectRequestRef.current) {
        setDetectStatus(null);
        onToast(`识别游戏进程失败：${errorMessage(error)}`, "error");
      }
    } finally {
      if (mountedRef.current && requestId === detectRequestRef.current) {
        setDetecting(false);
      }
    }
  };

  const saveProfile = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (locked) {
      return;
    }
    if (!draft.name.trim()) {
      onToast("游戏名称不能为空", "error");
      return;
    }
    if (!draft.displayDevice) {
      onToast("请选择目标显示器", "error");
      return;
    }
    const selectedMode = modes.find((mode) => modeKey(mode) === draft.modeKey);
    if (!selectedMode) {
      onToast("请选择有效的显示模式", "error");
      return;
    }
    if (!draft.processName.trim()) {
      onToast("请填写游戏主进程，或使用“启动并识别”", "error");
      return;
    }
    if (draft.launchType === "steam" && !/^\d+$/.test(draft.steamAppId.trim())) {
      onToast("Steam App ID 必须是数字", "error");
      return;
    }
    if (draft.launchType === "executable" && !draft.executable.trim()) {
      onToast("请选择游戏 EXE", "error");
      return;
    }
    if (draft.launchType === "shortcut" && !draft.shortcutPath.trim()) {
      onToast("请选择启动器或游戏快捷方式", "error");
      return;
    }

    const launchTimeoutMinutes = Number(draft.launchTimeoutMinutes);
    const restoreDelaySeconds = Number(draft.restoreDelaySeconds);
    if (!Number.isFinite(launchTimeoutMinutes) || launchTimeoutMinutes < 1 || launchTimeoutMinutes > 60) {
      onToast("等待游戏启动时间必须在 1 到 60 分钟之间", "error");
      return;
    }
    if (!Number.isFinite(restoreDelaySeconds) || restoreDelaySeconds < 0 || restoreDelaySeconds > 30) {
      onToast("恢复延迟必须在 0 到 30 秒之间", "error");
      return;
    }

    const nextProfile: GameProfile = {
      id: draft.id,
      name: draft.name.trim(),
      launchType: draft.launchType,
      executable: draft.executable.trim(),
      arguments: draft.arguments.trim(),
      workingDirectory: draft.workingDirectory.trim(),
      steamAppId: draft.steamAppId.trim(),
      shortcutPath: draft.shortcutPath.trim(),
      launcherProvider: draft.launchType === "shortcut"
        ? (draft.launcherProvider === "Riot Client" ? "Riot Client" : "启动器")
        : "",
      processName: draft.processName.trim(),
      switchTiming: draft.switchTiming,
      launchTimeoutSec: Math.round(launchTimeoutMinutes * 60),
      displayDevice: draft.displayDevice,
      width: selectedMode.width,
      height: selectedMode.height,
      refreshRate: selectedMode.refreshRate,
      restoreDelayMs: Math.round(restoreDelaySeconds * 1000),
    };

    setSaving(true);
    try {
      await onSave(nextProfile);
      if (mountedRef.current) {
        dispatch({ type: "saved" });
      }
    } catch (error) {
      if (mountedRef.current) {
        onToast(`保存游戏配置失败：${errorMessage(error)}`, "error");
      }
    } finally {
      if (mountedRef.current) {
        setSaving(false);
      }
    }
  };

  const selectDisplay = (deviceName: string) => {
    const display = displays.find((item) => item.deviceName === deviceName);
    patch({
      displayDevice: deviceName,
      modeKey: display ? modeKey(display.currentMode) : "",
    });
  };

  const selectCandidate = (application: RecentApplication) => {
    patch({ processName: application.processName });
    setCandidates([]);
    setDetectStatus(`已选择 ${application.processName}`);
  };

  const detectLabel = draft.launchType === "shortcut"
    ? (launcherReady ? "开始识别" : "打开启动器")
    : "启动并识别";
  const processHint = draft.launchType === "shortcut"
    ? launcherReady
      ? "登录和更新完成后，先点击“开始识别”，再从启动器进入游戏。"
      : "首次添加：先打开启动器并完成登录或更新，然后再识别游戏主进程。"
    : "首次添加：点击“启动并识别”，然后正常进入游戏。识别期间不会切换分辨率。";
  const savedValorantProfile = profiles.find((item) =>
    item.launchType === "shortcut"
    && item.processName.trim().toLowerCase() === VALORANT_PROCESS_NAME.toLowerCase()
    && item.shortcutPath.trim(),
  );
  const valorantShortcutPath = draft.launcherProvider === "Riot Client"
    ? draft.shortcutPath
    : savedValorantProfile?.shortcutPath ?? "";

  return (
    <form className="profile-editor" onSubmit={(event) => void saveProfile(event)} noValidate>
      <header className="profile-editor-header">
        <div className="profile-editor-heading">
          <Tooltip label="返回游戏库" side="right">
            <button type="button" className="icon-button" onClick={onBack} aria-label="返回游戏库">
              <ArrowLeft size={18} />
            </button>
          </Tooltip>
          <div>
            <p className="page-kicker">游戏库 / {profile ? "编辑配置" : "新建配置"}</p>
            <h1>{profile ? `编辑 ${profile.name}` : "添加游戏配置"}</h1>
          </div>
        </div>
        <div className="profile-editor-actions">
          <span className={`edit-state${draft.dirty ? " is-dirty" : ""}`} role="status">
            {draft.dirty ? "有未保存更改" : (profile ? "更改已保存" : "尚未保存")}
          </span>
          <button type="submit" className="button button-primary" disabled={locked || modesLoading}>
            <Save size={16} />
            {saving ? "正在保存..." : "保存配置"}
          </button>
        </div>
      </header>

      <fieldset className="profile-editor-fields" disabled={locked}>
        <section className="editor-section source-section" aria-labelledby="launch-source-heading">
          <div className="section-heading">
            <div>
              <span className="section-index">01</span>
              <h2 id="launch-source-heading">启动来源与进程</h2>
            </div>
            <p>选择平时启动游戏的方式，并指定需要监控的主进程。</p>
          </div>

          <SlidingTabs
            value={draft.source}
            options={sourceTabs}
            onValueChange={changeSource}
            ariaLabel="游戏启动来源"
            className="profile-source-tabs"
          />

          <div className={`source-panel source-panel-${draft.source}`} aria-live="polite">
            {draft.source === "steam" && (
              <>
                <div className="source-toolbar">
                  <label className="search-field">
                    <Search size={15} aria-hidden="true" />
                    <span className="sr-only">搜索 Steam 游戏</span>
                    <input
                      type="search"
                      value={steamSearch}
                      onChange={(event) => setSteamSearch(event.currentTarget.value)}
                      placeholder="搜索已安装的 Steam 游戏"
                    />
                  </label>
                  <Tooltip label="刷新 Steam 游戏库">
                    <button
                      type="button"
                      className="icon-button compact"
                      onClick={() => void loadSteamGames()}
                      disabled={steamLoading}
                      aria-label="刷新 Steam 游戏库"
                    >
                      <RefreshCw size={15} />
                    </button>
                  </Tooltip>
                </div>
                <div className={`source-list${steamLoading ? " is-loading" : ""}`}>
                  {steamLoading ? (
                    <div className="source-skeleton" aria-label="正在读取 Steam 游戏库">
                      <span /><span /><span />
                    </div>
                  ) : steamError ? (
                    <div className="source-error" role="alert">
                      <p>{steamError}</p>
                      <button type="button" className="button button-secondary" onClick={() => void loadSteamGames()}>
                        <RefreshCw size={14} />重试
                      </button>
                    </div>
                  ) : filteredSteamGames.length === 0 ? (
                    <p className="source-empty">
                      {steamSearch.trim() ? "没有匹配的 Steam 游戏" : "没有发现已安装的 Steam 游戏"}
                    </p>
                  ) : filteredSteamGames.map((game) => {
                    const selected = draft.selectedSource?.key === `steam:${game.appId}`;
                    return (
                      <button
                        key={game.appId}
                        type="button"
                        className={`source-item${selected ? " is-selected" : ""}`}
                        onClick={() => selectSteamGame(game)}
                        aria-pressed={selected}
                      >
                        <AppIcon
                          className="source-item-mark"
                          source={{ steamAppId: game.appId }}
                          fallback={<Gamepad2 size={17} />}
                        />
                        <span className="source-item-copy">
                          <strong>{game.name}</strong>
                          <span title={game.installDir}>{game.installDir}</span>
                        </span>
                        <span className="source-item-meta">{game.appId}</span>
                        {selected && <Check size={16} aria-hidden="true" />}
                      </button>
                    );
                  })}
                </div>
              </>
            )}

            {draft.source === "shortcut" && (
              <div className="launcher-source-grid">
                <button type="button" className="launcher-preset" onClick={() => void browseShortcut(true)}>
                  <AppIcon
                    className="launcher-preset-mark"
                    source={{
                      shortcutPath: valorantShortcutPath,
                      processName: VALORANT_PROCESS_NAME,
                    }}
                    fallback={<Shield size={21} />}
                  />
                  <span><strong>无畏契约</strong><small>选择 Riot / VALORANT 快捷方式</small></span>
                </button>
                <button type="button" className="browse-source" onClick={() => void browseShortcut(false)}>
                  <Link2 size={20} />
                  <span><strong>其他启动器</strong><small>选择 .lnk 或 .url 快捷方式</small></span>
                </button>
              </div>
            )}

            {draft.source === "executable" && (
              <button type="button" className="browse-source wide" onClick={() => void browseExecutable(true)}>
                <FolderOpen size={22} />
                <span><strong>选择游戏可执行文件</strong><small>直接启动本地 EXE</small></span>
              </button>
            )}

            {draft.source === "recent" && (
              <>
                <div className="source-toolbar source-toolbar-copy">
                  <span>最近出现过窗口的应用</span>
                  <Tooltip label="刷新最近应用">
                    <button
                      type="button"
                      className="icon-button compact"
                      onClick={() => void loadRecentApps()}
                      disabled={recentLoading}
                      aria-label="刷新最近应用"
                    >
                      <RefreshCw size={15} />
                    </button>
                  </Tooltip>
                </div>
                <div className={`source-list${recentLoading ? " is-loading" : ""}`}>
                  {recentLoading ? (
                    <div className="source-skeleton" aria-label="正在读取最近应用">
                      <span /><span /><span />
                    </div>
                  ) : recentError ? (
                    <div className="source-error" role="alert">
                      <p>{recentError}</p>
                      <button type="button" className="button button-secondary" onClick={() => void loadRecentApps()}>
                        <RefreshCw size={14} />重试
                      </button>
                    </div>
                  ) : recentApps.length === 0 ? (
                    <p className="source-empty">暂时没有最近活动的应用</p>
                  ) : recentApps.map((application) => {
                    const selected = draft.selectedSource?.key === `recent:${application.executable.toLowerCase()}`;
                    return (
                      <button
                        key={`${application.pid}:${application.executable}`}
                        type="button"
                        className={`source-item${selected ? " is-selected" : ""}`}
                        onClick={() => selectRecentApplication(application)}
                        aria-pressed={selected}
                      >
                        <AppIcon
                          className="source-item-mark"
                          source={{ executable: application.executable }}
                          fallback={<SquareTerminal size={17} />}
                        />
                        <span className="source-item-copy">
                          <strong>{application.name}</strong>
                          <span title={application.executable}>{application.executable}</span>
                        </span>
                        <span className="source-item-meta">{application.processName}</span>
                        {selected && <Check size={16} aria-hidden="true" />}
                      </button>
                    );
                  })}
                </div>
              </>
            )}
          </div>

          {draft.selectedSource && (
            <div className="selected-source">
              <AppIcon
                className="selected-source-mark"
                source={{
                  steamAppId: draft.launchType === "steam" ? draft.steamAppId : "",
                  shortcutPath: draft.launchType === "shortcut" ? draft.shortcutPath : "",
                  executable: draft.launchType === "executable" ? draft.executable : "",
                  processName: draft.processName,
                }}
                fallback={draft.launchType === "steam"
                  ? <Gamepad2 size={17} />
                  : draft.launchType === "shortcut"
                    ? <Link2 size={17} />
                    : <SquareTerminal size={17} />}
              />
              <span className="selected-source-copy">
                <strong>{draft.selectedSource.name}</strong>
                <span title={draft.selectedSource.detail}>{draft.selectedSource.detail}</span>
              </span>
              <span className="source-tag">{draft.selectedSource.type}</span>
            </div>
          )}

          <div className="field-grid source-fields">
            <label className="field" htmlFor="profile-name">
              <span className="field-label">游戏名称</span>
              <input
                id="profile-name"
                value={draft.name}
                onChange={(event) => patch({ name: event.currentTarget.value })}
                maxLength={80}
                placeholder="例如：Counter-Strike 2"
                required
              />
            </label>
            <label className="field" htmlFor="profile-process">
              <span className="field-label">游戏主进程</span>
              <span className="input-with-action">
                <input
                  id="profile-process"
                  value={draft.processName}
                  onChange={(event) => patch({ processName: event.currentTarget.value })}
                  placeholder="例如：cs2.exe"
                  required
                />
                <button
                  type="button"
                  className="button button-secondary detect-button"
                  onClick={() => void detectProcess()}
                  disabled={detecting}
                >
                  <Radar size={16} />
                  {detecting ? "正在识别..." : detectLabel}
                </button>
              </span>
            </label>
          </div>

          {draft.selectedSource && !draft.processName.trim() && (
            <p className="process-hint">{processHint}</p>
          )}
          {detectStatus && <p className="detect-status" role="status">{detectStatus}</p>}
          {candidates.length > 0 && (
            <div className="process-candidates" aria-label="候选游戏进程">
              {candidates.map((application) => (
                <button
                  key={`${application.pid}:${application.processName}`}
                  type="button"
                  className="candidate-button"
                  title={application.executable}
                  onClick={() => selectCandidate(application)}
                >
                  {application.processName}
                </button>
              ))}
            </div>
          )}
        </section>

        <section className="editor-section" aria-labelledby="display-mode-heading">
          <div className="section-heading">
            <div>
              <span className="section-index">02</span>
              <h2 id="display-mode-heading">显示模式与恢复</h2>
            </div>
            <p>游戏运行时使用此显示模式，退出后自动恢复桌面。</p>
          </div>
          <div className="field-grid mode-fields">
            <label className="field" htmlFor="profile-display">
              <span className="field-label"><Monitor size={14} />目标显示器</span>
              <SelectControl
                id="profile-display"
                value={draft.displayDevice}
                options={displays.map((display) => ({
                  value: display.deviceName,
                  label: display.friendlyName || display.deviceName,
                  detail: display.isPrimary ? "主显示器" : undefined,
                }))}
                onValueChange={selectDisplay}
                placeholder="没有活动显示器"
                required
              />
            </label>
            <label className="field" htmlFor="profile-mode">
              <span className="field-label">分辨率与刷新率</span>
              <SelectControl
                id="profile-mode"
                value={draft.modeKey}
                options={modes.map((mode) => ({
                  value: modeKey(mode),
                  label: `${mode.width} × ${mode.height}`,
                  detail: `${mode.refreshRate} Hz`,
                }))}
                onValueChange={(value) => patch({ modeKey: value })}
                placeholder={modesLoading ? "正在读取显示模式..." : "没有可用显示模式"}
                disabled={modesLoading || Boolean(modesError)}
                required
              />
            </label>
            <label className="field compact-field" htmlFor="profile-restore-delay">
              <span className="field-label"><Clock3 size={14} />退出后恢复延迟</span>
              <span className="unit-input">
                <input
                  id="profile-restore-delay"
                  type="number"
                  min="0"
                  max="30"
                  step="1"
                  value={draft.restoreDelaySeconds}
                  onChange={(event) => patch({ restoreDelaySeconds: event.currentTarget.value })}
                  required
                />
                <span>秒</span>
              </span>
            </label>
          </div>
          {modesError && (
            <div className="inline-error" role="alert">
              <span>{modesError}</span>
              <button type="button" className="button button-secondary" onClick={() => setModesReload((value) => value + 1)}>
                <RefreshCw size={14} />重试
              </button>
            </div>
          )}
        </section>

        <Accordion
          className="editor-section advanced-section"
          defaultOpen={Boolean(profile)}
          title={(
            <span className="section-heading accordion-heading">
              <span><span className="section-index">03</span><strong>高级启动设置</strong></span>
              <small>启动路径、切换时机与等待上限</small>
            </span>
          )}
        >
          <div className="advanced-fields">
            {draft.launchType === "executable" && (
              <>
                <label className="field field-wide" htmlFor="profile-executable">
                  <span className="field-label">游戏 EXE</span>
                  <span className="input-with-action">
                    <input
                      id="profile-executable"
                      value={draft.executable}
                      onChange={(event) => patch({ executable: event.currentTarget.value })}
                      placeholder="选择游戏可执行文件"
                      required
                    />
                    <Tooltip label="选择 EXE">
                      <button
                        type="button"
                        className="icon-button"
                        onClick={() => void browseExecutable(false)}
                        aria-label="选择游戏 EXE"
                      >
                        <FolderOpen size={16} />
                      </button>
                    </Tooltip>
                  </span>
                </label>
                <label className="field field-wide" htmlFor="profile-arguments">
                  <span className="field-label">启动参数</span>
                  <input
                    id="profile-arguments"
                    value={draft.arguments}
                    onChange={(event) => patch({ arguments: event.currentTarget.value })}
                    placeholder="可选"
                  />
                </label>
                <label className="field field-wide" htmlFor="profile-working-directory">
                  <span className="field-label">工作目录</span>
                  <input
                    id="profile-working-directory"
                    value={draft.workingDirectory}
                    onChange={(event) => patch({ workingDirectory: event.currentTarget.value })}
                    placeholder="默认使用 EXE 所在目录"
                  />
                </label>
              </>
            )}

            {draft.launchType === "steam" && (
              <label className="field field-wide" htmlFor="profile-steam-id">
                <span className="field-label">Steam App ID</span>
                <input
                  id="profile-steam-id"
                  inputMode="numeric"
                  pattern="[0-9]+"
                  value={draft.steamAppId}
                  onChange={(event) => patch({ steamAppId: event.currentTarget.value })}
                  placeholder="例如：730"
                  required
                />
              </label>
            )}

            {draft.launchType === "shortcut" && (
              <label className="field field-wide" htmlFor="profile-shortcut">
                <span className="field-label">启动器或游戏快捷方式</span>
                <span className="input-with-action">
                  <input
                    id="profile-shortcut"
                    value={draft.shortcutPath}
                    onChange={(event) => {
                      patch({ shortcutPath: event.currentTarget.value });
                      setLauncherReady(false);
                    }}
                    placeholder=".lnk 或 .url"
                    required
                  />
                  <Tooltip label="选择快捷方式">
                    <button
                      type="button"
                      className="icon-button"
                      onClick={() => void browseShortcut(draft.launcherProvider === "Riot Client")}
                      aria-label="选择快捷方式"
                    >
                      <FolderOpen size={16} />
                    </button>
                  </Tooltip>
                </span>
              </label>
            )}

            <div className="field-grid field-wide">
              <label className="field" htmlFor="profile-switch-timing">
                <span className="field-label">分辨率切换时机</span>
                <SelectControl
                  id="profile-switch-timing"
                  value={draft.switchTiming}
                  options={switchTimingOptions}
                  onValueChange={(value) => patch({ switchTiming: value })}
                  placeholder="选择切换时机"
                />
              </label>
              <label className="field" htmlFor="profile-launch-timeout">
                <span className="field-label">等待游戏启动</span>
                <span className="unit-input">
                  <input
                    id="profile-launch-timeout"
                    type="number"
                    min="1"
                    max="60"
                    step="1"
                    value={draft.launchTimeoutMinutes}
                    onChange={(event) => patch({ launchTimeoutMinutes: event.currentTarget.value })}
                    required
                  />
                  <span>分钟</span>
                </span>
              </label>
            </div>
          </div>
        </Accordion>
      </fieldset>
    </form>
  );
}
