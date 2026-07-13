import type { Dictionary } from "@/i18n/dictionaries";
import { AppWindow } from "@/components/AppWindow";
import { ButtonLink } from "@/components/Button";
import { IconDownload, IconApple, IconWindows } from "@/components/Icons";

export function Hero({ dict }: { dict: Dictionary }) {
  const { hero, shot } = dict;

  return (
    <section className="relative overflow-hidden">
      {/* ambient glow + grid */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-grid [mask-image:radial-gradient(80%_60%_at_50%_0%,#000,transparent)]"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute -top-40 left-1/2 h-[520px] w-[900px] -translate-x-1/2 rounded-full bg-[radial-gradient(closest-side,hsl(var(--teal)/0.16),transparent)] blur-2xl"
      />

      <div className="container-x relative grid items-center gap-14 pb-16 pt-14 lg:grid-cols-[1.02fr_0.98fr] lg:gap-10 lg:pb-24 lg:pt-20">
        <div>
          <p className="animate-fade-up font-mono text-[12px] uppercase tracking-[0.16em] text-teal [animation-delay:60ms]">
            {hero.eyebrow}
          </p>
          <h1 className="mt-5 animate-fade-up text-[clamp(2.1rem,5.2vw,3.75rem)] font-extrabold leading-[1.02] tracking-[-0.03em] [animation-delay:120ms]">
            {hero.titlePre}{" "}
            <span className="text-teal">{hero.titleEm}</span>{" "}
            {hero.titlePost}
          </h1>
          <p className="mt-6 max-w-xl animate-fade-up text-[1.05rem] leading-relaxed text-muted [animation-delay:200ms]">
            {hero.sub}
          </p>

          <div className="mt-8 flex animate-fade-up flex-wrap items-center gap-3 [animation-delay:280ms]">
            <ButtonLink href="#download" size="lg">
              <IconApple className="size-[18px]" />
              {hero.ctaPrimary}
            </ButtonLink>
            <ButtonLink href="#download" variant="outline" size="lg">
              <IconWindows className="size-4" />
              {hero.ctaSecondary}
            </ButtonLink>
          </div>
          <p className="mt-4 animate-fade-up font-mono text-[11.5px] text-faint [animation-delay:340ms]">
            {hero.platforms}
          </p>

          <dl className="mt-10 flex animate-fade-up flex-wrap gap-x-10 gap-y-5 border-t border-line/[0.07] pt-7 [animation-delay:420ms]">
            {hero.stats.map((stat) => (
              <div key={stat.l}>
                <dt className="font-mono text-[1.5rem] font-semibold tracking-tight text-amber">
                  {stat.n}
                </dt>
                <dd className="mt-1 text-[12.5px] text-faint">{stat.l}</dd>
              </div>
            ))}
          </dl>
        </div>

        <div className="animate-fade-up [animation-delay:320ms]">
          <div className="relative">
            <div
              aria-hidden
              className="pointer-events-none absolute -inset-x-6 -bottom-8 top-10 rounded-[24px] bg-[radial-gradient(closest-side,hsl(var(--teal)/0.14),transparent)] blur-2xl"
            />
            <AppWindow shot={shot} className="relative" />
          </div>
        </div>
      </div>
    </section>
  );
}
