import { useEffect, useRef, useState } from "react";
import { Search, X } from "lucide-react";
import { useCatalog } from "../store/catalog";

/** Buscador global (M3) con debounce y atajo ⌘/Ctrl+F. */
export function SearchBar() {
  const [value, setValue] = useState("");
  const runSearch = useCatalog((s) => s.runSearch);
  const clearSearch = useCatalog((s) => s.clearSearch);
  const inputRef = useRef<HTMLInputElement>(null);

  // Debounce de 180 ms: búsqueda incremental sin saturar el backend.
  useEffect(() => {
    const t = setTimeout(() => runSearch(value), 180);
    return () => clearTimeout(t);
  }, [value, runSearch]);

  // ⌘/Ctrl+F enfoca el buscador.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
      if (e.key === "Escape" && document.activeElement === inputRef.current) {
        setValue("");
        clearSearch();
        inputRef.current?.blur();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [clearSearch]);

  return (
    <div className="relative w-full max-w-md">
      <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-neutral-500" />
      <input
        ref={inputRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder="Buscar…  ej: render tag:boda ext:mov size>1gb after:2023-01-01  (⌘F)"
        title="Tokens: tag:boda,4k · ext:mov,mp4 · size>1gb · size<500mb · after:2023-01-01 · before:2024-06-01 · type:file|folder"
        className="w-full rounded-md border border-neutral-700 bg-neutral-900 py-1.5 pl-8 pr-8 text-xs text-neutral-200 placeholder:text-neutral-600 focus:border-neutral-500 focus:outline-none"
      />
      {value && (
        <button
          onClick={() => {
            setValue("");
            clearSearch();
          }}
          className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-0.5 text-neutral-500 hover:text-neutral-200"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}
