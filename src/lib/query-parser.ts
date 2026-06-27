// Parser de la barra de búsqueda (M4). Convierte texto libre con "tokens de
// atributo" en filtros estructurados que entiende el backend (search_advanced).
//
// Ejemplos:
//   "C0001"                              → nombre
//   "ext:mov,mp4 size>1gb"               → extensiones + tamaño mínimo
//   "render after:2023-01-01 type:file"  → nombre + fecha + tipo
//   "size<500mb before:2024-06-01"       → rango de tamaño/fecha sin nombre

export interface SearchFilters {
  text: string;
  exts: string[];
  tags: string[];
  min_size?: number;
  max_size?: number;
  modified_after?: number;
  modified_before?: number;
  kind?: "file" | "folder";
}

/** Categorías de tipo de archivo (estilo Dropbox) → sets de extensiones. */
export const FILE_CATEGORIES: Record<string, { label: string; exts: string[] }> = {
  imagen: {
    label: "Imágenes",
    exts: [
      "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif", "svg",
      "dng", "arw", "cr2", "cr3", "crw", "nef", "nrw", "raf", "orf", "rw2", "pef", "srw",
    ],
  },
  video: {
    label: "Videos",
    exts: [
      "mp4", "mov", "m4v", "avi", "mkv", "mxf", "mts", "m2ts", "wmv", "webm", "mpg",
      "mpeg", "3gp", "flv", "ogv", "vob", "m2v", "r3d", "braw",
    ],
  },
  audio: {
    label: "Audio",
    exts: ["mp3", "wav", "aiff", "aif", "flac", "aac", "m4a", "ogg", "oga", "wma", "caf"],
  },
  documento: {
    label: "Documentos",
    exts: [
      "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "rtf", "md", "csv",
      "pages", "numbers", "key", "odt", "ods",
    ],
  },
  comprimido: {
    label: "Comprimidos",
    exts: ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "dmg", "iso", "cbz", "cbr"],
  },
};

/** Normaliza un valor de categoría escrito por el usuario a su clave canónica. */
function normalizeCategory(v: string): string | null {
  const s = v.trim().toLowerCase();
  if (FILE_CATEGORIES[s]) return s;
  const aliases: Record<string, string> = {
    imagenes: "imagen", "imágenes": "imagen", image: "imagen", images: "imagen", foto: "imagen", fotos: "imagen",
    videos: "video", clip: "video", clips: "video",
    sonido: "audio", music: "audio", musica: "audio", "música": "audio",
    documentos: "documento", doc: "documento", docs: "documento",
    comprimidos: "comprimido", archive: "comprimido", zip: "comprimido",
  };
  return aliases[s] ?? null;
}

const SIZE_UNITS: Record<string, number> = {
  b: 1,
  kb: 1024,
  mb: 1024 ** 2,
  gb: 1024 ** 3,
  tb: 1024 ** 4,
};

export function parseSize(s: string): number | undefined {
  const m = /^(\d+(?:[.,]\d+)?)\s*(b|kb|mb|gb|tb)?$/i.exec(s.trim());
  if (!m) return undefined;
  const n = parseFloat(m[1].replace(",", "."));
  const unit = (m[2] || "b").toLowerCase();
  return Math.round(n * SIZE_UNITS[unit]);
}

export function parseDate(s: string): number | undefined {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s.trim());
  if (!m) return undefined;
  const ms = Date.UTC(+m[1], +m[2] - 1, +m[3]);
  if (Number.isNaN(ms)) return undefined;
  return Math.floor(ms / 1000);
}

const DAY = 86400;

export function parseQuery(input: string): SearchFilters {
  const f: SearchFilters = { text: "", exts: [], tags: [] };
  const textParts: string[] = [];

  for (const tok of input.split(/\s+/).filter(Boolean)) {
    const lower = tok.toLowerCase();

    // ext:mov  /  ext:mov,mp4
    let m = /^ext:(.+)$/i.exec(tok);
    if (m) {
      for (const e of m[1].split(",")) {
        const clean = e.trim().replace(/^\./, "").toLowerCase();
        if (clean) f.exts.push(clean);
      }
      continue;
    }

    // cat:imagen  /  categoria:video  → expande a su set de extensiones
    m = /^(?:cat|categoria|categoría):(.+)$/i.exec(lower);
    if (m) {
      const key = normalizeCategory(m[1]);
      if (key) {
        for (const e of FILE_CATEGORIES[key].exts) {
          if (!f.exts.includes(e)) f.exts.push(e);
        }
      }
      continue;
    }

    // tag:boda  /  tag:boda,4k  (keyword:… como alias)
    m = /^(?:tag|keyword):(.+)$/i.exec(tok);
    if (m) {
      for (const t of m[1].split(",")) {
        const clean = t.trim().toLowerCase();
        if (clean && !f.tags.includes(clean)) f.tags.push(clean);
      }
      continue;
    }

    // type:file | type:folder  (acepta también archivo/carpeta)
    m = /^(?:type|kind):(.+)$/i.exec(lower);
    if (m) {
      const v = m[1];
      if (v.startsWith("fold") || v.startsWith("carp") || v.startsWith("dir")) f.kind = "folder";
      else if (v.startsWith("file") || v.startsWith("arch")) f.kind = "file";
      continue;
    }

    // size>1gb size>=1gb size<500mb size<=500mb
    m = /^size(>=|<=|>|<)(.+)$/i.exec(lower);
    if (m) {
      const val = parseSize(m[2]);
      if (val !== undefined) {
        if (m[1].startsWith(">")) f.min_size = val;
        else f.max_size = val;
      }
      continue;
    }

    // after:DATE | since:DATE | modified>DATE
    m = /^(?:after|since):(.+)$/i.exec(lower) || /^modified>(.+)$/i.exec(lower);
    if (m) {
      const d = parseDate(m[1]);
      if (d !== undefined) f.modified_after = d;
      continue;
    }

    // before:DATE | until:DATE | modified<DATE  (incluye todo ese día)
    m = /^(?:before|until):(.+)$/i.exec(lower) || /^modified<(.+)$/i.exec(lower);
    if (m) {
      const d = parseDate(m[1]);
      if (d !== undefined) f.modified_before = d + DAY - 1;
      continue;
    }

    textParts.push(tok);
  }

  f.text = textParts.join(" ");
  return f;
}

/** ¿El query tiene algún criterio (texto o filtro)? */
export function hasCriteria(f: SearchFilters): boolean {
  return (
    f.text.trim().length > 0 ||
    f.exts.length > 0 ||
    f.tags.length > 0 ||
    f.min_size !== undefined ||
    f.max_size !== undefined ||
    f.modified_after !== undefined ||
    f.modified_before !== undefined ||
    f.kind !== undefined
  );
}
