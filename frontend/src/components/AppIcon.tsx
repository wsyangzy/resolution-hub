import { AppWindow, Gamepad2, Link2, SquareTerminal } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";

import { tauriApi } from "../lib/tauri";
import type { ApplicationIcon, ApplicationIconRequest, GameProfile } from "../types";

const iconCache = new Map<string, Promise<string | null>>();

function sourceKey(source: ApplicationIconRequest): string {
  return [
    source.steamAppId?.trim() ?? "",
    source.shortcutPath?.trim().toLocaleLowerCase() ?? "",
    source.executable?.trim().toLocaleLowerCase() ?? "",
    source.processName?.trim().toLocaleLowerCase() ?? "",
  ].join("|");
}

function encodedDataUrl(icon: ApplicationIcon): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener("load", () => resolve(String(reader.result)));
    reader.addEventListener("error", () => reject(reader.error));
    reader.readAsDataURL(new Blob([Uint8Array.from(icon.data)], { type: icon.mimeType }));
  });
}

function rgbaDataUrl(icon: ApplicationIcon): string | null {
  if (icon.width <= 0 || icon.height <= 0 || icon.data.length !== icon.width * icon.height * 4) {
    return null;
  }
  const canvas = document.createElement("canvas");
  canvas.width = icon.width;
  canvas.height = icon.height;
  const context = canvas.getContext("2d");
  if (!context) {
    return null;
  }
  const image = context.createImageData(icon.width, icon.height);
  image.data.set(icon.data);
  context.putImageData(image, 0, 0);
  return canvas.toDataURL("image/png");
}

async function loadIcon(source: ApplicationIconRequest): Promise<string | null> {
  try {
    const icon = await tauriApi.getApplicationIcon(source);
    if (!icon) {
      return null;
    }
    return icon.kind === "encoded" ? encodedDataUrl(icon) : rgbaDataUrl(icon);
  } catch {
    return null;
  }
}

export interface AppIconProps {
  source: ApplicationIconRequest;
  className?: string;
  fallback?: ReactNode;
}

export function AppIcon({ source, className = "", fallback }: AppIconProps) {
  const key = sourceKey(source);
  const [url, setUrl] = useState<string | null>(null);
  const request = useMemo(() => ({ ...source }), [key]);

  useEffect(() => {
    let active = true;
    setUrl(null);
    if (!key.replaceAll("|", "")) {
      return () => {
        active = false;
      };
    }
    const pending = iconCache.get(key) ?? loadIcon(request);
    iconCache.set(key, pending);
    void pending.then((nextUrl) => {
      if (active) {
        setUrl(nextUrl);
      }
    });
    return () => {
      active = false;
    };
  }, [key, request]);

  return (
    <span className={`app-icon${className ? ` ${className}` : ""}`} aria-hidden="true">
      {url
        ? <img src={url} alt="" draggable={false} />
        : <span className="app-icon-fallback">{fallback ?? <AppWindow size={18} />}</span>}
    </span>
  );
}

export function profileIconSource(profile: GameProfile): ApplicationIconRequest {
  return {
    steamAppId: profile.launchType === "steam" ? profile.steamAppId : "",
    shortcutPath: profile.launchType === "shortcut" ? profile.shortcutPath : "",
    executable: profile.launchType === "executable" ? profile.executable : "",
    processName: profile.processName,
  };
}

export function profileIconFallback(profile: GameProfile, size = 19): ReactNode {
  if (profile.launchType === "steam") {
    return <Gamepad2 size={size} />;
  }
  if (profile.launchType === "shortcut") {
    return <Link2 size={size} />;
  }
  return <SquareTerminal size={size} />;
}
