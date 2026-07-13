import Link from "next/link";
import type { AnchorHTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/cn";

type Variant = "primary" | "outline" | "ghost";
type Size = "md" | "lg";

const variants: Record<Variant, string> = {
  primary:
    "bg-teal text-teal-ink font-semibold shadow-glow hover:brightness-110 active:brightness-95",
  outline:
    "border border-line/15 text-fg hover:bg-line/[0.06] hover:border-line/25",
  ghost: "text-muted hover:text-fg hover:bg-line/[0.05]",
};

const sizes: Record<Size, string> = {
  md: "h-10 px-4 text-[0.9rem] rounded-md gap-2",
  lg: "h-12 px-5 text-[0.95rem] rounded-lg gap-2.5",
};

type Props = {
  href: string;
  variant?: Variant;
  size?: Size;
  children: ReactNode;
  external?: boolean;
  className?: string;
} & Omit<AnchorHTMLAttributes<HTMLAnchorElement>, "href">;

export function ButtonLink({
  href,
  variant = "primary",
  size = "md",
  external,
  className,
  children,
  ...rest
}: Props) {
  const classes = cn(
    "inline-flex items-center justify-center whitespace-nowrap transition-[filter,background-color,border-color,color] duration-150 focus-visible:outline-teal",
    variants[variant],
    sizes[size],
    className,
  );

  const isExternal = external ?? /^https?:\/\//.test(href);
  if (isExternal) {
    return (
      <a
        href={href}
        target="_blank"
        rel="noreferrer noopener"
        className={classes}
        {...rest}
      >
        {children}
      </a>
    );
  }
  return (
    <Link href={href} className={classes} {...rest}>
      {children}
    </Link>
  );
}
