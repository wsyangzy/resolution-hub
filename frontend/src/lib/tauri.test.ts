import { beforeEach, describe, expect, it, vi } from "vitest";

const windowMocks = vi.hoisted(() => ({
  center: vi.fn<() => Promise<void>>(),
  innerSize: vi.fn<() => Promise<{
    toLogical: (scaleFactor: number) => { width: number; height: number };
  }>>(),
  onResized: vi.fn(),
  scaleFactor: vi.fn<() => Promise<number>>(),
  setSize: vi.fn<() => Promise<void>>(),
  setSizeConstraints: vi.fn<() => Promise<void>>(),
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
    tauriApi.setWindowSizePersistenceSuspended(false);
    windowMocks.center.mockReset().mockResolvedValue(undefined);
    windowMocks.innerSize.mockReset().mockResolvedValue({
      toLogical: () => ({ width: 980, height: 720 }),
    });
    windowMocks.onResized.mockReset().mockRejectedValue(new Error("native resize unavailable"));
    windowMocks.scaleFactor.mockReset().mockResolvedValue(1);
    windowMocks.setSize.mockReset().mockResolvedValue(undefined);
    windowMocks.setSizeConstraints.mockReset().mockResolvedValue(undefined);
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
    expect(windowMocks.setSizeConstraints).toHaveBeenCalledWith({
      minWidth: 760,
      minHeight: 560,
    });
    expect(windowMocks.center).toHaveBeenCalledOnce();

    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1040 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 700 });
    window.dispatchEvent(new Event("resize"));
    expect(JSON.parse(window.localStorage.getItem("resolution-hub-window-size") ?? "null"))
      .toEqual({ width: 1040, height: 700 });

    Object.defineProperty(window, "innerWidth", { configurable: true, value: 640 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 480 });
    window.dispatchEvent(new Event("resize"));
    expect(JSON.parse(window.localStorage.getItem("resolution-hub-window-size") ?? "null"))
      .toEqual({ width: 1040, height: 700 });
    release();
  });

  it("reapplies the saved logical size after a display mode change", async () => {
    window.localStorage.setItem(
      "resolution-hub-window-size",
      JSON.stringify({ width: 1120, height: 760 }),
    );

    await tauriApi.ensureWindowSize();

    expect(windowMocks.setSizeConstraints).toHaveBeenCalledWith({
      minWidth: 760,
      minHeight: 560,
    });
    expect(windowMocks.setSize).toHaveBeenCalledWith(
      expect.objectContaining({ width: 1120, height: 760 }),
    );
    expect(windowMocks.center).not.toHaveBeenCalled();
  });

  it("still restores the saved size if dynamic constraints are unavailable", async () => {
    window.localStorage.setItem(
      "resolution-hub-window-size",
      JSON.stringify({ width: 1120, height: 760 }),
    );
    windowMocks.setSizeConstraints.mockRejectedValue(new Error("permission denied"));

    await expect(tauriApi.ensureWindowSize()).resolves.toBeUndefined();
    expect(windowMocks.setSize).toHaveBeenCalledWith(
      expect.objectContaining({ width: 1120, height: 760 }),
    );
  });

  it("restores the Windows minimum client size after a restart", async () => {
    type ResizeHandler = (event: {
      payload: {
        toLogical: (scaleFactor: number) => { width: number; height: number };
      };
    }) => void;
    let resizeHandler: ResizeHandler | undefined;
    const releaseNativeResize = vi.fn<() => void>();
    windowMocks.onResized.mockImplementation(async (handler) => {
      resizeHandler = handler as ResizeHandler;
      return releaseNativeResize;
    });
    windowMocks.scaleFactor.mockResolvedValue(1.5);

    Object.defineProperty(window, "innerWidth", { configurable: true, value: 900 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 650 });
    const release = await tauriApi.initializeWindowSizePersistence();
    window.dispatchEvent(new Event("resize"));
    expect(window.localStorage.getItem("resolution-hub-window-size")).toBeNull();
    const toLogical = vi.fn((scaleFactor: number) => ({
      width: 1122 / scaleFactor,
      height: 831 / scaleFactor,
    }));
    resizeHandler?.({ payload: { toLogical } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(toLogical).toHaveBeenCalledWith(1.5);

    expect(JSON.parse(window.localStorage.getItem("resolution-hub-window-size") ?? "null"))
      .toEqual({ width: 748, height: 554 });
    release();
    windowMocks.setSize.mockClear();

    const releaseAfterRestart = await tauriApi.initializeWindowSizePersistence();
    expect(windowMocks.setSize).toHaveBeenCalledWith(
      expect.objectContaining({ width: 748, height: 554 }),
    );
    releaseAfterRestart();
    expect(releaseNativeResize).toHaveBeenCalledTimes(2);
  });

  it("does not persist transient sizes while display mode is managed", async () => {
    const release = await tauriApi.initializeWindowSizePersistence();
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1080 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 700 });

    tauriApi.setWindowSizePersistenceSuspended(true);
    window.dispatchEvent(new Event("resize"));
    expect(window.localStorage.getItem("resolution-hub-window-size")).toBeNull();

    tauriApi.setWindowSizePersistenceSuspended(false);
    window.dispatchEvent(new Event("resize"));
    expect(JSON.parse(window.localStorage.getItem("resolution-hub-window-size") ?? "null"))
      .toEqual({ width: 1080, height: 700 });
    release();
  });

  it("clamps the current logical size when there is no saved size", async () => {
    const toLogical = vi.fn((_: number) => ({ width: 640, height: 480 }));
    windowMocks.scaleFactor.mockResolvedValue(1.5);
    windowMocks.innerSize.mockResolvedValue({ toLogical });

    await tauriApi.ensureWindowSize();

    expect(toLogical).toHaveBeenCalledWith(1.5);
    expect(windowMocks.setSize).toHaveBeenCalledWith(
      expect.objectContaining({ width: 760, height: 560 }),
    );
  });
});
