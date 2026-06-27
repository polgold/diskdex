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

---

## 9. Roadmap de features nuevas (acordado con el usuario, ir por pasos)

> Diseño detallado en [`DISENO-cloud-y-backup.md`](./DISENO-cloud-y-backup.md). Decisiones
> tomadas: comparación de backup **por hash**; v1 con **reporte + copiar**; NL search con
> **CLIP** (visión) + motor **API Claude**; **mobile** queda como roadmap futuro.
>
> Idea central que une todo: **el escaneo lee cada archivo una sola vez** → de esa lectura
> salen hash + GPS/cámara + (opcional) embeddings. El "paso de extracción enriquecida" es la
> columna vertebral; casi todas las features cuelgan de ahí.

### Bloque A — Fundaciones
- [x] **A1. Migraciones de schema** (aditivas, no rompen catálogos) — ✅ HECHO (uncommitted).
      `db::apply_migrations()` corre en `open`/`open_in_memory` tras el SCHEMA: `ALTER TABLE
      ADD COLUMN` tolerante a "duplicate column name" (idempotente) para
      `entries.content_hash/hashed_at/cloud_state/gps_lat/gps_lon/gps_place/captured_at/
      camera_make/camera_model` + `disks.cloud_provider/cloud_root`, e índices
      `idx_entries_hash`/`idx_entries_place`. Columnas NULL hasta que las pueble el escaneo
      enriquecido (A2). Test `migrations_add_columns_and_are_idempotent` (54 cargo tests verdes).
      ⚠️ re-scan es full-replace → en A2/B1 hace falta snapshot por path para no perder lo derivado.
- [~] **A2. Paso de extracción enriquecida en el scan** (opt-in `ScanOptions.enrich`) —
      ✅ HASHING HECHO (uncommitted); GPS/cámara pendiente (sub-tarea A2-meta).
      `scan::enrich_entries(root, &disk, progress, cancel)` recorre el árbol ya escaneado y
      calcula **BLAKE3** (`scan::hash_file`, streaming 1 MiB → seguro para clips de decenas de
      GB) por archivo → `Vec<EntryEnrichment>` alineado por índice. `db::ingest_scanned` ahora
      toma `enrichment: Option<&[EntryEnrichment]>` y persiste `content_hash/hashed_at` (+ los
      campos GPS/cámara, hoy `None`). En `scan_disk_blocking`: si `opts.enrich`, fase de progreso
      propia `pct=-3` ("Calculando hashes…"), cancelable. UI: toggle **Fingerprint** en ScanDialog
      (`PostScanOptions.enrich`, default OFF) + strings ES/EN. Tests `enrich_computes_blake3_per_file`
      + `ingest_persists_enrichment_hashes` (56 cargo tests verdes; tsc limpio).
      ✅ **A2-meta (video) HECHO:** `video::probe_location` (ffprobe `-show_format/-show_streams`)
      extrae GPS (parse ISO 6709), cámara (make/model) y `captured_at` (parse ISO 8601 →
      `days_from_civil`); `video::is_location_video_ext` gatea qué archivos sondear. `enrich_entries`
      lo llama por archivo de video y puebla los campos GPS de `EntryEnrichment` (degrada si ffprobe
      falta). Visible en el Inspector (sección `MetaInfo` vía comando `get_entry_meta` + `db::EntryMeta`).
      Tests `parses_iso6709`/`parses_iso8601`/`location_video_ext_check` (63 cargo tests; tsc limpio).
      **Pendiente A2-meta-fotos:** exiftool (sidecar nuevo) para GPS de fotos/RAW.
      ✅ **C1 HECHO:** `geo::place_for(lat,lon)` (crate `reverse_geocoder`, dataset offline) llena
      `gps_place` en el enrich; búsqueda por token `place:Jujuy` (`gps_place LIKE`).
      ✅ **A2-preserve HECHO:** `ingest_scanned` ahora snapshotea el enriquecimiento (hash/GPS)
      de los discos viejos por rel_path ANTES de borrarlos (`snapshot_enrichment`, guard barato
      `EXISTS` para no recorrer el árbol si no hay nada enriquecido) y lo restaura en los archivos
      cuyo **tamaño+mtime no cambiaron** (`tree_rel_paths` para casar). Un re-escaneo SIN enrich ya
      NO pierde los hashes; un archivo editado descarta el hash viejo. Hash fresco siempre gana.
      Test `rescan_preserves_hash_when_unchanged` (59 cargo tests).
      (Nota: enrich lee TODOS los archivos aunque el árbol venga de un re-escaneo incremental —
      el hash requiere leer el contenido; es el costo esperado del opt-in.)

### Bloque B — Auditoría de backup (comparar / copiar)
- [x] **B1. `compare_subtrees(source, dest)`** + UI de reporte — ✅ HECHO (uncommitted).
      `db::compare_subtrees(conn, source_root, dest_root)` compara dos subárboles por **rel_path**
      y clasifica cada archivo del source: OK (hash idéntico) / MISSING / MISMATCH (hash difiere o
      tamaño difiere sin hash) / UNVERIFIED (presente, mismo tamaño, sin hash) + EXTRA (en dest, no
      en source) + `missing_bytes` + `fully_backed_up`. OFFLINE (catálogo-vs-catálogo). Helpers
      `db::disk_root_entry` + `collect_subtree_files` (CTE recursiva). Comando `compare_backup`
      (args en una struct `CompareArgs`: disk_id + entry_id opcional → disco entero o carpeta),
      registrado en lib.rs. UI: [`BackupAuditDialog.tsx`](../src/components/BackupAuditDialog.tsx)
      (botón **ShieldCheck "Backup"** en ContentToolbar): dropdowns source→dest, veredicto
      verde/ámbar, contadores y listas (mismatch/missing/unverified, cap 200). i18n ES/EN.
      Tests `compare_subtrees_classifies_files` (58 cargo tests; tsc limpio).
      **Follow-up B1-folder:** el backend/comando ya aceptan `*_entry_id` (comparar a nivel
      carpeta); la UI hoy solo ofrece disco-entero. Falta UI para elegir carpeta source/dest.
- [x] **B2. `copy_missing`** — ✅ HECHO (uncommitted). `scan::copy_file_verified(src, dst)`:
      copia atómica (temporal `.ddtmp` + `fsync` + `rename`) calculando BLAKE3 del origen en una
      lectura, y **re-hashea el destino** para verificar (si falla, borra el destino y error).
      Comando `copy_missing` (args `CopyMissingArgs`: disk_id + entry_id opcional + `dry_run`):
      resuelve rutas reales (requiere ambos discos MONTADOS), corre `compare_subtrees`, y copia los
      `missing` — **nunca sobreescribe** (saltea si el destino existe), emite `copy-progress`,
      cancelable (`COPY_CANCELS` + comando `cancel_copy(dest_disk_id)`). `dry_run` devuelve solo el
      plan. `CopyResult` { planned, copied, verified, skipped, cancelled, failed[], … }. UI: botón
      "Copiar lo que falta (N · bytes)" en BackupAuditDialog con confirm + progreso + cancel +
      resumen de resultado; refresca el reporte tras copiar. i18n ES/EN. Test
      `copy_file_verified_copies_and_verifies` (60 cargo tests; tsc limpio; tool bins OK).

### Bloque C — Ubicación y búsqueda
- [x] **C1. Reverse-geocode offline** — ✅ HECHO (uncommitted). Módulo `geo.rs` con crate
      `reverse_geocoder` (dataset GeoNames embebido, `OnceLock` cargado una vez): `geo::place_for`
      mapea lat/lon → "Ciudad, Provincia, CC". `enrich_entries` lo llama tras extraer GPS y puebla
      `entries.gps_place`. Búsqueda: token `place:`/`lugar:`/`ubicacion:` en el query-parser →
      `SearchFilters.place` → `gps_place LIKE` en search_advanced. Tests `geo::resolves_known_coordinates`
      (verifica Jujuy) + `search_by_place_filters_on_gps_place` (66 cargo tests; tsc limpio).
- [ ] **C2. Posición solar** (efemérides desde GPS + hora) → flag atardecer/amanecer **sin ML**.
- [ ] **C3. NL → query vía Claude API**: frase + esquema → tokens de búsqueda (existentes +
      `place:`/`camera:`/`light:sunset`), ejecución local. Solo viaja el texto de la consulta.

### Bloque D — Plan de copia multi-disco
- [ ] **D1. `GatherPlan`** + sesión guiada disco-por-disco **reanudable** ("conectá SF41 →
      copio 12 → conectá SF28 → …"), sobre el motor de copia de B2 + el watcher de volúmenes.

### Bloque E — Cloud storage
- [ ] **E1. Cloud Fase 1**: carpeta sincronizada (iCloud/Dropbox/Drive) como "disco cloud" +
      detección de placeholders (no forzar descarga en previews) + badges. Independiente.
- [ ] **E2.** *(Opcional)* Cloud Fase 2: conectores OAuth Dropbox/Drive (iCloud no tiene API).

### Bloque F — Pesado / futuro
- [ ] **F1. CLIP** (visión): embeddings por keyframe en el scan + índice vectorial
      (`sqlite-vec`) + semántica libre ("playa", "gente"). Opt-in, al final.
- [ ] **F2. Cliente mobile** con chat: cliente read-only sobre el `.dccat` portátil
      (sync vía Dropbox). Se diseña cuando A–F1 estén encaminados.

### Orden sugerido para empezar
**A1 → A2 → B1** desbloquea lo más pedido (auditar backup) con el menor riesgo. B2 agrega el
copiar. Recién después C (ubicación/NL) y D (gather), que se apoyan en la misma extracción.
E (cloud) es independiente y se puede intercalar cuando quieras. F es lo más pesado, al final.
