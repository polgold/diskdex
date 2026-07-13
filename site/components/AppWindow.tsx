import type { Dictionary } from "@/i18n/dictionaries";
import { cn } from "@/lib/cn";
import { IconSearch } from "@/components/Icons";

type Shot = Dictionary["shot"];

function StateDot({ state }: { state: string }) {
  return (
    <span
      className={cn(
        "inline-block size-[7px] shrink-0 rounded-full",
        state === "online"
          ? "bg-online shadow-[0_0_8px_hsl(var(--online)/0.7)]"
          : "bg-faint/50",
      )}
    />
  );
}

export function AppWindow({
  shot,
  className,
}: {
  shot: Shot;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "overflow-hidden rounded-xl border border-line/10 bg-surface shadow-pop",
        className,
      )}
    >
      {/* title bar */}
      <div className="flex h-10 items-center gap-2 border-b border-line/[0.07] bg-surface-2/60 px-4">
        <span className="size-3 rounded-full bg-[#ff5f57]/90" />
        <span className="size-3 rounded-full bg-[#febc2e]/90" />
        <span className="size-3 rounded-full bg-[#28c840]/90" />
        <span className="mx-auto font-mono text-[11px] text-faint">
          catalog.dccat — DiskDex
        </span>
      </div>

      {/* toolbar */}
      <div className="flex items-center gap-3 border-b border-line/[0.07] px-4 py-3">
        <div className="flex h-9 flex-1 items-center gap-2.5 rounded-md border border-line/10 bg-bg/60 px-3">
          <IconSearch className="size-4 text-teal" />
          <span className="font-mono text-[13px] text-fg">
            {shot.query}
            <span className="ml-0.5 inline-block h-[15px] w-[7px] translate-y-[2px] animate-caret-blink bg-teal align-middle" />
          </span>
          <span className="ml-auto font-mono text-[10.5px] text-faint">
            {shot.resultsMeta}
          </span>
        </div>
        <span className="hidden rounded-md border border-teal/25 bg-teal/10 px-2.5 py-1.5 font-mono text-[11px] font-medium text-teal sm:inline">
          {shot.results}
        </span>
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)] sm:grid-cols-[188px_minmax(0,1fr)]">
        {/* sidebar */}
        <aside className="hidden flex-col gap-0.5 border-r border-line/[0.07] p-3 sm:flex">
          <div className="px-2 pb-2 font-mono text-[10px] tracking-[0.14em] text-faint">
            {shot.sidebarTitle}
          </div>
          {shot.disks.map((disk) => (
            <div
              key={disk.name}
              className={cn(
                "flex items-center gap-2.5 rounded-md px-2 py-1.5",
                disk.state === "online" && "bg-line/[0.04]",
              )}
            >
              <StateDot state={disk.state} />
              <div className="min-w-0">
                <div className="truncate text-[12.5px] font-medium text-fg">
                  {disk.name}
                </div>
                <div className="truncate font-mono text-[10px] text-faint">
                  {disk.meta}
                </div>
              </div>
            </div>
          ))}
        </aside>

        {/* results table */}
        <div className="min-w-0">
          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-line/[0.06] px-4 py-2 font-mono text-[10px] tracking-wide text-faint">
            <span>NAME · PATH</span>
            <span>SIZE</span>
          </div>
          {shot.rows.map((row, i) => (
            <div
              key={row.name}
              className={cn(
                "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-4 py-2.5",
                i === 0 && "bg-teal/[0.06]",
                i !== 0 && "border-t border-line/[0.05]",
              )}
            >
              <div className="flex min-w-0 items-center gap-3">
                <span className="grid size-7 shrink-0 place-items-center rounded-[6px] bg-teal/10 font-mono text-[9px] text-teal">
                  MOV
                </span>
                <div className="min-w-0">
                  <div className="truncate text-[13px] font-medium text-fg">
                    {row.name}
                  </div>
                  <div className="truncate font-mono text-[10.5px] text-faint">
                    {row.path}
                  </div>
                </div>
              </div>
              <div className="flex items-center gap-3">
                <span className="hidden font-mono text-[11px] text-muted sm:inline">
                  {row.size}
                </span>
                <span
                  className={cn(
                    "flex items-center gap-1.5 rounded-full border px-2 py-0.5 font-mono text-[10px]",
                    row.state === "online"
                      ? "border-online/25 text-online"
                      : "border-line/10 text-faint",
                  )}
                >
                  <StateDot state={row.state} />
                  {row.state === "online" ? shot.online : shot.offline}
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
