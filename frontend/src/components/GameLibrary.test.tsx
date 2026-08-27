import { render, screen } from "@testing-library/react";
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
};

function renderLibrary(runtime: RuntimeStatus) {
  return render(
    <GameLibrary
      profiles={[profile]}
      runtime={runtime}
      loading={false}
      onAdd={vi.fn()}
      onEdit={vi.fn()}
      onDelete={vi.fn()}
      onLaunch={vi.fn()}
    />,
  );
}

describe("GameLibrary", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("keeps the active action consistent with the launch action", async () => {
    renderLibrary({
      ...idle,
      phase: "running",
      profileId: profile.id,
      profileName: profile.name,
      message: "游戏运行中，退出后将自动恢复",
      canRestore: true,
    });

    const activeAction = await screen.findByRole("button", { name: "Test Game：游戏运行中" });
    expect(activeAction).not.toHaveClass("is-compact");
    expect(activeAction).toHaveTextContent("运行中");
  });

  it("renders summary pills instead of repeated detail rows", async () => {
    renderLibrary(idle);

    const tags = await screen.findByRole("list", { name: "配置摘要" });
    expect(tags).toBeVisible();
    // 显示模式是卡片上唯一需要一眼扫到的数值，单独强调。
    expect(tags.querySelector('[data-kind="mode"]')).toHaveTextContent("1920 × 1080 · 60 Hz");
    // 显示器与模式的文案本身即完整信息，不再挂常驻 tooltip。
    expect(tags.querySelector('[data-kind="display"]')).not.toHaveAttribute("data-tooltip");
    expect(tags.querySelector('[data-kind="mode"]')).not.toHaveAttribute("data-tooltip");
    // 启动来源药丸只显示类型，完整路径靠 tooltip 承载。
    expect(tags.querySelector('[data-kind="source"]')).toHaveAttribute("data-tooltip");
  });
});
