// C3 — Búsqueda en lenguaje natural vía Claude (API de Anthropic). Cada usuario pone
// su propia API key (ver settings.ts). Claude convierte una frase libre como
// "clips de Jujuy del atardecer del 2023 que pesen más de 2gb" en filtros
// estructurados (lugar/luz/fecha/tipo/tamaño) + un concepto visual residual para la
// búsqueda por similitud (CLIP). Devuelve el MISMO shape que el parser local
// (`NLQuery`), así el resto del pipeline de búsqueda no cambia.
//
// La llamada va directo del cliente a la API de Anthropic con el header
// `anthropic-dangerous-direct-browser-access`. Solo viaja el TEXTO de la consulta
// (nunca el catálogo ni rutas de archivos).

import { FILE_CATEGORIES, type SearchFilters } from "./query-parser";
import type { NLQuery } from "./nl-parser";

// Haiku: rápido y barato, suficiente para clasificar una frase de búsqueda.
const MODEL = "claude-haiku-4-5-20251001";

const CATEGORY_KEYS = Object.keys(FILE_CATEGORIES).join(", ");

const SYSTEM = `Convertís la búsqueda en lenguaje natural de un usuario sobre su catálogo personal de discos (fotos, videos, audio, documentos) en un filtro JSON ESTRICTO. Respondé SOLO el JSON, sin texto adicional, sin markdown.

Esquema:
{
  "categories": string[],   // subconjunto de: ${CATEGORY_KEYS}
  "place": string|null,     // nombre de lugar mencionado (ciudad/provincia/país), ej "Jujuy"
  "light": string|null,     // uno de: sunset, sunrise, golden, twilight, night, day (atardecer=sunset, amanecer=sunrise, hora dorada=golden, noche=night)
  "min_size_mb": number|null,
  "max_size_mb": number|null,
  "after_year": number|null,
  "before_year": number|null,
  "kind": "file"|"folder"|null,
  "concept": string|null    // concepto VISUAL residual para buscar por contenido (objetos/escenas: "gente en la playa", "perros"), o null si no hay
}

Reglas:
- Un lugar geográfico va en "place", NO en "concept".
- Momentos del día (atardecer, amanecer, noche) van en "light", NO en "concept".
- "concept" es solo para contenido visual que no sea lugar ni momento del día.
- Si un campo no aplica, ponelo en null (o [] para categories).`;

interface ClaudeJson {
  categories?: string[];
  place?: string | null;
  light?: string | null;
  min_size_mb?: number | null;
  max_size_mb?: number | null;
  after_year?: number | null;
  before_year?: number | null;
  kind?: "file" | "folder" | null;
  concept?: string | null;
}

/** Extrae el primer objeto JSON de un texto (por si el modelo agrega prosa). */
function extractJson(s: string): string {
  const a = s.indexOf("{");
  const b = s.lastIndexOf("}");
  return a >= 0 && b > a ? s.slice(a, b + 1) : s;
}

/** Convierte la respuesta de Claude al shape NLQuery (filtros + concepto). */
export function claudeJsonToNLQuery(j: ClaudeJson): NLQuery {
  const filters: SearchFilters = { text: "", exts: [], tags: [] };
  for (const c of j.categories ?? []) {
    const cat = FILE_CATEGORIES[c];
    if (cat) for (const e of cat.exts) if (!filters.exts.includes(e)) filters.exts.push(e);
  }
  if (j.place && String(j.place).trim()) filters.place = String(j.place).trim();
  if (j.light && String(j.light).trim()) filters.light = String(j.light).trim().toLowerCase();
  if (typeof j.min_size_mb === "number") filters.min_size = Math.round(j.min_size_mb * 1024 * 1024);
  if (typeof j.max_size_mb === "number") filters.max_size = Math.round(j.max_size_mb * 1024 * 1024);
  if (typeof j.after_year === "number") filters.modified_after = Math.floor(Date.UTC(j.after_year, 0, 1) / 1000);
  if (typeof j.before_year === "number")
    filters.modified_before = Math.floor(Date.UTC(j.before_year, 11, 31, 23, 59, 59) / 1000);
  if (j.kind === "file" || j.kind === "folder") filters.kind = j.kind;
  const concept = (j.concept ?? "").toString().trim();
  return { concept, filters };
}

/** Pide a Claude que interprete la frase. Lanza si la API falla (el caller degrada). */
export async function claudeNLToQuery(input: string, apiKey: string): Promise<NLQuery> {
  const res = await fetch("https://api.anthropic.com/v1/messages", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-api-key": apiKey,
      "anthropic-version": "2023-06-01",
      "anthropic-dangerous-direct-browser-access": "true",
    },
    body: JSON.stringify({
      model: MODEL,
      max_tokens: 400,
      system: SYSTEM,
      messages: [{ role: "user", content: input }],
    }),
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`Claude API ${res.status}: ${body.slice(0, 200)}`);
  }
  const data = await res.json();
  const text: string = (data?.content?.[0]?.text ?? "").trim();
  const json = JSON.parse(extractJson(text)) as ClaudeJson;
  return claudeJsonToNLQuery(json);
}
