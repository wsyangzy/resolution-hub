import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SettingsPage } from "./SettingsPage";

describe("SettingsPage", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.dataset.theme = "light";
  });

  it("persists and applies the selected theme", async () => {
    const user = userEvent.setup();
    render(
      <SettingsPage
        autoStartEnabled={false}
        minimizeBehavior="taskbar"
        closeBehavior="hideToTray"
        confirmCloseBehavior={false}
        version="0.1.1"
        onAutoStartChange={vi.fn()}
        onWindowPreferencesChange={vi.fn()}
        onToast={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("tab", { name: "深色" }));

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("resolution-hub-theme")).toBe("dark");
    expect(screen.getByText("v0.1.1")).toBeInTheDocument();
  });

  it("reports an autostart failure without changing the controlled value", async () => {
    const user = userEvent.setup();
    const onToast = vi.fn();
    const onAutoStartChange = vi.fn().mockRejectedValue(new Error("注册表写入失败"));
    render(
      <SettingsPage
        autoStartEnabled={false}
        minimizeBehavior="taskbar"
        closeBehavior="hideToTray"
        confirmCloseBehavior={false}
        version="0.1.1"
        onAutoStartChange={onAutoStartChange}
        onWindowPreferencesChange={vi.fn()}
        onToast={onToast}
      />,
    );

    const toggle = screen.getByRole("switch", { name: "Windows 开机自动启动" });
    await user.click(toggle);

    await waitFor(() => expect(onToast).toHaveBeenCalledWith("注册表写入失败", "error"));
    expect(toggle).not.toBeChecked();
  });

  it("saves the two window behavior choices", async () => {
    const user = userEvent.setup();
    const onWindowPreferencesChange = vi.fn();
    render(
      <SettingsPage
        autoStartEnabled={false}
        minimizeBehavior="taskbar"
        closeBehavior="hideToTray"
        confirmCloseBehavior={true}
        version="0.1.1"
        onAutoStartChange={vi.fn()}
        onWindowPreferencesChange={onWindowPreferencesChange}
        onToast={vi.fn()}
      />,
    );

    const minimizeGroup = screen.getByRole("radiogroup", { name: "最小化主窗口后的行为" });
    await user.click(within(minimizeGroup).getByRole("radio", { name: "隐藏到托盘" }));
    expect(onWindowPreferencesChange).toHaveBeenCalledWith(
      "hideToTray",
      "hideToTray",
      true,
    );
  });

  it("keeps controlled window preferences when persistence fails", async () => {
    const user = userEvent.setup();
    const onToast = vi.fn();
    const onWindowPreferencesChange = vi.fn().mockRejectedValue(new Error("设置文件写入失败"));
    render(
      <SettingsPage
        autoStartEnabled={false}
        minimizeBehavior="taskbar"
        closeBehavior="hideToTray"
        confirmCloseBehavior={true}
        version="0.1.1"
        onAutoStartChange={vi.fn()}
        onWindowPreferencesChange={onWindowPreferencesChange}
        onToast={onToast}
      />,
    );

    const closeGroup = screen.getByRole("radiogroup", { name: "点击关闭按钮时的行为" });
    const hideRadio = within(closeGroup).getByRole("radio", { name: "隐藏到托盘" });
    await user.click(within(closeGroup).getByRole("radio", { name: "退出程序" }));

    expect(onWindowPreferencesChange).toHaveBeenCalledWith("taskbar", "exit", true);
    await waitFor(() => expect(onToast).toHaveBeenCalledWith("设置文件写入失败", "error"));
    expect(hideRadio).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "关闭按钮时询问我" })).toBeChecked();
  });
});
