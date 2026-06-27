import { useState } from "react";
import { Sparkles, Eye, EyeOff } from "lucide-react";
import { useCatalog } from "../store/catalog";
import { useT } from "../lib/i18n";
import { getClaudeKey, setClaudeKey } from "../lib/settings";
import { Modal } from "./StatsDialog";

/**
 * C3 — Ajustes de búsqueda con IA (Claude). Cada usuario pega su propia API key de
 * Anthropic; se guarda solo en este equipo. Con la key + el toggle activado, las
 * búsquedas en lenguaje natural se interpretan con Claude (lugar/luz/fecha/tipo).
 */
export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const nlClaude = useCatalog((s) => s.nlClaude);
  const setNlClaude = useCatalog((s) => s.setNlClaude);
  const [key, setKey] = useState(getClaudeKey());
  const [show, setShow] = useState(false);

  function save() {
    setClaudeKey(key);
    onClose();
  }

  const hasKey = key.trim().length > 0;

  return (
    <Modal onClose={onClose} title={t("settings.title")} icon={<Sparkles className="h-4 w-4 text-violet-400" />}>
      <div className="space-y-4 text-xs">
        <div>
          <label className="flex items-center justify-between gap-2">
            <span className="font-medium text-neutral-200">{t("settings.nlClaude")}</span>
            <button
              onClick={() => setNlClaude(!nlClaude)}
              disabled={!hasKey}
              className={`relative h-5 w-9 rounded-full transition-colors disabled:opacity-40 ${nlClaude && hasKey ? "bg-violet-600" : "bg-neutral-700"}`}
              title={hasKey ? "" : t("settings.needKey")}
            >
              <span className={`absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all ${nlClaude && hasKey ? "left-[18px]" : "left-0.5"}`} />
            </button>
          </label>
          <p className="mt-1 text-[11px] text-neutral-500">{t("settings.nlClaudeHelp")}</p>
        </div>

        <div>
          <label className="mb-1 block font-medium text-neutral-200">{t("settings.apiKey")}</label>
          <div className="flex items-center gap-1.5">
            <input
              type={show ? "text" : "password"}
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder="sk-ant-…"
              className="flex-1 rounded border border-neutral-700 bg-neutral-900 px-2 py-1.5 font-mono text-[11px] text-neutral-200"
            />
            <button
              onClick={() => setShow((v) => !v)}
              className="rounded border border-neutral-700 p-1.5 text-neutral-400 hover:bg-neutral-800"
              title={show ? t("settings.hide") : t("settings.show")}
            >
              {show ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
            </button>
          </div>
          <p className="mt-1 text-[11px] text-neutral-500">{t("settings.apiKeyHelp")}</p>
        </div>

        <div className="flex justify-end gap-2 border-t border-border pt-3">
          <button onClick={onClose} className="rounded border border-neutral-700 px-3 py-1.5 text-neutral-300 hover:bg-neutral-800">
            {t("common.cancel")}
          </button>
          <button onClick={save} className="rounded border border-violet-700 bg-violet-950/50 px-3 py-1.5 text-violet-200 hover:bg-violet-900/50">
            {t("settings.save")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
