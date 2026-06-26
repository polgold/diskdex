# DiskDex — Handoff técnico

> Estado al cierre de esta sesión. Todo lo marcado ✅ está implementado **y verificado**
> (tests + corridas reales). Lo marcado ⏳ está pendiente.

---

## 1. Qué es

App de escritorio (macOS/Windows) para **catalogar discos** al estilo DiskCatalogMaker:
indexa el contenido de volúmenes y permite **buscar/navegar aunque el disco esté
desconectado**. Importa el catálogo histórico `.dcmf`, escanea discos nuevos al
conectarlos, y (a futuro) expone un conector remoto seguro para traer archivos.

**Caso de uso real:** productora audiovisual con ~54 discos de respaldo (~6,8 M archivos,
+116 TB). Cuando entra un trabajo nuevo, encontrar material viejo sin enchufar disco por disco.

---

## 2. Stack

| Capa | Elección |
|---|---|
| Shell de escritorio | **Tauri 2** |
| Motor nativo | **Rust** (importador, scan, DB, comandos IPC) |
| UI | **React 19 + TypeScript + Vite + Tailwind** |
| Estado UI | **Zustand** |
| Base de datos | **SQLite + FTS5** (un archivo `.dccat` por catálogo) |
| Iconos | **lucide-react** |

Toda la lógica de FS/parsing vive en Rust; el frontend solo consume datos ya indexados vía IPC.

---

## 3. Estado por hito

| Hito | Estado | Notas |
|---|---|---|
| **M0** Scaffold + dark mode + IPC | ✅ | `ping` ok, build limpio |
| **M1** Importador `.dcmf` → SQLite + FTS | ✅ | **validado contra el catálogo real** (ver §6) |
| **M2/M3 backend** navegación + búsqueda | ✅ | comandos + tests; falta la UI |
| **M5** Motor de escaneo + detección al conectar | ✅ | engine, fingerprint, re-escaneo, watcher, UI |
| **M2 UI** 3 paneles (árbol/tabla/inspector) | ✅ | tree lazy, tabla virtualizada, breadcrumb, inspector |
| **M3 UI** barra de búsqueda con resultados | ✅ | debounce + ⌘F, modos browse/search, disco+ruta |
| **M4** búsqueda por atributos/booleana | ✅ | parser de tokens (ext:/size>/after:/type:) + SQL estructurado; verificado en real |
| **M6** acciones (revelar/abrir/copiar) | ✅ | resuelve ruta real si el disco está montado; online/offline |
| **M7** export + comentarios/ubicación/categoría | ✅ | export CSV/TSV/JSON/HTML(→PDF); comentario por entrada; ubicación/categoría/comentario por disco |
| **M8** duplicados + estadísticas | ✅ | dups por nombre+tamaño (60,6 TB recuperables en real); stats por ext + más grandes |
| **P2** tabs multi-catálogo · thumbnails · NAS | ✅ | pestañas (switch=reopen); preview on-demand de imágenes (data URL PNG); "Escanear carpeta…" para shares de NAS montados |
| **M9** conector remoto seguro | ✅ | agente axum read-only: pairing→JWT, scopes (default deny), fingerprint+canonicalize gating, Range, BLAKE3, auditoría, revocación. UI "Compartir". **Transfers = links firmados temporales** (verificado). Push a buckets S3/B2/Drive: ⏳ capa de transporte pendiente |

---

## 4. Funcionalidades implementadas

### Importar catálogo `.dcmf` (M1) ✅
- Parser nativo del formato propietario de DiskCatalogMaker (ingeniería inversa, sección 6
  del spec). Lee por **escaneo de cabeceras zlib** (robusto ante regiones no-zlib).
- Tamaños **uint64 big-endian** (archivos >4 GB **sin truncar**), fechas HFS→Unix, árbol
  completo (nombres UTF-16 BE, registros de 24 B, atributos de archivo de 40 B).
- Inserta en SQLite por lotes y reconstruye el índice FTS al final.
- **Agregación recursiva de tamaños**: carpetas y volumen muestran la suma de su subárbol.

### Escanear un disco montado (M5) ✅
- Recorrido recursivo iterativo (sin recursión → sin stack overflow).
- Captura: nombre, es_carpeta, **tamaño lógico**, **tamaño físico** (asignado en disco:
  `st_blocks×512` en Unix / `GetCompressedFileSizeW` en Windows), fechas creación/modificación.
- **Fingerprint del volumen** (UUID por `diskutil` en macOS, serial en Windows) → se guarda
  en `disks.volume_uuid` para reconocer el disco al re-montarlo.
- **Re-escaneo**: si el fingerprint ya existe, reemplaza el disco anterior (sin duplicar),
  con mantenimiento **incremental** del FTS (no reconstruye todo el índice).
- Opciones: excluir nombres, saltear ocultos, saltear artefactos de Time Machine, no seguir symlinks.

### Detección de disco al conectar (M5) ✅
- Watcher en segundo plano que sondea los volúmenes montados cada 2,5 s y emite eventos
  `volume-added` / `volume-removed`.
- La UI muestra un **banner** al detectar un disco nuevo: *"Disco detectado: NAME (1.8 TB) —
  Escanear ahora"*. También hay un **selector de volúmenes** manual (botón "Escanear disco").

### Online / offline (M6, base) ✅
- `refresh_online_status` compara los volúmenes montados (por fingerprint o nombre) y marca
  cada disco como online/offline + guarda su `mount_path` actual.

### Búsqueda full-text (M3, backend) ✅
- FTS5 sobre el nombre, con tokenización segura y **búsqueda por prefijo** incremental.
- Devuelve total de coincidencias + items con **disco y ruta completa** reconstruida.

### Navegación (M2, backend) ✅
- `list_children` (hijos directos, carpetas primero), `entry_path` (ruta completa vía CTE
  recursiva), `get_entry` (detalle para inspector).

---

## 5. Arquitectura / mapa de archivos

```
diskdex/
├─ src/                         # React + TS (UI)
│  ├─ App.tsx                   # shell: importar/abrir/escanear, banner de detección, grilla de discos
│  ├─ components/ScanDialog.tsx # selector de volúmenes para escanear
│  ├─ lib/ipc.ts               # wrappers tipados de los comandos Rust + eventos
│  ├─ lib/format.ts            # tamaños/fechas legibles
│  └─ store/catalog.ts         # estado (zustand)
├─ src-tauri/src/
│  ├─ dcmf.rs                  # importador .dcmf (sección 6) + 7 tests
│  ├─ db.rs                    # SQLite + FTS5: ingesta, agregación, navegación, búsqueda + 13 tests
│  ├─ scan.rs                  # motor de escaneo, fingerprint, list_volumes + 3 tests
│  ├─ commands.rs             # comandos Tauri (IPC) + watcher de volúmenes
│  ├─ lib.rs                  # registro de comandos
│  └─ bin/
│     ├─ validate_dcmf.rs     # validador offline del importador (sin GUI)
│     └─ scan_probe.rs        # prueba del motor de escaneo sobre una carpeta real
└─ docs/                       # este handoff + presentación
```

### Comandos IPC expuestos
`ping`, `import_dcmf`, `open_catalog`, `list_disks`, `list_children`, `entry_path`,
`get_entry`, `search_entries`, `list_volumes`, `scan_disk`, `start_volume_watch`,
`refresh_online_status`.

### Esquema de datos (SQLite, `.dccat`)
Tablas: `disks`, `entries` (+ índices), `entries_fts` (FTS5 contentless externo),
`locations`, `categories`, `tags`, `entry_tags`, `access_log`, `devices` (estas últimas
para el conector M9, ya creadas). Detalle en [`db.rs`](../src-tauri/src/db.rs).

---

## 6. Validación contra el catálogo real

Corrido sobre `~/Dropbox/catalog 2023.dcmf/Catalog.dcmf` (164 MB):

| Criterio | Esperado | Obtenido |
|---|---|---|
| Discos | 54 | **54** ✅ |
| Entradas | 6.828.850 | **6.828.850** ✅ (exacto) |
| Archivos / carpetas | ~5,85 M / ~0,98 M | **5.847.763 / 981.087** ✅ |
| `C0001.MP4` ruta + tamaño | `/SF28/HUFNAGL PILAR/private/M4ROOT/CLIP/C0001.MP4` = 4.25 GB | **exacto** ✅ |
| Tamaños 64-bit | archivos >4 GB intactos | mayor = **174,94 GB** (`…C0002.mov`, MIRROR5) ✅ |
| Archivos `.mov` | ~256 k | **261.991** (FTS `mov*`) ✅ |
| Parseo | rápido | **11,1 s** (6,8 M entradas) |
| Ingesta a SQLite + FTS | — | **161 s**, `.dccat` de 884 MB |
| Búsqueda `.mov` | <1 s | **568 ms** ✅ |

**Tests:** 27 tests de Rust en verde (`cargo test --lib`).

---

## 7. Cómo correr / desarrollar

```bash
# UI
npm install
npm run tauri dev          # app de escritorio (compila el binario la 1ª vez, ~1–2 min)
npm run build              # build de la UI (tsc + vite)

# Backend Rust
cd src-tauri
cargo test --lib          # 27 tests (importador, DB, scan)
cargo build --release     # build optimizado

# Herramientas de validación (sin GUI)
cargo run --release --bin validate_dcmf -- "/ruta/Catalog.dcmf" [salida.dccat]
cargo run --release --bin scan_probe   -- "/ruta/a/escanear" [salida.dccat]
```

---

## 8. Decisiones y notas para quien siga

- **Por qué Rust para el motor:** parsear 164 MB → ~1 GB descomprimido e insertar 6,8 M filas
  es CPU/IO intensivo; en Rust no traba la UI y el import real corre en ~minutos.
- **Truco de ingesta:** se insertan las entradas en orden de índice local en una transacción
  fresca; como los rowids son contiguos, `parent_id = base + parent_local` evita un segundo
  pase de UPDATE sobre millones de filas.
- **FTS:** en import masivo se reconstruye el índice una vez al final; en escaneo se mantiene
  **incremental** (insert/delete por disco) para no penalizar catálogos grandes.
- **Tamaño de carpeta** = suma recursiva guardada en la propia fila de la carpeta/volumen
  (igual que DiskCatalogMaker), así la navegación es instantánea.
- **Dropbox online-only:** el `.dcmf` puede estar como placeholder de 0 bytes; hay que hacer
  "Make Available Offline" antes de importar. Los binarios detectan y avisan el caso.
- **Git:** el proyecto vive dentro del repo git de `$HOME`. Conviene `git init` propio en
  `~/Dev/diskdex` antes de versionar.

### Próximo paso recomendado
**M2 UI + M3 UI** (3 paneles + barra de búsqueda). El backend de ambos ya está hecho y
testeado; es "solo" frontend: árbol de discos/carpetas, tabla virtualizada (TanStack Virtual,
ya instalado) e inspector, más la barra de búsqueda que consume `search_entries`.

Después: M4 (filtros por atributos), M6 (revelar/abrir original), M7 (export), M8 (duplicados),
M9 (conector seguro — empezar por malla Tailscale + `GET /v1/file` read-only autenticado).
