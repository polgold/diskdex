import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type { CopySummary } from "./ipc";
import { STRINGS } from "./i18n/strings";
import { useI18n } from "./i18n";

/** Traduce fuera de React (el store no puede usar el hook useT). */
function tr(key: string, vars?: Record<string, string | number>): string {
  const lang = useI18n.getState().lang;
  let s = STRINGS[lang]?.[key] ?? STRINGS.es[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(String(v));
  }
  return s;
}

/** Pide permiso una sola vez y avisa si quedó denegado (devuelve si se puede). */
async function ensurePermission(): Promise<boolean> {
  try {
    if (await isPermissionGranted()) return true;
    return (await requestPermission()) === "granted";
  } catch {
    return false;
  }
}

/** Pitido corto vía Web Audio: no hace falta empaquetar ningún archivo de sonido,
 *  y suena aunque macOS tenga silenciadas las notificaciones de la app. */
function beep(ok: boolean) {
  try {
    const Ctx =
      window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.connect(gain);
    gain.connect(ctx.destination);
    // Dos tonos ascendentes si salió bien, uno grave si hubo fallas.
    osc.frequency.value = ok ? 880 : 300;
    gain.gain.setValueAtTime(0.0001, ctx.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.15, ctx.currentTime + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + 0.45);
    osc.start();
    if (ok) osc.frequency.setValueAtTime(1175, ctx.currentTime + 0.16);
    osc.stop(ctx.currentTime + 0.5);
    osc.onended = () => ctx.close().catch(() => {});
  } catch {
    /* sin audio disponible: la notificación visual alcanza */
  }
}

/** Avisa que terminó una copia larga. Pensado para cuando la app está minimizada
 *  en el tray: por eso notificación del sistema y no un cartel dentro de la app. */
export async function notifyTaskDone(s: CopySummary, label: string) {
  const failed = s.failed > 0;
  const ok = !s.cancelled && !failed;
  beep(ok);

  const title = s.cancelled
    ? tr("notify.copyCancelled")
    : failed
      ? tr("notify.copyWithErrors")
      : tr("notify.copyDone");
  const body = `${label} — ${tr("notify.copyBody", {
    copied: s.copied,
    verified: s.verified,
    failed: s.failed,
  })}`;

  if (!(await ensurePermission())) return;
  try {
    sendNotification({ title, body });
  } catch {
    /* la notificación es un extra: nunca debe romper el flujo */
  }
}
