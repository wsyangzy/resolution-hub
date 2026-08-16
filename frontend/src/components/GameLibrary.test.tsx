import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { GameProfile, RuntimeStatus } from "../types";
import { GameLibrary } from "./GameLibrary";

vi.mock("../lib/tauri", () => ({
  tauriApi: {
    getApplicationIcon: vi.fn().mockResolvedValue(null),
  },
}));

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
  autoSwitch: true,
  switchTiming: "beforeLaunch",
  launchTimeoutSec: 600,
  displayDevice: "DISPLAY1",
  width: 1920,
  height: 1080,
  refreshRate: 60,
  restoreDelayMs: 2000,
};

const idle: RuntimeStatus = {
  phase: "idle",
  profileId: null,
  profileName: null,
  message: "就绪",
  canRestore: false,
  canCancel: false,
};

function renderLibrary(runtime: RuntimeStatus) {
  return render(
    <GameLibrary
      profiles={[profile]}
      runtime={runtime}
      loading={false}
      autoSwitchEnabled={false}
      onAdd={vi.fn()}
      onEdit={vi.fn()}
      onDelete={vi.fn()}
      onLaunch={vi.fn()}
      onAutoSwitchChange={vi.fn()}
      onAutoSwitchError={vi.fn()}
    />,
  );
}

describe("GameLibrary", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("keeps the active action consistent with the launch action in the single-row layout", async () => {
    const user = userEvent.setup();
    renderLibrary({
      ...idle,
      phase: "running",
      profileId: profile.id,
      profileName: profile.name,
      message: "游戏运行中，退出后将自动恢复",
      canRestore: true,
    });

    await user.click(await screen.findByRole("button", { name: "切换为单行展示" }));
    const activeAction = screen.getByRole("button", { name: "Test Game：游戏运行中" });
    expect(activeAction).not.toHaveClass("is-compact");
    expect(activeAction).toHaveTextContent("运行中");
  });
});
