# TODO — Presentación comercial

> **ESTADO: borrador comercial hecho** → [`slides-comercial.html`](./slides-comercial.html)
> (11 slides, framing de producto genérico: hook → producto → 3 pasos → features por
> beneficio → vs competencia → verticales → seguridad → pricing → prueba social → roadmap → CTA).
> Quedan **decisiones del usuario** antes de presentarlo en serio:
> - **Pricing**: números reales y empaquetado (Free / Pro / Equipo). El conector seguro es el candidato a premium.
> - **Branding**: nombre definitivo ("DiskDex"?), logo, paleta, tagline.
> - **Contacto/CTA**: email/web reales (placeholder `hola@diskdex.app`).
> - Opcional: rehacer en un generador de slides (Gamma) cuando haya créditos, o sumar capturas reales de la app.
> El deck "interno" original sigue en `slides.html` / `PRESENTACION.md`.

---

## Guía original (referencia)

> La presentación actual ([slides.html](./slides.html) y [PRESENTACION.md](./PRESENTACION.md))
> está escrita "para adentro": usa el caso real del usuario (productora, discos SF28, etc.)
> y tono de avance de proyecto. **Hay que rehacerla como un producto vendible a cualquier
> cliente.** Retomar esto en otra sesión (idealmente con un generador de slides con créditos,
> p. ej. Gamma, o un deck HTML pulido).

## Qué cambiar (de "demo interna" → "producto")
- **Quitar lo específico del usuario**: nada de "SF28", "54 discos de esta productora",
  rutas reales, ni cifras del catálogo personal como protagonistas. Usarlas, si acaso, como
  "caso de estudio" anonimizado, no como el cuerpo del pitch.
- **Posicionamiento de producto**: qué es DiskDex para *cualquiera*, no para una persona.
- **Mercado objetivo**: casas de post/productoras, fotógrafos, agencias, archivos/medios,
  estudios, cualquiera con muchos discos externos/backups o un NAS que crece.
- **Propuesta de valor genérica**: "encontrá cualquier archivo en segundos sin enchufar
  discos; sumá un disco nuevo en un clic; traé el archivo a donde lo necesites, seguro".
- **Diferenciadores vs. competencia**: DiskCatalogMaker, NeoFinder, WinCatalog, CatalogMyDisks.
  El gran diferencial: **conector remoto seguro** (traer archivos de la LAN a la nube / otro
  equipo, read-only y cifrado) + multiplataforma (mac/win) + importación del histórico .dcmf.
- **Estructura sugerida de pitch comercial**:
  1. Hook / dolor universal (TB dispersos, búsquedas a ciegas)
  2. Producto en una frase
  3. Cómo funciona (3 pasos: catalogá → buscá → traé)
  4. Funcionalidades clave (con íconos, beneficio antes que feature)
  5. Diferenciadores / por qué no la competencia
  6. Casos de uso por vertical (post, foto, agencias, NAS hogareño/PYME)
  7. Seguridad (para vender el conector con confianza)
  8. Planes / pricing (definir: free + pro + ¿equipo?) — *pendiente decidir*
  9. Roadmap corto y creíble
  10. CTA (probar / contacto)
- **Tono**: orientado a beneficio y resultado, no a stack técnico. El "cómo está hecho"
  pasa a ser una nota al pie de credibilidad, no una slide central.
- **Branding**: definir logo, paleta, nombre definitivo (¿"DiskDex"?), tagline.
- **Decisiones abiertas**: pricing, nombre/branding final, si el conector es feature paga.

## Material reutilizable (datos verificados, por si sirven como prueba social)
- Importa catálogos .dcmf grandes sin pérdida (probado: 6,8 M de entradas, archivos de
  +170 GB sin truncar, búsqueda full-text en <1 s).
- Multiplataforma (Tauri 2 + Rust), catálogo portable (un archivo SQLite).
- Escaneo con detección automática al conectar disco; re-escaneo sin duplicar.

— Dejar este archivo hasta que la presentación comercial esté hecha.
