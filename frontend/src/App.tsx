import {
  Gamepad2,
  LogOut,
  Minus,
  MonitorCog,
  PanelLeftClose,
  PanelLeftOpen,
  PanelTopClose,
  RotateCcw,
  Settings,
  ShieldCheck,
  Square,
  X,
} from "lucide-react";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";

import { DisplaysPage } from "./components/DisplaysPage";
import { GameLibrary } from "./components/GameLibrary";
import { ProfileEditor } from "./components/ProfileEditor";
import { RunningPage } from "./components/RunningPage";
import { SettingsPage } from "./components/SettingsPage";
import {
  type ConfirmOptions,
  ConfirmDialog,
  RuntimeText,
  Toast,
  TooltipLayer,
} from "./components/ui";
import { tauriApi } from "./lib/tauri";
import type {
  AppSettings,
  CloseBehavior,
  DisplayInfo,
  GameProfile,
  MinimizeBehavior,
  PendingRestore,
  RuntimeStatus,
} from "./types";

type Workspace = "library" | "displays" | "settings";
type ToastTone = "error" | "success" | "info";

const COMPACT_SIDEBAR_QUERY = "(max-width: 820px)";
const SIDEBAR_COLLAPSED_STORAGE_KEY = "resolution-hub-sidebar-collapsed";

function initialSidebarCollapsed(): boolean {
  try {
    const stored = window.localStorage.getItem(SIDEBAR_COLLAPSED_STORAGE_KEY);
    if (stored === "true" || stored === "false") {
      return stored === "true";
    }
  } catch {
    // Unavailable storage falls back to the responsive default.
  }
  return window.matchMedia(COMPACT_SIDEBAR_QUERY).matches;
}

function storeSidebarCollapsed(collapsed: boolean) {
  try {
    window.localStorage.setItem(SIDEBAR_COLLAPSED_STORAGE_KEY, String(collapsed));
  } catch {
    // UI preferences must not block sidebar interaction.
  }
}

interface AppState {
  profiles: GameProfile[];
  displays: DisplayInfo[];
  settings: AppSettings;
  runtime: RuntimeStatus;
  pendingRestore: PendingRestore | null;
  autoStartEnabled: boolean;
  version: string;
  loading: boolean;
}

type AppAction =
  | { type: "profiles"; value: GameProfile[] }
  | { type: "displays"; value: DisplayInfo[] }
  | { type: "settings"; value: AppSettings }
  | { type: "runtime"; value: RuntimeStatus }
  | { type: "pendingRestore"; value: PendingRestore | null }
  | { type: "autoStart"; value: boolean }
  | { type: "version"; value: string }
  | { type: "ready" };

const IDLE_RUNTIME: RuntimeStatus = {
  phase: "idle",
  profileId: null,
  profileName: null,
  message: "就绪",
  canRestore: false,
};

const INITIAL_STATE: AppState = {
  profiles: [],
  displays: [],
  settings: {
    defaultDisplayModes: [],
    minimizeBehavior: "taskbar",
    closeBehavior: "hideToTray",
    confirmCloseBehavior: true,
  },
  runtime: IDLE_RUNTIME,
  pendingRestore: null,
  autoStartEnabled: false,
  version: "",
  loading: true,
};

function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "profiles":
      return { ...state, profiles: action.value };
    case "displays":
      return { ...state, displays: action.value };
    case "settings":
      return { ...state, settings: action.value };
    case "runtime":
      return { ...state, runtime: action.value };
    case "pendingRestore":
      return { ...state, pendingRestore: action.value };
    case "autoStart":
      return { ...state, autoStartEnabled: action.value };
    case "version":
      return { ...state, version: action.value };
    case "ready":
      return { ...state, loading: false };
  }
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return "操作失败，请稍后重试";
}

function defaultModeLabel(state: AppState): string {
  const display = state.displays.find((item) => item.isPrimary) ?? state.displays[0];
  if (!display) {
    return "尚未读取显示器";
  }
  const mode = state.settings.defaultDisplayModes.find(
    (item) => item.deviceName === display.deviceName,
  ) ?? display.currentMode;
  return `${mode.width} × ${mode.height} · ${mode.refreshRate} Hz`;
}

interface SidebarItemProps {
  label: string;
  active: boolean;
  icon: ReactNode;
  disabled?: boolean;
  onClick: () => void;
}

function SidebarItem({ label, active, icon, disabled = false, onClick }: SidebarItemProps) {
  return (
    <button
      type="button"
      className={`sidebar-item${active ? " is-active" : ""}`}
      aria-label={label}
      aria-current={active ? "page" : undefined}
      disabled={disabled}
      onClick={onClick}
    >
      <span className="sidebar-item-icon" aria-hidden="true">{icon}</span>
      <span className="sidebar-item-label">{label}</span>
    </button>
  );
}

export default function App() {
  const [state, dispatch] = useReducer(appReducer, INITIAL_STATE);
  const [workspace, setWorkspace] = useState<Workspace>("library");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(initialSidebarCollapsed);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editingProfile, setEditingProfile] = useState<GameProfile | null>(null);
  const [editorDirty, setEditorDirty] = useState(false);
  const [confirm, setConfirmState] = useState<ConfirmOptions | null>(null);
  const [toast, setToast] = useState<{ message: string; tone: ToastTone } | null>(null);
  const runtimePhaseRef = useRef(state.runtime.phase);
  const runtimeEventSeenRef = useRef(false);

  useEffect(() => {
    const compactSidebar = window.matchMedia(COMPACT_SIDEBAR_QUERY);
    const updateSidebar = (event: MediaQueryListEvent) => {
      setSidebarCollapsed(event.matches);
      storeSidebarCollapsed(event.matches);
    };
    compactSidebar.addEventListener("change", updateSidebar);
    return () => compactSidebar.removeEventListener("change", updateSidebar);
  }, []);

  const toggleSidebar = () => {
    setSidebarCollapsed((collapsed) => {
      const next = !collapsed;
      storeSidebarCollapsed(next);
      return next;
    });
  };

  const openConfirm = useCallback((options: ConfirmOptions) => {
    const activeElement = document.activeElement;
    setConfirmState({
      ...options,
      returnFocus: options.returnFocus
        ?? (activeElement instanceof HTMLElement ? activeElement : null),
    });
  }, []);

  const showToast = useCallback((message: string, tone: ToastTone = "error") => {
    if (tone === "error") {
      setToast({ message, tone });
    }
  }, []);

  const showError = useCallback((error: unknown) => {
    showToast(errorMessage(error), "error");
  }, [showToast]);

  const refreshProfiles = useCallback(async () => {
    dispatch({ type: "profiles", value: await tauriApi.listProfiles() });
  }, []);

  const refreshDisplays = useCallback(async () => {
    const [displays, settings] = await Promise.all([
      tauriApi.enumerateDisplays(),
      tauriApi.getSettings(),
    ]);
    dispatch({ type: "displays", value: displays });
    dispatch({ type: "settings", value: settings });
  }, []);

  const restoreDesktop = useCallback(async () => {
    tauriApi.setWindowSizePersistenceSuspended(true);
    await tauriApi.restoreDisplay();
    dispatch({ type: "pendingRestore", value: null });
    void refreshDisplays().catch(showError);
  }, [refreshDisplays, showError]);

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | undefined;
    let windowSizeNormalizationId = 0;
    void tauriApi.listenRuntime((runtime) => {
      if (!mounted) {
        return;
      }
      const previousPhase = runtimePhaseRef.current;
      runtimeEventSeenRef.current = true;
      runtimePhaseRef.current = runtime.phase;
      dispatch({ type: "runtime", value: runtime });
      if (runtime.phase === "error") {
        showError(runtime.message);
      }
      if (previousPhase !== runtime.phase) {
        const normalizationId = ++windowSizeNormalizationId;
        tauriApi.setWindowSizePersistenceSuspended(true);
        const shouldNormalizeWindow = (
          (previousPhase === "switching" && runtime.phase !== "error")
          || runtime.phase === "idle"
          || runtime.phase === "error"
        );
        if (shouldNormalizeWindow) {
          // 回到 idle/error 说明刚做完一次显示模式恢复，必须等工作区尺寸稳定后
          // 再归一化，否则窗口会被按旧分辨率夹小并把错误尺寸持久化下来。
          const settled = runtime.phase === "idle" || runtime.phase === "error";
          const normalize = settled
            ? tauriApi.settleWindowSizeAfterDisplayChange()
            : tauriApi.ensureWindowSize();
          void normalize.catch(() => undefined).finally(() => {
            if (
              mounted
              && normalizationId === windowSizeNormalizationId
              && (runtime.phase === "idle" || runtime.phase === "error")
              && !runtime.canRestore
            ) {
              tauriApi.setWindowSizePersistenceSuspended(false);
            }
          });
        }
      }
      if (previousPhase !== "idle" && runtime.phase === "idle") {
        void refreshDisplays().catch(showError);
      }
    }).then((release) => {
      if (mounted) {
        unlisten = release;
      } else {
        release();
      }
    }).catch(showError);

    void (async () => {
      const results = await Promise.allSettled([
        tauriApi.listProfiles(),
        tauriApi.enumerateDisplays(),
        tauriApi.getSettings(),
        tauriApi.getRuntimeStatus(),
        tauriApi.getPendingRestore(),
        tauriApi.getAutoStartEnabled(),
        tauriApi.getVersion(),
      ] as const);
      if (!mounted) {
        return;
      }

      const [profiles, displays, settings, runtime, pending, autoStart, version] = results;
      if (!runtimeEventSeenRef.current) {
        const runtimeOwnsDisplay = runtime.status === "fulfilled"
          && (
            (runtime.value.phase !== "idle" && runtime.value.phase !== "error")
            || runtime.value.canRestore
          );
        const pendingRestoreExists = pending.status === "fulfilled" && pending.value !== null;
        tauriApi.setWindowSizePersistenceSuspended(
          runtimeOwnsDisplay || pendingRestoreExists,
        );
      }
      if (profiles.status === "fulfilled") dispatch({ type: "profiles", value: profiles.value });
      if (displays.status === "fulfilled") dispatch({ type: "displays", value: displays.value });
      if (settings.status === "fulfilled") dispatch({ type: "settings", value: settings.value });
      if (runtime.status === "fulfilled" && !runtimeEventSeenRef.current) {
        runtimePhaseRef.current = runtime.value.phase;
        dispatch({ type: "runtime", value: runtime.value });
      }
      if (pending.status === "fulfilled") dispatch({ type: "pendingRestore", value: pending.value });
      if (autoStart.status === "fulfilled") dispatch({ type: "autoStart", value: autoStart.value });
      if (version.status === "fulfilled") dispatch({ type: "version", value: version.value });
      dispatch({ type: "ready" });

      const failed = results.find((result) => result.status === "rejected");
      if (failed?.status === "rejected") {
        showError(failed.reason);
      }

      if (pending.status === "fulfilled" && pending.value) {
        const restorePoint = pending.value;
        openConfirm({
          title: "检测到桌面恢复点",
          description: `上次运行“${restorePoint.profileName}”后显示模式未确认恢复。建议现在恢复到 ${restorePoint.mode.width} × ${restorePoint.mode.height} · ${restorePoint.mode.refreshRate} Hz。`,
          confirmLabel: "立即恢复",
          tone: "warning",
          onConfirm: restoreDesktop,
        });
      }
    })();

    return () => {
      mounted = false;
      windowSizeNormalizationId += 1;
      unlisten?.();
    };
  }, [openConfirm, refreshDisplays, restoreDesktop, showError]);

  const runtimeBusy = state.runtime.phase !== "idle" && state.runtime.phase !== "error";
  const canRestore = state.runtime.canRestore || state.pendingRestore !== null;
  const statusMode = useMemo(() => defaultModeLabel(state), [state.displays, state.settings]);
  // 接管期间锁定在专注页，navigate 用 ref 读取避免把自身绑进依赖链。
  const runtimeBusyRef = useRef(runtimeBusy);
  runtimeBusyRef.current = runtimeBusy;
  const runningProfile = useMemo(
    () => state.profiles.find((item) => item.id === state.runtime.profileId) ?? null,
    [state.profiles, state.runtime.profileId],
  );

  const leaveEditor = useCallback((afterLeave: () => void) => {
    if (!editorOpen) {
      afterLeave();
      return;
    }
    if (!editorDirty) {
      setEditorOpen(false);
      setEditingProfile(null);
      afterLeave();
      return;
    }
    openConfirm({
      title: "丢弃未保存的更改？",
      description: "当前游戏配置尚未保存，离开后这些更改将无法恢复。",
      confirmLabel: "丢弃并离开",
      tone: "danger",
      onConfirm: () => {
        setEditorDirty(false);
        setEditorOpen(false);
        setEditingProfile(null);
        afterLeave();
      },
    });
  }, [editorDirty, editorOpen, openConfirm]);

  const navigate = useCallback((next: Workspace) => {
    if (runtimeBusyRef.current) {
      return;
    }
    leaveEditor(() => setWorkspace(next));
  }, [leaveEditor]);

  const openEditor = useCallback((profile: GameProfile | null) => {
    setWorkspace("library");
    setEditingProfile(profile);
    setEditorDirty(false);
    setEditorOpen(true);
  }, []);

  const saveProfile = useCallback(async (profile: GameProfile) => {
    await tauriApi.saveProfile(profile);
    await refreshProfiles();
    setEditorDirty(false);
    setEditorOpen(false);
    setEditingProfile(null);
    showToast("游戏配置已保存", "success");
  }, [refreshProfiles, showToast]);

  const deleteProfile = useCallback((profile: GameProfile, returnFocus?: HTMLElement | null) => {
    openConfirm({
      title: `删除“${profile.name}”？`,
      description: "此操作只删除 Resolution Hub 中的配置，不会删除游戏文件。",
      confirmLabel: "删除配置",
      tone: "danger",
      returnFocus,
      onConfirm: async () => {
        await tauriApi.deleteProfile(profile.id);
        await refreshProfiles();
        showToast("游戏配置已删除", "success");
      },
    });
  }, [openConfirm, refreshProfiles, showToast]);

  const launchProfile = useCallback(async (profile: GameProfile) => {
    try {
      await tauriApi.launchProfile(profile.id);
    } catch (error) {
      showError(error);
    }
  }, [showError]);

  const requestRestore = useCallback(() => {
    openConfirm({
      title: "恢复桌面显示模式？",
      description: "将立即停止当前显示模式接管并恢复已记录的桌面模式。",
      confirmLabel: "立即恢复",
      tone: "warning",
      onConfirm: restoreDesktop,
    });
  }, [openConfirm, restoreDesktop]);

  // 结束会话就是停止监控 + 恢复桌面，守候阶段没有恢复点时恢复本身是空操作。
  const endSession = useCallback(() => {
    openConfirm({
      title: "结束当前会话？",
      description: "将停止监控该游戏并恢复桌面显示模式，已经在运行的游戏不会被关闭。",
      confirmLabel: "结束会话",
      tone: "warning",
      onConfirm: restoreDesktop,
    });
  }, [openConfirm, restoreDesktop]);

  const exitNow = useCallback(async () => {
    tauriApi.setWindowSizePersistenceSuspended(true);
    await tauriApi.close();
    if (editorDirty) {
      setEditorDirty(false);
    }
  }, [editorDirty]);

  const requestExit = useCallback(() => {
    if (!canRestore && !editorDirty) {
      void exitNow().catch(showError);
      return;
    }
    const combined = canRestore && editorDirty;
    openConfirm({
      title: combined
        ? "恢复桌面、丢弃更改并关闭？"
        : canRestore
          ? "恢复桌面并关闭？"
          : "丢弃更改并关闭？",
      description: combined
        ? "显示模式仍在接管中，当前表单也有未保存更改。恢复成功后将丢弃更改并关闭 Resolution Hub。"
        : canRestore
          ? "显示模式仍在接管中。只有恢复成功后才会关闭 Resolution Hub。"
          : "当前游戏配置尚未保存，关闭后这些更改将无法恢复。",
      confirmLabel: combined ? "恢复并关闭" : canRestore ? "恢复并关闭" : "丢弃并关闭",
      tone: canRestore ? "warning" : "danger",
      onConfirm: exitNow,
    });
  }, [canRestore, editorDirty, exitNow, openConfirm, showError]);

  const saveWindowPreferences = useCallback(async (
    minimizeBehavior: MinimizeBehavior,
    behavior: CloseBehavior,
    confirmCloseBehavior: boolean,
  ) => {
    const settings = await tauriApi.setCloseBehavior(
      behavior,
      confirmCloseBehavior,
      minimizeBehavior,
    );
    dispatch({ type: "settings", value: settings });
  }, []);

  const hideToTray = useCallback(async () => {
    await tauriApi.hide();
  }, []);

  const closeWindow = useCallback(() => {
    if (state.loading) {
      showToast("正在读取应用状态，请稍候", "info");
      return;
    }
    if (!state.settings.confirmCloseBehavior) {
      if (state.settings.closeBehavior === "hideToTray") {
        void hideToTray().catch(showError);
      } else {
        requestExit();
      }
      return;
    }

    let description = "请选择关闭主窗口后的处理方式。可通过下方开关决定以后关闭时是否继续提示。";
    if (editorDirty) {
      description += " 隐藏到托盘会保留当前未保存内容；退出程序会丢弃这些更改。";
    }
    if (canRestore) {
      description += " 退出前会先恢复桌面显示模式；恢复失败时不会退出。";
    } else if (runtimeBusy) {
      description += " 退出前会先停止当前游戏会话。";
    }

    openConfirm({
      title: "关闭主窗口时如何处理？",
      description,
      confirmLabel: "确定",
      choices: [
        {
          value: "hideToTray",
          label: "隐藏到系统托盘",
          detail: "应用继续在后台运行",
          icon: <PanelTopClose size={16} />,
        },
        {
          value: "exit",
          label: "退出程序",
          detail: "安全恢复显示模式后退出",
          icon: <LogOut size={16} />,
          tone: "danger",
        },
      ],
      choiceDefault: state.settings.closeBehavior,
      showDontAskAgain: true,
      hideCancel: true,
      onConfirm: async (result) => {
        const behavior: CloseBehavior = result?.choice === "exit" ? "exit" : "hideToTray";
        await saveWindowPreferences(
          state.settings.minimizeBehavior,
          behavior,
          !(result?.dontAskAgain ?? false),
        );
        if (behavior === "exit") {
          await exitNow();
        } else {
          await hideToTray();
        }
      },
    });
  }, [canRestore, editorDirty, exitNow, hideToTray, openConfirm, requestExit, runtimeBusy, saveWindowPreferences, showError, showToast, state.loading, state.settings.closeBehavior, state.settings.confirmCloseBehavior, state.settings.minimizeBehavior]);

  const closeWindowRef = useRef(closeWindow);
  const requestExitRef = useRef(requestExit);
  closeWindowRef.current = closeWindow;
  requestExitRef.current = requestExit;

  useEffect(() => {
    let active = true;
    const releases: Array<() => void> = [];
    const attach = (listener: Promise<() => void>) => {
      void listener.then((release) => {
        if (active) {
          releases.push(release);
        } else {
          release();
        }
      }).catch(showError);
    };

    attach(tauriApi.listenWindowCloseRequested(() => closeWindowRef.current()));
    attach(tauriApi.listenTrayExitRequested(() => requestExitRef.current()));
    attach(tauriApi.listenTrayActionError(showError));
    return () => {
      active = false;
      releases.forEach((release) => release());
    };
  }, [showError]);

  const updateAutoStart = useCallback(async (enabled: boolean) => {
    const previous = state.autoStartEnabled;
    dispatch({ type: "autoStart", value: enabled });
    try {
      const saved = await tauriApi.setAutoStartEnabled(enabled);
      dispatch({ type: "autoStart", value: saved });
    } catch (error) {
      dispatch({ type: "autoStart", value: previous });
      throw error;
    }
  }, [state.autoStartEnabled]);

  const pageTitle = runtimeBusy
    ? state.runtime.profileName ?? "游戏运行中"
    : editorOpen
      ? editingProfile ? "编辑游戏" : "添加游戏"
      : workspace === "library" ? "游戏库"
        : workspace === "displays" ? "显示设置" : "应用设置";

  return (
    <div className={`app-shell${sidebarCollapsed ? " is-sidebar-collapsed" : ""}`}>
      <aside className="sidebar">
        <div className="brand-block">
          <img src="/app-icon.png" alt="" className="brand-icon" />
          <div className="brand-copy">
            <strong>Resolution</strong>
            <span>Hub</span>
          </div>
        </div>
        <nav className="sidebar-nav" aria-label="主导航">
          <SidebarItem
            label="游戏库"
            active={workspace === "library"}
            icon={<Gamepad2 size={19} />}
            disabled={runtimeBusy}
            onClick={() => navigate("library")}
          />
          <SidebarItem
            label="显示设置"
            active={workspace === "displays"}
            icon={<MonitorCog size={19} />}
            disabled={runtimeBusy}
            onClick={() => navigate("displays")}
          />
          <SidebarItem
            label="应用设置"
            active={workspace === "settings"}
            icon={<Settings size={19} />}
            disabled={runtimeBusy}
            onClick={() => navigate("settings")}
          />
        </nav>
        <div className="sidebar-foot">
          <ShieldCheck size={16} aria-hidden="true" />
          <span className="sidebar-foot-copy">显示保护已启用</span>
          <button
            type="button"
            className="sidebar-toggle"
            aria-label={sidebarCollapsed ? "展开侧边栏" : "收起侧边栏"}
            onClick={toggleSidebar}
          >
            {sidebarCollapsed ? <PanelLeftOpen size={12} /> : <PanelLeftClose size={12} />}
          </button>
        </div>
      </aside>

      <header className="titlebar" data-tauri-drag-region>
        <div className="titlebar-context" data-tauri-drag-region>
          <span className="titlebar-kicker" data-tauri-drag-region>Resolution Hub</span>
          <strong data-tauri-drag-region>{pageTitle}</strong>
        </div>
        <div className="window-actions">
          <button type="button" className="window-button" onClick={() => void tauriApi.minimize().catch(showError)} aria-label="最小化">
            <Minus size={16} />
          </button>
          <button type="button" className="window-button" onClick={() => void tauriApi.toggleMaximize().catch(showError)} aria-label="最大化或还原">
            <Square size={13} />
          </button>
          <button type="button" className="window-button window-close" onClick={() => void closeWindow()} aria-label="关闭">
            <X size={16} />
          </button>
        </div>
      </header>

      <main className="workspace-main">
        {runtimeBusy ? (
          <div className="page-scroll t-view-enter" key="running">
            <RunningPage
              runtime={state.runtime}
              profile={runningProfile}
              onEnd={endSession}
            />
          </div>
        ) : workspace === "library" ? (
          <div
            className="library-pages t-page-slide t-view-enter"
            key="library"
            data-page={editorOpen ? "2" : "1"}
          >
            <div className="t-page page-scroll" data-page-id="1" aria-hidden={editorOpen}>
              <GameLibrary
                profiles={state.profiles}
                runtime={state.runtime}
                loading={state.loading}
                onAdd={() => openEditor(null)}
                onEdit={(profile) => openEditor(profile)}
                onDelete={deleteProfile}
                onLaunch={(profile) => void launchProfile(profile)}
              />
            </div>
            <div className="t-page page-scroll" data-page-id="2" aria-hidden={!editorOpen}>
              {editorOpen && (
                <ProfileEditor
                  profile={editingProfile}
                  profiles={state.profiles}
                  displays={state.displays}
                  busy={runtimeBusy}
                  onSave={saveProfile}
                  onBack={() => leaveEditor(() => setWorkspace("library"))}
                  onDirtyChange={setEditorDirty}
                  onToast={showToast}
                />
              )}
            </div>
          </div>
        ) : workspace === "displays" ? (
          <div className="page-scroll t-view-enter" key="displays">
            <DisplaysPage
              displays={state.displays}
              settings={state.settings}
              runtime={state.runtime}
              loading={state.loading}
              onSettingsChange={(value) => dispatch({ type: "settings", value })}
              onRefresh={refreshDisplays}
              onToast={showToast}
            />
          </div>
        ) : (
          <div className="page-scroll t-view-enter" key="settings">
            <SettingsPage
              autoStartEnabled={state.autoStartEnabled}
              minimizeBehavior={state.settings.minimizeBehavior}
              closeBehavior={state.settings.closeBehavior}
              confirmCloseBehavior={state.settings.confirmCloseBehavior}
              version={state.version}
              onAutoStartChange={updateAutoStart}
              onOpenProjectHomepage={tauriApi.openProjectHomepage}
              onWindowPreferencesChange={saveWindowPreferences}
              onToast={showToast}
            />
          </div>
        )}
      </main>

      <footer className="safety-bar">
        <div className="safety-default">
          <MonitorCog size={16} aria-hidden="true" />
          <span className="safety-label">桌面默认</span>
          <strong data-tooltip-overflow={`桌面默认模式：${statusMode}`}>{statusMode}</strong>
        </div>
        <div className="safety-actions">
          {/* 接管期间这些操作由专注页承担；此处只保留 idle 状态下残留恢复点的入口。 */}
          {!runtimeBusy && canRestore && (
            <button type="button" className="button button-warning compact" onClick={requestRestore}>
              <RotateCcw size={14} aria-hidden="true" />
              立即恢复
            </button>
          )}
        </div>
        <div className="runtime-state" data-phase={state.runtime.phase} role="status">
          <span className="runtime-dot" aria-hidden="true" />
          <RuntimeText>{state.runtime.message || "就绪"}</RuntimeText>
        </div>
      </footer>

      <TooltipLayer />
      <ConfirmDialog options={confirm} onClose={() => setConfirmState(null)} onError={showError} />
      <Toast
        message={toast?.message ?? null}
        tone={toast?.tone}
        onClose={() => setToast(null)}
      />
    </div>
  );
}
