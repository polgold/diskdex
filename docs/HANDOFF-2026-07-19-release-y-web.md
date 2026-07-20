# Handoff — sesión 2026-07-19 · M9, release v0.1.0 y web

Resumen de todo lo que se hizo (y lo que quedó pendiente/bloqueado) en esta sesión.
Tres frentes: la feature M9 (app), el primer release descargable, y los botones de
descarga del sitio. **Lo único sin terminar es el deploy del sitio** — detalle abajo.

---

## 1. M9 — comparación de discos y mirror de backup (APP) · ✅ HECHO, en `main`, pusheado

Estaba escrito pero sin commitear. Se verificó, auditó, arregló, commiteó y pusheó.

- **Commits en `main`:**
  - `4d48f28` — M9: comparación de discos y mirror de backup
  - `08d106c` — M9: replicar carpetas vacías y reportar colisiones de tipo
- **Tests:** 61 Rust + 12 vitest, todos en verde. `tsc` y `clippy` limpios.
- **Bugs encontrados y arreglados en la auditoría:**
  1. `missing_count` contaba carpetas pero el mirror solo copiaba archivos → el botón
     ofrecía copiar más ítems de los que después informaba. Se agregó `missing_file_count`
     y luego se rediseñó para que el mirror **sí** cree las carpetas.
  2. **Carpetas vacías** no se replicaban (nacían de rebote del `create_dir_all` de sus
     archivos, pero una carpeta vacía nunca llegaba al destino). Ahora `copy_plan` incluye
     toda entrada faltante y `run_copy` crea las carpetas explícitamente; el plan va
     ordenado por ruta para que la carpeta se cree antes que su contenido.
  3. **Colisiones de tipo** (misma ruta = archivo de un lado, carpeta del otro) se
     trataban como coincidencia silenciosa. Ahora salen como "conflicto" en su propia
     sección y quedan fuera del plan de copia (pisar destruiría datos del destino).
- Se agregaron 3 tests nuevos que cubren esos casos.

---

## 2. Release v0.1.0 — DMG descargable (macOS) · ✅ HECHO

- **Repo pasado a PÚBLICO** (era privado). Esto era necesario para que las descargas
  del release funcionen para cualquiera: GitHub no permite releases públicos en repos
  privados. Antes de hacerlo se escaneó árbol + historial buscando secretos/.env/claves:
  **limpio** (el JWT secret de `agent.rs` se genera random en runtime con `getrandom`).
- **Build:** `DiskDex.app` compila. El `.app` se genera bien.
- **DMG:** el `tauri build` normal FALLA al empaquetar el DMG por un tema de permisos de
  macOS (el paso de AppleScript que acomoda la ventana del Finder tira
  `AppleEvent timed out -1712` → falta permiso de Automatización para controlar el Finder).
  Se sorteó armando el DMG con `bundle_dmg.sh --sandbox-safe`, que saltea esa decoración.
  - Comando usado (desde `src-tauri/target/release/bundle/dmg/`):
    ```
    bash ./bundle_dmg.sh --volname "DiskDex" --sandbox-safe \
      --icon "DiskDex.app" 180 170 --app-drop-link 480 170 \
      --window-size 660 400 --hide-extension "DiskDex.app" \
      "DiskDex_0.1.0_x64.dmg" "../macos"
    ```
  - OJO: la carpeta origen es `../macos` (la que CONTIENE el .app), NO el .app directo
    (eso arma un DMG mal, con `Contents` suelto en la raíz).
- **Release creado:** https://github.com/polgold/diskdex/releases/tag/v0.1.0
  - Asset: `DiskDex_0.1.0_x64.dmg` (~57 MB). Descarga anónima verificada (HTTP 200).
  - URL directa del DMG:
    https://github.com/polgold/diskdex/releases/download/v0.1.0/DiskDex_0.1.0_x64.dmg
- **Caveats del binario (importantes para quien lo baje):**
  - **Sin firmar** → Gatekeeper lo bloquea; hay que abrirlo con clic derecho → Abrir la
    primera vez. Firmarlo requiere cuenta Apple Developer (US$99/año).
  - **Solo Intel (x86_64)** → en Apple Silicon corre por Rosetta. No hay build ARM nativo.

---

## 3. Sitio diskdex.app — botones de descarga · ⚠️ CÓDIGO HECHO, DEPLOY PENDIENTE/BLOQUEADO

### Qué se encontró
- El fuente del sitio **está en este mismo repo**, en la branch **`feat/website`**, carpeta
  **`site/`** (Next.js App Router, bilingüe ES/EN). No estaba local porque solo se tenía
  `main` bajada.

### Qué se editó (commit `663b68d` en `feat/website`)
Los botones de descarga tenían un flag `available` **global** (prendía Mac y Windows
juntos). Se pasó a disponibilidad **por plataforma**:
- `site/lib/site.ts` — Mac ahora `available: true` y apunta al DMG real de v0.1.0.
  Windows queda `available: false`. `version` a "0.1.0".
- `site/components/sections/Download.tsx` — usa `available` por plataforma; nota de
  "próximamente" se muestra si alguna plataforma falta (`someSoon`).
- `site/i18n/dictionaries/{es,en}.ts` — se corrigió `macMeta` (decía "Universal · Apple
  Silicon + Intel", que es FALSO; el binario es Intel x86_64, corre en Apple Silicon vía
  Rosetta). El `soonNote` ahora habla solo de Windows.
- `npm run build` del sitio pasa limpio localmente.
- Además hay un commit vacío `4421692` que se hizo para intentar disparar un redeploy
  (no funcionó — ver abajo). Se puede borrar si molesta.

### Por qué el deploy NO está hecho (lo que hay que resolver)
diskdex.app **sigue sirviendo el código viejo** (verificado por curl: muestra
"Próximamente" y "Universal · Apple Silicon", sin link al DMG). Los cambios NO están en
vivo. Motivos:

1. **El proyecto de Vercel `diskdex` que se ve por API (`prj_AeJg2FPVczopotnZmCGN2ahu14tn`,
   team `team_cf49XHleXvbKosmAOAaM02r3`) NO parece ser el que sirve diskdex.app**:
   ese dominio no figura entre sus aliases (solo `diskdex.vercel.app`), y NO reacciona a
   los pushes a `feat/website` (se probó con 2 commits, cero deploys nuevos). Su producción
   sigue clavada en el deploy original por CLI del commit `4112234`.
2. **Mismatch de branch probable:** el sitio vive solo en `feat/website`, pero la branch
   por defecto del repo es `main` (que NO tiene la carpeta `site/`). Si el proyecto de
   Vercel despliega producción desde `main`, un push a `feat/website` nunca llega a
   producción.
3. El deploy a producción vía la herramienta MCP `deploy_to_vercel` quedó **bloqueado por
   el sistema de permisos** (acción de producción, sesión no interactiva). Un preview por
   MCP sí funcionó, pero fue a un proyecto que no es diskdex.app, y encima tuvo que dejar
   afuera `package-lock.json` y re-encodear el logo por límite de tokens del tool. Ese
   camino NO es el bueno.

### Cómo terminarlo (pendiente para vos, desde tu máquina con acceso a Vercel)
El código ya está pusheado en `feat/website`. Falta solo desplegarlo. Opciones:
- **Mergear `feat/website` → `main`** si tu proyecto de Vercel despliega producción desde
  `main` (así el sitio y los futuros pushes se despliegan solos). El sitio y la app Tauri
  conviven en `main` (`site/` aparte).
- **O** dejar la web en `feat/website` y en Vercel (Settings → Git → Production Branch)
  poner `feat/website` como branch de producción; después un push redespliega.
- **O** si conectaste el repo a OTRO proyecto de Vercel (no el que se veía por API),
  disparar el redeploy desde ese dashboard / confirmar que su Production Branch y Root
  Directory (`site/`) apunten bien.
- Verificar siempre que el **Root Directory** del proyecto sea `site/` (el Next.js no está
  en la raíz del repo, está en `site/`).

### Después del deploy, chequear en diskdex.app
- El botón de macOS debe estar activo y linkear a
  `.../releases/download/v0.1.0/DiskDex_0.1.0_x64.dmg`.
- El de Windows debe seguir en "próximamente".
- El texto de Mac debe decir "Intel 64-bit (corre en Apple Silicon vía Rosetta)".

---

## Estado de git al cerrar la sesión
- `main` → `08d106c` (M9 completo, pusheado).
- `feat/website` → `4421692` (edits del sitio `663b68d` + commit vacío de trigger).
- `fix/duplicados-firmlink-inode` → `cee0069` (rama previa, sin tocar).
- `package-lock.json`: cambios de metadata de npm (flags `peer`/`libc`, sin deps nuevas)
  se dejaron SIN commitear durante toda la sesión, en `main` y en `site/`.

## IDs y links útiles
- Repo (ahora PÚBLICO): https://github.com/polgold/diskdex
- Release: https://github.com/polgold/diskdex/releases/tag/v0.1.0
- DMG directo: https://github.com/polgold/diskdex/releases/download/v0.1.0/DiskDex_0.1.0_x64.dmg
- Vercel team: `team_cf49XHleXvbKosmAOAaM02r3` (Pablo's projects)
- Vercel project visto (NO confirmado como el de diskdex.app): `prj_AeJg2FPVczopotnZmCGN2ahu14tn`

## Artefactos locales (esta máquina)
- DMG: `src-tauri/target/release/bundle/dmg/DiskDex_0.1.0_x64.dmg`
- App: `src-tauri/target/release/bundle/macos/DiskDex.app`

## Pendientes más allá del deploy
- Build de **Windows** (`.exe`/`.msi`): no existe. No se puede cross-compilar desde macOS;
  necesita runner Windows o GitHub Actions. Recomendado: workflow que compile en 3 runners
  (macOS Intel, macOS ARM nativo, Windows) y suba los instaladores al release en cada tag.
- Build **ARM nativo** de macOS (target `aarch64-apple-darwin` o universal).
- **Firmar/notarizar** el DMG para evitar el bloqueo de Gatekeeper.
