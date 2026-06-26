// Helpers de formato: tamaños legibles y fechas. Usa unidades binarias (GiB)
// como DiskCatalogMaker / Finder de macOS muestran tamaños de disco.

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const i = Math.min(UNITS.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const val = bytes / Math.pow(1024, i);
  const decimals = i === 0 ? 0 : val >= 100 ? 0 : val >= 10 ? 1 : 2;
  return `${val.toFixed(decimals)} ${UNITS[i]}`;
}

export function formatDate(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "—";
  const d = new Date(unixSeconds * 1000);
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatCount(n: number): string {
  return n.toLocaleString();
}

/** Duración en ms → "1:02:03" o "2:05". */
export function formatDuration(ms: number | null | undefined): string {
  if (!ms || ms <= 0) return "—";
  const total = Math.round(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** Bitrate en bits/s → "12.0 Mbps". */
export function formatBitrate(bps: number | null | undefined): string {
  if (!bps || bps <= 0) return "—";
  const mbps = bps / 1_000_000;
  return mbps >= 1 ? `${mbps.toFixed(1)} Mbps` : `${(bps / 1000).toFixed(0)} kbps`;
}
