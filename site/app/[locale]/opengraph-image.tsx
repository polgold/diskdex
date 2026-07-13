import { ImageResponse } from "next/og";
import { isLocale } from "@/i18n/config";

export const alt = "DiskDex — offline disk catalog and search";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

const copy = {
  es: {
    eyebrow: "CATÁLOGO DE DISCOS OFFLINE",
    title: "Encontrá cualquier archivo de tus discos, sin enchufar ninguno.",
    stats: [
      ["6.828.850", "archivos"],
      ["116 TB", "54 discos"],
      ["< 0,5 s", "por búsqueda"],
    ],
  },
  en: {
    eyebrow: "OFFLINE DISK CATALOG",
    title: "Find any file across your drives, without plugging in a disk.",
    stats: [
      ["6,828,850", "files"],
      ["116 TB", "54 drives"],
      ["< 0.5 s", "per search"],
    ],
  },
} as const;

export default async function Image({
  params,
}: {
  params: Promise<{ locale: string }>;
}) {
  const { locale } = await params;
  const t = isLocale(locale) ? copy[locale] : copy.es;

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          padding: "72px",
          backgroundColor: "#0a0b0d",
          backgroundImage:
            "linear-gradient(125deg, rgba(52,207,227,0.16), rgba(10,11,13,0) 42%)",
          color: "#e9edf1",
          fontFamily: "sans-serif",
        }}
      >
        {/* brand row */}
        <div style={{ display: "flex", alignItems: "center", gap: 20 }}>
          <div style={{ display: "flex", position: "relative", width: 64, height: 64 }}>
            <div
              style={{
                position: "absolute",
                left: 0,
                top: 16,
                width: 40,
                height: 40,
                borderRadius: 999,
                border: "9px solid #34cfe3",
              }}
            />
            <div
              style={{
                position: "absolute",
                left: 20,
                top: 0,
                width: 40,
                height: 40,
                borderRadius: 999,
                border: "9px solid #ffb627",
              }}
            />
          </div>
          <div style={{ fontSize: 34, fontWeight: 700, letterSpacing: -1 }}>
            DiskDex
          </div>
        </div>

        {/* headline */}
        <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
          <div
            style={{
              fontSize: 18,
              letterSpacing: 4,
              color: "#34cfe3",
              fontFamily: "monospace",
            }}
          >
            {t.eyebrow}
          </div>
          <div
            style={{
              fontSize: 62,
              fontWeight: 800,
              lineHeight: 1.05,
              letterSpacing: -2,
              maxWidth: 960,
            }}
          >
            {t.title}
          </div>
        </div>

        {/* stats */}
        <div style={{ display: "flex", gap: 56 }}>
          {t.stats.map(([n, l]) => (
            <div key={l} style={{ display: "flex", flexDirection: "column" }}>
              <div
                style={{
                  fontSize: 34,
                  fontWeight: 700,
                  color: "#ffb627",
                  fontFamily: "monospace",
                }}
              >
                {n}
              </div>
              <div style={{ fontSize: 18, color: "#7d8894" }}>{l}</div>
            </div>
          ))}
          <div
            style={{
              marginLeft: "auto",
              alignSelf: "flex-end",
              fontSize: 18,
              color: "#7d8894",
              fontFamily: "monospace",
            }}
          >
            macOS · Windows · diskdex.app
          </div>
        </div>
      </div>
    ),
    size,
  );
}
