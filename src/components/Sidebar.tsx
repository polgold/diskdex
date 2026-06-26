import { useState } from "react";
import { ChevronRight, Folder, HardDrive } from "lucide-react";
import { api, type EntryRow } from "../lib/ipc";
import { useCatalog, type Crumb } from "../store/catalog";

/** Árbol lateral: discos → carpetas (carga perezosa, solo carpetas). */
export function Sidebar() {
  const disks = useCatalog((s) => s.disks);
  const selectedDiskId = useCatalog((s) => s.selectedDiskId);

  if (disks.length === 0) {
    return (
      <div className="p-3 text-xs text-neutral-600">Importá o escaneá un disco para empezar.</div>
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
          highlighted={selectedDiskId === d.id}
        />
      ))}
    </nav>
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
  highlighted?: boolean;
}

function TreeNode({ diskId, parentId, label, trail, depth, expandable, isDisk, online }: NodeProps) {
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
        className={`group flex cursor-pointer items-center gap-1 rounded py-1 pr-2 hover:bg-neutral-800/60 ${
          isCurrent ? "bg-neutral-800 text-white" : "text-neutral-300"
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
          <HardDrive className={`h-4 w-4 shrink-0 ${online ? "text-emerald-400" : "text-neutral-500"}`} />
        ) : (
          <Folder className="h-4 w-4 shrink-0 text-sky-400/80" />
        )}
        <span className="truncate">{label}</span>
      </div>

      {open && (
        <div>
          {loading && <div className="py-1 text-[11px] text-neutral-600" style={{ paddingLeft: (depth + 1) * 14 + 22 }}>cargando…</div>}
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
              (sin subcarpetas)
            </div>
          )}
        </div>
      )}
    </div>
  );
}
