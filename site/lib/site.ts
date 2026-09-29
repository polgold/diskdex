export const site = {
  name: "DiskDex",
  domain: "diskdex.app",
  url: "https://diskdex.app",
  repo: "https://github.com/polgold/diskdex",
  releases: "https://github.com/polgold/diskdex/releases",
  /** owner/repo, para la API de GitHub (ver lib/releases.ts). */
  ghRepo: "polgold/diskdex",

  /**
   * Red de contención, NO la fuente de verdad.
   *
   * La versión y los links salen del último release de GitHub (`getRelease()`);
   * esto se usa solo si la API no responde. Actualizarlo es opcional —conviene
   * cada tantos releases para que el fallback no ofrezca algo muy viejo—, pero
   * olvidarse ya no deja el sitio desactualizado.
   */
  fallback: {
    version: "0.2.2",
    downloads: {
      macArm: {
        available: true,
        href: "https://github.com/polgold/diskdex/releases/download/v0.2.2/DiskDex_0.2.2_aarch64.dmg",
      },
      macIntel: {
        available: true,
        href: "https://github.com/polgold/diskdex/releases/download/v0.2.2/DiskDex_0.2.2_x64.dmg",
      },
      win: {
        available: true,
        href: "https://github.com/polgold/diskdex/releases/download/v0.2.2/DiskDex_0.2.2_x64-setup.exe",
      },
    },
  },
};

export type Site = typeof site;
