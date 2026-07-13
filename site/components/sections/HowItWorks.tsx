import type { Dictionary } from "@/i18n/dictionaries";
import { SectionHeading } from "@/components/SectionHeading";
import { Reveal } from "@/components/Reveal";

export function HowItWorks({ dict }: { dict: Dictionary }) {
  const { how } = dict;

  return (
    <section id="how" className="scroll-mt-24 border-t border-line/[0.07] bg-surface/30 py-20 lg:py-28">
      <div className="container-x">
        <SectionHeading eyebrow={how.eyebrow} title={how.title} />

        <ol className="mt-14 grid gap-8 md:grid-cols-3">
          {how.steps.map((step, i) => (
            <Reveal as="li" key={step.n} delay={i * 90} className="relative">
              <div className="flex items-center gap-4">
                <span className="font-mono text-[2rem] font-semibold text-teal">
                  {step.n}
                </span>
                <span className="h-px flex-1 bg-gradient-to-r from-line/20 to-transparent" />
              </div>
              <h3 className="mt-5 text-[1.15rem] font-semibold tracking-[-0.01em]">
                {step.title}
              </h3>
              <p className="mt-2.5 text-[0.95rem] leading-relaxed text-muted">
                {step.body}
              </p>
            </Reveal>
          ))}
        </ol>
      </div>
    </section>
  );
}
