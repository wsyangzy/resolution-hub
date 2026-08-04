import { beforeEach, describe, expect, it, vi } from "vitest";

const windowMocks = vi.hoisted(() => ({
  center: vi.fn<() => Promise<void>>(),
  setSize: vi.fn<() => Promise<void>>(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => windowMocks,
  LogicalSize: class {
    constructor(public width: number, public height: number) {}
  },
}));

import { tauriApi } from "./tauri";

describe("window size persistence", () => {
  beforeEach(() => {
    window.localStorage.clear();
    windowMocks.center.mockReset().mockResolvedValue(undefined);
    windowMocks.setSize.mockReset().mockResolvedValue(undefined);
  });

  it("restores the saved logical size and remembers later resizes", async () => {
    window.localStorage.setItem(
      "resolution-hub-window-size",
      JSON.stringify({ width: 1120, height: 760 }),
    );

    const release = await tauriApi.initializeWindowSizePersistence();
    expect(windowMocks.setSize).toHaveBeenCalledWith(
      expect.objectContaining({ width: 1120, height: 760 }),
    );
    expect(windowMocks.center).toHaveBeenCalledOnce();

    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1040 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 700 });
    window.dispatchEvent(new Event("resize"));
    expect(JSON.parse(window.localStorage.getItem("resolution-hub-window-size") ?? "null"))
      .toEqual({ width: 1040, height: 700 });
    release();
  });
});
