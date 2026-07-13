import type { Dictionary } from "@/i18n/dictionaries";
import { SectionHeading } from "@/components/SectionHeading";
import { Reveal } from "@/components/Reveal";
import { AppWindow } from "@/components/AppWindow";

export function Screenshots({ dict }: { dict: Dictionary }) {
  const { screenshots, shot } = dict;
  const captions = [
    screenshots.captions.main,
    screenshots.captions.search,
    screenshots.captions.inspector,
  ];

  return (
    <section id="screenshots" className="scroll-mt-24 py-20 lg:py-28">
      <div className="container-x">
        <SectionHeading
          eyebrow={screenshots.eyebrow}
          title={screenshots.title}
          subtitle={screenshots.subtitle}
        />

        <Reveal className="relative mt-14">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-x-8 -top-6 bottom-0 rounded-[28px] bg-[radial-gradient(60%_60%_at_50%_0%,hsl(var(--teal)/0.12),transparent)] blur-2xl"
          />
          <div className="relative mx-auto max-w-4xl">
            <AppWindow shot={shot} />
          </div>
        </Reveal>

        <ul className="mx-auto mt-10 grid max-w-4xl gap-4 sm:grid-cols-3">
          {captions.map((caption, i) => (
            <Reveal as="li" key={caption} delay={i * 70} className="flex gap-3">
              <span className="mt-1.5 size-1.5 shrink-0 rounded-full bg-teal" />
              <span className="text-[0.9rem] leading-relaxed text-muted">
                {caption}
              </span>
            </Reveal>
          ))}
        </ul>
      </div>
    </section>
  );
}
