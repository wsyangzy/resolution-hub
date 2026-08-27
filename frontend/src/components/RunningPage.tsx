import { MonitorCog, PowerOff } from "lucide-react";

import type { GameProfile, RuntimeStatus } from "../types";
import { AppIcon, profileIconFallback, profileIconSource } from "./AppIcon";
import { RuntimeText } from "./ui";

export interface RunningPageProps {
  runtime: RuntimeStatus;
  profile: GameProfile | null;
  onEnd: () => void;
}

function modeLabel(profile: GameProfile | null): string | null {
  if (!profile) {
    return null;
  }
  return `${profile.width} × ${profile.height} · ${profile.refreshRate} Hz`;
}

export function RunningPage({ runtime, profile, onEnd }: RunningPageProps) {
  const title = runtime.profileName ?? profile?.name ?? "游戏会话";
  const mode = modeLabel(profile);

  return (
    <section className="running-page" aria-labelledby="running-title">
      <div className="running-card">
        {profile
          ? (
            <AppIcon
              className="running-avatar"
              source={profileIconSource(profile)}
              fallback={profileIconFallback(profile, 30)}
            />
          )
          : (
            <span className="app-icon running-avatar" aria-hidden="true">
              <span className="app-icon-fallback"><MonitorCog size={30} /></span>
            </span>
          )}

        <h1 className="running-title" id="running-title" data-tooltip-overflow={title}>
          {title}
        </h1>

        <div className="running-state" data-phase={runtime.phase} role="status">
          <span className="runtime-dot" aria-hidden="true" />
          <RuntimeText>{runtime.message || "正在处理"}</RuntimeText>
        </div>

        {mode && (
          <p className="running-mode">
            <MonitorCog size={15} aria-hidden="true" />
            <span>{mode}</span>
          </p>
        )}

        {/* 会话在这个页面存在期间一直有效，游戏可以反复启停，所以只需要一个退出口。 */}
        <div className="running-actions">
          <button type="button" className="button button-warning" onClick={onEnd}>
            <PowerOff size={15} aria-hidden="true" />
            结束会话
          </button>
        </div>
      </div>
    </section>
  );
}
