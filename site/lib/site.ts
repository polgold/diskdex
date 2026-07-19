export const site = {
  name: "DiskDex",
  domain: "diskdex.app",
  url: "https://diskdex.app",
  repo: "https://github.com/polgold/diskdex",
  releases: "https://github.com/polgold/diskdex/releases",
  version: "0.1.0",
  downloads: {
    // Disponibilidad por plataforma: Mac ya tiene binario, Windows todavía no.
    mac: {
      available: true,
      href: "https://github.com/polgold/diskdex/releases/download/v0.1.0/DiskDex_0.1.0_x64.dmg",
    },
    win: {
      available: false,
      href: "https://github.com/polgold/diskdex/releases",
    },
  },
};

export type Site = typeof site;
