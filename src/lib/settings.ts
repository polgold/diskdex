// Ajustes locales del usuario (C3). La API key de Anthropic la pega CADA usuario;
// se guarda solo en este equipo (localStorage), nunca se sube a ningún lado salvo a
// la propia API de Anthropic al hacer una búsqueda en lenguaje natural.

const KEY_ANTHROPIC = "diskdex:anthropicKey";
const KEY_NL = "diskdex:nlClaude";

export function getClaudeKey(): string {
  try {
    return localStorage.getItem(KEY_ANTHROPIC) ?? "";
  } catch {
    return "";
  }
}

export function setClaudeKey(v: string): void {
  try {
    if (v.trim()) localStorage.setItem(KEY_ANTHROPIC, v.trim());
    else localStorage.removeItem(KEY_ANTHROPIC);
  } catch {
    /* ignore */
  }
}

export function getNlClaudeEnabled(): boolean {
  try {
    return localStorage.getItem(KEY_NL) === "1";
  } catch {
    return false;
  }
}

export function setNlClaudeEnabled(b: boolean): void {
  try {
    localStorage.setItem(KEY_NL, b ? "1" : "0");
  } catch {
    /* ignore */
  }
}
