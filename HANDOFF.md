# HANDOFF — qué hay en este repo y dónde vive cada cosa

> Actualizado: 2026-07-20 (post-unificación de backup-compare). Para retomar el
> trabajo en cualquier computadora después de `git clone` / `git pull`.

## TL;DR

| Qué | Dónde en el repo | Rama de git | Deploy |
|---|---|---|---|
| **App de escritorio DiskDex** (Tauri 2 + Rust + React) | Raíz: `src/` (UI), `src-tauri/` (backend) | `main` | Release **v0.1.0** en GitHub (DMG macOS Intel) |
| **Sitio de marketing** (Next.js 15, ES/EN) | `site/` (aislado, package.json propio) | `feat/website` | Vercel → **https://diskdex.app** |

Un solo repo (**ahora PÚBLICO**): https://github.com/polgold/diskdex (remoto `origin`).

## Ramas

- **`main`** — la app de escritorio. Al 2026-07-20 tiene TODO mergeado: el roadmap A–D
  (hash BLAKE3 + auditoría de backup + copiar faltantes, GPS/cámara vía ffprobe,
  reverse-geocode offline, luz/atardecer por posición solar, búsqueda NL vía Claude API,
  gather multi-disco) **y** el M9 hecho en la otra máquina (comparación de discos +
  mirror). `site/` **no existe** en esta rama.
  - ✅ **Backup-compare unificado** (2026-07-20). Las dos máquinas habían implementado la
    feature en paralelo y el merge dejó ambas; ahora hay una sola: botón **"Comparar"**
    (`CompareDialog`) con selector de criterio **Rápido** (tamaño) / **Profundo** (hash
    BLAKE3). Comandos: `compare_disks` y `copy_missing`, ambos con flag `deep`; se cancela
    con `cancel_copy(dst_disk_id)` (el mismo mecanismo que gather).
    - La copia es **atómica y verificada por hash** en los dos modos (`scan::copy_file_verified`):
      antes el mirror hacía `fs::copy` directo y pisaba el destino sin verificar.
    - Un destino ocupado **solo se reemplaza si el plan lo marcó** (`CopyItem.overwrite`),
      para que un catálogo desactualizado no borre datos.
    - `db::classify` es la única fuente de verdad: la usan tanto lo que se muestra
      (`compare_disks`) como lo que se copia (`copy_plan`).
    - Verificado: 80 tests Rust + 22 vitest + `tsc` limpio.
- **`feat/website`** — nace de `690730e` y agrega `site/`. **No incluye** los commits
  nuevos de `main`; para tocar la app hacelo desde `main`. Cuando el sitio se dé por
  estable, mergear a `main` (o configurar Vercel para deployar desde `main`).
  - ⚠️ El merge `ac2d028` había colado en `main` 80 MB de artefactos (`site/.next/`,
    `site/.vercel/`) sin ningún archivo fuente del sitio. Se quitaron del índice en
    `cd51dd6` y el `.gitignore` de la raíz ahora los bloquea. **Siguen en el historial**
    (no se reescribió), así que un clone completo todavía los baja.
- **`fix/duplicados-firmlink-inode`** — rama vieja de una auditoría, **no mergeada**.
  Revisar vigencia antes de retomarla.
- Tag **`v0.1.0`** → release https://github.com/polgold/diskdex/releases/tag/v0.1.0
  (asset: `DiskDex_0.1.0_x64.dmg`, ~57 MB, sin firmar, solo Intel — corre en Apple
  Silicon vía Rosetta).

## Sitio web + Vercel

- **Proyecto Vercel:** `diskdex` · team `pablos-projects-b2fa3c06` (cuenta pablogoldberg).
  **Confirmado 2026-07-20: este proyecto ES el que sirve diskdex.app** (se verificó
  deployando y viendo el cambio en vivo; la API de Vercel no lista el dominio custom,
  no confundirse con eso). IDs para relinkear:
  `projectId: prj_AeJg2FPVczopotnZmCGN2ahu14tn` · `orgId: team_cf49XHleXvbKosmAOAaM02r3`.
- **NO hay git-integration**: pushear NO deploya (ya se comprobó: commits de trigger no
  hicieron nada). El deploy es **manual por CLI**:
  ```bash
  cd site
  npx vercel --prod
  ```
  Primera vez en una compu nueva: `npx vercel link` (team `pablos-projects-b2fa3c06`,
  proyecto `diskdex`) o crear `site/.vercel/project.json` con los IDs de arriba
  (`site/.vercel/` está gitignoreado, no viaja por git). Requiere `npx vercel login`.
- **Estado live (verificado 2026-07-20):** botón de descarga macOS activo → DMG v0.1.0;
  Windows en "próximamente"; texto Mac = "Intel 64-bit (Rosetta en Apple Silicon)".
  Disponibilidad por plataforma en `site/lib/site.ts` (`downloads.macos/windows`).
- **Dominios:** `diskdex.app` + `www` (redirige al apex) + `diskdex.vercel.app`.
- **DNS:** dominio en **Cloudflare** (NS mary/yahir.ns.cloudflare.com). Apex y `www` son
  CNAME → `b7064760f326e982.vercel-dns-016.com` en modo **DNS only (nube gris)**.
  ⚠️ No activar el proxy naranja: rompe la verificación/SSL de Vercel.
- **Capturas:** el "product shot" es la UI recreada en `site/components/AppWindow.tsx`;
  reemplazar por capturas reales cuando existan.

## Puesta en marcha en una compu nueva

```bash
git clone https://github.com/polgold/diskdex.git && cd diskdex

# App de escritorio (rama main)
npm install
./scripts/fetch-ffmpeg.sh        # ffmpeg/ffprobe NO están en git (gitignoreados)
npm run tauri dev                # desarrollo
CI=true npm run tauri build      # instalador (CI=true evita el paso AppleScript del DMG)
# Si el empaquetado DMG falla por permisos de Automatización, ver el workaround
# bundle_dmg.sh --sandbox-safe en docs/HANDOFF-2026-07-19-release-y-web.md §2.

# Sitio (rama feat/website)
git checkout feat/website
cd site && npm install
npm run dev                      # http://localhost:3000
```

Requisitos app: Rust (rustup), Node. Para build Apple Silicon: `rustup target add
aarch64-apple-darwin` y `./scripts/fetch-ffmpeg.sh all-macos`.

## Qué NO viaja por git

- `node_modules/` (raíz y `site/`) → `npm install` en cada uno.
- `src-tauri/binaries/` (ffmpeg/ffprobe sidecar) → `scripts/fetch-ffmpeg.sh`.
- `site/.vercel/` (link al proyecto Vercel) → `npx vercel link` o recrear el JSON (IDs arriba).
- El catálogo del usuario: `~/Dropbox/catalog.dccat` (datos, no código; sincroniza por Dropbox).
- Certificados Apple: firma/notarización **bloqueada** hasta enrolarse en el Apple
  Developer Program (ver `SIGNING.md`).

## Pendientes conocidos

- **Build Windows** (`.exe`/`.msi`): no existe; necesita runner Windows o GitHub Actions
  (recomendado: workflow con 3 runners — macOS Intel, macOS ARM, Windows — que suba
  instaladores al release en cada tag).
- **Build macOS ARM nativo** (hoy el DMG es Intel, corre por Rosetta).
- **Firmar/notarizar** (Gatekeeper bloquea el DMG; abrir con clic derecho → Abrir).

## Docs de referencia

- `docs/HANDOFF.md` — handoff técnico de la app (arquitectura, hitos, validación) + plan maestro §9.
- `docs/HANDOFF-2026-07-19-release-y-web.md` — sesión release v0.1.0 (workaround DMG, repo público).
- `docs/DISENO-cloud-y-backup.md` — diseño del roadmap de features (cloud, backup, GPS, NL, mobile).
- `site/README.md` — detalle del sitio (stack, i18n, deploy).
- `SIGNING.md` — firma/notarización de la app (pendiente).
