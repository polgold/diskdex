const es = {
  langName: "Español",
  meta: {
    title: "DiskDex — Encontrá cualquier archivo de tus discos, sin enchufar ninguno",
    description:
      "DiskDex indexa el contenido de todos tus discos de respaldo y te deja buscar y navegar aunque estén desconectados. Importá tu catálogo, escaneá al conectar y encontrá cualquier archivo en medio segundo. macOS y Windows.",
    ogAlt: "DiskDex — catálogo y búsqueda de discos offline",
  },
  nav: {
    features: "Funciones",
    how: "Cómo funciona",
    screenshots: "Capturas",
    roadmap: "Roadmap",
    download: "Descargar",
    cta: "Descargar",
    menu: "Menú",
  },
  hero: {
    eyebrow: "Catálogo de discos offline",
    titlePre: "Encontrá cualquier archivo de tus discos",
    titleEm: "sin enchufar",
    titlePost: "ninguno.",
    sub: "DiskDex indexa el contenido de todos tus discos de respaldo y te deja buscar y navegar aunque estén guardados en un cajón. Importá tu catálogo histórico, escaneá discos nuevos al conectarlos y encontrá lo que buscás al instante.",
    ctaPrimary: "Descargar para macOS",
    ctaSecondary: "Descargar para Windows",
    platforms: "Gratis · macOS 12+ y Windows 10/11 · Apple Silicon e Intel",
    stats: [
      { n: "6.828.850", l: "archivos indexados" },
      { n: "116 TB", l: "en 54 discos" },
      { n: "< 0,5 s", l: "por búsqueda" },
    ],
  },
  shot: {
    search: "Buscar",
    query: "*.mov",
    results: "261.991 resultados",
    resultsMeta: "0,48 s · 54 discos",
    online: "online",
    offline: "offline",
    sidebarTitle: "DISCOS",
    disks: [
      { name: "SF28", meta: "1,8 TB · 214.502 arch.", state: "online" },
      { name: "RAID_04", meta: "16 TB · 1.204.881 arch.", state: "offline" },
      { name: "LTO_BACKUP_12", meta: "12 TB · 88.310 arch.", state: "offline" },
      { name: "BACKUP_07", meta: "4 TB · 512.019 arch.", state: "offline" },
    ],
    rows: [
      { name: "C0001.MP4", path: "SF28/HUFNAGL PILAR/…/CLIP", size: "3,4 GB", state: "online" },
      { name: "entrega_final_v3.mov", path: "RAID_04/2019/PROYECTOS", size: "48,1 GB", state: "offline" },
      { name: "master_color.mov", path: "LTO_BACKUP_12/COLOR", size: "174,94 GB", state: "offline" },
      { name: "render_4k_final.mov", path: "RAID_01/ENTREGAS", size: "22,7 GB", state: "offline" },
      { name: "camA_take12.mov", path: "BACKUP_07/RUSHES/DIA_03", size: "9,8 GB", state: "offline" },
    ],
  },
  trust: {
    line: "Probado contra el catálogo real de una productora:",
    highlight: "261.991 resultados de «.mov» en 0,48 segundos",
    tail: "sobre 6,8 millones de archivos repartidos en 54 discos.",
  },
  features: {
    eyebrow: "Funciones",
    title: "Todo tu archivo, bajo control",
    subtitle:
      "DiskDex reemplaza el «ir enchufando disco por disco» con un catálogo único, portable y buscable al instante.",
    items: [
      {
        key: "import",
        title: "Importá tu catálogo histórico",
        body: "Traé tu archivo .dcmf de DiskCatalogMaker sin perder nada: nombres, jerarquía completa, fechas y tamaños reales de hasta cientos de GB. Validado sobre 54 discos y 6,8 M de entradas.",
      },
      {
        key: "scan",
        title: "Escaneá al conectar",
        body: "Conectá un disco y DiskDex lo detecta solo. Guarda el árbol completo con tamaño lógico y físico, fechas y una huella digital del volumen para reconocerlo la próxima vez.",
      },
      {
        key: "offline",
        title: "Sabé qué disco está a mano",
        body: "Cada disco aparece como online u offline según lo que esté montado, con su capacidad y cantidad de archivos. Buscás siempre; enchufás solo cuando hace falta.",
      },
      {
        key: "search",
        title: "Búsqueda instantánea multi-disco",
        body: "Búsqueda full-text por nombre sobre todos los discos a la vez, con la ruta completa y el disco de cada resultado. Menos de un segundo sobre millones de archivos.",
      },
      {
        key: "duplicates",
        title: "Duplicados y limpieza",
        body: "Encontrá copias repetidas entre discos para recuperar espacio, sin contar dos veces el mismo archivo físico. Ideal para consolidar backups viejos.",
      },
      {
        key: "stats",
        title: "Estadísticas y auditoría de backup",
        body: "Una visión clara del archivo: qué ocupa más, cómo se reparte por disco y tipo, y qué material está (o no) respaldado en más de un lugar.",
      },
    ],
  },
  how: {
    eyebrow: "Cómo funciona",
    title: "De 54 discos a un buscador, en tres pasos",
    steps: [
      {
        n: "01",
        title: "Importá o escaneá",
        body: "Traé tu .dcmf existente o conectá un disco y dejá que DiskDex lo indexe. El trabajo pesado corre en Rust, nunca traba la interfaz.",
      },
      {
        n: "02",
        title: "Todo queda en un catálogo",
        body: "Un solo archivo portable con SQLite + búsqueda full-text. Escala a millones de archivos y viaja con vos entre máquinas.",
      },
      {
        n: "03",
        title: "Buscá aunque esté desconectado",
        body: "Escribí un nombre o una extensión y obtené el disco y la ruta al instante. Enchufás el disco solo cuando ya sabés cuál es.",
      },
    ],
  },
  screenshots: {
    eyebrow: "Capturas",
    title: "Sobrio, rápido, tipo herramienta de post",
    subtitle:
      "Modo oscuro, atajos de teclado y listas virtualizadas que se mueven fluido aunque haya millones de filas.",
    captions: {
      main: "Vista principal — discos, contenido y búsqueda en un solo lugar",
      search: "Búsqueda full-text con ruta y disco de cada resultado",
      inspector: "Inspector — detalle de cada archivo o carpeta",
    },
  },
  roadmap: {
    eyebrow: "Roadmap",
    title: "Lo que viene",
    subtitle: "El motor ya está listo; seguimos sumando funciones en la interfaz.",
    connectorTitle: "Conector remoto seguro",
    connectorBody:
      "Traer los archivos reales desde la red local a la nube o a otro equipo — sin mover el disco. Solo lectura, autenticado por dispositivo y cifrado.",
    items: [
      { label: "Importar .dcmf", state: "done" },
      { label: "Escanear al conectar", state: "done" },
      { label: "Online / offline", state: "done" },
      { label: "Búsqueda multi-disco", state: "done" },
      { label: "Duplicados y estadísticas", state: "progress" },
      { label: "Filtros avanzados (tipo, tamaño, fecha)", state: "progress" },
      { label: "Exportar (CSV / JSON / PDF)", state: "planned" },
      { label: "Conector remoto seguro", state: "planned" },
    ],
    stateLabels: {
      done: "Listo",
      progress: "En curso",
      planned: "Planeado",
    },
  },
  download: {
    eyebrow: "Descargar",
    title: "Empezá a catalogar hoy",
    sub: "Descarga gratuita para macOS y Windows. Sin cuenta, sin nube obligatoria: tu catálogo vive en tu máquina.",
    mac: "Descargar para macOS",
    win: "Descargar para Windows",
    macMeta: "Universal · Apple Silicon + Intel",
    winMeta: "Windows 10 / 11 · 64-bit",
    soon: "Próximamente",
    soonNote:
      "Estamos preparando el primer binario público. Mientras tanto, seguí el repo para enterarte del lanzamiento.",
    repo: "Ver el código en GitHub",
    note: "Open source · el catálogo nunca sale de tu equipo salvo que vos lo decidas.",
  },
  faq: {
    eyebrow: "Preguntas",
    title: "Preguntas frecuentes",
    items: [
      {
        q: "¿Necesito tener los discos conectados para buscar?",
        a: "No. Ese es el punto: una vez indexado un disco, buscás y navegás su contenido aunque esté apagado en un cajón. DiskDex te dice en qué disco está cada archivo y su ruta completa.",
      },
      {
        q: "¿Puedo traer mi catálogo de DiskCatalogMaker?",
        a: "Sí. DiskDex importa el formato .dcmf sin pérdida: nombres, jerarquía, fechas y tamaños reales. Lo probamos contra un catálogo de 54 discos y 6,8 millones de entradas.",
      },
      {
        q: "¿Dónde se guardan mis datos?",
        a: "En tu máquina, en un único archivo de catálogo portable (SQLite). Nada se sube a la nube salvo que vos lo pidas explícitamente.",
      },
      {
        q: "¿Qué pasa si reconecto un disco ya catalogado?",
        a: "DiskDex lo reconoce por su huella de volumen. Re-escanear lo actualiza en su lugar, sin duplicarlo en el catálogo.",
      },
      {
        q: "¿Sirve para volúmenes muy grandes?",
        a: "Sí. Está pensado para escalas de post-producción: millones de archivos y decenas de TB, con búsqueda en menos de un segundo y una interfaz que no se traba.",
      },
    ],
  },
  footer: {
    tagline: "Catálogo y búsqueda de discos offline.",
    made: "Hecho con Tauri, Rust y React.",
    product: "Producto",
    resources: "Recursos",
    rights: "Todos los derechos reservados.",
    links: {
      features: "Funciones",
      download: "Descargar",
      roadmap: "Roadmap",
      github: "GitHub",
      changelog: "Novedades",
      privacy: "Privacidad",
    },
  },
};

export default es;
