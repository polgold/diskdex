import type { Dictionary } from "@/i18n/dictionaries";
import { SectionHeading } from "@/components/SectionHeading";
import { Reveal } from "@/components/Reveal";
import { featureIcons } from "@/components/Icons";

export function Features({ dict }: { dict: Dictionary }) {
  const { features } = dict;

  return (
    <section id="features" className="scroll-mt-24 py-20 lg:py-28">
      <div className="container-x">
        <SectionHeading
          eyebrow={features.eyebrow}
          title={features.title}
          subtitle={features.subtitle}
        />

        <div className="mt-14 grid gap-px overflow-hidden rounded-xl border border-line/[0.08] bg-line/[0.06] sm:grid-cols-2 lg:grid-cols-3">
          {features.items.map((item, i) => {
            const Icon = featureIcons[item.key as keyof typeof featureIcons];
            return (
              <Reveal
                key={item.key}
                delay={(i % 3) * 70}
                className="group relative bg-bg p-7 transition-colors hover:bg-surface"
              >
                <span className="grid size-11 place-items-center rounded-lg border border-teal/20 bg-teal/[0.08] text-teal transition-transform duration-300 group-hover:-translate-y-0.5">
                  {Icon ? <Icon className="size-[22px]" /> : null}
                </span>
                <h3 className="mt-5 text-[1.05rem] font-semibold tracking-[-0.01em]">
                  {item.title}
                </h3>
                <p className="mt-2.5 text-[0.92rem] leading-relaxed text-muted">
                  {item.body}
                </p>
              </Reveal>
            );
          })}
        </div>
      </div>
    </section>
  );
}
