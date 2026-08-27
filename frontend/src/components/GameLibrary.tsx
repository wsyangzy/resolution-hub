import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import {
  Activity,
  Cpu,
  LibraryBig,
  Monitor,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Trash2,
} from "lucide-react";
import { useRef } from "react";

import type { GameProfile, RuntimePhase, RuntimeStatus } from "../types";
import { AppIcon, profileIconFallback, profileIconSource } from "./AppIcon";
import { RuntimeText } from "./ui";

export interface GameLibraryProps {
  profiles: GameProfile[];
  runtime: RuntimeStatus;
  loading: boolean;
  onAdd: () => void;
  onEdit: (profile: GameProfile) => void;
  onDelete: (profile: GameProfile, returnFocus?: HTMLElement | null) => void;
  onLaunch: (profile: GameProfile) => void;
}

const RUNTIME_LABELS: Record<RuntimePhase, string> = {
  idle: "就绪",
  preparing: "准备中",
  switching: "切换显示模式",
  launching: "正在启动",
  waiting: "等待游戏进程",
  running: "游戏运行中",
  restoring: "恢复桌面",
  error: "运行异常",
};

const RUNTIME_ACTION_LABELS: Record<RuntimePhase, string> = {
  idle: "启动",
  preparing: "准备中",
  switching: "切换中",
  launching: "启动中",
  waiting: "等待中",
  running: "运行中",
  restoring: "恢复中",
  error: "异常",
};

export function launchSourceLabel(profile: GameProfile): string {
  if (profile.launchType === "steam") {
    return "Steam";
  }
  if (profile.launchType === "shortcut") {
    return profile.launcherProvider || "启动器 / 快捷方式";
  }
  return "本地 EXE";
}

export function launchSourceDetail(profile: GameProfile): string {
  if (profile.launchType === "steam") {
    return profile.steamAppId ? `App ID ${profile.steamAppId}` : "未设置 App ID";
  }
  if (profile.launchType === "shortcut") {
    return profile.shortcutPath || "未设置快捷方式";
  }
  return profile.executable || "未设置可执行文件";
}

export function formatProfileMode(profile: GameProfile): string {
  return `${profile.width} × ${profile.height} · ${profile.refreshRate} Hz`;
}

export function runtimePhaseLabel(phase: RuntimePhase): string {
  return RUNTIME_LABELS[phase];
}

function LibrarySkeleton() {
  return (
    <div
      className="profile-grid game-library-skeleton t-skel-skeleton is-pulsing"
      role="status"
      aria-label="正在加载游戏配置"
    >
      {[0, 1, 2, 3].map((index) => (
        <article className="profile-card profile-card-skeleton" key={index} aria-hidden="true">
          <div className="skeleton-line skeleton-title" />
          <div className="skeleton-line skeleton-subtitle" />
          <div className="skeleton-details">
            <div className="skeleton-line" />
            <div className="skeleton-line" />
            <div className="skeleton-line" />
          </div>
          <div className="skeleton-line skeleton-action" />
        </article>
      ))}
    </div>
  );
}

interface ProfileCardProps {
  profile: GameProfile;
  runtime: RuntimeStatus;
  busy: boolean;
  onEdit: (profile: GameProfile) => void;
  onDelete: (profile: GameProfile, returnFocus?: HTMLElement | null) => void;
  onLaunch: (profile: GameProfile) => void;
}

function ProfileCard({
  profile,
  runtime,
  busy,
  onEdit,
  onDelete,
  onLaunch,
}: ProfileCardProps) {
  const active = busy && runtime.profileId === profile.id;
  const phaseLabel = runtimePhaseLabel(runtime.phase);
  const actionLabel = active ? RUNTIME_ACTION_LABELS[runtime.phase] : "启动";
  const menuTriggerRef = useRef<HTMLButtonElement>(null);

  return (
    <article
      className={`profile-card${active ? " is-active" : ""}`}
      data-launch-type={profile.launchType}
      data-runtime-phase={active ? runtime.phase : undefined}
      aria-busy={active || undefined}
    >
      <header className="profile-card-header">
        <AppIcon
          className="profile-avatar"
          source={profileIconSource(profile)}
          fallback={profileIconFallback(profile)}
        />
        <div className="profile-identity">
          <div className="profile-title-row">
            <h2 className="profile-name" data-tooltip-overflow={profile.name}>
              {profile.name}
            </h2>
            {active && (
              <span className="profile-active-badge">
                <Activity size={13} aria-hidden="true" />
                {phaseLabel}
              </span>
            )}
          </div>
          <p className="profile-process">
            <Cpu size={14} aria-hidden="true" />
            <span data-tooltip-overflow={profile.processName}>{profile.processName || "未配置主进程"}</span>
          </p>
        </div>
      </header>

      {active && (
        <div className="profile-runtime-message" role="status">
          <span className="profile-runtime-indicator" aria-hidden="true" />
          <RuntimeText>{runtime.message || phaseLabel}</RuntimeText>
        </div>
      )}

      <ul className="profile-tags" aria-label="配置摘要">
        <li
          className="profile-tag"
          data-kind="source"
          data-tooltip={`启动来源：${launchSourceLabel(profile)} · ${launchSourceDetail(profile)}`}
        >
          <span className="profile-tag-icon" aria-hidden="true">
            {profileIconFallback(profile, 12)}
          </span>
          <span>{launchSourceLabel(profile)}</span>
        </li>
        <li
          className="profile-tag"
          data-kind="display"
        >
          <span className="profile-tag-icon" aria-hidden="true">
            <Monitor size={12} />
          </span>
          <span data-tooltip-overflow={profile.displayDevice}>{profile.displayDevice}</span>
        </li>
        <li
          className="profile-tag is-accent"
          data-kind="mode"
        >
          <span>{formatProfileMode(profile)}</span>
        </li>
      </ul>

      <footer className="profile-card-actions">
        <button
          type="button"
          className={`button button-primary profile-launch-button${active ? " is-running" : ""}`}
          disabled={busy}
          aria-label={active ? `${profile.name}：${phaseLabel}` : `启动 ${profile.name}`}
          onClick={() => onLaunch(profile)}
        >
          {active ? <Activity size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
          <span>{actionLabel}</span>
        </button>

        <DropdownMenuPrimitive.Root>
          <DropdownMenuPrimitive.Trigger asChild>
            <button
              ref={menuTriggerRef}
              type="button"
              className="icon-button profile-menu-trigger"
              disabled={busy}
              aria-label={`管理 ${profile.name}`}
            >
              <MoreHorizontal size={18} aria-hidden="true" />
            </button>
          </DropdownMenuPrimitive.Trigger>
          <DropdownMenuPrimitive.Portal>
            <DropdownMenuPrimitive.Content
              className="profile-menu-content"
              align="end"
              sideOffset={6}
              collisionPadding={10}
            >
              <DropdownMenuPrimitive.Item
                className="profile-menu-item"
                disabled={busy}
                onSelect={() => onEdit(profile)}
              >
                <Pencil size={15} aria-hidden="true" />
                编辑配置
              </DropdownMenuPrimitive.Item>
              <DropdownMenuPrimitive.Separator className="profile-menu-separator" />
              <DropdownMenuPrimitive.Item
                className="profile-menu-item is-danger"
                disabled={busy}
                onSelect={() => onDelete(profile, menuTriggerRef.current)}
              >
                <Trash2 size={15} aria-hidden="true" />
                删除配置
              </DropdownMenuPrimitive.Item>
            </DropdownMenuPrimitive.Content>
          </DropdownMenuPrimitive.Portal>
        </DropdownMenuPrimitive.Root>
      </footer>
    </article>
  );
}

export function GameLibrary({
  profiles,
  runtime,
  loading,
  onAdd,
  onEdit,
  onDelete,
  onLaunch,
}: GameLibraryProps) {
  const busy = runtime.phase !== "idle" && runtime.phase !== "error";

  return (
    <section className="page game-library-page" aria-labelledby="game-library-title">
      <header className="page-header game-library-header">
        <div className="page-heading">
          <h1 id="game-library-title">游戏库</h1>
          <p>共 {profiles.length} 个游戏配置</p>
        </div>
        <div className="game-library-actions">
          <button type="button" className="button button-primary add-profile-button" onClick={onAdd} disabled={busy}>
            <Plus size={17} aria-hidden="true" />
            <span>添加游戏</span>
          </button>
        </div>
      </header>

      {loading ? (
        <LibrarySkeleton />
      ) : profiles.length === 0 ? (
        <div className="game-library-empty">
          <LibraryBig size={30} strokeWidth={1.7} aria-hidden="true" />
          <h2>还没有游戏配置</h2>
          <p>添加游戏并指定启动时使用的显示模式。</p>
          <button type="button" className="button button-primary" onClick={onAdd} disabled={busy}>
            <Plus size={17} aria-hidden="true" />
            <span>添加第一个游戏</span>
          </button>
        </div>
      ) : (
        <div className="profile-grid">
          {profiles.map((profile) => (
            <ProfileCard
              key={profile.id}
              profile={profile}
              runtime={runtime}
              busy={busy}
              onEdit={onEdit}
              onDelete={onDelete}
              onLaunch={onLaunch}
            />
          ))}
        </div>
      )}
    </section>
  );
}
