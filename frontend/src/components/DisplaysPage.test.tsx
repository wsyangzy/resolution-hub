import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { tauriApi } from "../lib/tauri";
import type { AppSettings, DisplayInfo, DisplayMode, RuntimeStatus } from "../types";
import { DisplaysPage, groupDisplayModes } from "./DisplaysPage";

vi.mock("../lib/tauri", () => ({
  tauriApi: {
    enumerateDisplayModes: vi.fn(),
    setDefaultDisplayMode: vi.fn(),
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

const runtime: RuntimeStatus = {
  phase: "idle",
  profileId: null,
  profileName: null,
  message: "就绪",
  canRestore: false,
};

const modes: DisplayMode[] = [
  display.currentMode,
  { ...display.currentMode, refreshRate: 144 },
  { ...display.currentMode, width: 2560, height: 1440, refreshRate: 60 },
  { ...display.currentMode, width: 1280, height: 720, refreshRate: 240 },
];

describe("display mode grouping", () => {
  it("sorts resolutions by pixel area and refresh rates descending", () => {
    const groups = groupDisplayModes(modes);

    expect(groups.map((group) => group.key)).toEqual([
      "2560x1440",
      "1920x1080",
      "1280x720",
    ]);
    expect(groups[1]?.modes.map((mode) => mode.refreshRate)).toEqual([144, 60]);
  });
});

describe("DisplaysPage", () => {
  beforeEach(() => {
    vi.mocked(tauriApi.enumerateDisplayModes).mockResolvedValue(modes);
  });

  it("applies the selected mode and refreshes server state", async () => {
    const user = userEvent.setup();
    const nextSettings: AppSettings = {
      defaultDisplayModes: [modes[1]!],
      minimizeBehavior: "taskbar",
      closeBehavior: "exit",
      confirmCloseBehavior: false,
    };
    vi.mocked(tauriApi.setDefaultDisplayMode).mockResolvedValue(nextSettings);
    const onSettingsChange = vi.fn();
    const onRefresh = vi.fn().mockResolvedValue(undefined);

    render(
      <DisplaysPage
        displays={[display]}
        settings={{
          defaultDisplayModes: [display.currentMode],
          minimizeBehavior: "taskbar",
          closeBehavior: "exit",
          confirmCloseBehavior: false,
            }}
        runtime={runtime}
        loading={false}
        onSettingsChange={onSettingsChange}
        onRefresh={onRefresh}
        onToast={vi.fn()}
      />,
    );

    await screen.findByText("144 Hz");
    fireEvent.click(screen.getByLabelText("144 Hz"));
    await user.click(screen.getByRole("button", { name: "应用为默认模式" }));

    await waitFor(() => expect(tauriApi.setDefaultDisplayMode).toHaveBeenCalledWith(modes[1]));
    expect(onSettingsChange).toHaveBeenCalledWith(nextSettings);
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it("keeps mode changes disabled during a game session", async () => {
    render(
      <DisplaysPage
        displays={[display]}
        settings={{
          defaultDisplayModes: [display.currentMode],
          minimizeBehavior: "taskbar",
          closeBehavior: "exit",
          confirmCloseBehavior: false,
            }}
        runtime={{ ...runtime, phase: "running", message: "游戏运行中" }}
        loading={false}
        onSettingsChange={vi.fn()}
        onRefresh={vi.fn()}
        onToast={vi.fn()}
      />,
    );

    const rate = await screen.findByLabelText("144 Hz");
    expect(rate).toBeDisabled();
    expect(screen.getByText(/当前可查看显示模式，但不能修改/)).toBeInTheDocument();
  });
});
