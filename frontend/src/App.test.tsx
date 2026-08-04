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
    cancelLaunchWait: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    hide: vi.fn(),
    close: vi.fn(),
    getVersion: vi.fn(),
    listenRuntime: vi.fn(),
    listenWindowCloseRequested: vi.fn(),
    listenTrayExitRequested: vi.fn(),
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
  canCancel: false,
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
const runtimeUnlistenSpy = vi.fn<() => void>();
const windowUnlistenSpy = vi.fn<() => void>();
const trayUnlistenSpy = vi.fn<() => void>();

function resetApi() {
  vi.clearAllMocks();
  runtimeHandler = undefined;
  windowCloseHandler = undefined;
  trayExitHandler = undefined;
  runtimeUnlistenSpy.mockReset();
  windowUnlistenSpy.mockReset();
  trayUnlistenSpy.mockReset();
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
  vi.mocked(tauriApi.listProfiles).mockResolvedValue([]);
  vi.mocked(tauriApi.saveProfile).mockImplementation(async (value) => value);
  vi.mocked(tauriApi.deleteProfile).mockResolvedValue(undefined);
  vi.mocked(tauriApi.listSteamGames).mockResolvedValue([]);
  vi.mocked(tauriApi.listRecentApps).mockResolvedValue([]);
  vi.mocked(tauriApi.getRuntimeStatus).mockResolvedValue(idle);
  vi.mocked(tauriApi.getPendingRestore).mockResolvedValue(null);
  vi.mocked(tauriApi.restoreDisplay).mockResolvedValue(undefined);
  vi.mocked(tauriApi.cancelLaunchWait).mockResolvedValue(undefined);
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
}

describe("App controller", () => {
  beforeEach(() => {
    resetApi();
  });

  it("lets the user collapse and expand the sidebar", async () => {
    const user = userEvent.setup();
    const { container } = render(<App />);
    const shell = container.querySelector(".app-shell");
    const collapseButton = screen.getByRole("button", { name: "收起侧边栏" });

    expect(collapseButton.closest(".sidebar-foot")).not.toBeNull();
    await user.click(collapseButton);
    expect(shell).toHaveClass("is-sidebar-collapsed");

    await user.click(screen.getByRole("button", { name: "展开侧边栏" }));
    expect(shell).not.toHaveClass("is-sidebar-collapsed");
  });

  it("dismisses a sidebar tooltip after its navigation button is clicked", async () => {
    const user = userEvent.setup();
    render(<App />);
    const settingsButton = screen.getByRole("button", { name: "应用设置" });
    const tooltip = settingsButton.closest(".t-tt-wrap");

    expect(tooltip).toHaveAttribute("data-dismissed", "false");
    await user.click(settingsButton);
    expect(tooltip).toHaveAttribute("data-dismissed", "true");
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
      canCancel: true,
    }));
    act(() => runtimeHandler?.({ ...idle, message: "已取消等待" }));

    await waitFor(() => {
      expect(tauriApi.enumerateDisplays).toHaveBeenCalledTimes(initialDisplayCalls + 1);
    });
    unmount();
    expect(runtimeUnlistenSpy).toHaveBeenCalledTimes(1);
    expect(windowUnlistenSpy).toHaveBeenCalledTimes(1);
    expect(trayUnlistenSpy).toHaveBeenCalledTimes(1);
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
    vi.mocked(tauriApi.restoreDisplay).mockRejectedValue(new Error("显示器拒绝恢复"));
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
    expect(tauriApi.close).not.toHaveBeenCalled();
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

  it("switches the game library between card and single-row layouts", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.listProfiles).mockResolvedValue([profile]);
    const { container } = render(<App />);

    const layout = await screen.findByRole("button", { name: "切换为单行展示" });
    expect(container.querySelector(".profile-grid")).toHaveAttribute("data-layout", "cards");

    await user.click(layout);
    expect(container.querySelector(".profile-grid")).toHaveAttribute("data-layout", "list");

    await user.click(screen.getByRole("button", { name: "切换为卡片展示" }));
    expect(container.querySelector(".profile-grid")).toHaveAttribute("data-layout", "cards");
  });

  it("keeps the active action consistent with the launch action in the single-row layout", async () => {
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

    await user.click(await screen.findByRole("button", { name: "切换为单行展示" }));
    const activeAction = screen.getByRole("button", { name: "Test Game：游戏运行中" });
    expect(activeAction).not.toHaveClass("is-compact");
    expect(activeAction).toHaveTextContent("运行中");
  });
});
