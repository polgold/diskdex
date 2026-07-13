"use client";

import { usePathname, useRouter } from "next/navigation";
import { locales, type Locale } from "@/i18n/config";
import { cn } from "@/lib/cn";

export function LanguageSwitcher({ current }: { current: Locale }) {
  const pathname = usePathname();
  const router = useRouter();

  function switchTo(locale: Locale) {
    if (locale === current) return;
    const segments = pathname.split("/");
    // segments[0] === "" , segments[1] === locale
    segments[1] = locale;
    const next = segments.join("/") || `/${locale}`;
    document.cookie = `NEXT_LOCALE=${locale}; path=/; max-age=31536000; samesite=lax`;
    router.push(next);
  }

  return (
    <div
      className="flex items-center rounded-md border border-line/10 p-0.5"
      role="group"
      aria-label="Language"
    >
      {locales.map((locale) => (
        <button
          key={locale}
          type="button"
          onClick={() => switchTo(locale)}
          aria-pressed={locale === current}
          className={cn(
            "rounded-[6px] px-2 py-1 font-mono text-[11px] uppercase tracking-wide transition-colors",
            locale === current
              ? "bg-line/[0.08] text-fg"
              : "text-faint hover:text-muted",
          )}
        >
          {locale}
        </button>
      ))}
    </div>
  );
}
