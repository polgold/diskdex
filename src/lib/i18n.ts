// i18n liviano (ES/EN) sin dependencias. El idioma vive en un store de zustand
// (persistido en localStorage, autodetectado del SO la primera vez). Los textos
// están en `DICT` por clave namespaced (p.ej. "app.scan"). `t(key, vars)` busca
// el texto del idioma actual, cae a ES y luego a la clave cruda; interpola {var}.
import { create } from "zustand";
import { STRINGS } from "./i18n/strings";

export type Lang = "es" | "en";

const STORE_KEY = "diskdex:lang";

function detectLang(): Lang {
  try {
    const saved = localStorage.getItem(STORE_KEY);
    if (saved === "es" || saved === "en") return saved;
  } catch {
    /* ignore */
  }
  const nav = (typeof navigator !== "undefined" && navigator.language) || "en";
  return nav.toLowerCase().startsWith("es") ? "es" : "en";
}

interface I18nState {
  lang: Lang;
  setLang: (l: Lang) => void;
  toggle: () => void;
}

export const useI18n = create<I18nState>((set, get) => ({
  lang: detectLang(),
  setLang: (lang) => {
    try {
      localStorage.setItem(STORE_KEY, lang);
    } catch {
      /* ignore */
    }
    set({ lang });
  },
  toggle: () => get().setLang(get().lang === "es" ? "en" : "es"),
}));

type Vars = Record<string, string | number>;

export function translate(lang: Lang, key: string, vars?: Vars): string {
  const s = STRINGS[lang]?.[key] ?? STRINGS.es?.[key] ?? key;
  if (!vars) return s;
  return s.replace(/\{(\w+)\}/g, (_, k: string) =>
    vars[k] !== undefined ? String(vars[k]) : `{${k}}`
  );
}

/** Hook: devuelve `t` ligado al idioma actual (re-renderiza al cambiarlo). */
export function useT() {
  const lang = useI18n((s) => s.lang);
  return (key: string, vars?: Vars) => translate(lang, key, vars);
}
