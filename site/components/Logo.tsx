import { cn } from "@/lib/cn";

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
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src="/logo.png"
        alt=""
        width={size}
        height={size}
        className="select-none"
        style={{ width: size, height: size }}
        draggable={false}
      />
      {withWordmark && (
        <span className="text-[1.05rem] font-semibold tracking-tight text-fg">
          DiskDex
        </span>
      )}
    </span>
  );
}
