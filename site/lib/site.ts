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
    version: "0.1.0",
    downloads: {
      macArm: {
        available: false,
        href: "https://github.com/polgold/diskdex/releases",
      },
      macIntel: {
        available: true,
        href: "https://github.com/polgold/diskdex/releases/download/v0.1.0/DiskDex_0.1.0_x64.dmg",
      },
      win: {
        available: false,
        href: "https://github.com/polgold/diskdex/releases",
      },
    },
  },
};

export type Site = typeof site;
