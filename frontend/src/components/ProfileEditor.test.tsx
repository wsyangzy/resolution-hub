import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { tauriApi } from "../lib/tauri";
import type { DisplayInfo, GameProfile, RecentApplication } from "../types";
import { ProfileEditor } from "./ProfileEditor";

vi.mock("../lib/tauri", () => ({
  tauriApi: {
    enumerateDisplayModes: vi.fn(),
    listSteamGames: vi.fn(),
    listRecentApps: vi.fn(),
    getApplicationIcon: vi.fn(),
    selectExecutable: vi.fn(),
    selectShortcut: vi.fn(),
    openLaunchSource: vi.fn(),
    detectGameProcess: vi.fn(),
  },
}));

const display: DisplayInfo = {
  deviceName: "DISPLAY1",
  friendlyName: "Primary Panel",
  isPrimary: true,
  currentMode: {
    deviceName: "DISPLAY1",
    width: 1920,
    height: 1080,
    refreshRate: 60,
    bitsPerPixel: 32,
  },
};

const profile: GameProfile = {
  id: "game-1",
  name: "Long Running Game",
  launchType: "executable",
  executable: "C:\\Games\\game.exe",
  arguments: "",
  workingDirectory: "C:\\Games",
  steamAppId: "",
  shortcutPath: "",
  launcherProvider: "",
  processName: "game.exe",
  switchTiming: "beforeLaunch",
  launchTimeoutSec: 600,
  displayDevice: "DISPLAY1",
  width: 1920,
  height: 1080,
  refreshRate: 60,
  restoreDelayMs: 2000,
};

function renderEditor(overrides: Partial<React.ComponentProps<typeof ProfileEditor>> = {}) {
  const props: React.ComponentProps<typeof ProfileEditor> = {
    profile,
    profiles: [],
    displays: [display],
    busy: false,
    onSave: vi.fn(),
    onBack: vi.fn(),
    onDirtyChange: vi.fn(),
    onToast: vi.fn(),
    ...overrides,
  };
  return { props, ...render(<ProfileEditor {...props} />) };
}

describe("ProfileEditor", () => {
  beforeEach(() => {
    vi.mocked(tauriApi.enumerateDisplayModes).mockResolvedValue([display.currentMode]);
    vi.mocked(tauriApi.listSteamGames).mockResolvedValue([]);
    vi.mocked(tauriApi.listRecentApps).mockResolvedValue([]);
    vi.mocked(tauriApi.getApplicationIcon).mockResolvedValue(null);
    vi.mocked(tauriApi.openLaunchSource).mockResolvedValue(undefined);
    vi.mocked(tauriApi.detectGameProcess).mockResolvedValue([]);
  });

  it("uses an existing VALORANT shortcut for the launcher preset icon", async () => {
    const user = userEvent.setup();
    const valorantProfile: GameProfile = {
      ...profile,
      id: "valorant",
      launchType: "shortcut",
      executable: "",
      shortcutPath: "E:\\无畏契约.lnk",
      launcherProvider: "Riot Client",
      processName: "VALORANT-Win64-Shipping.exe",
    };
    renderEditor({ profile: null, profiles: [valorantProfile] });

    await user.click(screen.getByRole("tab", { name: "启动器" }));
    await waitFor(() => expect(tauriApi.getApplicationIcon).toHaveBeenCalledWith({
      shortcutPath: "E:\\无畏契约.lnk",
      processName: "VALORANT-Win64-Shipping.exe",
    }));
  });

  it("uses the compact source panel for a local executable", async () => {
    const user = userEvent.setup();
    const { container } = renderEditor({ profile: null });
    await user.click(screen.getByRole("tab", { name: "本地 EXE" }));
    expect(container.querySelector(".source-panel")).toHaveClass("source-panel-executable");
  });

  it("converts minutes to seconds and seconds to milliseconds when saving", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(undefined);
    renderEditor({ onSave });

    await waitFor(() => expect(tauriApi.enumerateDisplayModes).toHaveBeenCalledWith("DISPLAY1"));
    const timeout = screen.getByLabelText(/等待游戏启动/) as HTMLInputElement;
    const restoreDelay = screen.getByLabelText(/退出后恢复延迟/) as HTMLInputElement;
    expect(timeout).toHaveValue(10);
    expect(restoreDelay).toHaveValue(2);

    await user.clear(timeout);
    await user.type(timeout, "3");
    await user.clear(restoreDelay);
    await user.type(restoreDelay, "4");
    await user.click(screen.getByRole("button", { name: "保存配置" }));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onSave.mock.calls[0]?.[0]).toMatchObject({
      launchTimeoutSec: 180,
      restoreDelayMs: 4000,
    });
  });

  it("uses the two-stage launcher detection flow and accepts a candidate", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.selectShortcut).mockResolvedValue("C:\\Games\\Launcher.lnk");
    const candidates: RecentApplication[] = [
      {
        pid: 101,
        name: "Launcher",
        executable: "C:\\Games\\launcher.exe",
        processName: "launcher.exe",
        lastSeen: 1,
      },
      {
        pid: 202,
        name: "Actual Game",
        executable: "C:\\Games\\actual-game.exe",
        processName: "actual-game.exe",
        lastSeen: 2,
      },
    ];
    vi.mocked(tauriApi.detectGameProcess).mockResolvedValue(candidates);
    renderEditor({ profile: null });

    await user.click(screen.getByRole("tab", { name: "启动器" }));
    await user.click(screen.getByRole("button", { name: /其他启动器/ }));
    await waitFor(() => expect(tauriApi.selectShortcut).toHaveBeenCalledTimes(1));

    await user.click(screen.getByRole("button", { name: "打开启动器" }));
    await waitFor(() => expect(tauriApi.openLaunchSource).toHaveBeenCalledTimes(1));
    expect(tauriApi.detectGameProcess).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "开始识别" }));
    await screen.findByRole("button", { name: "actual-game.exe" });
    await user.click(screen.getByRole("button", { name: "actual-game.exe" }));

    expect(screen.getByLabelText("游戏主进程")).toHaveValue("actual-game.exe");
    expect(tauriApi.detectGameProcess).toHaveBeenCalledWith(expect.objectContaining({
      launchType: "shortcut",
      launchBeforeDetect: false,
      shortcutPath: "C:\\Games\\Launcher.lnk",
    }));
  });

  it("keeps the draft intact when saving fails", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockRejectedValue(new Error("磁盘写入失败"));
    const onToast = vi.fn();
    renderEditor({ onSave, onToast });

    const name = screen.getByLabelText("游戏名称");
    await user.clear(name);
    await user.type(name, "Edited Name");
    await user.click(screen.getByRole("button", { name: "保存配置" }));

    await waitFor(() => expect(onToast).toHaveBeenCalledWith("保存游戏配置失败：磁盘写入失败", "error"));
    expect(name).toHaveValue("Edited Name");
    expect(screen.getByText("有未保存更改")).toBeInTheDocument();
  });

  it("selects a display mode through the styled keyboard-accessible menu", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriApi.enumerateDisplayModes).mockResolvedValue([
      display.currentMode,
      { ...display.currentMode, width: 2560, height: 1440, refreshRate: 144 },
    ]);
    renderEditor();

    const trigger = screen.getByLabelText("分辨率与刷新率");
    await waitFor(() => expect(trigger).toHaveTextContent("1920 × 1080"));
    await user.click(trigger);
    await user.click(screen.getByRole("menuitemradio", { name: "2560 × 1440，144 Hz" }));

    expect(trigger).toHaveTextContent("2560 × 1440");
    expect(trigger).toHaveTextContent("144 Hz");
  });
});
