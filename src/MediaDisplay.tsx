import { convertFileSrc } from "@tauri-apps/api/core";
import { useState } from "react";

interface MediaProps {
  path: string;
  type: "image" | "video" | "sticker" | "audio";
  className?: string;
  onClick?: (src: string) => void;
}

export function MediaDisplay({ path, type, className, onClick }: MediaProps) {
  const [failedPath, setFailedPath] = useState<string | null>(null);
  const src = convertFileSrc(path);

  if (failedPath === path) {
    return (
      <div style={{
        padding: "8px 12px", borderRadius: 8,
        background: "rgba(0,0,0,0.06)",
        fontSize: 13, color: "#667781",
        display: "flex", alignItems: "center", gap: 8
      }}>
        🖼️ {path.split("/").pop()}
      </div>
    );
  }

  if (type === "image" || type === "sticker") {
    return (
      <img
        src={src}
        className={className}
        style={{
          display: "block",
          maxWidth: type === "sticker" ? 160 : "100%",
          maxHeight: type === "sticker" ? 160 : 330,
          borderRadius: 6,
          objectFit: "cover",
          cursor: onClick ? "pointer" : "default",
        }}
        alt=""
        loading="lazy"
        onError={() => setFailedPath(path)}
        onClick={() => onClick?.(src)}
      />
    );
  }

  if (type === "video") {
    return (
      <video
        src={src}
        controls
        preload="metadata"
        onError={() => setFailedPath(path)}
        style={{ display: "block", maxWidth: "100%", maxHeight: 330, borderRadius: 6 }}
      />
    );
  }

  return (
    <audio
      src={src}
      controls
      preload="metadata"
      onError={() => setFailedPath(path)}
      style={{ width: "100%", maxWidth: 280 }}
    />
  );
}
