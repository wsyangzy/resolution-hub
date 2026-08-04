import {
  Check,
  CircleAlert,
  Lock,
  Monitor,
  RefreshCw,
  Settings2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { tauriApi } from "../lib/tauri";
import type {
  AppSettings,
  DisplayInfo,
  DisplayMode,
  RuntimeStatus,
} from "../types";
import { Accordion } from "./ui";

export type PageToastTone = "error" | "success" | "info";

export interface DisplaysPageProps {
  displays: DisplayInfo[];
  settings: AppSettings;
  runtime: RuntimeStatus;
  loading: boolean;
  onSettingsChange: (settings: AppSettings) => void;
  onRefresh: () => Promise<void> | void;
  onToast: (message: string, tone?: PageToastTone) => void;
}

export interface DisplayModeGroup {
  key: string;
  width: number;
  height: number;
  modes: DisplayMode[];
}

export function sameDisplayMode(
  left: DisplayMode | null | undefined,
  right: DisplayMode | null | undefined,
): boolean {
  return Boolean(
    left &&
      right &&
      left.width === right.width &&
      left.height === right.height &&
      left.refreshRate === right.refreshRate,
  );
}

export function groupDisplayModes(modes: DisplayMode[]): DisplayModeGroup[] {
  const groups = new Map<string, DisplayModeGroup>();

  for (const mode of modes) {
    const key = `${mode.width}x${mode.height}`;
    const group = groups.get(key);
    if (group) {
      group.modes.push(mode);
    } else {
      groups.set(key, {
        key,
        width: mode.width,
        height: mode.height,
        modes: [mode],
      });
    }
  }

  return [...groups.values()]
    .sort(
      (left, right) =>
        right.width * right.height - left.width * left.height ||
        right.width - left.width ||
        right.height - left.height,
    )
    .map((group) => ({
      ...group,
      modes: [...group.modes].sort(
        (left, right) =>
          right.refreshRate - left.refreshRate ||
          right.bitsPerPixel - left.bitsPerPixel,
      ),
    }));
}

function modeKey(mode: DisplayMode): string {
  return [
    mode.deviceName,
    mode.width,
    mode.height,
    mode.refreshRate,
    mode.bitsPerPixel,
  ].join(":");
}

function formatMode(mode: DisplayMode | null | undefined): string {
  return mode
    ? `${mode.width} × ${mode.height} · ${mode.refreshRate} Hz`
    : "未配置";
}

function errorMessage(error: unknown, fallback: string): string {
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  return fallback;
}

function DisplaysSkeleton() {
  return (
    <div
      className="displays-workspace displays-skeleton t-skel-skeleton is-pulsing"
      role="status"
      aria-label="正在加载显示器"
    >
      <div className="display-list-panel" aria-hidden="true">
        <div className="skeleton-line skeleton-title" />
        {[0, 1].map((item) => (
          <div className="display-option display-option-skeleton" key={item}>
            <div className="skeleton-block" />
            <div className="skeleton-line" />
            <div className="skeleton-line skeleton-short" />
          </div>
        ))}
      </div>
      <div className="display-detail-panel" aria-hidden="true">
        <div className="skeleton-line skeleton-title" />
        <div className="display-mode-summary">
          <div className="skeleton-block" />
          <div className="skeleton-block" />
        </div>
        <div className="skeleton-line" />
        <div className="skeleton-line" />
        <div className="skeleton-line" />
      </div>
    </div>
  );
}

export function DisplaysPage({
  displays,
  settings,
  runtime,
  loading,
  onSettingsChange,
  onRefresh,
  onToast,
}: DisplaysPageProps) {
  const [selectedDevice, setSelectedDevice] = useState("");
  const [modes, setModes] = useState<DisplayMode[]>([]);
  const [selectedModeKey, setSelectedModeKey] = useState("");
  const [modesLoading, setModesLoading] = useState(false);
  const [modesError, setModesError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const [applying, setApplying] = useState(false);
  const requestId = useRef(0);
  const displaysRef = useRef(displays);
  const settingsRef = useRef(settings);
  displaysRef.current = displays;
  settingsRef.current = settings;

  useEffect(() => {
    setSelectedDevice((current) => {
      if (displays.some((display) => display.deviceName === current)) {
        return current;
      }
      return (
        displays.find((display) => display.isPrimary)?.deviceName ??
        displays[0]?.deviceName ??
        ""
      );
    });
  }, [displays]);

  const selectedDisplay =
    displays.find((display) => display.deviceName === selectedDevice) ?? null;
  const configuredMode =
    settings.defaultDisplayModes.find(
      (mode) => mode.deviceName === selectedDevice,
    ) ?? null;

  useEffect(() => {
    const currentRequest = ++requestId.current;
    if (!selectedDevice) {
      setModes([]);
      setSelectedModeKey("");
      setModesError(null);
      setModesLoading(false);
      return;
    }

    setModesLoading(true);
    setModesError(null);

    void tauriApi
      .enumerateDisplayModes(selectedDevice)
      .then((nextModes) => {
        if (requestId.current !== currentRequest) {
          return;
        }
        const display = displaysRef.current.find(
          (item) => item.deviceName === selectedDevice,
        );
        const configured = settingsRef.current.defaultDisplayModes.find(
          (mode) => mode.deviceName === selectedDevice,
        );
        setSelectedModeKey((current) => {
          if (nextModes.some((mode) => modeKey(mode) === current)) {
            return current;
          }
          const preferred =
            nextModes.find((mode) => sameDisplayMode(mode, configured)) ??
            nextModes.find((mode) =>
              sameDisplayMode(mode, display?.currentMode),
            ) ??
            nextModes[0];
          return preferred ? modeKey(preferred) : "";
        });
        setModes(nextModes);
      })
      .catch((error: unknown) => {
        if (requestId.current !== currentRequest) {
          return;
        }
        setModes([]);
        setModesError(errorMessage(error, "无法读取该显示器的可用模式"));
      })
      .finally(() => {
        if (requestId.current === currentRequest) {
          setModesLoading(false);
        }
      });

    return () => {
      if (requestId.current === currentRequest) {
        requestId.current += 1;
      }
    };
  }, [reloadKey, selectedDevice]);

  useEffect(() => {
    if (modes.length === 0) {
      setSelectedModeKey("");
      return;
    }

    setSelectedModeKey((current) => {
      if (modes.some((mode) => modeKey(mode) === current)) {
        return current;
      }
      const preferred =
        modes.find((mode) => sameDisplayMode(mode, configuredMode)) ??
        modes.find((mode) => sameDisplayMode(mode, selectedDisplay?.currentMode)) ??
        modes[0];
      return preferred ? modeKey(preferred) : "";
    });
  }, [configuredMode, modes, selectedDisplay?.currentMode]);

  const groups = useMemo(() => groupDisplayModes(modes), [modes]);
  const selectedMode =
    modes.find((mode) => modeKey(mode) === selectedModeKey) ?? null;
  const modificationLocked =
    runtime.phase !== "idle" && runtime.phase !== "error";

  const selectDisplay = (deviceName: string) => {
    if (deviceName === selectedDevice) {
      return;
    }
    setModes([]);
    setSelectedModeKey("");
    setModesError(null);
    setSelectedDevice(deviceName);
  };

  const refresh = async () => {
    if (refreshing) {
      return;
    }
    setRefreshing(true);
    try {
      await onRefresh();
      setReloadKey((value) => value + 1);
    } catch (error) {
      onToast(errorMessage(error, "刷新显示器信息失败"), "error");
    } finally {
      setRefreshing(false);
    }
  };

  const applyDefaultMode = async () => {
    if (!selectedMode || modificationLocked || applying) {
      return;
    }

    setApplying(true);
    try {
      const nextSettings = await tauriApi.setDefaultDisplayMode(selectedMode);
      onSettingsChange(nextSettings);
      onToast("桌面默认显示模式已更新", "success");

      try {
        await onRefresh();
      } catch (error) {
        onToast(
          `默认模式已应用，但${errorMessage(error, "刷新显示器信息失败")}`,
          "error",
        );
      }
      setReloadKey((value) => value + 1);
    } catch (error) {
      onToast(errorMessage(error, "应用默认显示模式失败"), "error");
    } finally {
      setApplying(false);
    }
  };

  return (
    <section className="page displays-page" aria-labelledby="displays-title">
      <header className="page-header displays-page-header">
        <div className="page-heading">
          <h1 id="displays-title">显示设置</h1>
          <p>管理活动显示器与桌面默认模式</p>
        </div>
        <button
          type="button"
          className="button button-secondary displays-refresh-button"
          onClick={() => void refresh()}
          disabled={loading || refreshing}
        >
          <RefreshCw
            size={16}
            className={refreshing ? "is-spinning" : undefined}
            aria-hidden="true"
          />
          <span>{refreshing ? "刷新中" : "刷新"}</span>
        </button>
      </header>

      {loading ? (
        <DisplaysSkeleton />
      ) : displays.length === 0 ? (
        <div className="page-empty displays-empty" role="status">
          <Monitor size={32} strokeWidth={1.6} aria-hidden="true" />
          <h2>没有读取到活动显示器</h2>
          <p>请确认显示器已连接并处于启用状态。</p>
          <button
            type="button"
            className="button button-primary"
            onClick={() => void refresh()}
            disabled={refreshing}
          >
            <RefreshCw size={16} aria-hidden="true" />
            重新检测
          </button>
        </div>
      ) : (
        <div className="displays-workspace">
          <aside className="display-list-panel" aria-label="活动显示器">
            <div className="section-heading compact-heading">
              <div>
                <span className="section-kicker">活动设备</span>
                <h2>{displays.length} 台显示器</h2>
              </div>
            </div>
            <div className="display-list">
              {displays.map((display, index) => {
                const selected = display.deviceName === selectedDevice;
                return (
                  <button
                    type="button"
                    className={`display-option${selected ? " is-selected" : ""}`}
                    aria-pressed={selected}
                    onClick={() => selectDisplay(display.deviceName)}
                    key={display.deviceName}
                  >
                    <span className="display-option-icon" aria-hidden="true">
                      <Monitor size={19} />
                    </span>
                    <span className="display-option-copy">
                      <span className="display-option-index">
                        屏幕 {String(index + 1).padStart(2, "0")}
                      </span>
                      <strong title={display.friendlyName || display.deviceName}>
                        {display.friendlyName || display.deviceName}
                      </strong>
                      <small>{formatMode(display.currentMode)}</small>
                    </span>
                    {display.isPrimary && (
                      <span className="status-tag status-tag-info">主显示器</span>
                    )}
                  </button>
                );
              })}
            </div>
          </aside>

          {selectedDisplay && (
            <div className="display-detail-panel">
              <header className="display-detail-header">
                <div>
                  <span className="section-kicker">所选显示器</span>
                  <h2>{selectedDisplay.friendlyName || selectedDisplay.deviceName}</h2>
                  <p title={selectedDisplay.deviceName}>
                    {selectedDisplay.deviceName}
                  </p>
                </div>
                {selectedDisplay.isPrimary && (
                  <span className="status-tag status-tag-info">主显示器</span>
                )}
              </header>

              <div className="display-mode-summary">
                <div className="mode-summary-item current-mode-summary">
                  <span className="mode-summary-icon" aria-hidden="true">
                    <Monitor size={18} />
                  </span>
                  <div>
                    <span>当前模式</span>
                    <strong>{formatMode(selectedDisplay.currentMode)}</strong>
                  </div>
                </div>
                <div className="mode-summary-item default-mode-summary">
                  <span className="mode-summary-icon" aria-hidden="true">
                    <Settings2 size={18} />
                  </span>
                  <div>
                    <span>已配置默认模式</span>
                    <strong>{formatMode(configuredMode)}</strong>
                  </div>
                </div>
              </div>

              {modificationLocked && (
                <div className="inline-notice warning-notice" role="note">
                  <Lock size={15} aria-hidden="true" />
                  <span>
                    {runtime.message || "游戏会话正在运行"}，当前可查看显示模式，但不能修改桌面默认模式。
                  </span>
                </div>
              )}

              <section
                className="available-modes-section"
                aria-labelledby="available-modes-title"
              >
                <div className="section-heading modes-heading">
                  <div>
                    <span className="section-kicker">支持的模式</span>
                    <h3 id="available-modes-title">可用分辨率</h3>
                  </div>
                  {!modesLoading && !modesError && (
                    <span className="section-count">{groups.length} 种</span>
                  )}
                </div>

                {modesLoading ? (
                  <div
                    className="modes-loading t-skel-skeleton is-pulsing"
                    role="status"
                  >
                    <span>正在读取显示模式...</span>
                    {[0, 1, 2].map((item) => (
                      <div className="skeleton-line" key={item} aria-hidden="true" />
                    ))}
                  </div>
                ) : modesError ? (
                  <div className="modes-error" role="alert">
                    <CircleAlert size={18} aria-hidden="true" />
                    <div>
                      <strong>无法读取显示模式</strong>
                      <p>{modesError}</p>
                    </div>
                    <button
                      type="button"
                      className="button button-secondary button-small"
                      onClick={() => setReloadKey((value) => value + 1)}
                    >
                      重试
                    </button>
                  </div>
                ) : groups.length === 0 ? (
                  <div className="modes-empty" role="status">
                    该显示器没有可用的显示模式。
                  </div>
                ) : (
                  <div className="resolution-groups">
                    {groups.map((group) => {
                      const resolutionIsCurrent =
                        selectedDisplay.currentMode.width === group.width &&
                        selectedDisplay.currentMode.height === group.height;
                      const resolutionIsDefault =
                        configuredMode?.width === group.width &&
                        configuredMode.height === group.height;
                      const resolutionIsSelected = group.modes.some(
                        (mode) => modeKey(mode) === selectedModeKey,
                      );

                      return (
                        <Accordion
                          key={`${selectedDevice}-${group.key}`}
                          className="resolution-group"
                          defaultOpen={resolutionIsSelected}
                          title={
                            <span className="resolution-group-title">
                              <span className="resolution-group-copy">
                                <strong>
                                  {group.width} × {group.height}
                                </strong>
                                <small>{group.modes.length} 个刷新率</small>
                              </span>
                              <span className="resolution-mode-tags">
                                {resolutionIsCurrent && (
                                  <span className="mode-tag current">当前</span>
                                )}
                                {resolutionIsDefault && (
                                  <span className="mode-tag default">默认</span>
                                )}
                              </span>
                            </span>
                          }
                        >
                          <div className="refresh-rate-grid">
                            {group.modes.map((mode) => {
                              const key = modeKey(mode);
                              const selected = key === selectedModeKey;
                              const isCurrent = sameDisplayMode(
                                mode,
                                selectedDisplay.currentMode,
                              );
                              const isDefault = sameDisplayMode(
                                mode,
                                configuredMode,
                              );
                              return (
                                <label
                                  className={`refresh-rate-option${selected ? " is-selected" : ""}${modificationLocked ? " is-disabled" : ""}`}
                                  key={key}
                                >
                                  <input
                                    type="radio"
                                    name="default-display-mode"
                                    value={key}
                                    checked={selected}
                                    disabled={modificationLocked || applying}
                                    onChange={() => setSelectedModeKey(key)}
                                  />
                                  <span className="resolution-radio" aria-hidden="true">
                                    {selected && <Check size={12} />}
                                  </span>
                                  <strong>{mode.refreshRate} Hz</strong>
                                  <span className="resolution-mode-tags">
                                    {isCurrent && (
                                      <span className="mode-tag current">当前</span>
                                    )}
                                    {isDefault && (
                                      <span className="mode-tag default">默认</span>
                                    )}
                                  </span>
                                </label>
                              );
                            })}
                          </div>
                        </Accordion>
                      );
                    })}
                  </div>
                )}
              </section>

              <footer className="display-apply-bar">
                <div className="display-selection-summary" aria-live="polite">
                  <span>待应用模式</span>
                  <strong>{formatMode(selectedMode)}</strong>
                </div>
                <button
                  type="button"
                  className="button button-primary"
                  onClick={() => void applyDefaultMode()}
                  disabled={
                    !selectedMode ||
                    modesLoading ||
                    applying ||
                    modificationLocked ||
                    sameDisplayMode(selectedMode, configuredMode)
                  }
                >
                  <Settings2 size={16} aria-hidden="true" />
                  <span>
                    {applying
                      ? "正在应用..."
                      : sameDisplayMode(selectedMode, configuredMode)
                        ? "当前默认模式"
                        : "应用为默认模式"}
                  </span>
                </button>
              </footer>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
