export const site = {
  name: "DiskDex",
  domain: "diskdex.app",
  url: "https://diskdex.app",
  repo: "https://github.com/polgold/diskdex",
  releases: "https://github.com/polgold/diskdex/releases",
  version: "0.1",
  downloads: {
    // Flip to true and set real asset URLs once the first binary ships.
    available: false,
    mac: "https://github.com/polgold/diskdex/releases",
    win: "https://github.com/polgold/diskdex/releases",
  },
};

export type Site = typeof site;
