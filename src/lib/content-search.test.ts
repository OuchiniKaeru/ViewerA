import { describe, expect, it } from "vitest";
import { pageContentSearchHits } from "./content-search";

describe("pageContentSearchHits", () => {
  it("slices cached results and clamps a page beyond the result range", () => {
    const hits = Array.from({ length: 205 }, (_, index) => index);

    expect(pageContentSearchHits(hits, 1, 100)).toEqual({
      items: hits.slice(100, 200),
      page: 1,
      pageSize: 100,
      total: 205,
    });
    expect(pageContentSearchHits(hits, 20, 100)).toEqual({
      items: hits.slice(200),
      page: 2,
      pageSize: 100,
      total: 205,
    });
  });

  it("returns an empty first page for no matches", () => {
    expect(pageContentSearchHits([], 0, 100)).toEqual({
      items: [],
      page: 0,
      pageSize: 100,
      total: 0,
    });
  });
});
