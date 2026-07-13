import type { Dictionary } from "@/i18n/dictionaries";
import { Reveal } from "@/components/Reveal";

export function Trust({ dict }: { dict: Dictionary }) {
  const { trust } = dict;
  return (
    <section className="border-y border-line/[0.07] bg-surface/40">
      <div className="container-x py-8">
        <Reveal className="flex flex-col items-start gap-2 sm:flex-row sm:items-center sm:gap-3">
          <span className="font-mono text-[11px] uppercase tracking-[0.12em] text-faint">
            {trust.line}
          </span>
          <p className="text-[0.98rem] text-muted">
            <span className="font-semibold text-fg">{trust.highlight}</span>{" "}
            {trust.tail}
          </p>
        </Reveal>
      </div>
    </section>
  );
}
