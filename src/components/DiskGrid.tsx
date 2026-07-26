import { HardDrive } from "lucide-react";
import { useCatalog } from "../store/catalog";
import type { DiskRow } from "../lib/ipc";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";

/**
 * Vista de todos los discos del catálogo, al abrir sin ninguno seleccionado.
 *
 * Muestra ocupado/libre de cada uno con lo que sabe el catálogo: el tamaño
 * catalogado siempre está; el total y el libre salen de la capacidad guardada
 * al escanear (o de la medición en vivo si el disco está montado). Un disco sin
 * capacidad guardada muestra solo lo catalogado, sin barra. Click abre el disco.
 */
export function DiskGrid() {
  const t = useT();
  const disks = useCatalog((s) => s.disks);
  const openDisk = useCatalog((s) => s.openDisk);

  if (disks.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-neutral-600">
        <HardDrive className="h-10 w-10 text-neutral-700" />
        <p className="text-sm text-neutral-500">{t("disk.noDisks")}</p>
      </div>
    );
  }

  // Online primero, después por nombre: lo conectado es lo accionable ahora.
  const sorted = [...disks].sort((a, b) => {
    if (a.is_online !== b.is_online) return a.is_online ? -1 : 1;
    return a.name.localeCompare(b.name);
  });

  return (
    <div className="h-full overflow-auto p-3">
      <div className="mb-2 flex items-center justify-between px-1 text-xs text-neutral-500">
        <span>{t("disk.allDisks")}</span>
        <span>{t("disk.count", { n: formatCount(disks.length) })}</span>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-2">
        {sorted.map((d) => (
          <DiskCard key={d.id} disk={d} onOpen={() => openDisk(d)} t={t} />
        ))}
      </div>
    </div>
  );
}

function DiskCard({
  disk,
  onOpen,
  t,
}: {
  disk: DiskRow;
  onOpen: () => void;
  t: (k: string, v?: Record<string, string | number>) => string;
}) {
  // El catálogo básico (listDisks) no trae capacity ni libre; con lo catalogado
  // igual mostramos algo útil, y la barra aparece solo si hay total conocido.
  const cataloged = disk.total_size;
  return (
    <button
      onClick={onOpen}
      className="flex flex-col gap-2 rounded-lg border border-border bg-neutral-900/40 p-3 text-left hover:border-neutral-600 hover:bg-neutral-900/70"
    >
      <div className="flex items-center gap-2">
        <HardDrive className={`h-4 w-4 shrink-0 ${disk.is_online ? "text-emerald-400" : "text-neutral-500"}`} />
        <span className="truncate text-sm font-medium text-neutral-200" title={disk.name}>
          {disk.name}
        </span>
        <span
          className={`ml-auto h-1.5 w-1.5 shrink-0 rounded-full ${disk.is_online ? "bg-emerald-500" : "bg-neutral-600"}`}
          title={disk.is_online ? t("common.online") : t("common.offline")}
        />
      </div>
      <div className="text-[11px] text-neutral-500">
        {t("disk.usedShort", { used: formatBytes(cataloged) })} ·{" "}
        {t("disk.filesShort", { n: formatCount(disk.file_count) })}
      </div>
      {disk.location && (
        <div className="truncate text-[11px] text-neutral-600" title={disk.location}>
          📍 {disk.location}
        </div>
      )}
    </button>
  );
}
