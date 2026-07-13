import Link from "next/link";

export default function NotFound() {
  return (
    <html lang="es">
      <body
        style={{
          margin: 0,
          minHeight: "100dvh",
          display: "grid",
          placeItems: "center",
          background: "#0a0b0d",
          color: "#e9edf1",
          fontFamily: "system-ui, sans-serif",
        }}
      >
        <div style={{ textAlign: "center", padding: "2rem" }}>
          <p style={{ fontFamily: "monospace", color: "#34cfe3", letterSpacing: 2 }}>
            404
          </p>
          <h1 style={{ fontSize: "1.6rem", margin: "0.5rem 0 1.25rem" }}>
            Página no encontrada · Page not found
          </h1>
          <Link href="/es" style={{ color: "#34cfe3" }}>
            DiskDex →
          </Link>
        </div>
      </body>
    </html>
  );
}
