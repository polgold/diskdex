import { useEffect, useState } from "react";
import { X, BarChart3, Loader2 } from "lucide-react";
import { api, type Stats } from "../lib/ipc";
import { useCatalog } from "../store/catalog";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";

export function StatsDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const selectedDiskId = useCatalog((s) => s.selectedDiskId);
  const disks = useCatalog((s) => s.disks);
  const [scope, setScope] = useState<"all" | "disk">("all");
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);

  const diskName = disks.find((d) => d.id === selectedDiskId)?.name;

  useEffect(() => {
    setLoading(true);
    api
      .catalogStats(scope === "disk" ? selectedDiskId ?? undefined : undefined)
      .then(setStats)
      .finally(() => setLoading(false));
  }, [scope, selectedDiskId]);

  const maxExt = stats?.by_ext[0]?.total_size ?? 1;

  return (
    <Modal onClose={onClose} title={t("stats.title")} icon={<BarChart3 className="h-4 w-4 text-emerald-400" />}>
      <div className="mb-3 flex items-center gap-2 text-xs">
        <button
          onClick={() => setScope("all")}
          className={`rounded px-2 py-1 ${scope === "all" ? "bg-neutral-700 text-white" : "text-neutral-400 hover:bg-neutral-800"}`}
        >
          {t("stats.scopeAll")}
        </button>
        {selectedDiskId != null && (
          <button
            onClick={() => setScope("disk")}
            className={`rounded px-2 py-1 ${scope === "disk" ? "bg-neutral-700 text-white" : "text-neutral-400 hover:bg-neutral-800"}`}
          >
            {t("stats.scopeDisk", { name: diskName ?? "" })}
          </button>
        )}
      </div>

      {loading || !stats ? (
        <div className="flex items-center gap-2 py-10 text-sm text-neutral-500">
          <Loader2 className="h-4 w-4 animate-spin" /> {t("stats.calculating")}
        </div>
      ) : (
        <div className="space-y-5">
          <div className="flex gap-6">
            <Stat n={formatCount(stats.file_count)} l={t("stats.files")} />
            <Stat n={formatCount(stats.folder_count)} l={t("stats.folders")} />
            <Stat n={formatBytes(stats.total_size)} l={t("stats.totalSize")} />
          </div>

          <div>
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-500">
              {t("stats.byExtension")}
            </h3>
            <div className="space-y-1">
              {stats.by_ext.map((e) => (
                <div key={e.ext} className="flex items-center gap-2 text-xs">
                  <span className="w-14 shrink-0 font-mono text-neutral-300">.{e.ext}</span>
                  <div className="h-3.5 flex-1 overflow-hidden rounded bg-neutral-800">
                    <div className="h-full bg-emerald-500/60" style={{ width: `${(e.total_size / maxExt) * 100}%` }} />
                  </div>
                  <span className="w-20 shrink-0 text-right font-mono text-neutral-400">{formatBytes(e.total_size)}</span>
                  <span className="w-16 shrink-0 text-right font-mono text-neutral-600">{formatCount(e.count)}</span>
                </div>
              ))}
            </div>
          </div>

          <div>
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-500">
              {t("stats.biggestFiles")}
            </h3>
            <div className="space-y-0.5">
              {stats.biggest.map((b) => (
                <div key={b.id} className="flex items-center gap-2 text-xs" title={b.path}>
                  <span className="w-20 shrink-0 text-right font-mono text-emerald-400">{formatBytes(b.size_logical)}</span>
                  <span className="w-24 shrink-0 truncate text-neutral-400">{b.disk_name}</span>
                  <span className="truncate font-mono text-[11px] text-neutral-500">{b.path}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </Modal>
  );
}

function Stat({ n, l }: { n: string; l: string }) {
  return (
    <div>
      <div className="font-mono text-xl text-neutral-100">{n}</div>
      <div className="text-xs text-neutral-500">{l}</div>
    </div>
  );
}

export function Modal({
  onClose,
  title,
  icon,
  children,
}: {
  onClose: () => void;
  title: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm animate-fade-in">
      <div className="flex max-h-[80vh] w-full max-w-2xl flex-col rounded-xl border border-border bg-card shadow-pop animate-zoom-in">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="flex items-center gap-2 text-sm font-semibold">
            {icon}
            {title}
          </h2>
          <button onClick={onClose} className="rounded-md p-1.5 text-neutral-400 transition-colors hover:bg-accent hover:text-neutral-100">
            <X className="h-4 w-4" />
          </button>
        </div>
        <div className="overflow-auto p-4">{children}</div>
      </div>
    </div>
  );
}
