import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

// Cache to avoid re-reading same file multiple times
const mediaCache = new Map<string, string>();

interface MediaProps {
  path: string;
  type: "image" | "video" | "sticker" | "audio";
  className?: string;
  onClick?: (src: string) => void;
}

export function MediaDisplay({ path, type, className, onClick }: MediaProps) {
  const [src, setSrc] = useState<string | null>(null);
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setError(false);
    setLoading(true);
    setSrc(null);

    // Check cache first
    if (mediaCache.has(path)) {
      setSrc(mediaCache.get(path)!);
      setLoading(false);
      return;
    }

    invoke<string>("read_media_as_base64", { path })
      .then((dataUrl) => {
        if (!cancelled) {
          mediaCache.set(path, dataUrl);
          setSrc(dataUrl);
          setLoading(false);
        }
      })
      .catch((e) => {
        console.warn("Failed to load media:", path, e);
        if (!cancelled) {
          setError(true);
          setLoading(false);
        }
      });

    return () => { cancelled = true; };
  }, [path]);

  if (loading) {
    return (
      <div style={{
        width: 200, height: 150, borderRadius: 8,
        background: "rgba(0,0,0,0.06)",
        display: "flex", alignItems: "center", justifyContent: "center",
        fontSize: 24, color: "rgba(0,0,0,0.2)"
      }}>
        ⏳
      </div>
    );
  }

  if (error || !src) {
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
        style={{ display: "block", maxWidth: "100%", maxHeight: 330, borderRadius: 6 }}
      />
    );
  }

  if (type === "audio") {
    return (
      <audio
        src={src}
        controls
        preload="metadata"
        style={{ width: "100%", maxWidth: 280 }}
      />
    );
  }

  return null;
}
