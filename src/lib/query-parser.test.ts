import { describe, it, expect } from "vitest";
import {
  parseQuery,
  parseSize,
  parseDate,
  hasCriteria,
  toggleToken,
  hasToken,
  FOLDER_TOKEN,
  CATEGORY_TOKEN_RX,
  TYPE_TOKEN_RX,
} from "./query-parser";

describe("parseSize", () => {
  it("parses units", () => {
    expect(parseSize("1gb")).toBe(1024 ** 3);
    expect(parseSize("500mb")).toBe(500 * 1024 ** 2);
    expect(parseSize("2")).toBe(2); // bytes por defecto
    expect(parseSize("1.5gb")).toBe(Math.round(1.5 * 1024 ** 3));
  });
  it("rejects garbage", () => {
    expect(parseSize("big")).toBeUndefined();
  });
});

describe("parseDate", () => {
  it("parses ISO date to unix seconds (UTC)", () => {
    expect(parseDate("2023-01-01")).toBe(Math.floor(Date.UTC(2023, 0, 1) / 1000));
  });
  it("rejects bad format", () => {
    expect(parseDate("01/01/2023")).toBeUndefined();
  });
});

describe("parseQuery", () => {
  it("plain text", () => {
    const f = parseQuery("render final");
    expect(f.text).toBe("render final");
    expect(f.exts).toEqual([]);
  });

  it("extensions, comma list, strips dots", () => {
    const f = parseQuery("ext:.mov,mp4");
    expect(f.exts).toEqual(["mov", "mp4"]);
    expect(f.text).toBe("");
  });

  it("size min and max", () => {
    expect(parseQuery("size>1gb").min_size).toBe(1024 ** 3);
    expect(parseQuery("size<=500mb").max_size).toBe(500 * 1024 ** 2);
  });

  it("dates after/before, before includes whole day", () => {
    const f = parseQuery("after:2023-01-01 before:2023-12-31");
    expect(f.modified_after).toBe(Math.floor(Date.UTC(2023, 0, 1) / 1000));
    expect(f.modified_before).toBe(Math.floor(Date.UTC(2023, 11, 31) / 1000) + 86399);
  });

  it("tags, comma list, lowercased and deduped", () => {
    const f = parseQuery("tag:Boda,4K tag:boda render");
    expect(f.tags).toEqual(["boda", "4k"]);
    expect(f.text).toBe("render");
  });

  it("type folder/file", () => {
    expect(parseQuery("type:folder").kind).toBe("folder");
    expect(parseQuery("type:archivo").kind).toBe("file");
  });

  it("mixes text and filters", () => {
    const f = parseQuery("C0001 ext:mp4 size>2gb after:2023-06-01 type:file");
    expect(f.text).toBe("C0001");
    expect(f.exts).toEqual(["mp4"]);
    expect(f.min_size).toBe(2 * 1024 ** 3);
    expect(f.kind).toBe("file");
    expect(f.modified_after).toBe(Math.floor(Date.UTC(2023, 5, 1) / 1000));
  });

  it("hasCriteria", () => {
    expect(hasCriteria(parseQuery(""))).toBe(false);
    expect(hasCriteria(parseQuery("   "))).toBe(false);
    expect(hasCriteria(parseQuery("ext:mov"))).toBe(true);
  });

  it("solo type:carpeta ya es un criterio buscable", () => {
    expect(hasCriteria(parseQuery(FOLDER_TOKEN))).toBe(true);
    expect(parseQuery(FOLDER_TOKEN).kind).toBe("folder");
  });
});

describe("toggleToken", () => {
  it("agrega el token sin tocar lo que ya estaba escrito", () => {
    expect(toggleToken("techo", FOLDER_TOKEN)).toBe("techo type:carpeta");
  });

  it("lo saca si ya estaba, y deja el resto", () => {
    expect(toggleToken("techo type:carpeta", FOLDER_TOKEN)).toBe("techo");
  });

  it("carpetas y categoría se excluyen: activar una saca la otra", () => {
    // Juntas darían cero resultados (una carpeta no tiene extensión).
    expect(toggleToken("techo cat:video", FOLDER_TOKEN, [CATEGORY_TOKEN_RX])).toBe(
      "techo type:carpeta",
    );
    expect(toggleToken("techo type:carpeta", "cat:video", [TYPE_TOKEN_RX])).toBe(
      "techo cat:video",
    );
  });

  it("dos categorías conviven (se suman las extensiones)", () => {
    expect(toggleToken("cat:imagen", "cat:video", [TYPE_TOKEN_RX])).toBe("cat:imagen cat:video");
    const f = parseQuery("cat:imagen cat:video");
    expect(f.exts).toContain("jpg");
    expect(f.exts).toContain("mov");
  });

  it("hasToken compara el token entero, no como subcadena", () => {
    expect(hasToken("techo type:carpeta", FOLDER_TOKEN)).toBe(true);
    expect(hasToken("type:carpetas-viejas", FOLDER_TOKEN)).toBe(false);
    expect(hasToken("techo", FOLDER_TOKEN)).toBe(false);
  });
});
