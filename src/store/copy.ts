import { create } from "zustand";
import { api, onCopyProgress, type CopyProgress, type CopySummary } from "../lib/ipc";
import { notifyTaskDone } from "../lib/notify";

/** Lo que hace falta para lanzar (y cancelar) una copia de respaldo. */
export interface CopyRequest {
  srcDiskId: number;
  dstDiskId: number;
  srcRootId: number | null;
  dstRootId: number | null;
  deep: boolean;
  includeMismatch: boolean;
  /** "SFBACKUP7/PLANTA&CANTA → SFBACKUP8/CLIENTES" — para la barra de progreso. */
  label: string;
  /** Ítems planificados: permite mostrar cuántos faltan sin re-comparar. */
  planned: number;
}

interface CopyState {
  /** La copia en curso, o null. Solo una a la vez (una por disco destino sería
   *  posible, pero copiar a dos discos en paralelo compite por I/O y no ayuda). */
  running: CopyRequest | null;
  progress: CopyProgress | null;
  /** Resultado de la última copia terminada, para mostrarlo al reabrir el diálogo. */
  lastSummary: (CopySummary & { label: string }) | null;
  error: string | null;
  start: (req: CopyRequest) => Promise<void>;
  cancel: () => void;
  clearSummary: () => void;
}

/** Suscripción única al evento de progreso: vive fuera del componente para que
 *  cerrar el diálogo no corte el seguimiento de la copia. */
let unlisten: (() => void) | null = null;

export const useCopy = create<CopyState>((set, get) => ({
  running: null,
  progress: null,
  lastSummary: null,
  error: null,

  start: async (req) => {
    if (get().running) return; // ya hay una copia en curso
    set({ running: req, progress: null, lastSummary: null, error: null });

    if (!unlisten) {
      unlisten = await onCopyProgress((p) => set({ progress: p }));
    }

    try {
      const s = await api.copyMissing(
        req.srcDiskId,
        req.dstDiskId,
        req.srcRootId,
        req.dstRootId,
        req.deep,
        req.includeMismatch,
      );
      set({ lastSummary: { ...s, label: req.label } });
      notifyTaskDone(s, req.label);
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ running: null, progress: null });
    }
  },

  // La cancelación es por disco destino (mismo mecanismo que gather), así que
  // basta con el id: el backend corta antes del próximo archivo.
  cancel: () => {
    const r = get().running;
    if (r) api.cancelCopy(r.dstDiskId).catch(() => {});
  },

  clearSummary: () => set({ lastSummary: null, error: null }),
}));
