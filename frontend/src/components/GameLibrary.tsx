import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import {
  Activity,
  Cpu,
  Gamepad2,
  LayoutGrid,
  LibraryBig,
  Link2,
  List,
  Monitor,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  SquareTerminal,
  Trash2,
} from "lucide-react";
import { useRef, useState } from "react";

import type { GameProfile, RuntimePhase, RuntimeStatus } from "../types";
import { AppIcon } from "./AppIcon";
import { RuntimeText } from "./ui";

type LibraryLayout = "cards" | "list";

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

function profileIconSource(profile: GameProfile) {
  return {
    steamAppId: profile.launchType === "steam" ? profile.steamAppId : "",
    shortcutPath: profile.launchType === "shortcut" ? profile.shortcutPath : "",
    executable: profile.launchType === "executable" ? profile.executable : "",
    processName: profile.processName,
  };
}

function profileIconFallback(profile: GameProfile) {
  if (profile.launchType === "steam") {
    return <Gamepad2 size={19} />;
  }
  if (profile.launchType === "shortcut") {
    return <Link2 size={19} />;
  }
  return <SquareTerminal size={19} />;
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
            <h2 className="profile-name" title={profile.name}>
              {profile.name}
            </h2>
            {active && (
              <span className="profile-active-badge">
                <Activity size={13} aria-hidden="true" />
                {phaseLabel}
              </span>
            )}
          </div>
          <p className="profile-process" title={profile.processName}>
            <Cpu size={14} aria-hidden="true" />
            <span>{profile.processName || "未配置主进程"}</span>
          </p>
        </div>
      </header>

      {active && (
        <div className="profile-runtime-message" role="status">
          <span className="profile-runtime-indicator" aria-hidden="true" />
          <RuntimeText>{runtime.message || phaseLabel}</RuntimeText>
        </div>
      )}

      <dl className="profile-details">
        <div className="profile-detail-row profile-launch-source-row">
          <dt>启动来源</dt>
          <dd>
            <strong>{launchSourceLabel(profile)}</strong>
            <span className="profile-detail-secondary" title={launchSourceDetail(profile)}>
              {launchSourceDetail(profile)}
            </span>
          </dd>
        </div>
        <div className="profile-detail-row profile-display-row">
          <dt>目标显示器</dt>
          <dd title={profile.displayDevice}>
            <Monitor size={14} aria-hidden="true" />
            <span>{profile.displayDevice}</span>
          </dd>
        </div>
        <div className="profile-detail-row profile-mode-row">
          <dt>游戏显示模式</dt>
          <dd title={formatProfileMode(profile)}>{formatProfileMode(profile)}</dd>
        </div>
      </dl>

      <footer className="profile-card-actions">
        <button
          type="button"
          className={`button button-primary profile-launch-button${active ? " is-running" : ""}`}
          disabled={busy}
          aria-label={active ? `${profile.name}：${phaseLabel}` : `启动 ${profile.name}`}
          title={active ? phaseLabel : `启动 ${profile.name}`}
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
  const [layout, setLayout] = useState<LibraryLayout>("cards");
  const layoutToggleLabel = layout === "cards" ? "切换为单行展示" : "切换为卡片展示";

  return (
    <section className="page game-library-page" aria-labelledby="game-library-title">
      <header className="page-header game-library-header">
        <div className="page-heading">
          <h1 id="game-library-title">游戏库</h1>
          <p>共 {profiles.length} 个游戏配置</p>
        </div>
        <div className="game-library-actions">
          {!loading && profiles.length > 0 && (
            <button
              type="button"
              className="icon-button library-layout-toggle"
              aria-label={layoutToggleLabel}
              title={layoutToggleLabel}
              onClick={() => setLayout((current) => current === "cards" ? "list" : "cards")}
            >
              {layout === "cards"
                ? <List size={18} aria-hidden="true" />
                : <LayoutGrid size={18} aria-hidden="true" />}
            </button>
          )}
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
        <div className={`profile-grid${layout === "list" ? " is-list" : ""}`} data-layout={layout}>
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
