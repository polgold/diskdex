import { useEffect, useState } from "react";
import { ChevronRight, Folder, HardDrive, ScanLine, Trash2 } from "lucide-react";
import { api, type EntryRow, type VolumeInfo, type DiskRow } from "../lib/ipc";
import { useCatalog, type Crumb } from "../store/catalog";
import { useT } from "../lib/i18n";

type DiskStatus = "upToDate" | "changed" | "offline";

/** Estado del disco vs. su volumen montado, por tamaño usado (lectura rápida):
 *  verde = al día, amarillo = cambió el tamaño (conviene re-escanear),
 *  offline = desconectado / no verificable. */
function diskScanStatus(disk: DiskRow, volumes: VolumeInfo[]): DiskStatus {
  const v = volumes.find((vol) => vol.name === disk.name);
  if (!v) return "offline";
  const used = v.total_space - v.available_space;
  const tol = Math.max(5 * 1024 ** 3, v.total_space * 0.15);
  return Math.abs(used - disk.total_size) <= tol ? "upToDate" : "changed";
}

/** Árbol lateral: discos → carpetas (carga perezosa, solo carpetas). */
export function Sidebar({ onRescan }: { onRescan?: (mount: string, name: string) => void }) {
  const t = useT();
  const disks = useCatalog((s) => s.disks);
  const selectedDiskId = useCatalog((s) => s.selectedDiskId);
  const [volumes, setVolumes] = useState<VolumeInfo[]>([]);
  const [menu, setMenu] = useState<{ x: number; y: number; disk: DiskRow } | null>(null);

  // Volúmenes montados ahora, para comparar tamaño usado vs. lo catalogado.
  // Se refresca cuando cambia la lista de discos (p.ej. tras un escaneo).
  useEffect(() => {
    let alive = true;
    api.listVolumes().then((v) => alive && setVolumes(v)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [disks]);

  if (disks.length === 0) {
    return (
      <div className="p-3 text-xs text-neutral-600">{t("sidebar.emptyHint")}</div>
    );
  }

  return (
    <nav className="select-none py-1 text-sm">
      {disks.map((d) => (
        <TreeNode
          key={d.id}
          diskId={d.id}
          parentId={null}
          label={d.name}
          trail={[{ id: null, name: d.name }]}
          depth={0}
          expandable={d.folder_count > 0}
          isDisk
          online={d.is_online}
          status={diskScanStatus(d, volumes)}
          highlighted={selectedDiskId === d.id}
          onContext={(e) => {
            e.preventDefault();
            setMenu({ x: e.clientX, y: e.clientY, disk: d });
          }}
        />
      ))}
      {menu && (
        <DiskMenu menu={menu} volumes={volumes} onRescan={onRescan} onClose={() => setMenu(null)} />
      )}
    </nav>
  );
}

/** Menú contextual sobre un disco: re-escanear (si está montado) o quitarlo del
 *  catálogo (útil para discos que ya no existen — no toca archivos). */
function DiskMenu({
  menu,
  volumes,
  onRescan,
  onClose,
}: {
  menu: { x: number; y: number; disk: DiskRow };
  volumes: VolumeInfo[];
  onRescan?: (mount: string, name: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const refreshDisks = useCatalog((s) => s.refreshDisks);
  const setError = useCatalog((s) => s.setError);
  const vol = volumes.find((v) => v.name === menu.disk.name);

  useEffect(() => {
    const close = () => onClose();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("click", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  async function remove() {
    onClose();
    const ok = window.confirm(t("sidebar.removeConfirm", { name: menu.disk.name }));
    if (!ok) return;
    try {
      await api.deleteDisk(menu.disk.id);
      // Si estábamos parados en ese disco, limpiar la navegación.
      if (useCatalog.getState().selectedDiskId === menu.disk.id) {
        useCatalog.setState({
          selectedDiskId: null,
          breadcrumb: [],
          contentEntries: [],
          selectedEntryId: null,
          selectedIds: [],
        });
      }
      await refreshDisks();
    } catch (e) {
      setError(String(e));
    }
  }

  const status = diskScanStatus(menu.disk, volumes);
  const stColor =
    status === "changed" ? "bg-amber-500" : status === "upToDate" ? "bg-emerald-500" : "bg-neutral-600";
  const stLabel =
    status === "changed" ? t("table.maybeStale") : status === "upToDate" ? t("table.upToDate") : t("common.offline");

  const left = Math.min(menu.x, window.innerWidth - 240);
  const top = Math.min(menu.y, window.innerHeight - 130);

  return (
    <div
      className="fixed z-[100] min-w-52 overflow-hidden rounded-lg border border-border bg-popover py-1 shadow-pop animate-zoom-in"
      style={{ left, top }}
      onClick={(e) => e.stopPropagation()}
    >
      {/* Estado del disco (color según escaneo) */}
      <div className="flex items-center gap-1.5 border-b border-neutral-800 px-3 py-1.5 text-[11px] text-neutral-500">
        <span className={`h-2 w-2 shrink-0 rounded-full ${stColor}`} />
        <span className="truncate">
          <span className="text-neutral-300">{menu.disk.name}</span> · {stLabel}
        </span>
      </div>
      {/* Re-escanear: siempre visible; deshabilitado si el disco no está montado. */}
      <button
        onClick={() => {
          if (!vol || !onRescan) return;
          onClose();
          onRescan(vol.mount_path, menu.disk.name);
        }}
        disabled={!vol}
        title={!vol ? t("sidebar.rescanOfflineTip") : undefined}
        className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors disabled:cursor-default disabled:opacity-40 ${
          status === "changed" ? "text-amber-300 hover:bg-amber-950/40" : "text-neutral-200 hover:bg-accent"
        }`}
      >
        <ScanLine className="h-3.5 w-3.5" />
        {t("sidebar.rescan")}
      </button>
      <button
        onClick={remove}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-red-300 transition-colors hover:bg-red-950/50"
      >
        <Trash2 className="h-3.5 w-3.5" />
        {t("sidebar.removeFromCatalog")}
      </button>
    </div>
  );
}

interface NodeProps {
  diskId: number;
  parentId: number | null;
  label: string;
  trail: Crumb[];
  depth: number;
  expandable: boolean;
  isDisk?: boolean;
  online?: boolean;
  status?: DiskStatus;
  highlighted?: boolean;
  onContext?: (e: React.MouseEvent) => void;
}

function TreeNode({ diskId, parentId, label, trail, depth, expandable, isDisk, online, status, onContext }: NodeProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [kids, setKids] = useState<EntryRow[] | null>(null);
  const [loading, setLoading] = useState(false);
  const gotoFolder = useCatalog((s) => s.gotoFolder);
  const breadcrumb = useCatalog((s) => s.breadcrumb);
  const mode = useCatalog((s) => s.mode);

  // Resaltar si este nodo es el último crumb del browse actual.
  const last = breadcrumb[breadcrumb.length - 1];
  const isCurrent =
    mode === "browse" &&
    breadcrumb.length === trail.length &&
    last?.id === parentId &&
    last?.name === label;

  async function toggle(e: React.MouseEvent) {
    e.stopPropagation();
    if (!open && kids === null) {
      setLoading(true);
      try {
        const c = await api.listChildren(diskId, parentId);
        setKids(c.filter((x) => x.is_folder));
      } finally {
        setLoading(false);
      }
    }
    setOpen((v) => !v);
  }

  function activate() {
    gotoFolder(diskId, parentId, trail);
  }

  return (
    <div>
      <div
        onClick={activate}
        onContextMenu={onContext}
        className={`group flex cursor-pointer items-center gap-1 rounded-md py-1 pr-2 transition-colors duration-150 ${
          isCurrent
            ? "bg-primary/10 text-foreground shadow-[inset_2px_0_0_0_hsl(var(--primary))]"
            : "text-neutral-300 hover:bg-accent/60"
        }`}
        style={{ paddingLeft: depth * 14 + 4 }}
        title={label}
      >
        <button
          onClick={toggle}
          className={`flex h-4 w-4 shrink-0 items-center justify-center text-neutral-500 ${
            expandable ? "hover:text-neutral-200" : "invisible"
          }`}
        >
          <ChevronRight className={`h-3.5 w-3.5 transition-transform ${open ? "rotate-90" : ""}`} />
        </button>
        {isDisk ? (
          <HardDrive className={`h-4 w-4 shrink-0 ${online ? "text-primary" : "text-neutral-500"}`} />
        ) : (
          <Folder className="h-4 w-4 shrink-0 text-sky-400/80" />
        )}
        <span className="truncate">{label}</span>
        {isDisk && status && (
          <span
            className={`ml-auto mr-0.5 h-2 w-2 shrink-0 rounded-full ${
              status === "changed"
                ? "bg-amber-500"
                : status === "upToDate"
                  ? "bg-emerald-500"
                  : "border border-neutral-500 bg-transparent"
            }`}
            title={
              status === "changed"
                ? t("table.maybeStaleTip")
                : status === "upToDate"
                  ? t("table.upToDateTip")
                  : t("common.offline")
            }
          />
        )}
      </div>

      {open && (
        <div>
          {loading && <div className="py-1 text-[11px] text-neutral-600" style={{ paddingLeft: (depth + 1) * 14 + 22 }}>{t("sidebar.loading")}</div>}
          {kids?.map((c) => (
            <TreeNode
              key={c.id}
              diskId={diskId}
              parentId={c.id}
              label={c.name}
              trail={[...trail, { id: c.id, name: c.name }]}
              depth={depth + 1}
              expandable={c.child_count > 0}
            />
          ))}
          {kids && kids.length === 0 && !loading && (
            <div className="py-1 text-[11px] text-neutral-600" style={{ paddingLeft: (depth + 1) * 14 + 22 }}>
              {t("sidebar.noSubfolders")}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
