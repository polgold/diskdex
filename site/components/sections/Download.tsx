import type { Dictionary } from "@/i18n/dictionaries";
import { site } from "@/lib/site";
import { Reveal } from "@/components/Reveal";
import { ButtonLink } from "@/components/Button";
import { IconApple, IconWindows, IconGithub } from "@/components/Icons";

export function Download({ dict }: { dict: Dictionary }) {
  const { download } = dict;
  const { mac, win } = site.downloads;

  const platforms = [
    { key: "mac", ...mac, Icon: IconApple, label: download.mac, meta: download.macMeta },
    { key: "win", ...win, Icon: IconWindows, label: download.win, meta: download.winMeta },
  ];
  const someSoon = platforms.some((p) => !p.available);

  return (
    <section id="download" className="scroll-mt-24 py-20 lg:py-28">
      <div className="container-x">
        <Reveal className="relative overflow-hidden rounded-2xl border border-line/[0.08] bg-surface px-6 py-14 text-center shadow-panel sm:px-14">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-x-0 -top-24 mx-auto h-64 w-[720px] rounded-full bg-[radial-gradient(closest-side,hsl(var(--teal)/0.14),transparent)] blur-2xl"
          />
          <p className="relative font-mono text-[12px] uppercase tracking-[0.14em] text-teal">
            {download.eyebrow}
          </p>
          <h2 className="relative mx-auto mt-3 max-w-2xl text-[clamp(1.8rem,3.6vw,2.6rem)] font-bold leading-[1.06] tracking-[-0.02em]">
            {download.title}
          </h2>
          <p className="relative mx-auto mt-4 max-w-xl text-[1.02rem] leading-relaxed text-muted">
            {download.sub}
          </p>

          <div className="relative mx-auto mt-9 flex max-w-xl flex-col gap-3 sm:flex-row sm:justify-center">
            {platforms.map(({ key, href, available, Icon, label, meta }) => (
              <div key={key} className="flex-1">
                <ButtonLink
                  href={href}
                  size="lg"
                  variant={key === "mac" ? "primary" : "outline"}
                  className="w-full"
                >
                  <Icon className="size-[18px]" />
                  {label}
                </ButtonLink>
                <p className="mt-2 font-mono text-[11px] text-faint">
                  {available ? meta : `${meta} · ${download.soon}`}
                </p>
              </div>
            ))}
          </div>

          {someSoon && (
            <p className="relative mx-auto mt-7 max-w-md text-[0.9rem] leading-relaxed text-muted">
              {download.soonNote}
            </p>
          )}

          <div className="relative mt-8 flex flex-col items-center gap-3">
            <ButtonLink href={site.repo} variant="ghost" size="md">
              <IconGithub className="size-[18px]" />
              {download.repo}
            </ButtonLink>
            <p className="font-mono text-[11px] text-faint">{download.note}</p>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
