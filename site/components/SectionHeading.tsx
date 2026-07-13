import { cn } from "@/lib/cn";
import { Reveal } from "@/components/Reveal";

export function SectionHeading({
  eyebrow,
  title,
  subtitle,
  align = "left",
  className,
}: {
  eyebrow: string;
  title: string;
  subtitle?: string;
  align?: "left" | "center";
  className?: string;
}) {
  return (
    <Reveal
      className={cn(
        "max-w-2xl",
        align === "center" && "mx-auto text-center",
        className,
      )}
    >
      <p className="font-mono text-[12px] uppercase tracking-[0.14em] text-teal">
        {eyebrow}
      </p>
      <h2 className="mt-3 text-[clamp(1.7rem,3.4vw,2.4rem)] font-bold leading-[1.08] tracking-[-0.02em]">
        {title}
      </h2>
      {subtitle && (
        <p className="mt-4 text-[1.02rem] leading-relaxed text-muted">
          {subtitle}
        </p>
      )}
    </Reveal>
  );
}
