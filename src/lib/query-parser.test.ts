import { describe, it, expect } from "vitest";
import { parseQuery, parseSize, parseDate, hasCriteria } from "./query-parser";

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
});
