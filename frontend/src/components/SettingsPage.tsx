import {
  ExternalLink,
  Info,
  Moon,
  MonitorCog,
  PanelTopClose,
  Power,
  ShieldCheck,
  Sun,
} from "lucide-react";
import { useEffect, useState } from "react";

import { MotionCheckbox, MotionSwitch, SlidingTabs } from "./ui";
import type { PageToastTone } from "./DisplaysPage";
import type { CloseBehavior, MinimizeBehavior } from "../types";

export type ThemePreference = "light" | "dark";

export interface SettingsPageProps {
  autoStartEnabled: boolean;
  minimizeBehavior: MinimizeBehavior;
  closeBehavior: CloseBehavior;
  confirmCloseBehavior: boolean;
  version: string;
  onAutoStartChange: (enabled: boolean) => Promise<void> | void;
  onOpenProjectHomepage: () => Promise<void> | void;
  onWindowPreferencesChange: (
    minimizeBehavior: MinimizeBehavior,
    behavior: CloseBehavior,
    confirmCloseBehavior: boolean,
  ) => Promise<void> | void;
  onToast: (message: string, tone?: PageToastTone) => void;
}

const THEME_STORAGE_KEY = "resolution-hub-theme";
const THEME_OPTIONS = [
  { value: "light" as const, label: "浅色", icon: <Sun size={15} /> },
  { value: "dark" as const, label: "深色", icon: <Moon size={15} /> },
];
function currentTheme(): ThemePreference {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (stored === "light" || stored === "dark") {
      return stored;
    }
  } catch {
    // The pre-mount value remains usable when storage is unavailable.
  }
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
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

export function SettingsPage({
  autoStartEnabled,
  minimizeBehavior,
  closeBehavior,
  confirmCloseBehavior,
  version,
  onAutoStartChange,
  onOpenProjectHomepage,
  onWindowPreferencesChange,
  onToast,
}: SettingsPageProps) {
  const [theme, setTheme] = useState<ThemePreference>(currentTheme);
  const [autoStartSaving, setAutoStartSaving] = useState(false);
  const [windowSettingSaving, setWindowSettingSaving] = useState<"minimize" | "close" | "prompt" | null>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const updateTheme = (nextTheme: ThemePreference) => {
    setTheme(nextTheme);
    document.documentElement.dataset.theme = nextTheme;
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, nextTheme);
    } catch {
      onToast("主题已切换，但无法保存主题偏好", "error");
    }
  };

  const updateAutoStart = async (enabled: boolean) => {
    if (autoStartSaving) {
      return;
    }

    setAutoStartSaving(true);
    try {
      await onAutoStartChange(enabled);
      onToast(
        enabled ? "已启用 Windows 开机自启" : "已关闭 Windows 开机自启",
        "success",
      );
    } catch (error) {
      onToast(
        errorMessage(error, "更新 Windows 开机自启失败"),
        "error",
      );
    } finally {
      setAutoStartSaving(false);
    }
  };

  const openProjectHomepage = async () => {
    try {
      await onOpenProjectHomepage();
    } catch (error) {
      onToast(errorMessage(error, "打开项目主页失败"), "error");
    }
  };

  const updateWindowPreferences = async (
    setting: "minimize" | "close" | "prompt",
    minimize: MinimizeBehavior,
    behavior: CloseBehavior,
    confirm: boolean,
    successMessage: string,
  ) => {
    if (
      windowSettingSaving
      || (
        minimize === minimizeBehavior
        && behavior === closeBehavior
        && confirm === confirmCloseBehavior
      )
    ) {
      return;
    }

    setWindowSettingSaving(setting);
    try {
      await onWindowPreferencesChange(minimize, behavior, confirm);
      onToast(successMessage, "success");
    } catch (error) {
      onToast(errorMessage(error, "更新窗口关闭行为失败"), "error");
    } finally {
      setWindowSettingSaving(null);
    }
  };

  return (
    <section className="page settings-page" aria-labelledby="settings-title">
      <header className="page-header settings-page-header">
        <div className="page-heading">
          <h1 id="settings-title">应用设置</h1>
          <p>调整界面偏好与 Windows 集成</p>
        </div>
      </header>

      <div className="settings-sections">
        <section className="settings-section" aria-labelledby="appearance-title">
          <header className="settings-section-header">
            <span className="settings-section-icon" aria-hidden="true">
              <MonitorCog size={19} />
            </span>
            <div>
              <h2 id="appearance-title">外观</h2>
              <p>界面主题会保存在此设备上。</p>
            </div>
          </header>
          <div className="setting-row theme-setting-row">
            <div className="setting-copy">
              <strong>颜色主题</strong>
              <span>选择明亮工作区或低亮度深色界面</span>
            </div>
            <SlidingTabs
              value={theme}
              options={THEME_OPTIONS}
              onValueChange={updateTheme}
              ariaLabel="颜色主题"
              className="theme-tabs"
            />
          </div>
        </section>

        <section className="settings-section" aria-labelledby="startup-title">
          <header className="settings-section-header">
            <span className="settings-section-icon" aria-hidden="true">
              <Power size={19} />
            </span>
            <div>
              <h2 id="startup-title">Windows 启动</h2>
              <p>使用当前用户的登录启动项，不需要管理员权限。</p>
            </div>
          </header>
          <div className="setting-row auto-start-setting-row">
            <div className="setting-copy">
              <strong>开机自动启动</strong>
              <span>
                {autoStartEnabled
                  ? "登录 Windows 后自动运行 Resolution Hub"
                  : "仅在手动打开时运行 Resolution Hub"}
              </span>
            </div>
            <div className="setting-control-with-status">
              {autoStartSaving && <span className="setting-saving">保存中</span>}
              <MotionSwitch
                checked={autoStartEnabled}
                disabled={autoStartSaving}
                onCheckedChange={(enabled) => void updateAutoStart(enabled)}
                ariaLabel="Windows 开机自动启动"
              />
            </div>
          </div>
        </section>

        <section className="settings-section" aria-labelledby="close-behavior-title">
          <header className="settings-section-header">
            <span className="settings-section-icon" aria-hidden="true">
              <PanelTopClose size={19} />
            </span>
            <div>
              <h2 id="close-behavior-title">开启与关闭</h2>
              <p>决定最小化和关闭主窗口时应用的停留位置。</p>
            </div>
          </header>
          <div className="window-behavior-list">
            <div className="setting-row window-behavior-row">
              <div className="setting-copy">
                <strong>最小化主窗口后</strong>
                <span>选择保留在任务栏，或只在系统托盘中运行</span>
              </div>
              <div className="behavior-control">
                {windowSettingSaving === "minimize" && <span className="setting-saving">保存中</span>}
                <div className="behavior-radio-group" role="radiogroup" aria-label="最小化主窗口后的行为">
                  <label className="behavior-radio">
                    <input
                      type="radio"
                      name="minimize-behavior"
                      checked={minimizeBehavior === "taskbar"}
                      disabled={windowSettingSaving !== null}
                      onChange={() => void updateWindowPreferences(
                        "minimize",
                        "taskbar",
                        closeBehavior,
                        confirmCloseBehavior,
                        "最小化后将保留在任务栏",
                      )}
                    />
                    <span className="behavior-radio-mark" aria-hidden="true" />
                    <span>保留在任务栏</span>
                  </label>
                  <label className="behavior-radio">
                    <input
                      type="radio"
                      name="minimize-behavior"
                      checked={minimizeBehavior === "hideToTray"}
                      disabled={windowSettingSaving !== null}
                      onChange={() => void updateWindowPreferences(
                        "minimize",
                        "hideToTray",
                        closeBehavior,
                        confirmCloseBehavior,
                        "最小化后将隐藏到系统托盘",
                      )}
                    />
                    <span className="behavior-radio-mark" aria-hidden="true" />
                    <span>隐藏到托盘</span>
                  </label>
                </div>
              </div>
            </div>
            <div className="setting-row window-behavior-row">
              <div className="setting-copy">
                <strong>点击关闭按钮时</strong>
                <span>未启用询问时，将直接执行所选行为</span>
              </div>
              <div className="behavior-control">
                {windowSettingSaving === "close" && <span className="setting-saving">保存中</span>}
                <div className="behavior-radio-group" role="radiogroup" aria-label="点击关闭按钮时的行为">
                  <label className="behavior-radio">
                    <input
                      type="radio"
                      name="close-behavior"
                      checked={closeBehavior === "hideToTray"}
                      disabled={windowSettingSaving !== null}
                      onChange={() => void updateWindowPreferences(
                        "close",
                        minimizeBehavior,
                        "hideToTray",
                        confirmCloseBehavior,
                        "关闭主窗口时将隐藏到系统托盘",
                      )}
                    />
                    <span className="behavior-radio-mark" aria-hidden="true" />
                    <span>隐藏到托盘</span>
                  </label>
                  <label className="behavior-radio">
                    <input
                      type="radio"
                      name="close-behavior"
                      checked={closeBehavior === "exit"}
                      disabled={windowSettingSaving !== null}
                      onChange={() => void updateWindowPreferences(
                        "close",
                        minimizeBehavior,
                        "exit",
                        confirmCloseBehavior,
                        "关闭主窗口时将安全退出程序",
                      )}
                    />
                    <span className="behavior-radio-mark" aria-hidden="true" />
                    <span>退出程序</span>
                  </label>
                </div>
              </div>
            </div>
            <div className="window-prompt-row">
              <MotionCheckbox
                checked={confirmCloseBehavior}
                disabled={windowSettingSaving !== null}
                onCheckedChange={(enabled) => void updateWindowPreferences(
                  "prompt",
                  minimizeBehavior,
                  closeBehavior,
                  enabled,
                  enabled ? "关闭主窗口前将询问处理方式" : "已关闭窗口关闭提示",
                )}
                label="关闭按钮时询问我"
              />
              <span>关闭后仍可在这里重新开启提示</span>
              {windowSettingSaving === "prompt" && <span className="setting-saving">保存中</span>}
            </div>
          </div>
        </section>

        <section className="settings-section app-information-section" aria-labelledby="app-information-title">
          <header className="settings-section-header">
            <span className="settings-section-icon" aria-hidden="true">
              <Info size={19} />
            </span>
            <div>
              <h2 id="app-information-title">应用信息</h2>
              <p>本地运行，不下载封面、不发送遥测数据。</p>
            </div>
          </header>

          <div className="application-identity">
            <div className="application-mark" aria-hidden="true">RH</div>
            <div>
              <strong>Resolution Hub</strong>
              <span>Windows 游戏分辨率启动器</span>
            </div>
            <span className="version-tag">{version ? `v${version}` : "读取中..."}</span>
          </div>

          <button
            type="button"
            className="button button-secondary project-homepage-button"
            onClick={() => void openProjectHomepage()}
          >
            <ExternalLink size={16} aria-hidden="true" />
            <span>GitHub 项目主页</span>
          </button>
          <dl className="technical-information">
            <div>
              <dt>开发者</dt>
              <dd>Juice &amp; Codex</dd>
            </div>
            <div>
              <dt>界面</dt>
              <dd>React 19 · Vite 8 · TypeScript · Tailwind CSS 4</dd>
            </div>
            <div>
              <dt>桌面运行时</dt>
              <dd>Tauri 2 · Rust 2024</dd>
            </div>
            <div>
              <dt>目标系统</dt>
              <dd>Windows 10 / 11 x64</dd>
            </div>
            <div>
              <dt>数据边界</dt>
              <dd>
                <ShieldCheck size={14} aria-hidden="true" />
                配置与运行状态仅保存在本机
              </dd>
            </div>
          </dl>
        </section>
      </div>
    </section>
  );
}
