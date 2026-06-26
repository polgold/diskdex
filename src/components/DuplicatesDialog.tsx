import { useEffect, useState } from "react";
import { Copy, Loader2, ChevronRight } from "lucide-react";
import { api, type DupGroup } from "../lib/ipc";
import { formatBytes, formatCount } from "../lib/format";
import { Modal } from "./StatsDialog";

const MIN_SIZE_OPTIONS = [
  { label: "≥ 1 MB", value: 1_048_576 },
  { label: "≥ 50 MB", value: 50 * 1_048_576 },
  { label: "≥ 500 MB", value: 500 * 1_048_576 },
  { label: "≥ 1 GB", value: 1024 * 1_048_576 },
];

export function DuplicatesDialog({ onClose }: { onClose: () => void }) {
  const [minSize, setMinSize] = useState(MIN_SIZE_OPTIONS[1].value);
  const [groups, setGroups] = useState<DupGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    api
      .findDuplicates(minSize, 500)
      .then(setGroups)
      .finally(() => setLoading(false));
  }, [minSize]);

  const totalWasted = groups.reduce((a, g) => a + g.wasted, 0);

  return (
    <Modal onClose={onClose} title="Duplicados" icon={<Copy className="h-4 w-4 text-amber-400" />}>
      <div className="mb-3 flex flex-wrap items-center gap-2 text-xs">
        <span className="text-neutral-500">Tamaño mínimo:</span>
        {MIN_SIZE_OPTIONS.map((o) => (
          <button
            key={o.value}
            onClick={() => setMinSize(o.value)}
            className={`rounded px-2 py-1 ${minSize === o.value ? "bg-neutral-700 text-white" : "text-neutral-400 hover:bg-neutral-800"}`}
          >
            {o.label}
          </button>
        ))}
        {!loading && (
          <span className="ml-auto text-neutral-400">
            {formatCount(groups.length)} grupos · <span className="text-amber-300">{formatBytes(totalWasted)}</span> recuperables
          </span>
        )}
      </div>

      {loading ? (
        <div className="flex items-center gap-2 py-10 text-sm text-neutral-500">
          <Loader2 className="h-4 w-4 animate-spin" /> buscando duplicados…
        </div>
      ) : groups.length === 0 ? (
        <p className="py-8 text-center text-sm text-neutral-500">Sin duplicados con ese tamaño mínimo.</p>
      ) : (
        <div className="space-y-1">
          {groups.map((g) => {
            const key = `${g.name}-${g.size}`;
            const open = expanded === key;
            return (
              <div key={key} className="rounded border border-neutral-800">
                <button
                  onClick={() => setExpanded(open ? null : key)}
                  className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-xs hover:bg-neutral-800/50"
                >
                  <ChevronRight className={`h-3.5 w-3.5 shrink-0 text-neutral-500 transition-transform ${open ? "rotate-90" : ""}`} />
                  <span className="truncate font-medium" title={g.name}>{g.name}</span>
                  <span className="ml-auto shrink-0 font-mono text-neutral-400">{formatBytes(g.size)}</span>
                  <span className="shrink-0 rounded bg-neutral-800 px-1.5 py-0.5 font-mono text-[10px] text-neutral-300">×{g.count}</span>
                  <span className="w-20 shrink-0 text-right font-mono text-amber-300">{formatBytes(g.wasted)}</span>
                </button>
                {open && (
                  <div className="border-t border-neutral-800 bg-neutral-950/40 px-2 py-1">
                    {g.items.map((it) => (
                      <div key={it.id} className="flex items-center gap-2 py-0.5 text-[11px]" title={it.path}>
                        <span className="w-24 shrink-0 truncate text-neutral-400">{it.disk_name}</span>
                        <span className="truncate font-mono text-neutral-500">{it.path}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </Modal>
  );
}
