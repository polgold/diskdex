import { HardDrive } from "lucide-react";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";

/** Cuántos puntos viajan a la vez. Cuatro alcanza para leer la dirección sin
 *  que parezca ruido; más puntos distraen del progreso real. */
const DOTS = 4;

/**
 * Origen → destino con puntos animados mientras se copia.
 *
 * La barra dice cuánto falta; esto dice QUÉ está pasando y HACIA DÓNDE. Con dos
 * discos de nombre parecido (SFBACKUP7 / SFBACKUP8) equivocarse de dirección es
 * fácil y caro, así que la dirección se ve de un vistazo en vez de leerse.
 */
export function TransferAnimation({
  from,
  to,
  done,
  total,
  bytesDone,
  bytesTotal,
  current,
}: {
  from: string;
  to: string;
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
  current: string;
}) {
  const t = useT();
  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;

  return (
    <div className="rounded-lg border border-border bg-neutral-950/40 p-3">
      <div className="flex items-center gap-3">
        <Disk name={from} active />
        <div className="relative min-w-0 flex-1">
          {/* Riel: el tramo hecho se pinta, el resto queda apagado. */}
          <div className="h-1 w-full overflow-hidden rounded bg-neutral-800">
            <div
              className="h-full rounded bg-emerald-500 transition-all duration-500"
              style={{ width: `${pct}%` }}
            />
          </div>
          {/* Puntos viajando. El desfase los reparte a lo largo del riel. */}
          <div className="pointer-events-none absolute inset-x-0 top-1/2 h-0">
            {Array.from({ length: DOTS }).map((_, i) => (
              <span
                key={i}
                className="absolute h-1.5 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-emerald-300 shadow-[0_0_6px_rgba(52,211,153,0.9)] animate-flow"
                style={{ animationDelay: `${(i * 1.8) / DOTS}s` }}
              />
            ))}
          </div>
          <p className="mt-2 text-center text-[11px] tabular-nums text-neutral-400">
            {formatCount(done)}/{formatCount(total)} · {pct}%
            {bytesTotal > 0 && (
              <span className="text-neutral-500">
                {" "}
                · {formatBytes(bytesDone)} / {formatBytes(bytesTotal)}
              </span>
            )}
          </p>
        </div>
        <Disk name={to} />
      </div>

      {current && (
        <p className="mt-1 truncate text-center font-mono text-[10px] text-neutral-600" title={current}>
          {current}
        </p>
      )}
      <p className="sr-only">{t("copy.running")}</p>
    </div>
  );
}

function Disk({ name, active }: { name: string; active?: boolean }) {
  return (
    <div className="flex w-24 shrink-0 flex-col items-center gap-1">
      <HardDrive className={`h-6 w-6 ${active ? "text-sky-400" : "text-emerald-400"}`} />
      <span className="w-full truncate text-center text-[11px] text-neutral-400" title={name}>
        {name}
      </span>
    </div>
  );
}
