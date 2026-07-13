# DiskDex — sitio web

Landing bilingüe (ES/EN) de DiskDex: descarga, funciones, capturas y roadmap.
Vive en el mismo repo que la app, aislado en `site/`.

**Stack:** Next.js 15 (App Router) · TypeScript · Tailwind CSS 3 · i18n nativo
(middleware + diccionarios) · sin backend (100% SSG). Dirección visual: *dark
técnico* tipo Linear, con la marca teal + ámbar de la app.

## Desarrollo

```bash
cd site
npm install
npm run dev      # http://localhost:3000  (redirige a /es o /en según el navegador)
npm run build    # build de producción
npm run start    # sirve el build
```

## Estructura

```
site/
├─ app/
│  ├─ [locale]/            # layout + page por idioma, opengraph-image
│  ├─ globals.css          # tokens de color (HSL) + base
│  ├─ robots.ts · sitemap.ts · manifest.ts
│  ├─ icon.png · apple-icon.png   # favicon (marca real)
│  └─ not-found.tsx
├─ components/             # Header, Hero, AppWindow (product-shot), secciones…
├─ i18n/
│  ├─ config.ts            # locales: es (default), en
│  └─ dictionaries/        # es.ts · en.ts — TODO el copy vive acá
├─ lib/site.ts             # dominio, repo, releases, flag de descargas
├─ middleware.ts           # detección/redirect de idioma
└─ public/                 # logo.png · llms.txt
```

## Editar contenido

- **Textos:** `i18n/dictionaries/es.ts` y `en.ts` (misma forma; TS avisa si falta una clave).
- **Descargas:** `lib/site.ts` → `downloads`. Cuando publiques el primer binario en
  GitHub Releases, poné `available: true` y las URLs reales de los assets (`mac`, `win`);
  eso cambia el estado «Próximamente» por descargas directas automáticamente.
- **Capturas:** el "screenshot" del hero/sección es la UI de la app recreada en
  `components/AppWindow.tsx` (no es una imagen). Cuando haya capturas reales, se
  pueden reemplazar ahí.

## Deploy en Vercel

1. Importá el repo `polgold/diskdex` en Vercel.
2. **Root Directory: `site`** ← importante (el repo tiene dos proyectos).
3. Framework: Next.js (autodetectado). Build/Install por defecto.
4. Deploy. Vercel te da una URL `*.vercel.app`.
5. **Dominio:** en *Settings → Domains* agregá `diskdex.app` y `www.diskdex.app`,
   y apuntá el DNS del dominio (`.app` requiere HTTPS, Vercel lo gestiona solo).
6. Cuando cambie el dominio final, actualizá `site.url` en `lib/site.ts`
   (afecta canonical, sitemap, OG y JSON-LD).

## SEO incluido

Metadata por idioma (title/description), Open Graph + Twitter cards con `og:image`
propia generada al vuelo, `canonical` + `hreflang` (es/en/x-default), `robots.txt`,
`sitemap.xml`, `manifest.webmanifest`, `public/llms.txt` y JSON-LD `SoftwareApplication`.
