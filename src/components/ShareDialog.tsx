import { useEffect, useState } from "react";
import { Share2, Loader2, KeyRound, Smartphone, Ban, RotateCcw, Wifi, ShieldCheck } from "lucide-react";
import { api, type AgentStatus, type DeviceRow } from "../lib/ipc";
import { Modal } from "./StatsDialog";
import { formatDate } from "../lib/format";

export function ShareDialog({ onClose }: { onClose: () => void }) {
  const [status, setStatus] = useState<AgentStatus>({ running: false, addr: null });
  const [busy, setBusy] = useState(false);
  const [code, setCode] = useState<string | null>(null);
  const [devices, setDevices] = useState<DeviceRow[]>([]);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    setStatus(await api.agentStatus());
    try {
      setDevices(await api.agentDevices());
    } catch {
      /* sin catálogo */
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function toggle() {
    setError(null);
    setBusy(true);
    try {
      if (status.running) {
        await api.agentStop();
        setCode(null);
      } else {
        await api.agentStart();
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function genCode() {
    setError(null);
    try {
      setCode(await api.agentPairCode());
    } catch (e) {
      setError(String(e));
    }
  }

  async function revoke(d: DeviceRow) {
    await api.agentRevoke(d.id, !d.revoked);
    await refresh();
  }

  return (
    <Modal onClose={onClose} title="Compartir (conector seguro)" icon={<Share2 className="h-4 w-4 text-sky-400" />}>
      {error && <div className="mb-3 rounded border border-red-900 bg-red-950/50 px-3 py-2 text-xs text-red-300">{error}</div>}

      {/* Estado / toggle */}
      <div className="flex items-center gap-3 rounded-lg border border-neutral-800 bg-neutral-900/50 p-3">
        <Wifi className={`h-5 w-5 ${status.running ? "text-emerald-400" : "text-neutral-600"}`} />
        <div className="flex-1">
          <div className="text-sm font-medium">
            {status.running ? "Conector activo" : "Conector apagado"}
          </div>
          <div className="font-mono text-[11px] text-neutral-500">
            {status.running ? `escuchando en ${status.addr}` : "read-only · autenticado por dispositivo"}
          </div>
        </div>
        <button
          onClick={toggle}
          disabled={busy}
          className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium text-white disabled:opacity-50 ${
            status.running ? "bg-neutral-700 hover:bg-neutral-600" : "bg-sky-600 hover:bg-sky-500"
          }`}
        >
          {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
          {status.running ? "Apagar" : "Activar"}
        </button>
      </div>

      {status.running && (
        <div className="mt-3 flex items-center gap-3 rounded-lg border border-neutral-800 p-3">
          <KeyRound className="h-5 w-5 text-amber-400" />
          <div className="flex-1">
            <div className="text-sm">Emparejar un dispositivo</div>
            <div className="text-[11px] text-neutral-500">Ingresá este código en el otro dispositivo (vale 5 min, un solo uso).</div>
          </div>
          {code ? (
            <span className="rounded bg-neutral-800 px-3 py-1.5 font-mono text-lg tracking-[0.3em] text-emerald-300">{code}</span>
          ) : (
            <button onClick={genCode} className="rounded-md border border-neutral-700 px-3 py-1.5 text-xs hover:bg-neutral-800">
              Generar código
            </button>
          )}
        </div>
      )}

      {/* Dispositivos */}
      <div className="mt-4">
        <h3 className="mb-2 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wide text-neutral-500">
          <Smartphone className="h-3.5 w-3.5" /> Dispositivos enrolados
        </h3>
        {devices.length === 0 ? (
          <p className="py-3 text-center text-xs text-neutral-600">Todavía no hay dispositivos.</p>
        ) : (
          <div className="space-y-1">
            {devices.map((d) => (
              <div key={d.id} className="flex items-center gap-2 rounded border border-neutral-800 px-2 py-1.5 text-xs">
                <span className={`h-2 w-2 shrink-0 rounded-full ${d.revoked ? "bg-red-500" : "bg-emerald-500"}`} />
                <div className="min-w-0 flex-1">
                  <div className="truncate">{d.name} <span className="font-mono text-neutral-600">{d.id.slice(0, 8)}</span></div>
                  <div className="text-[10px] text-neutral-500">
                    scopes: {d.scopes} · visto {d.last_seen ? formatDate(d.last_seen) : "—"}
                  </div>
                </div>
                <button
                  onClick={() => revoke(d)}
                  className="inline-flex items-center gap-1 rounded px-2 py-1 text-[11px] hover:bg-neutral-800"
                  title={d.revoked ? "Re-habilitar" : "Revocar"}
                >
                  {d.revoked ? <RotateCcw className="h-3.5 w-3.5 text-emerald-400" /> : <Ban className="h-3.5 w-3.5 text-red-400" />}
                  {d.revoked ? "Re-habilitar" : "Revocar"}
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="mt-4 flex gap-2 rounded-lg border border-neutral-800 bg-neutral-900/40 p-3 text-[11px] text-neutral-500">
        <ShieldCheck className="h-4 w-4 shrink-0 text-emerald-400" />
        <p>
          Solo lectura. Sirve únicamente archivos del catálogo cuyo volumen esté montado y verificado por fingerprint;
          rechaza rutas fuera del volumen. Por defecto escucha en loopback — para acceso remoto, ligalo a una malla
          privada (Tailscale/WireGuard) o un túnel TLS. Toda descarga queda en el log de auditoría.
        </p>
      </div>
    </Modal>
  );
}
