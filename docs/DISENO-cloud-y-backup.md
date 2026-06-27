# Diseno -- Cloud storage + Auditoria de backup

> Documento de diseno para dos features nuevas de DiskDex. Decisiones tomadas con el
> usuario: (1) disenar las dos antes de construir; (2) la comparacion de backup se basa en
> **hash de contenido**; (3) la primera version incluye **reporte + copiar lo faltante**.
> Estado: DISENO (nada implementado todavia).

Contexto del stack: Tauri (Rust) + React/TS, catalogos SQLite `.dccat`. El catalogo es
**offline-first**: guarda la ubicacion de archivos aunque el disco este desconectado.
Schema relevante en [db.rs](../src-tauri/src/db.rs): tabla `entries` con
`name, is_folder, size_logical, size_physical, created_at, modified_at, ext` -- sin hash hoy.

---

## Feature 1 -- Cloud storage (iCloud / Dropbox / Google Drive)

### Realidad de cada proveedor (define el esfuerzo)

| Proveedor | API publica de archivos | Via carpeta sincronizada local |
|-----------|-------------------------|--------------------------------|
| **iCloud Drive** | NO existe (CloudKit es solo para el contenedor de la propia app) | SI: `~/Library/Mobile Documents/com~apple~CloudDocs` |
| **Dropbox** | SI: API REST + OAuth2 | SI: `~/Dropbox` |
| **Google Drive** | SI: Drive API v3 + OAuth2 | SI: `~/Google Drive` (File Stream) |

Conclusion estructural: **iCloud solo se puede via filesystem**. Por eso la Fase 1 pasa por
el filesystem (cubre los 3 de una) y la API queda como Fase 2 opcional (solo Dropbox/Drive).

### Fase 1 -- Carpeta sincronizada como "disco cloud" (barato, alto valor)

DiskDex **ya** puede escanear `~/Dropbox` como carpeta. Lo que falta es tratarla como
ciudadano de primera y manejar el detalle de los **placeholders** (archivos "online-only" /
"Optimizar almacenamiento del Mac"): existen en el filesystem con nombre/tamano/fecha
correctos, pero el contenido **no esta bajado**. Listarlos esta perfecto; pedirles
thumbnail/frame forzaria una descarga o fallaria.

**Cambios de schema** ([db.rs](../src-tauri/src/db.rs)):
- `disks`: agregar `cloud_provider TEXT` (null | icloud | dropbox | gdrive) y
  `cloud_root TEXT` (ruta de la carpeta sync). Complementa al `kind` actual.
- `entries`: agregar `cloud_state INTEGER DEFAULT 0` -- 0=local/materializado,
  1=placeholder (solo nube). Barato (1 byte por fila), permite filtrar/avisar.

**Backend** ([scan.rs](../src-tauri/src/scan.rs)):
- `detect_cloud_root(path) -> Option<CloudProvider>`: matchea prefijos conocidos
  (`com~apple~CloudDocs`, `~/Dropbox`, `~/Google Drive` / `CloudStorage/GoogleDrive-*`).
- En el walk, por cada archivo detectar si es **dataless/placeholder**:
  - macOS: flag `SF_DATALESS` via `stat`/`getattrlist` (`st_flags & SF_DATALESS` con `libc`,
    que ya es dependencia del proyecto). Para iCloud tambien sirve el ubiquitous item flag.
  - Si es placeholder -> `cloud_state=1` y **saltear extraccion de preview** (no forzar descarga).
- En post_scan/preview: si `cloud_state==1`, no intentar thumbnail; la UI muestra badge "nube".

**UI**:
- ScanDialog: cuando el path elegido cae bajo una carpeta cloud conocida, mostrar chip
  "Detecte Dropbox/iCloud/Drive" y proponer escanearla como disco cloud.
- Sidebar/tabla: icono de nube en discos cloud; badge "solo en la nube" en entries placeholder.
- Filtro: "ocultar/mostrar solo-nube" (reutiliza el filtro por tipo que ya existe).

**Riesgo:** bajo. Es casi todo lectura de metadata; el unico cuidado real es **no disparar
descargas** al generar previews. Default: nunca materializar un placeholder.

### Fase 2 -- Conectores OAuth (Dropbox + Google Drive) -- opcional, mas adelante

Para catalogar cuentas **sin** la app de escritorio (o cuentas que no sincronizas localmente).

- OAuth2 con PKCE; el token va al **keychain del SO** (no a localStorage ni al `.dccat`).
  En Tauri: `tauri-plugin-oauth` (loopback) + crate `keyring` para storage seguro.
- Connector trait: `list_children(folderId) -> Vec<RemoteEntry>` paginado; mapear a `entries`
  con `disk.kind="cloud-api"`, `cloud_provider`, y un `remote_id` por entry (nueva columna
  `entries.remote_id TEXT` para refrescar/abrir despues).
- Refresh incremental: Dropbox usa `cursor` (delta); Drive usa `changes.list(pageToken)`.
- Rate limits: backoff exponencial; el escaneo cloud es I/O remoto, mucho mas lento que local.
- iCloud: **no aplica** (sin API). Queda solo en Fase 1.

Es un connector por proveedor + flujo OAuth + manejo de tokens -- laburo grande. Solo vale si
el usuario quiere cuentas no-sincronizadas. Por defecto **no** se construye ahora.

---

## Feature 2 -- Auditoria de backup (comparar source vs destination)

Caso real: tarjeta de camara -> disco de proyecto -> backup al disco del proyecto entero.
Pregunta a responder: **"esta TODO en el destino? si no, copiame lo que falta."**

### Decision: hash de contenido, calculado en el escaneo

El usuario quiere comparacion por hash. Para no romper el "offline-first", el hash se
**calcula durante el escaneo y se guarda en el catalogo**. Asi la auditoria es
**catalogo-vs-catalogo** (no necesita discos montados) pero basada en hash real. Solo el
**copiar** necesita el source montado.

**Cambios de schema** ([db.rs](../src-tauri/src/db.rs)):
- `entries`: agregar `content_hash TEXT` (hex) y `hashed_at INTEGER`. Nullable: un entry
  puede estar catalogado sin hash (catalogos viejos, o escaneo sin hashing).
- Indice `idx_entries_hash ON entries(content_hash)` -- sirve para la auditoria **y** para
  deteccion de duplicados entre discos a futuro (mismo hash = mismo contenido).

**Algoritmo de hash:**
- **BLAKE3** (crate `blake3`): mucho mas rapido que SHA-256, paraleliza, ideal para discos grandes.
- Para archivos enormes (clips de decenas de GB) el hash completo es caro. Opcion configurable:
  hash **completo** (gold standard) vs hash **rapido** (primeros+ultimos N MB + tamano --
  detecta copias truncadas/parciales sin leer 40 GB). Default sugerido: completo para < 2 GB,
  rapido para mayores, togglable.
- El hashing es **opt-in por escaneo** (`ScanOptions.compute_hashes: bool`) porque lee todos
  los bytes -- lento. Sin el toggle, el escaneo sigue como hoy (rapido, sin hash) y la auditoria
  cae a comparacion por nombre+tamano+fecha (degradado, avisado en UI).

### Comparacion (catalogo-vs-catalogo, offline)

Backend nuevo en [db.rs](../src-tauri/src/db.rs) + [commands.rs](../src-tauri/src/commands.rs):

```
compare_subtrees(source: {disk_id, root_entry_id},
                 dest:   {disk_id, root_entry_id}) -> BackupReport
```

- Construye el set de archivos (no carpetas) de cada subarbol, **keyed por path relativo** al
  root elegido (asi `Tarjeta/DCIM/A001.mov` matchea `Backup/Proyecto/DCIM/A001.mov` si elegis
  los roots correctos).
- Por cada archivo del source clasifica:
  - **OK** -- existe en dest con **mismo hash** (respaldado y verificado).
  - **FALTA** -- no existe en dest.
  - **MISMATCH** -- existe en dest mismo path pero **hash distinto** (copia corrupta/parcial/
    version vieja). Es el caso peligroso que el ojo no ve.
  - **SIN_HASH** -- falta hash de un lado -> cae a comparacion por tamano+fecha, marcado "no verificado".
- Tambien reporta **EXTRA** (en dest, no en source) -- informativo, normalmente OK.

`BackupReport`:
```
{ ok: u64, missing: Vec<FileRef>, mismatch: Vec<FileRef>, unverified: Vec<FileRef>,
  extra: u64, missing_bytes: u64, source_total: u64, fully_backed_up: bool }
```

**UI** (modal/panel "Auditar backup"):
- Elegir Source (disco o carpeta del catalogo) y Destination (idem). Ambos del catalogo -> no
  hace falta montar nada para el **reporte**.
- Resultado: "Todo respaldado (N archivos verificados por hash)" o
  "Faltan N archivos (X GB)" + lista; seccion aparte para MISMATCH (rojo) y SIN_HASH (amarillo).
- Boton "Copiar lo que falta" -- habilitado solo si el source esta montado.

### Copiar lo faltante (la unica operacion que escribe a disco externo)

Primera vez que DiskDex **escribe** en un disco que no es la Papelera. Maximo cuidado.

- **Requiere ambos discos montados** (resolver `mount_path` actual; reusar `resolve_real_path`
  que ya existe).
- Flujo:
  1. **Dry-run / preview**: lista exacta de archivos a copiar + destino calculado + bytes totales.
     Nada se escribe sin confirmacion explicita.
  2. **Copia** con progreso (evento Tauri, igual que el scan progress). Crea carpetas intermedias
     faltantes. Copia a `archivo.tmp` y `rename` al final (atomico, no deja archivos a medias).
  3. **Verificacion post-copia**: re-hashea lo copiado y confirma == hash del source. Si no
     coincide -> marca error, no borra nada.
  4. Nunca **sobrescribe** un archivo existente por default (solo copia faltantes). Los MISMATCH
     se muestran pero requieren opt-in explicito "reemplazar" aparte.
- Cancelable (mismo patron que `SCAN_CANCELS` / `cancel_scan` en commands.rs).
- Tras copiar, ofrecer "re-escanear destino" para que el catalogo quede al dia.

**Comando:**
```
copy_missing(plan derivado del BackupReport, dry_run: bool) -> CopyResult
```

### Casos borde a manejar
- Source root y dest root no alineados (carpeta equivocada) -> el preview lo hace evidente
  (todo aparece como FALTA). Mostrar % de match para detectarlo.
- Archivos solo-en-la-nube (Feature 1, `cloud_state=1`): no se pueden hashear/copiar sin
  bajarlos -> marcarlos "no verificable (en la nube)".
- Nombres iguales con distinto case (APFS vs exFAT) -> normalizar comparacion segun fstype.
- Symlinks / hardlinks -> seguir el comportamiento del scan actual (documentar).
- Destino sin espacio -> chequear free space (ya hay `disk_detail` con espacio libre) antes de copiar.

---

---

## Feature 3 -- Metadata de camara (GPS / ubicacion) + busqueda por lugar

Objetivo: "buscame los clips que grabe en Jujuy". La metadata se extrae **en el mismo paso
de escaneo que el hash** (se lee el archivo una vez, se saca hash + GPS + lo que haga falta).

**Extraccion** ([scan.rs](../src-tauri/src/scan.rs) / [video.rs](../src-tauri/src/video.rs)):
- Herramienta: **exiftool** como sidecar (igual que ffmpeg/ffprobe ya bundleados) -- es el
  estandar para GPS de fotos y video de miles de formatos. Parcialmente tambien `ffprobe`
  (ya presente) lee el atom de ubicacion ISO-6709 (`com.apple.quicktime.location.ISO6709`)
  de muchos MP4/XAVC. Sony a veces deja GPS en sidecars `M01.XML` de la tarjeta.
- **Caveat honesto:** el GPS solo existe si la camara lo grabo. Mucho equipo profesional NO
  geotagea. Donde esta, se extrae; donde no, queda sin ubicacion. No hay forma de inventarlo.

**Reverse-geocoding OFFLINE** (coordenadas -> nombre de lugar):
- Dataset de limites administrativos embebido (pais/provincia/ciudad). Offline por privacidad
  (no mandar todas tus ubicaciones a un servicio) y para funcionar sin internet.
- Para "Jujuy" alcanza resolucion provincial -- dataset chico. Guardar la jerarquia resuelta.

**Schema** ([db.rs](../src-tauri/src/db.rs)):
- `entries`: `gps_lat REAL, gps_lon REAL, gps_place TEXT` (texto resuelto pais/provincia/ciudad),
  `captured_at INTEGER` (timestamp real de captura de la metadata, no el mtime del archivo),
  `camera_make TEXT, camera_model TEXT`.
- `gps_place` entra al **FTS5 que ya existe** -> "Jujuy" es una busqueda de texto normal.

**Atardeceres SIN ML (atajo):** con `gps_lat/lon` + `captured_at` se calcula la **posicion del
sol** (algoritmo de efemerides, sin red ni modelo) -> flag derivado "golden hour / atardecer /
amanecer / noche". Cubre el ejemplo de "atardeceres" gratis, con datos que ya juntamos. Para
semantica visual arbitraria ("playa", "gente") hace falta CLIP (Feature 4).

---

## Feature 4 -- Busqueda en lenguaje natural (NL -> query) + vision (CLIP)

Dos motores distintos, se combinan.

### 4a. NL sobre metadata -- via API Claude
- El usuario eligio **API (Claude)** como motor NL. La frase ("clips de Jujuy de 2023 de la
  A7S") + el **esquema de campos** del catalogo van a la API; vuelve una **query estructurada**
  (place / rango de fechas / camara / tipo / atardecer-flag) que se ejecuta **local** sobre
  SQLite/FTS. NO se sube data de archivos, solo la frase + el esquema.
- Reutiliza el parser de tokens que ya existe (`ext:` `size>` `after:` `type:`): el LLM emite
  esos mismos tokens + los nuevos (`place:` `camera:` `light:sunset`), asi la ejecucion es la
  via probada. Cae a busqueda normal si no hay internet.
- Privacidad: documentar claramente que solo viaja el texto de la consulta.

### 4b. Vision semantica -- embeddings CLIP (la pieza pesada)
El usuario pidio sumar vision para semantica libre ("playa", "gente", "autos").
- **Embeddings CLIP** por keyframe (reutiliza los frames que ya extrae el pipeline de video /
  los thumbnails). Modelo via ONNX Runtime o `candle` (Rust), bundleado. Se computa **en el
  escaneo** (opt-in, caro como el hash) y se guarda el vector en el catalogo.
- **Schema:** tabla `entry_embeddings(entry_id, model TEXT, vec BLOB)`. Busqueda por
  similitud: embeber el texto de la consulta -> nearest-neighbor sobre los vectores. Para el
  indice ANN: `sqlite-vec` (extension) o un flat index en memoria para catalogos chicos.
- **Costo:** modelo bundleado (cientos de MB), computo por frame en cada escaneo, storage de
  vectores. Es la feature mas pesada del roadmap -> va al final, opt-in, despues de que GPS+NL
  por metadata ya den valor.
- Flujo combinado: el motor NL (4a) decide si la consulta es estructurada (metadata) o
  semantica (CLIP) o ambas, y mezcla resultados.

---

## Feature 5 -- Plan de copia multi-disco (gather)

Extiende `copy_missing` (Feature 2) a "junta estos archivos repartidos en N discos".
Caso: "reuni todos los atardeceres y arma un plan de copia".

- Input: un **resultado de busqueda** (de Feature 3/4) cuyos archivos viven en varios discos,
  varios desconectados. El catalogo sabe en que disco esta cada uno aunque este desenchufado.
- **GatherPlan**: agrupa los matches por disco -> sesion guiada disco por disco:
  "Conecta SF41 -> copio estos 12 -> desconecta -> conecta SF28 -> ...". Estado de sesion
  persistido (que discos ya, cuales faltan, **reanudable** si cerras la app).
- Detecta el disco al montarse (ya existe el watcher `start_volume_watch`) y avanza el paso
  automaticamente cuando aparece el disco esperado.
- Reusa el motor de copia de Feature 2 (dry-run + progreso + verificacion por hash +
  cancelacion). Lo nuevo es solo la **orquestacion/estado multi-disco**.
- Destino unico (carpeta donde se juntan todos los matches), preservando o aplanando estructura
  segun preferencia.

---

## Feature 6 (ROADMAP, no se disena en detalle ahora) -- Cliente mobile con chat

El usuario lo dejo como **direccion futura**, no para disenar/construir ahora. Notas para que
no se pierda la idea:
- El `.dccat` (SQLite) es el **artefacto portatil** y ya es offline-first. El mobile **no
  escanea discos**: lee una copia del catalogo (sync natural via el Dropbox donde ya vive) y
  ofrece busqueda + chat de solo-lectura.
- "En que disco tengo los clips de Jujuy?" = query sobre el catalogo que devuelve el nombre del
  disco, sin montar nada. La arquitectura ya lo soporta.
- Tauri 2 hace iOS/Android; el motor de scan/ffmpeg NO aplica en mobile -> el cliente mobile es
  basicamente la capa de busqueda + el NL (Feature 4a via API) sobre el catalogo sincronizado.
- Es un **segundo cliente** (esfuerzo aparte). Se disena cuando el desktop tenga las features 1-5.

---

## ROADMAP maestro -- orden de construccion (ir por pasos)

Todo se apoya en una idea: **el escaneo lee cada archivo una vez** y de ahi salen hash + GPS +
(opcional) embeddings. Por eso el "paso de extraccion enriquecida" es la columna vertebral.

**Bloque A -- Fundaciones (habilita casi todo lo demas)**
1. **Migraciones de schema** (aditivas, ALTER TABLE ADD COLUMN, no rompen catalogos):
   `entries.content_hash/hashed_at/cloud_state/gps_lat/gps_lon/gps_place/captured_at/
   camera_make/camera_model`; `disks.cloud_provider/cloud_root`; indices `idx_entries_hash`,
   `idx_entries_place`. OJO re-scan full-replace -> snapshot por path para no perder lo derivado
   (mismo TODO que tags/thumbnails en el backlog).
2. **Paso de extraccion enriquecida en el scan** (opt-in `ScanOptions.enrich`): por archivo,
   en una sola lectura -> BLAKE3 + (exiftool/ffprobe) GPS/camara/captura. UI toggle en ScanDialog.

**Bloque B -- Backup audit (Feature 2)**
3. **compare_subtrees** + UI de reporte (offline, catalogo-vs-catalogo por hash). <- valor inmediato.
4. **copy_missing** con dry-run + progreso + verificacion + cancelacion. <- primera escritura a disco.

**Bloque C -- Ubicacion y busqueda (Feature 3 + 4a)**
5. **Reverse-geocode offline** + poblar `gps_place` -> "clips de Jujuy" via FTS.
6. **Posicion solar** -> flag atardecer/amanecer (sin ML), datos ya juntados.
7. **NL -> query via Claude** (4a): frase + esquema -> tokens de busqueda existentes + nuevos.

**Bloque D -- Gather multi-disco (Feature 5)**
8. **GatherPlan** + sesion guiada disco-por-disco reanudable, sobre el motor de copia del paso 4.

**Bloque E -- Cloud (Feature 1)**
9. **Cloud Fase 1**: carpeta sync como disco cloud + deteccion de placeholders + badges. Independiente.
10. *(Opcional)* **Cloud Fase 2**: connectores OAuth Dropbox/Drive.

**Bloque F -- Pesado / futuro**
11. **CLIP** (4b): embeddings visuales en el scan + indice vectorial + semantica libre. Opt-in, al final.
12. **Cliente mobile** (Feature 6): se disena cuando 1-11 esten encaminados.

## Inversiones que sirven a futuro
- `content_hash` + indice habilita **deteccion de duplicados entre discos** casi gratis.
- El patron `copy_missing` (dry-run + progreso + verificacion + cancelacion) es la base de
  cualquier operacion de mover/consolidar/gather.
- El paso de extraccion enriquecida (una lectura por archivo) amortiza hash + GPS + embeddings
  juntos -> no se re-lee el disco por cada feature.
