import type { Config } from "tailwindcss";

const config: Config = {
  darkMode: "class",
  content: [
    "./app/**/*.{ts,tsx}",
    "./components/**/*.{ts,tsx}",
    "./i18n/**/*.{ts,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        bg: "hsl(var(--bg))",
        surface: "hsl(var(--surface))",
        "surface-2": "hsl(var(--surface-2))",
        line: "hsl(var(--line) / <alpha-value>)",
        fg: "hsl(var(--fg))",
        muted: "hsl(var(--muted))",
        faint: "hsl(var(--faint))",
        teal: {
          DEFAULT: "hsl(var(--teal))",
          deep: "hsl(var(--teal-deep))",
          ink: "hsl(var(--teal-ink))",
        },
        amber: {
          DEFAULT: "hsl(var(--amber))",
          deep: "hsl(var(--amber-deep))",
        },
        online: "hsl(var(--online))",
      },
      fontFamily: {
        sans: ["var(--font-inter)", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["var(--font-mono)", "ui-monospace", "SFMono-Regular", "monospace"],
      },
      borderRadius: {
        sm: "8px",
        md: "10px",
        lg: "12px",
        xl: "16px",
        "2xl": "20px",
      },
      boxShadow: {
        panel:
          "0 1px 0 0 rgba(255,255,255,0.04) inset, 0 10px 30px -12px rgba(0,0,0,0.6)",
        pop: "0 1px 0 0 rgba(255,255,255,0.05) inset, 0 18px 50px -12px rgba(0,0,0,0.7)",
        glow: "0 10px 40px -12px hsl(var(--teal) / 0.45)",
      },
      maxWidth: {
        content: "1180px",
      },
      keyframes: {
        "fade-up": {
          from: { opacity: "0", transform: "translateY(12px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "fade-in": { from: { opacity: "0" }, to: { opacity: "1" } },
        "caret-blink": { "0%,100%": { opacity: "1" }, "50%": { opacity: "0" } },
        "scan-line": {
          "0%": { transform: "translateY(-100%)" },
          "100%": { transform: "translateY(1200%)" },
        },
      },
      animation: {
        "fade-up": "fade-up 600ms cubic-bezier(0.16,1,0.3,1) both",
        "fade-in": "fade-in 500ms ease-out both",
        "caret-blink": "caret-blink 1.1s steps(1) infinite",
      },
    },
  },
  plugins: [],
};

export default config;
