// Parser de lenguaje natural (IA Fase 3). Separa una frase libre en:
//   - filtros estructurados (tipo/fecha/tamaño → SearchFilters, como search_advanced)
//   - concepto visual residual (texto que va a la búsqueda semántica)
//
// 100% local y determinístico (sin modelo). Multilingüe ES/EN. Ejemplos:
//   "videos del 2022 con gente en la playa que pesen más de 2gb"
//     → filtros {exts: video…, after:2022-01-01, before:2022-12-31, min_size:2gb}
//     → concepto "gente en la playa"
//   "fotos de un atardecer"      → {exts: imagen…} + concepto "un atardecer"
//   "archivos grandes del 2021"  → {min_size:1gb, after/before 2021} + concepto ""

import { FILE_CATEGORIES, parseSize, type SearchFilters } from "./query-parser";
import type { SemanticItem } from "./ipc";

const GB = 1024 ** 3;
const MB = 1024 ** 2;

const yStart = (y: number) => Math.floor(Date.UTC(y, 0, 1) / 1000);
const yEnd = (y: number) => Math.floor(Date.UTC(y, 11, 31, 23, 59, 59) / 1000);

// Conectores que se recortan SOLO de los extremos del concepto (no del interior,
// para no romper frases como "gente en la playa").
const FILLERS = new Set([
  "del", "de", "la", "el", "los", "las", "un", "una", "unos", "unas", "con", "que",
  "y", "mis", "mi", "sobre", "tipo", "archivos", "archivo", "cosas", "algo",
  "pesen", "pesan", "pese", "pesa", "midan", "miden",
  "of", "the", "with", "that", "and", "in", "on", "a", "an", "some", "files", "file",
  "weigh", "weighing", "weighs",
]);

export interface NLQuery {
  concept: string;
  filters: SearchFilters;
  /** Idioma hablado pedido ("en español") → filtra transcripciones (Fase 4). */
  lang?: string;
}

function addCategory(filters: SearchFilters, key: string) {
  for (const e of FILE_CATEGORIES[key].exts) {
    if (!filters.exts.includes(e)) filters.exts.push(e);
  }
}

/** Convierte una frase en {filtros, concepto}. */
export function parseNaturalQuery(input: string): NLQuery {
  const filters: SearchFilters = { text: "", exts: [], tags: [] };
  const year = new Date().getFullYear();
  let lang: string | undefined;
  let s = ` ${input.toLowerCase()} `;

  // ── Tamaño con comparador ──
  s = s.replace(
    /\b(?:m[áa]s de|mayor(?:es)?(?: a| que)?|arriba de|over|more than|bigger than)\s+(\d+(?:[.,]\d+)?)\s*(tb|gb|mb|kb)\b/g,
    (_m, n, u) => {
      const v = parseSize(`${n}${u}`);
      if (v !== undefined) filters.min_size = v;
      return " ";
    },
  );
  s = s.replace(
    /\b(?:menos de|menor(?:es)?(?: a| que)?|por debajo de|under|less than|smaller than)\s+(\d+(?:[.,]\d+)?)\s*(tb|gb|mb|kb)\b/g,
    (_m, n, u) => {
      const v = parseSize(`${n}${u}`);
      if (v !== undefined) filters.max_size = v;
      return " ";
    },
  );
  // Tamaño vago.
  s = s.replace(/\b(grandes?|pesad[oa]s?|big|heavy|large)\b/g, () => {
    if (filters.min_size === undefined) filters.min_size = GB;
    return " ";
  });
  s = s.replace(/\b(chic[oa]s?|peque[ñn][oa]s?|livian[oa]s?|small|tiny|light)\b/g, () => {
    if (filters.max_size === undefined) filters.max_size = 50 * MB;
    return " ";
  });

  // ── Fechas ──
  s = s.replace(/\b(?:despu[ée]s de|desde|posteriores? a|after|since)\s+((?:19|20)\d{2})\b/g, (_m, y) => {
    filters.modified_after = yStart(+y);
    return " ";
  });
  s = s.replace(/\b(?:antes de|hasta|anteriores? a|before|until)\s+((?:19|20)\d{2})\b/g, (_m, y) => {
    filters.modified_before = yEnd(+y);
    return " ";
  });
  s = s.replace(/\b(?:este a[ñn]o|this year)\b/g, () => {
    filters.modified_after = yStart(year);
    return " ";
  });
  s = s.replace(/\b(?:el a[ñn]o pasado|last year)\b/g, () => {
    filters.modified_after = yStart(year - 1);
    filters.modified_before = yEnd(year - 1);
    return " ";
  });
  // Año suelto ("2022", "del 2022", "en 2022").
  s = s.replace(/\b(?:del?|en)?\s*((?:19|20)\d{2})\b/g, (_m, y) => {
    if (filters.modified_after === undefined) filters.modified_after = yStart(+y);
    if (filters.modified_before === undefined) filters.modified_before = yEnd(+y);
    return " ";
  });

  // ── Categorías de tipo ──
  const cats: [RegExp, string][] = [
    [/\b(videos?|clips?|filmaciones?|metrajes?|footage|pel[ií]culas?)\b/g, "video"],
    [/\b(fotos?|im[aá]genes?|fotograf[ií]as?|photos?|images?|pictures?)\b/g, "imagen"],
    [/\b(audios?|m[uú]sica|canciones?|songs?|tracks?)\b/g, "audio"],
    [/\b(documentos?|docs?|pdfs?)\b/g, "documento"],
    [/\b(comprimidos?|zips?|rars?)\b/g, "comprimido"],
  ];
  for (const [re, key] of cats) {
    s = s.replace(re, () => {
      addCategory(filters, key);
      return " ";
    });
  }

  // ── Carpetas ──
  s = s.replace(/\b(carpetas?|folders?|directorios?)\b/g, () => {
    filters.kind = "folder";
    return " ";
  });

  // ── Idioma hablado ("en español", "in english") → filtra transcripciones ──
  // Requiere "en/in/hablado en/spoken in" antes del idioma para no comerse
  // frases como "en la playa".
  const langs: [RegExp, string][] = [
    [/\b(?:en|hablado en|in|spoken in)\s+(?:espa[ñn]ol|castellano|spanish)\b/g, "es"],
    [/\b(?:en|hablado en|in|spoken in)\s+(?:ingl[ée]s|english)\b/g, "en"],
    [/\b(?:en|hablado en|in|spoken in)\s+(?:portugu[ée]s|portuguese)\b/g, "pt"],
    [/\b(?:en|hablado en|in|spoken in)\s+(?:franc[ée]s|french)\b/g, "fr"],
    [/\b(?:en|hablado en|in|spoken in)\s+(?:italiano|italian)\b/g, "it"],
    [/\b(?:en|hablado en|in|spoken in)\s+(?:alem[áa]n|german)\b/g, "de"],
  ];
  for (const [re, code] of langs) {
    s = s.replace(re, () => {
      if (!lang) lang = code;
      return " ";
    });
  }

  // ── Concepto residual: recortar conectores de los extremos ──
  let toks = s.replace(/\s+/g, " ").trim().split(" ").filter(Boolean);
  while (toks.length && FILLERS.has(toks[0])) toks.shift();
  while (toks.length && FILLERS.has(toks[toks.length - 1])) toks.pop();
  const concept = toks.join(" ");

  return { concept, filters, lang };
}

function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  return i >= 0 ? name.slice(i + 1).toLowerCase() : "";
}

/** Post-filtra resultados semánticos por los filtros estructurados (preserva el ranking). */
export function applyNLFilters(items: SemanticItem[], f: SearchFilters): SemanticItem[] {
  return items.filter((it) => {
    if (f.exts.length && (it.is_folder || !f.exts.includes(extOf(it.name)))) return false;
    if (f.min_size !== undefined && it.size_logical < f.min_size) return false;
    if (f.max_size !== undefined && it.size_logical > f.max_size) return false;
    if (f.modified_after !== undefined && (it.modified_at == null || it.modified_at < f.modified_after))
      return false;
    if (f.modified_before !== undefined && (it.modified_at == null || it.modified_at > f.modified_before))
      return false;
    if (f.kind === "folder" && !it.is_folder) return false;
    if (f.kind === "file" && it.is_folder) return false;
    return true;
  });
}

/** ¿Los filtros tienen algún criterio estructurado (sin contar el concepto)? */
export function hasStructured(f: SearchFilters): boolean {
  return (
    f.exts.length > 0 ||
    f.min_size !== undefined ||
    f.max_size !== undefined ||
    f.modified_after !== undefined ||
    f.modified_before !== undefined ||
    f.kind !== undefined ||
    (f.place != null && f.place.trim() !== "") ||
    (f.light != null && f.light.trim() !== "")
  );
}
