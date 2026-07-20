import type { Dictionary } from "../dictionaries";

const en: Dictionary = {
  langName: "English",
  meta: {
    title: "DiskDex — Find any file across your drives, without plugging in a disk",
    description:
      "DiskDex indexes the contents of all your backup drives so you can search and browse them even when they're unplugged. Import your catalog, scan on connect, and find any file in half a second. macOS and Windows.",
    ogAlt: "DiskDex — offline disk catalog and search",
  },
  nav: {
    features: "Features",
    how: "How it works",
    screenshots: "Screenshots",
    roadmap: "Roadmap",
    download: "Download",
    cta: "Download",
    menu: "Menu",
  },
  hero: {
    eyebrow: "Offline disk catalog",
    titlePre: "Find any file across your drives",
    titleEm: "without plugging in",
    titlePost: "a single disk.",
    sub: "DiskDex indexes the contents of all your backup drives and lets you search and browse them even when they're sitting in a drawer. Import your legacy catalog, scan new drives as you connect them, and find what you need instantly.",
    ctaPrimary: "Download for macOS",
    ctaSecondary: "Download for Windows",
    platforms: "Free · macOS 12+ and Windows 10/11 · Apple Silicon and Intel",
    stats: [
      { n: "6,828,850", l: "files indexed" },
      { n: "116 TB", l: "across 54 drives" },
      { n: "< 0.5 s", l: "per search" },
    ],
  },
  shot: {
    search: "Search",
    query: "*.mov",
    results: "261,991 results",
    resultsMeta: "0.48 s · 54 drives",
    online: "online",
    offline: "offline",
    sidebarTitle: "DRIVES",
    disks: [
      { name: "SF28", meta: "1.8 TB · 214,502 files", state: "online" },
      { name: "RAID_04", meta: "16 TB · 1,204,881 files", state: "offline" },
      { name: "LTO_BACKUP_12", meta: "12 TB · 88,310 files", state: "offline" },
      { name: "BACKUP_07", meta: "4 TB · 512,019 files", state: "offline" },
    ],
    rows: [
      { name: "C0001.MP4", path: "SF28/HUFNAGL PILAR/…/CLIP", size: "3.4 GB", state: "online" },
      { name: "entrega_final_v3.mov", path: "RAID_04/2019/PROJECTS", size: "48.1 GB", state: "offline" },
      { name: "master_color.mov", path: "LTO_BACKUP_12/COLOR", size: "174.94 GB", state: "offline" },
      { name: "render_4k_final.mov", path: "RAID_01/DELIVERIES", size: "22.7 GB", state: "offline" },
      { name: "camA_take12.mov", path: "BACKUP_07/RUSHES/DAY_03", size: "9.8 GB", state: "offline" },
    ],
  },
  trust: {
    line: "Tested against a real production house catalog:",
    highlight: "261,991 “.mov” results in 0.48 seconds",
    tail: "across 6.8 million files spread over 54 drives.",
  },
  features: {
    eyebrow: "Features",
    title: "Your whole archive, under control",
    subtitle:
      "DiskDex replaces “plug in one disk after another” with a single, portable, instantly searchable catalog.",
    items: [
      {
        key: "import",
        title: "Import your legacy catalog",
        body: "Bring in your DiskCatalogMaker .dcmf file with nothing lost: names, full hierarchy, dates and real sizes up to hundreds of GB. Validated on 54 drives and 6.8M entries.",
      },
      {
        key: "scan",
        title: "Scan on connect",
        body: "Plug in a drive and DiskDex detects it on its own. It stores the full tree with logical and physical size, dates and a volume fingerprint to recognize it next time.",
      },
      {
        key: "offline",
        title: "Know which drive is at hand",
        body: "Each drive shows as online or offline based on what's mounted, with its capacity and file count. You always search; you only plug in when you actually need to.",
      },
      {
        key: "search",
        title: "Instant multi-drive search",
        body: "Full-text search by name across every drive at once, with the full path and drive for each result. Under a second over millions of files.",
      },
      {
        key: "duplicates",
        title: "Duplicates and cleanup",
        body: "Find repeated copies across drives to reclaim space, without counting the same physical file twice. Perfect for consolidating old backups.",
      },
      {
        key: "stats",
        title: "Stats and backup audit",
        body: "A clear view of your archive: what takes the most space, how it splits by drive and type, and which material is (or isn't) backed up in more than one place.",
      },
    ],
  },
  how: {
    eyebrow: "How it works",
    title: "From 54 drives to one search box, in three steps",
    steps: [
      {
        n: "01",
        title: "Import or scan",
        body: "Bring in your existing .dcmf or connect a drive and let DiskDex index it. The heavy lifting runs in Rust and never blocks the UI.",
      },
      {
        n: "02",
        title: "It all lands in one catalog",
        body: "A single portable file backed by SQLite + full-text search. It scales to millions of files and travels with you between machines.",
      },
      {
        n: "03",
        title: "Search even when unplugged",
        body: "Type a name or an extension and get the drive and path instantly. You only plug in the disk once you already know which one it is.",
      },
    ],
  },
  screenshots: {
    eyebrow: "Screenshots",
    title: "Quiet, fast, built like a post tool",
    subtitle:
      "Dark mode, keyboard shortcuts and virtualized lists that stay smooth even with millions of rows.",
    captions: {
      main: "Main view — drives, contents and search in one place",
      search: "Full-text search with the path and drive of every result",
      inspector: "Inspector — details for each file or folder",
    },
  },
  roadmap: {
    eyebrow: "Roadmap",
    title: "What's next",
    subtitle: "The engine is ready; we keep adding features to the interface.",
    connectorTitle: "Secure remote connector",
    connectorBody:
      "Pull the actual files from your local network to the cloud or another machine — without moving the disk. Read-only, device-authenticated and encrypted.",
    items: [
      { label: "Import .dcmf", state: "done" },
      { label: "Scan on connect", state: "done" },
      { label: "Online / offline", state: "done" },
      { label: "Multi-drive search", state: "done" },
      { label: "Duplicates and stats", state: "progress" },
      { label: "Advanced filters (type, size, date)", state: "progress" },
      { label: "Export (CSV / JSON / PDF)", state: "planned" },
      { label: "Secure remote connector", state: "planned" },
    ],
    stateLabels: {
      done: "Done",
      progress: "In progress",
      planned: "Planned",
    },
  },
  download: {
    eyebrow: "Download",
    title: "Start cataloging today",
    sub: "Free download for macOS and Windows. No account, no mandatory cloud: your catalog lives on your machine.",
    mac: "Download for macOS",
    win: "Download for Windows",
    macMeta: "macOS · Intel 64-bit (runs on Apple Silicon via Rosetta)",
    winMeta: "Windows 10 / 11 · 64-bit",
    soon: "Coming soon",
    soonNote:
      "The Windows build is on the way. In the meantime, follow the repo to hear when it ships.",
    repo: "View the code on GitHub",
    note: "Open source · your catalog never leaves your machine unless you decide otherwise.",
  },
  faq: {
    eyebrow: "FAQ",
    title: "Frequently asked questions",
    items: [
      {
        q: "Do I need the drives connected to search?",
        a: "No. That's the whole point: once a drive is indexed, you search and browse its contents even when it's powered off in a drawer. DiskDex tells you which drive each file is on and its full path.",
      },
      {
        q: "Can I bring my DiskCatalogMaker catalog?",
        a: "Yes. DiskDex imports the .dcmf format losslessly: names, hierarchy, dates and real sizes. We tested it against a catalog of 54 drives and 6.8 million entries.",
      },
      {
        q: "Where is my data stored?",
        a: "On your machine, in a single portable catalog file (SQLite). Nothing is uploaded to the cloud unless you explicitly ask for it.",
      },
      {
        q: "What happens if I reconnect a drive I already cataloged?",
        a: "DiskDex recognizes it by its volume fingerprint. Re-scanning updates it in place, without duplicating it in the catalog.",
      },
      {
        q: "Does it work for very large volumes?",
        a: "Yes. It's built for post-production scale: millions of files and dozens of TB, with sub-second search and an interface that never stalls.",
      },
    ],
  },
  footer: {
    tagline: "Offline disk catalog and search.",
    made: "Built with Tauri, Rust and React.",
    product: "Product",
    resources: "Resources",
    rights: "All rights reserved.",
    links: {
      features: "Features",
      download: "Download",
      roadmap: "Roadmap",
      github: "GitHub",
      changelog: "Changelog",
      privacy: "Privacy",
    },
  },
};

export default en;
