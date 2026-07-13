import type { Dictionary } from "@/i18n/dictionaries";
import { cn } from "@/lib/cn";
import { SectionHeading } from "@/components/SectionHeading";
import { Reveal } from "@/components/Reveal";
import { IconShield, IconCheck } from "@/components/Icons";

const stateStyles: Record<string, string> = {
  done: "border-online/25 bg-online/10 text-online",
  progress: "border-amber/25 bg-amber/10 text-amber",
  planned: "border-line/12 bg-line/[0.04] text-faint",
};

export function Roadmap({ dict }: { dict: Dictionary }) {
  const { roadmap } = dict;

  return (
    <section
      id="roadmap"
      className="scroll-mt-24 border-t border-line/[0.07] bg-surface/30 py-20 lg:py-28"
    >
      <div className="container-x">
        <SectionHeading
          eyebrow={roadmap.eyebrow}
          title={roadmap.title}
          subtitle={roadmap.subtitle}
        />

        <div className="mt-14 grid gap-6 lg:grid-cols-[0.9fr_1.1fr] lg:items-start">
          {/* highlighted connector */}
          <Reveal className="relative overflow-hidden rounded-xl border border-teal/20 bg-teal/[0.05] p-8 shadow-panel">
            <div
              aria-hidden
              className="pointer-events-none absolute -right-16 -top-16 size-48 rounded-full bg-[radial-gradient(closest-side,hsl(var(--teal)/0.2),transparent)] blur-xl"
            />
            <span className="relative grid size-12 place-items-center rounded-lg border border-teal/25 bg-teal/10 text-teal">
              <IconShield className="size-6" />
            </span>
            <span className="relative mt-5 inline-block rounded-full border border-amber/25 bg-amber/10 px-2.5 py-1 font-mono text-[10px] uppercase tracking-wide text-amber">
              {roadmap.stateLabels.planned}
            </span>
            <h3 className="relative mt-4 text-[1.3rem] font-semibold tracking-[-0.01em]">
              {roadmap.connectorTitle}
            </h3>
            <p className="relative mt-3 text-[0.98rem] leading-relaxed text-muted">
              {roadmap.connectorBody}
            </p>
          </Reveal>

          {/* checklist */}
          <Reveal delay={80}>
            <ul className="divide-y divide-line/[0.07] overflow-hidden rounded-xl border border-line/[0.08]">
              {roadmap.items.map((item) => (
                <li
                  key={item.label}
                  className="flex items-center justify-between gap-4 px-5 py-4"
                >
                  <span className="flex items-center gap-3 text-[0.95rem]">
                    {item.state === "done" ? (
                      <IconCheck className="size-4 text-online" />
                    ) : (
                      <span
                        className={cn(
                          "size-2 rounded-full",
                          item.state === "progress" ? "bg-amber" : "bg-faint/40",
                        )}
                      />
                    )}
                    <span
                      className={cn(
                        item.state === "planned" ? "text-muted" : "text-fg",
                      )}
                    >
                      {item.label}
                    </span>
                  </span>
                  <span
                    className={cn(
                      "shrink-0 rounded-full border px-2.5 py-0.5 font-mono text-[10px] uppercase tracking-wide",
                      stateStyles[item.state],
                    )}
                  >
                    {roadmap.stateLabels[item.state as keyof typeof roadmap.stateLabels]}
                  </span>
                </li>
              ))}
            </ul>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
