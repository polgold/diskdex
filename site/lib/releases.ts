import { site } from "./site";

/**
 * Versión y links de descarga leídos del último release de GitHub.
 *
 * Antes esto vivía a mano en `site.ts`: publicar un binario nuevo y olvidarse de
 * tocar el sitio dejaba diskdex.app ofreciendo una versión vieja, que es la peor
 * forma de quedar desactualizado (nadie se entera). Ahora la única fuente de
 * verdad es el release.
 *
 * Los valores de `site.fallback` se usan si GitHub no responde (rate limit de la
 * API anónima: 60 req/hora por IP, y en Vercel la IP es compartida). El sitio
 * nunca queda sin botones de descarga por un problema de red.
 *
 * Los releases en BORRADOR no salen por este endpoint, así que un release a
 * medio subir —los tres runners del CI tardan distinto— no se publica solo en la
 * web: aparece recién cuando lo publicás a mano.
 */

export interface PlatformDownload {
  available: boolean;
  href: string;
}

export interface ReleaseInfo {
  version: string;
  macArm: PlatformDownload;
  macIntel: PlatformDownload;
  win: PlatformDownload;
  /** false = se está mostrando el fallback estático, no el release real. */
  live: boolean;
}

interface GhAsset {
  name: string;
  browser_download_url: string;
}

/** Página de releases: destino de cualquier plataforma que todavía no tenga binario. */
const RELEASES: PlatformDownload = { available: false, href: site.releases };

/** Primer asset que cumpla el predicado, o el fallback a la página de releases. */
function pick(assets: GhAsset[], match: (name: string) => boolean): PlatformDownload {
  const hit = assets.find((a) => match(a.name.toLowerCase()));
  return hit ? { available: true, href: hit.browser_download_url } : RELEASES;
}

function fallback(): ReleaseInfo {
  return { version: site.fallback.version, ...site.fallback.downloads, live: false };
}

export async function getRelease(): Promise<ReleaseInfo> {
  try {
    const res = await fetch(
      `https://api.github.com/repos/${site.ghRepo}/releases/latest`,
      {
        headers: { Accept: "application/vnd.github+json" },
        // Revalidación horaria: el sitio se actualiza solo dentro de la hora de
        // publicar un release, sin gastar una llamada a la API por visita.
        next: { revalidate: 3600 },
      },
    );
    if (!res.ok) return fallback();

    const data = (await res.json()) as { tag_name?: string; assets?: GhAsset[] };
    const assets = data.assets ?? [];
    if (!data.tag_name || assets.length === 0) return fallback();

    // El instalador NSIS (.exe) antes que el .msi: es el que espera la mayoría.
    const winExe = pick(assets, (n) => n.endsWith(".exe"));
    const win = winExe.available ? winExe : pick(assets, (n) => n.endsWith(".msi"));

    return {
      version: data.tag_name.replace(/^v/, ""),
      // Nombres que produce Tauri: DiskDex_0.2.0_aarch64.dmg,
      // DiskDex_0.2.0_x64.dmg, DiskDex_0.2.0_x64-setup.exe.
      macArm: pick(assets, (n) => n.endsWith(".dmg") && n.includes("aarch64")),
      macIntel: pick(assets, (n) => n.endsWith(".dmg") && !n.includes("aarch64")),
      win,
      live: true,
    };
  } catch {
    // Red caída, JSON raro, timeout: mostrar el fallback es siempre mejor que
    // romper la página entera por la sección de descargas.
    return fallback();
  }
}
