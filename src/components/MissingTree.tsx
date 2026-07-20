import { useMemo, useState } from "react";
import { ChevronRight, ChevronDown, Folder, Minus, Check } from "lucide-react";
import type { MissingNode } from "../lib/ipc";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";

/** Nodo del árbol ya jerarquizado (el backend manda una lista plana de carpetas). */
interface TreeNode {
  path: string;
  name: string;
  files: number;
  bytes: number;
  children: TreeNode[];
}

/** Arma la jerarquía desde la lista plana. Las rutas vienen ordenadas, así que
 *  el padre siempre existe antes que el hijo. */
function buildTree(nodes: MissingNode[]): TreeNode[] {
  const byPath = new Map<string, TreeNode>();
  const roots: TreeNode[] = [];
  for (const n of nodes) {
    if (n.rel_path === "") continue; // la raíz se representa con el total, no como fila
    const parts = n.rel_path.split("/");
    const node: TreeNode = {
      path: n.rel_path,
      name: parts[parts.length - 1],
      files: n.files,
      bytes: n.bytes,
      children: [],
    };
    byPath.set(n.rel_path, node);
    const parentPath = parts.slice(0, -1).join("/");
    const parent = parentPath ? byPath.get(parentPath) : undefined;
    if (parent) parent.children.push(node);
    else if (!parentPath) roots.push(node);
    // Si el padre no está en el mapa (no tenía faltantes propios) el nodo queda
    // colgado; no puede pasar porque el backend suma a todos los ancestros.
  }
  return roots;
}

/** Estado del check de una carpeta a partir de la selección de rutas. */
type Check = "on" | "off" | "partial";

function checkOf(node: TreeNode, selected: Set<string>): Check {
  if (selected.has(node.path)) return "on";
  // Si algún descendiente está elegido, es parcial.
  const anyChild = node.children.some((c) => checkOf(c, selected) !== "off");
  return anyChild ? "partial" : "off";
}

function Checkbox({ state, onClick }: { state: Check; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={`flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border ${
        state === "off"
          ? "border-neutral-600"
          : state === "partial"
            ? "border-sky-500 bg-sky-500/30"
            : "border-sky-500 bg-sky-500"
      }`}
    >
      {state === "on" && <Check className="h-2.5 w-2.5 text-white" />}
      {state === "partial" && <Minus className="h-2.5 w-2.5 text-sky-200" />}
    </button>
  );
}

function Row({
  node,
  depth,
  selected,
  toggle,
}: {
  node: TreeNode;
  depth: number;
  selected: Set<string>;
  toggle: (n: TreeNode) => void;
}) {
  // Colapsado por defecto: con miles de carpetas, expandir todo hace la ventana
  // inusable y obliga a scrollear antes de entender nada.
  const [open, setOpen] = useState(false);
  const state = checkOf(node, selected);
  const hasChildren = node.children.length > 0;

  return (
    <>
      <div
        className="flex items-center gap-1.5 px-2 py-1 text-xs hover:bg-neutral-800/60"
        style={{ paddingLeft: 8 + depth * 14 }}
      >
        <button
          onClick={() => hasChildren && setOpen((o) => !o)}
          className={`shrink-0 ${hasChildren ? "text-neutral-500 hover:text-neutral-300" : "invisible"}`}
        >
          {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        </button>
        <Checkbox state={state} onClick={() => toggle(node)} />
        <Folder className="h-3 w-3 shrink-0 text-sky-400/70" />
        <span className="truncate text-neutral-300" title={node.path}>
          {node.name}
        </span>
        <span className="ml-auto shrink-0 whitespace-nowrap text-[11px] text-neutral-500">
          {formatCount(node.files)} · {formatBytes(node.bytes)}
        </span>
      </div>
      {open &&
        node.children.map((c) => (
          <Row key={c.path} node={c} depth={depth + 1} selected={selected} toggle={toggle} />
        ))}
    </>
  );
}

/**
 * Árbol de carpetas con faltantes para elegir qué copiar.
 *
 * La selección se guarda como el conjunto MÍNIMO de rutas que cubre lo elegido:
 * marcar una carpeta guarda esa ruta y borra las de sus descendientes. Así lo
 * que se manda al backend es corto y el filtro por prefijo hace el resto.
 */
export function MissingTree({
  nodes,
  selected,
  onChange,
}: {
  nodes: MissingNode[];
  selected: Set<string>;
  onChange: (s: Set<string>) => void;
}) {
  const t = useT();
  const roots = useMemo(() => buildTree(nodes), [nodes]);
  const total = nodes.find((n) => n.rel_path === "");

  function toggle(node: TreeNode) {
    const next = new Set(selected);
    const isOn = checkOf(node, selected) === "on";
    // Sacar todo lo que cuelgue de esta carpeta (y la carpeta misma).
    for (const p of Array.from(next)) {
      if (p === node.path || p.startsWith(`${node.path}/`)) next.delete(p);
    }
    if (!isOn) {
      // Al marcar, si un ancestro ya estaba elegido hay que "bajarlo": se quita
      // el ancestro y se agregan sus otros hijos, para no perder la selección.
      let anc = node.path;
      while (anc.includes("/")) {
        anc = anc.slice(0, anc.lastIndexOf("/"));
        if (next.has(anc)) {
          next.delete(anc);
          expandAncestor(anc, node.path, roots, next);
        }
      }
      next.add(node.path);
    }
    onChange(next);
  }

  if (roots.length === 0) return null;

  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between text-xs">
        <span className="font-semibold text-neutral-300">{t("compare.pickFolders")}</span>
        <span className="flex gap-2 text-[11px]">
          <button onClick={() => onChange(new Set([""]))} className="text-sky-400 hover:text-sky-300">
            {t("compare.selectAll")}
          </button>
          <button onClick={() => onChange(new Set())} className="text-neutral-400 hover:text-neutral-200">
            {t("compare.selectNone")}
          </button>
        </span>
      </div>
      <div className="max-h-64 overflow-auto rounded border border-border bg-neutral-950/40">
        {/* Fila raíz: seleccionar todo el subárbol comparado. */}
        <div className="flex items-center gap-1.5 border-b border-border/60 px-2 py-1 text-xs">
          <span className="w-3 shrink-0" />
          <Checkbox
            state={selected.has("") ? "on" : selected.size > 0 ? "partial" : "off"}
            onClick={() => onChange(selected.has("") ? new Set() : new Set([""]))}
          />
          <span className="font-medium text-neutral-200">{t("compare.everything")}</span>
          {total && (
            <span className="ml-auto text-[11px] text-neutral-500">
              {formatCount(total.files)} · {formatBytes(total.bytes)}
            </span>
          )}
        </div>
        {roots.map((r) => (
          <Row key={r.path} node={r} depth={0} selected={selected} toggle={toggle} />
        ))}
      </div>
    </div>
  );
}

/** Al desmarcar un ancestro elegido, conserva sus otros hijos como elegidos. */
function expandAncestor(ancestor: string, keepOut: string, roots: TreeNode[], out: Set<string>) {
  const node = findNode(roots, ancestor);
  if (!node) return;
  for (const c of node.children) {
    if (keepOut === c.path || keepOut.startsWith(`${c.path}/`)) continue;
    out.add(c.path);
  }
}

function findNode(nodes: TreeNode[], path: string): TreeNode | null {
  for (const n of nodes) {
    if (n.path === path) return n;
    if (path.startsWith(`${n.path}/`)) {
      const hit = findNode(n.children, path);
      if (hit) return hit;
    }
  }
  return null;
}
