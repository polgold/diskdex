"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import type { Locale } from "@/i18n/config";
import type { Dictionary } from "@/i18n/dictionaries";
import { cn } from "@/lib/cn";
import { Logo } from "@/components/Logo";
import { LanguageSwitcher } from "@/components/LanguageSwitcher";
import { ButtonLink } from "@/components/Button";
import { IconDownload, IconMenu, IconClose } from "@/components/Icons";

export function Header({
  locale,
  nav,
}: {
  locale: Locale;
  nav: Dictionary["nav"];
}) {
  const [scrolled, setScrolled] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    document.body.style.overflow = open ? "hidden" : "";
    return () => {
      document.body.style.overflow = "";
    };
  }, [open]);

  const links = [
    { href: "#features", label: nav.features },
    { href: "#how", label: nav.how },
    { href: "#screenshots", label: nav.screenshots },
    { href: "#roadmap", label: nav.roadmap },
  ];

  return (
    <header
      className={cn(
        "sticky top-0 z-50 transition-colors duration-300",
        scrolled
          ? "border-b border-line/[0.07] bg-bg/80 backdrop-blur-xl"
          : "border-b border-transparent",
      )}
    >
      <div className="container-x flex h-16 items-center justify-between gap-4">
        <Link href={`/${locale}`} aria-label="DiskDex" className="shrink-0">
          <Logo />
        </Link>

        <nav className="hidden items-center gap-1 md:flex">
          {links.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="rounded-md px-3 py-2 text-[0.9rem] text-muted transition-colors hover:text-fg"
            >
              {link.label}
            </a>
          ))}
        </nav>

        <div className="flex items-center gap-2.5">
          <LanguageSwitcher current={locale} />
          <ButtonLink href="#download" className="hidden sm:inline-flex">
            <IconDownload className="size-4" />
            {nav.cta}
          </ButtonLink>
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-label={nav.menu}
            aria-expanded={open}
            className="grid size-10 place-items-center rounded-md border border-line/10 text-fg md:hidden"
          >
            {open ? <IconClose className="size-5" /> : <IconMenu className="size-5" />}
          </button>
        </div>
      </div>

      {/* mobile menu */}
      {open && (
        <div className="border-t border-line/[0.07] bg-bg/95 backdrop-blur-xl md:hidden">
          <nav className="container-x flex flex-col py-3">
            {links.map((link) => (
              <a
                key={link.href}
                href={link.href}
                onClick={() => setOpen(false)}
                className="rounded-md px-2 py-3 text-[0.95rem] text-muted hover:text-fg"
              >
                {link.label}
              </a>
            ))}
            <ButtonLink
              href="#download"
              size="lg"
              className="mt-2"
              onClick={() => setOpen(false)}
            >
              <IconDownload className="size-4" />
              {nav.download}
            </ButtonLink>
          </nav>
        </div>
      )}
    </header>
  );
}
