import { useEffect, useState } from "react";
import { Database } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { APP_VERSION } from "../lib/version";
import { useT } from "../lib/i18n";

/** Pantalla de inicio: logo + nombre + crédito del creador. Se muestra al
 *  arrancar y se desvanece sola (clic para saltearla). */
export function Splash({ onDone }: { onDone: () => void }) {
  const t = useT();
  const [leaving, setLeaving] = useState(false);

  function dismiss() {
    setLeaving(true);
    setTimeout(onDone, 480); // esperar a que termine el fundido
  }

  useEffect(() => {
    const t = setTimeout(dismiss, 3800); // +2s respecto al original
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      onClick={dismiss}
      className={`fixed inset-0 z-[200] flex cursor-pointer flex-col items-center justify-center gap-6 bg-gradient-to-b from-neutral-900 to-neutral-950 transition-opacity duration-500 ${
        leaving ? "opacity-0" : "opacity-100"
      }`}
    >
      {/* Logo */}
      <div className="animate-splash-logo">
        <div className="relative grid h-24 w-24 place-items-center rounded-3xl bg-primary/15 ring-1 ring-primary/30 shadow-[0_0_60px_-12px_hsl(var(--primary))]">
          <Database className="h-12 w-12 text-primary" />
        </div>
      </div>

      {/* Nombre + tagline */}
      <div className="animate-splash-up text-center" style={{ animationDelay: "120ms" }}>
        <h1 className="text-3xl font-semibold tracking-tight text-neutral-100">DiskDex</h1>
        <p className="mt-1 text-sm text-neutral-500">{t("app.tagline")}</p>
      </div>

      {/* Crédito del creador (más grande y protagónico) */}
      <div
        className="animate-splash-up absolute bottom-12 flex flex-col items-center gap-1.5 text-center"
        style={{ animationDelay: "280ms" }}
      >
        <span className="text-[11px] uppercase tracking-[0.2em] text-neutral-600">
          Desarrollado por
        </span>
        <span className="text-xl font-medium text-neutral-200">Pablo Goldberg</span>
        <button
          onClick={(e) => {
            e.stopPropagation();
            openUrl("https://exitmedia.com.ar");
          }}
          className="text-2xl font-bold tracking-tight text-primary transition-opacity hover:opacity-80"
          title="exitmedia.com.ar"
        >
          ExitMedia
        </button>
        <span className="mt-1 text-[11px] text-neutral-700">v{APP_VERSION}</span>
      </div>
    </div>
  );
}
