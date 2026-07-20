import { cn } from "@/lib/cn";

export function LogoMark({
  size = 26,
  className,
}: {
  size?: number;
  className?: string;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 40 40"
      fill="none"
      className={className}
      aria-hidden
    >
      {/* teal ring (bottom-left) */}
      <circle cx="15" cy="25" r="9" stroke="#34cfe3" strokeWidth="5.4" />
      <circle cx="15" cy="25" r="3.4" fill="#34cfe3" />
      {/* amber ring (top-right) — drawn on top for the interlock */}
      <circle cx="25" cy="15" r="9" stroke="#ffb627" strokeWidth="5.4" />
      <circle cx="25" cy="15" r="3.4" fill="#ffb627" />
    </svg>
  );
}

export function Logo({
  size = 26,
  withWordmark = true,
  className,
}: {
  size?: number;
  withWordmark?: boolean;
  className?: string;
}) {
  return (
    <span className={cn("inline-flex items-center gap-2.5", className)}>
      <LogoMark size={size} />
      {withWordmark && (
        <span className="text-[1.05rem] font-semibold tracking-tight text-fg">
          DiskDex
        </span>
      )}
    </span>
  );
}
