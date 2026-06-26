# DiskDex

App de escritorio (macOS/Windows) para catalogar discos al estilo DiskCatalogMaker:
indexa el contenido de volúmenes y permite buscar/navegar aunque el disco no esté
conectado. Importa catálogos `.dcmf` existentes y (a futuro) escanea discos nuevos y
expone un conector remoto seguro.

**Stack:** Tauri 2 + Rust (motor) · React 18/19 + TypeScript + Vite + Tailwind (UI) ·
SQLite + FTS5 (un archivo `.dccat` por catálogo).

## Estado actual

| Hito | Estado |
|---|---|
| M0 — Scaffold (Tauri + React + TS + SQLite, dark mode, IPC `ping`) | ✅ |
| M1 — Importador `.dcmf` (Rust, sección 6) → SQLite + FTS, con tests | ✅ |
| M1 — Validación contra el catálogo real (54 discos / 6,8 M) | ⛔ bloqueado: el `.dcmf` está como *placeholder online-only* de Dropbox (0 bytes locales) |
| M2 — Navegación 3 paneles | ⏳ |
| M3 — Búsqueda FTS multi-disco | ⏳ |
| M4–M9 | ⏳ |

### Desbloquear la validación M1
El archivo `~/Dropbox/catalog 2023.dcmf/Catalog.dcmf` no está descargado localmente
(extended attribute `com.dropbox.placeholder`, 0 bytes). En Finder: click derecho sobre
`catalog 2023.dcmf` → **"Make Available Offline"**, esperar el tilde verde (~164 MB), y
después correr el validador (ver abajo).

## Desarrollo

```bash
npm install                 # deps de la UI
npm run tauri dev           # app de escritorio en modo dev
npm run build               # build de la UI (tsc + vite)
npm test                    # tests de la UI (vitest)
```

### Backend Rust

```bash
cd src-tauri
cargo test --lib            # tests del importador y la DB (13 tests)
cargo build --release       # build optimizado
```

### Validar el importador contra un `.dcmf` real (sin GUI)

```bash
cd src-tauri
cargo run --release --bin validate_dcmf -- "/ruta/al/Catalog.dcmf"
```

Imprime: cantidad de discos, total de entradas (archivos/carpetas), tiempo de parseo,
la ruta reconstruida y el tamaño de `C0001.MP4` (verifica 64-bit / 4.25 GB), el conteo
de `.mov` y el archivo más grande. Criterios de aceptación de la sección 12.

## Arquitectura

```
src/                      React + TS (UI)
  lib/ipc.ts              wrappers tipados de los comandos Rust
  lib/format.ts          tamaños/fechas legibles
  store/catalog.ts       estado (zustand)
  App.tsx                shell: importar / abrir / grilla de discos
src-tauri/src/
  dcmf.rs                importador .dcmf (sección 6) + tests
  db.rs                  SQLite + FTS5, ingesta masiva + agregación recursiva + tests
  commands.rs            comandos Tauri (ping, import_dcmf, open_catalog, list_disks)
  bin/validate_dcmf.rs   validador CLI offline
```

### Formato `.dcmf` (resumen)
Contenedor: header de 10 B + bloques `[u32 BE ulen][stream zlib]`. Lectura robusta por
escaneo de cabeceras zlib. Por disco: tabla de nombres (UTF-16 BE) · registros de 24 B
(árbol) · atributos de archivo de 40 B (tamaños **uint64 BE** en offset 24/32, fechas HFS
en segundos desde 1904 → restar 2 082 844 800 para Unix). Detalle completo en `dcmf.rs`.
