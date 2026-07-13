import type { Locale } from "@/i18n/config";
import type { Dictionary } from "@/i18n/dictionaries";
import { site } from "@/lib/site";
import { Logo } from "@/components/Logo";

export function Footer({
  dict,
}: {
  locale: Locale;
  dict: Dictionary;
}) {
  const { footer } = dict;
  const year = 2026;

  const product = [
    { label: footer.links.features, href: "#features" },
    { label: footer.links.download, href: "#download" },
    { label: footer.links.roadmap, href: "#roadmap" },
  ];
  const resources = [
    { label: footer.links.github, href: site.repo, external: true },
    { label: footer.links.changelog, href: site.releases, external: true },
  ];

  return (
    <footer className="border-t border-line/[0.07] bg-surface/40">
      <div className="container-x grid gap-10 py-14 sm:grid-cols-2 lg:grid-cols-[1.6fr_1fr_1fr]">
        <div className="max-w-xs">
          <Logo />
          <p className="mt-4 text-[0.9rem] leading-relaxed text-muted">
            {footer.tagline}
          </p>
          <p className="mt-3 font-mono text-[11px] text-faint">{footer.made}</p>
        </div>

        <div>
          <h4 className="font-mono text-[11px] uppercase tracking-[0.12em] text-faint">
            {footer.product}
          </h4>
          <ul className="mt-4 space-y-2.5">
            {product.map((link) => (
              <li key={link.label}>
                <a
                  href={link.href}
                  className="text-[0.9rem] text-muted transition-colors hover:text-fg"
                >
                  {link.label}
                </a>
              </li>
            ))}
          </ul>
        </div>

        <div>
          <h4 className="font-mono text-[11px] uppercase tracking-[0.12em] text-faint">
            {footer.resources}
          </h4>
          <ul className="mt-4 space-y-2.5">
            {resources.map((link) => (
              <li key={link.label}>
                <a
                  href={link.href}
                  target="_blank"
                  rel="noreferrer noopener"
                  className="text-[0.9rem] text-muted transition-colors hover:text-fg"
                >
                  {link.label}
                </a>
              </li>
            ))}
          </ul>
        </div>
      </div>

      <div className="border-t border-line/[0.06]">
        <div className="container-x flex flex-col items-start justify-between gap-2 py-6 sm:flex-row sm:items-center">
          <p className="font-mono text-[11px] text-faint">
            © {year} {site.name}. {footer.rights}
          </p>
          <p className="font-mono text-[11px] text-faint">{site.domain}</p>
        </div>
      </div>
    </footer>
  );
}
