import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";
import { tauriApi } from "./lib/tauri";
import type { GameProfile, RuntimeStatus } from "./types";

vi.mock("./lib/tauri", () => ({
  tauriApi: {
    enumerateDisplays: vi.fn(),
    enumerateDisplayModes: vi.fn(),
    getSettings: vi.fn(),
    setDefaultDisplayMode: vi.fn(),
    setCloseBehavior: vi.fn(),
    openProjectHomepage: vi.fn(),
    getAutoStartEnabled: vi.fn(),
    setAutoStartEnabled: vi.fn(),
    listProfiles: vi.fn(),
    saveProfile: vi.fn(),
    deleteProfile: vi.fn(),
    selectExecutable: vi.fn(),
    selectShortcut: vi.fn(),
    listSteamGames: vi.fn(),
    listRecentApps: vi.fn(),
    getApplicationIcon: vi.fn(),
    openLaunchSource: vi.fn(),
    detectGameProcess: vi.fn(),
    launchProfile: vi.fn(),
    getRuntimeStatus: vi.fn(),
    getPendingRestore: vi.fn(),
    restoreDisplay: vi.fn(),
    ensureWindowSize: vi.fn(),
    settleWindowSizeAfterDisplayChange: vi.fn(),
    setWindowSizePersistenceSuspended: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    hide: vi.fn(),
    close: vi.fn(),
    getVersion: vi.fn(),
    listenRuntime: vi.fn(),
    listenWindowCloseRequested: vi.fn(),
    listenTrayExitRequested: vi.fn(),
    listenTrayActionError: vi.fn(),
  },
}));

const mode = {
  deviceName: "DISPLAY1",
  width: 1920,
  height: 1080,
  refreshRate: 60,
  bitsPerPixel: 32,
};

const display = {
  deviceName: "DISPLAY1",
  friendlyName: "Primary Panel",
  isPrimary: true,
  currentMode: mode,
};

const idle: RuntimeStatus = {
  phase: "idle",
  profileId: null,
  profileName: null,
  message: "就绪",
  canRestore: false,
};

const profile: GameProfile = {
  id: "game-1",
  name: "Test Game",
  launchType: "executable",
  executable: "C:\\Games\\test.exe",
  arguments: "",
  workingDirectory: "",
  steamAppId: "",
  shortcutPath: "",
  launcherProvider: "",
  launcherProcessName: "",
  processName: "test.exe",
  switchTiming: "beforeLaunch",
  launchTimeoutSec: 600,
  displayDevice: "DISPLAY1",
  width: 1920,
  height: 1080,
  refreshRate: 60,
  restoreDelayMs: 2000,
};

let runtimeHandler: ((status: RuntimeStatus) => void) | undefined;
let windowCloseHandler: (() => void) | undefined;
let trayExitHandler: (() => void) | undefined;
let trayActionErrorHandler: ((message: string) => void) | undefined;
const runtimeUnlistenSpy = vi.fn<() => void>();
const windowUnlistenSpy = vi.fn<() => void>();
const trayUnlistenSpy = vi.fn<() => void>();
const trayActionErrorUnlistenSpy = vi.fn<() => void>();

function resetApi() {
  vi.clearAllMocks();
  runtimeHandler = undefined;
  windowCloseHandler = undefined;
  trayExitHandler = undefined;
  trayActionErrorHandler = undefined;
  runtimeUnlistenSpy.mockReset();
  windowUnlistenSpy.mockReset();
  trayUnlistenSpy.mockReset();
  trayActionErrorUnlistenSpy.mockReset();
  vi.mocked(tauriApi.enumerateDisplays).mockResolvedValue([display]);
  vi.mocked(tauriApi.enumerateDisplayModes).mockResolvedValue([mode]);
  vi.mocked(tauriApi.getSettings).mockResolvedValue({
    defaultDisplayModes: [mode],
    minimizeBehavior: "taskbar",
    closeBehavior: "hideToTray",
    confirmCloseBehavior: true,
  });
  vi.mocked(tauriApi.setCloseBehavior).mockImplementation(async (
    closeBehavior,
    confirmCloseBehavior,
    minimizeBehavior,
  ) => ({
    defaultDisplayModes: [mode],
    minimizeBehavior,
    closeBehavior,
    confirmCloseBehavior,
  }));
  vi.mocked(tauriApi.getAutoStartEnabled).mockResolvedValue(false);
  vi.mocked(tauriApi.setAutoStartEnabled).mockImplementation(async (enabled) => enabled);
  vi.mocked(tauriApi.openProjectHomepage).mockResolvedValue(undefined);
  vi.mocked(tauriApi.listProfiles).mockResolvedValue([]);
  vi.mocked(tauriApi.saveProfile).mockImplementation(async (value) => value);
  vi.mocked(tauriApi.deleteProfile).mockResolvedValue(undefined);
  vi.mocked(tauriApi.listSteamGames).mockResolvedValue([]);
  vi.mocked(tauriApi.listRecentApps).mockResolvedValue([]);
  vi.mocked(tauriApi.getRuntimeStatus).mockResolvedValue(idle);
  vi.mocked(tauriApi.getPendingRestore).mockResolvedValue(null);
  vi.mocked(tauriApi.restoreDisplay).mockResolvedValue(undefined);
  vi.mocked(tauriApi.ensureWindowSize).mockResolvedValue(undefined);
  vi.mocked(tauriApi.settleWindowSizeAfterDisplayChange).mockResolvedValue(undefined);
  vi.mocked(tauriApi.minimize).mockResolvedValue(undefined);
  vi.mocked(tauriApi.toggleMaximize).mockResolvedValue(undefined);
  vi.mocked(tauriApi.hide).mockResolvedValue(undefined);
  vi.mocked(tauriApi.close).mockResolvedValue(undefined);
  vi.mocked(tauriApi.getVersion).mockResolvedValue("0.1.1");
  vi.mocked(tauriApi.listenRuntime).mockImplementation(async (handler) => {
    runtimeHandler = handler;
    return runtimeUnlistenSpy;
  });
  vi.mocked(tauriApi.listenWindowCloseRequested).mockImplementation(async (handler) => {
    windowCloseHandler = handler;
    return windowUnlistenSpy;
  });
  vi.mocked(tauriApi.listenTrayExitRequested).mockImplementation(async (handler) => {
    trayExitHandler = handler;
    return trayUnlistenSpy;
  });
  vi.mocked(tauriApi.listenTrayActionError).mockImplementation(async (handler) => {
    trayActionErrorHandler = handler;
    return trayActionErrorUnlistenSpy;
  });
}

describe("App controller", () => {
  beforeEach(() => {
    window.localStorage.clear();
    resetApi();
  });

  it("restores and remembers the sidebar state", async () => {
    const user = userEvent.setup();
    window.localStorage.setItem("resolution-hub-sidebar-collapsed", "true");
    const { container } = render(<App />);
    const shell = container.querySelector(".app-shell");

    expect(shell).toHaveClass("is-sidebar-collapsed");
    const expandButton = screen.getByRole("button", { name: "展开侧边栏" });
    const settingsButton = screen.getByRole("button", { name: "应用设置" });
    expect(expandButton.closest(".sidebar-foot")).not.toBeNull();
    expect(expandButton).not.toHaveAttribute("data-tooltip");
    expect(settingsButton).not.toHaveAttribute("data-tooltip");

    await user.click(expandButton);
    expect(shell).not.toHaveClass("is-sidebar-collapsed");
    expect(window.localStorage.getItem("resolution-hub-sidebar-collapsed")).toBe("false");

    const collapseButton = screen.getByRole("button", { name: "收起侧边栏" });
    expect(collapseButton).not.toHaveAttribute("data-tooltip");
    await user.click(collapseButton);
    expect(shell).toHaveClass("is-sidebar-collapsed");
    expect(window.localStorage.getItem("resolution-hub-sidebar-collapsed")).toBe("true");
  });

  it("uses the shared tooltip layer instead of native title hints", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.listProfiles).mockResolvedValue([profile]);
    render(<App />);
    await screen.findByRole("button", { name: `启动 ${profile.name}` });
    // 常驻 tooltip 目前唯一的实际使用点是配置摘要里的「启动来源」药丸。
    const sourceTag = document.querySelector<HTMLElement>('[data-kind="source"]')!;

    expect(sourceTag).not.toHaveAttribute("title");
    await user.hover(sourceTag);
    expect(await screen.findByRole("tooltip")).toHaveTextContent("启动来源");

    await user.click(sourceTag);
    await waitFor(() => expect(screen.queryByRole("tooltip")).not.toBeInTheDocument());

    const statusMode = document.querySelector<HTMLElement>("[data-tooltip-overflow]")!;
    await user.hover(statusMode);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    Object.defineProperties(statusMode, {
      clientWidth: { value: 100 },
      scrollWidth: { value: 200 },
    });
    await user.unhover(statusMode);
    await user.hover(statusMode);
    expect(await screen.findByRole("tooltip")).toHaveTextContent(statusMode.dataset.tooltipOverflow!);
  });

  it("collapses the sidebar when the window becomes narrow", () => {
    let changeHandler: ((event: MediaQueryListEvent) => void) | undefined;
    const mediaQuery = {
      matches: false,
      media: "(max-width: 820px)",
      onchange: null,
      addEventListener: vi.fn((_type, handler) => {
        changeHandler = handler as (event: MediaQueryListEvent) => void;
      }),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(() => false),
    } as MediaQueryList;
    const matchMediaSpy = vi.spyOn(window, "matchMedia").mockReturnValue(mediaQuery);
    const { container } = render(<App />);

    act(() => changeHandler?.({ matches: true } as MediaQueryListEvent));

    expect(container.querySelector(".app-shell")).toHaveClass("is-sidebar-collapsed");
    expect(screen.getByRole("button", { name: "展开侧边栏" })).toBeVisible();
    expect(window.localStorage.getItem("resolution-hub-sidebar-collapsed")).toBe("true");
    matchMediaSpy.mockRestore();
  });

  it("subscribes to runtime updates, refreshes after idle, and unsubscribes", async () => {
    const { unmount } = render(<App />);
    await screen.findByRole("button", { name: "添加第一个游戏" });
    expect(runtimeHandler).toBeTypeOf("function");
    const initialDisplayCalls = vi.mocked(tauriApi.enumerateDisplays).mock.calls.length;

    act(() => runtimeHandler?.({
      ...idle,
      phase: "waiting",
      profileId: "game-1",
      profileName: "Test Game",
      message: "等待游戏进程",
    }));
    act(() => runtimeHandler?.({ ...idle, message: "已取消等待" }));
    act(() => trayActionErrorHandler?.("托盘快捷启动失败"));
    expect(await screen.findByText("托盘快捷启动失败")).toBeVisible();

    await waitFor(() => {
      expect(tauriApi.settleWindowSizeAfterDisplayChange).toHaveBeenCalledOnce();
      expect(tauriApi.setWindowSizePersistenceSuspended).toHaveBeenCalledWith(true);
      expect(tauriApi.setWindowSizePersistenceSuspended).toHaveBeenLastCalledWith(false);
      expect(tauriApi.enumerateDisplays).toHaveBeenCalledTimes(initialDisplayCalls + 1);
    });
    unmount();
    expect(runtimeUnlistenSpy).toHaveBeenCalledTimes(1);
    expect(windowUnlistenSpy).toHaveBeenCalledTimes(1);
    expect(trayUnlistenSpy).toHaveBeenCalledTimes(1);
    expect(trayActionErrorUnlistenSpy).toHaveBeenCalledTimes(1);
  });

  it("normalizes the logical window size after the display switch completes", async () => {
    const { unmount } = render(<App />);
    await screen.findByRole("button", { name: "添加第一个游戏" });
    vi.mocked(tauriApi.ensureWindowSize).mockClear();
    vi.mocked(tauriApi.setWindowSizePersistenceSuspended).mockClear();

    act(() => runtimeHandler?.({
      ...idle,
      phase: "switching",
      profileId: "game-1",
      profileName: "Test Game",
      message: "正在切换显示模式",
    }));
    act(() => runtimeHandler?.({
      ...idle,
      phase: "running",
      profileId: "game-1",
      profileName: "Test Game",
      message: "游戏运行中",
      canRestore: true,
    }));

    await waitFor(() => {
      expect(tauriApi.ensureWindowSize).toHaveBeenCalledOnce();
      expect(tauriApi.setWindowSizePersistenceSuspended).toHaveBeenLastCalledWith(true);
    });
    unmount();
  });

  it("lets the user disable future close prompts before hiding to tray", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "关闭" }));
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("关闭主窗口时如何处理？");
    expect(screen.queryByRole("button", { name: "取消" })).not.toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /隐藏到系统托盘/ })).toBeChecked();
    const dontAskAgain = screen.getByRole("checkbox", { name: "不再提醒我" });
    expect(dontAskAgain).not.toBeChecked();
    await user.click(dontAskAgain);
    await user.click(screen.getByRole("button", { name: "确定" }));

    await waitFor(() => expect(tauriApi.setCloseBehavior).toHaveBeenCalledWith(
      "hideToTray",
      false,
      "taskbar",
    ));
    expect(tauriApi.hide).toHaveBeenCalledTimes(1);
    expect(tauriApi.close).not.toHaveBeenCalled();

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(tauriApi.hide).toHaveBeenCalledTimes(2);
  });

  it("uses the saved hide behavior for a native window close request", async () => {
    vi.mocked(tauriApi.getSettings).mockResolvedValue({
      defaultDisplayModes: [mode],
      minimizeBehavior: "taskbar",
      closeBehavior: "hideToTray",
      confirmCloseBehavior: false,
      });
    render(<App />);
    await screen.findByRole("button", { name: "添加第一个游戏" });

    act(() => windowCloseHandler?.());

    await waitFor(() => expect(tauriApi.hide).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("treats the tray exit command as an exit even when close hides", async () => {
    vi.mocked(tauriApi.getSettings).mockResolvedValue({
      defaultDisplayModes: [mode],
      minimizeBehavior: "taskbar",
      closeBehavior: "hideToTray",
      confirmCloseBehavior: false,
      });
    render(<App />);
    await screen.findByRole("button", { name: "添加第一个游戏" });

    act(() => trayExitHandler?.());

    await waitFor(() => expect(tauriApi.close).toHaveBeenCalledTimes(1));
    expect(tauriApi.hide).not.toHaveBeenCalled();
  });

  it("delegates active session cleanup to the atomic backend exit", async () => {
    vi.mocked(tauriApi.getRuntimeStatus).mockResolvedValue({
      ...idle,
      phase: "switching",
      profileId: profile.id,
      profileName: profile.name,
      message: "正在切换显示模式",
      canRestore: false,
    });
    render(<App />);
    // 会话进行中主区域是专注页，游戏库不再渲染。
    await screen.findByRole("heading", { name: profile.name });

    act(() => trayExitHandler?.());

    await waitFor(() => expect(tauriApi.close).toHaveBeenCalledTimes(1));
    expect(tauriApi.restoreDisplay).not.toHaveBeenCalled();
    expect(tauriApi.setWindowSizePersistenceSuspended).toHaveBeenLastCalledWith(true);
  });

  it("restores a startup recovery point through the modal", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.getPendingRestore).mockResolvedValue({
      profileName: "Test Game",
      mode,
    });
    render(<App />);

    expect(await screen.findByRole("alertdialog")).toHaveTextContent("检测到桌面恢复点");
    await user.click(screen.getByRole("button", { name: "立即恢复" }));

    await waitFor(() => expect(tauriApi.restoreDisplay).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });

  it("does not close or discard a dirty draft when combined restore fails", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.getSettings).mockResolvedValue({
      defaultDisplayModes: [mode],
      minimizeBehavior: "taskbar",
      closeBehavior: "exit",
      confirmCloseBehavior: false,
      });
    vi.mocked(tauriApi.getPendingRestore).mockResolvedValue({
      profileName: "Test Game",
      mode,
    });
    vi.mocked(tauriApi.close).mockRejectedValue(new Error("显示器拒绝恢复"));
    render(<App />);

    await screen.findByRole("alertdialog");
    await user.click(screen.getByRole("button", { name: "取消" }));
    await user.click(await screen.findByRole("button", { name: "添加第一个游戏" }));
    const name = await screen.findByLabelText("游戏名称");
    await user.type(name, "Unsaved Game");
    await user.click(screen.getByRole("button", { name: "关闭" }));

    expect(await screen.findByText("恢复桌面、丢弃更改并关闭？")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "恢复并关闭" }));

    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeInTheDocument());
    expect(tauriApi.close).toHaveBeenCalledTimes(1);
    expect(tauriApi.restoreDisplay).not.toHaveBeenCalled();
    expect(name).toHaveValue("Unsaved Game");
    expect(screen.getByRole("alertdialog")).toHaveTextContent("显示器拒绝恢复");
  });

  it("rolls back the controlled autostart switch when persistence fails", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.setAutoStartEnabled).mockRejectedValue(new Error("注册表写入失败"));
    render(<App />);

    await user.click(screen.getByRole("button", { name: "应用设置" }));
    const toggle = await screen.findByRole("switch", { name: "Windows 开机自动启动" });
    await user.click(toggle);

    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(screen.getByRole("alert")).toHaveTextContent("注册表写入失败");
  });

  it("does not show a routine success toast", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "应用设置" }));
    await user.click(await screen.findByRole("switch", { name: "Windows 开机自动启动" }));

    await waitFor(() => expect(tauriApi.setAutoStartEnabled).toHaveBeenCalledWith(true));
    expect(screen.queryByText("已启用 Windows 开机自启")).not.toBeInTheDocument();
  });

  it("requires confirmation before deleting a profile", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.listProfiles)
      .mockResolvedValueOnce([profile])
      .mockResolvedValueOnce([]);
    render(<App />);

    const trigger = await screen.findByRole("button", { name: "管理 Test Game" });
    await user.click(trigger);
    await user.click(await screen.findByRole("menuitem", { name: "删除配置" }));
    expect(await screen.findByText("删除“Test Game”？")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(trigger).toHaveFocus());

    await user.click(trigger);
    await user.click(await screen.findByRole("menuitem", { name: "删除配置" }));
    await user.click(screen.getByRole("button", { name: "删除配置" }));

    await waitFor(() => expect(tauriApi.deleteProfile).toHaveBeenCalledWith("game-1"));
    expect(await screen.findByRole("button", { name: "添加第一个游戏" })).toBeInTheDocument();
  });

  it("shows the running game on a focused page and locks navigation", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.listProfiles).mockResolvedValue([profile]);
    vi.mocked(tauriApi.getRuntimeStatus).mockResolvedValue({
      ...idle,
      phase: "running",
      profileId: profile.id,
      profileName: profile.name,
      message: "游戏运行中，退出后将自动恢复",
      canRestore: true,
    });
    render(<App />);

    await screen.findByRole("heading", { name: profile.name });
    expect(screen.getByText("游戏运行中，退出后将自动恢复")).toBeVisible();
    expect(screen.getByText("1920 × 1080 · 60 Hz", { selector: ".running-mode span" }))
      .toBeVisible();
    // 游戏库不再渲染，三个导航项在接管期间禁用。
    expect(screen.queryByRole("button", { name: `启动 ${profile.name}` })).not.toBeInTheDocument();
    for (const label of ["游戏库", "显示设置", "应用设置"]) {
      expect(screen.getByRole("button", { name: label })).toBeDisabled();
    }
    // 会话操作改由专注页承担，底部状态栏不再重复渲染它们。
    expect(document.querySelector(".safety-actions button")).toBeNull();

    // 会话是守护式的，游戏可以反复启停，所以专注页只提供「结束会话」这一个出口。
    await user.click(screen.getByRole("button", { name: "结束会话" }));
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("结束当前会话？");
    await user.click(screen.getByRole("button", { name: "结束会话" }));
    await waitFor(() => expect(tauriApi.restoreDisplay).toHaveBeenCalledTimes(1));
  });
});
