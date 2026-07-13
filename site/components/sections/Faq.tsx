import type { Dictionary } from "@/i18n/dictionaries";
import { SectionHeading } from "@/components/SectionHeading";
import { Reveal } from "@/components/Reveal";
import { IconChevron } from "@/components/Icons";

export function Faq({ dict }: { dict: Dictionary }) {
  const { faq } = dict;

  return (
    <section className="border-t border-line/[0.07] py-20 lg:py-28">
      <div className="container-x">
        <SectionHeading eyebrow={faq.eyebrow} title={faq.title} />

        <Reveal className="mx-auto mt-12 max-w-3xl divide-y divide-line/[0.08] border-y border-line/[0.08]">
          {faq.items.map((item) => (
            <details key={item.q} className="group">
              <summary className="flex cursor-pointer list-none items-center justify-between gap-4 py-5 text-left text-[1.02rem] font-medium text-fg transition-colors hover:text-teal [&::-webkit-details-marker]:hidden">
                {item.q}
                <IconChevron className="size-4 shrink-0 text-faint transition-transform duration-300 group-open:rotate-180" />
              </summary>
              <p className="-mt-1 max-w-2xl pb-5 text-[0.95rem] leading-relaxed text-muted">
                {item.a}
              </p>
            </details>
          ))}
        </Reveal>
      </div>
    </section>
  );
}
