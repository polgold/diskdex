import { describe, it, expect } from "vitest";
import { parseNaturalQuery, hasStructured, applyNLFilters } from "./nl-parser";
import { FILE_CATEGORIES } from "./query-parser";
import type { SemanticItem } from "./ipc";

const GB = 1024 ** 3;

describe("parseNaturalQuery", () => {
  it("separa tipo + año + tamaño del concepto visual", () => {
    const { concept, filters } = parseNaturalQuery(
      "videos del 2022 con gente en la playa que pesen más de 2gb",
    );
    expect(concept).toBe("gente en la playa");
    expect(filters.exts).toEqual(FILE_CATEGORIES.video.exts);
    expect(filters.min_size).toBe(2 * GB);
    expect(filters.modified_after).toBe(Math.floor(Date.UTC(2022, 0, 1) / 1000));
    expect(filters.modified_before).toBe(Math.floor(Date.UTC(2022, 11, 31, 23, 59, 59) / 1000));
  });

  it("categoría foto + concepto, sin filtros de fecha", () => {
    const { concept, filters } = parseNaturalQuery("fotos de un atardecer");
    expect(concept).toBe("atardecer");
    expect(filters.exts).toContain("jpg");
    expect(filters.modified_after).toBeUndefined();
  });

  it("solo filtros estructurados → concepto vacío", () => {
    const { concept, filters } = parseNaturalQuery("archivos grandes del 2021");
    expect(concept).toBe("");
    expect(filters.min_size).toBe(GB);
    expect(hasStructured(filters)).toBe(true);
  });

  it("inglés: menos de + before", () => {
    const { concept, filters } = parseNaturalQuery("photos of a dog under 500mb before 2020");
    expect(concept).toBe("dog");
    expect(filters.max_size).toBe(500 * 1024 ** 2);
    expect(filters.modified_before).toBe(Math.floor(Date.UTC(2020, 11, 31, 23, 59, 59) / 1000));
  });

  it("concepto puro sin filtros", () => {
    const { concept, filters } = parseNaturalQuery("perros");
    expect(concept).toBe("perros");
    expect(hasStructured(filters)).toBe(false);
  });

  it("detecta idioma hablado y lo saca del concepto", () => {
    const a = parseNaturalQuery("videos en español de una pelea");
    expect(a.lang).toBe("es");
    expect(a.concept).toBe("pelea");
    const b = parseNaturalQuery("clips in english about dogs");
    expect(b.lang).toBe("en");
    const c = parseNaturalQuery("gente en la playa"); // "en" preposicional, NO idioma
    expect(c.lang).toBeUndefined();
    expect(c.concept).toBe("gente en la playa");
  });
});

describe("applyNLFilters", () => {
  const mk = (over: Partial<SemanticItem>): SemanticItem => ({
    id: 1,
    disk_id: 1,
    disk_name: "D",
    name: "x.mp4",
    is_folder: false,
    size_logical: 0,
    modified_at: null,
    path: "/x",
    score: 0.1,
    frame_ts: null,
    ...over,
  });

  it("filtra por extensión y tamaño preservando el orden", () => {
    const items = [
      mk({ id: 1, name: "a.mp4", size_logical: 3 * GB }),
      mk({ id: 2, name: "b.jpg", size_logical: 3 * GB }),
      mk({ id: 3, name: "c.mp4", size_logical: 1024 }),
    ];
    const out = applyNLFilters(items, {
      text: "",
      tags: [],
      exts: ["mp4"],
      min_size: GB,
    });
    expect(out.map((i) => i.id)).toEqual([1]);
  });
});
