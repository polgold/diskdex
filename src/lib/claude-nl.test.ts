import { describe, it, expect } from "vitest";
import { claudeJsonToNLQuery } from "./claude-nl";
import { FILE_CATEGORIES } from "./query-parser";

const GB = 1024 ** 3;

describe("claudeJsonToNLQuery", () => {
  it("mapea lugar + luz + categoría + año + tamaño", () => {
    const { concept, filters } = claudeJsonToNLQuery({
      categories: ["video"],
      place: "Jujuy",
      light: "sunset",
      min_size_mb: 2048,
      after_year: 2023,
      before_year: 2023,
      kind: null,
      concept: null,
    });
    expect(concept).toBe("");
    expect(filters.exts).toEqual(FILE_CATEGORIES.video.exts);
    expect(filters.place).toBe("Jujuy");
    expect(filters.light).toBe("sunset");
    expect(filters.min_size).toBe(2 * GB);
    expect(filters.modified_after).toBe(Math.floor(Date.UTC(2023, 0, 1) / 1000));
    expect(filters.modified_before).toBe(Math.floor(Date.UTC(2023, 11, 31, 23, 59, 59) / 1000));
  });

  it("preserva el concepto visual y normaliza la luz a minúsculas", () => {
    const { concept, filters } = claudeJsonToNLQuery({
      categories: [],
      light: "SUNRISE",
      concept: "gente en la playa",
    });
    expect(concept).toBe("gente en la playa");
    expect(filters.light).toBe("sunrise");
    expect(filters.exts).toEqual([]);
  });

  it("campos nulos → filtros vacíos", () => {
    const { concept, filters } = claudeJsonToNLQuery({});
    expect(concept).toBe("");
    expect(filters.place).toBeUndefined();
    expect(filters.light).toBeUndefined();
  });
});
