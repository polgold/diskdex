// Acciones sobre el ítem (M6): revelar/abrir el original y copiar rutas.
// La resolución de la ruta real vive en el backend (verifica disco montado);
// acá solo abrimos/revelamos con el plugin opener o copiamos al portapapeles.
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "./ipc";

/** Revela el original en Finder/Explorer (requiere el disco montado). */
export async function revealOriginal(entryId: number): Promise<void> {
  const real = await api.resolveFsPath(entryId);
  await revealItemInDir(real);
}

/** Abre el original con la app por defecto del sistema. */
export async function openOriginal(entryId: number): Promise<void> {
  const real = await api.resolveFsPath(entryId);
  await openPath(real);
}

/** Copia texto al portapapeles. */
export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}
