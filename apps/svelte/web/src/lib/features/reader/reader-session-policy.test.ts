import { describe, expect, it } from "vitest";

import {
  clampPageIndex,
  getScrollRenderRange,
  isDownloadedPageRoute,
  nextReadProgressReport,
  requiresBaseUrl,
  withSkipPageCache,
} from "./reader-session-policy";

describe("reader session policy", () => {
  it("clamps page indexes to the available Chapter Page range", () => {
    expect(clampPageIndex(-2, 5)).toBe(0);
    expect(clampPageIndex(2, 5)).toBe(2);
    expect(clampPageIndex(99, 5)).toBe(4);
    expect(clampPageIndex(3, 0)).toBe(0);
  });

  it("builds an overscanned scroll render range", () => {
    expect(getScrollRenderRange(0, 10, 4)).toEqual({ start: 0, end: 4 });
    expect(getScrollRenderRange(5, 10, 4)).toEqual({ start: 1, end: 9 });
    expect(getScrollRenderRange(9, 10, 4)).toEqual({ start: 5, end: 9 });
  });

  it("detects Chapter Page references that need a source base URL", () => {
    expect(requiresBaseUrl("/images/1.jpg")).toBe(true);
    expect(requiresBaseUrl("//cdn.example.test/1.jpg")).toBe(false);
    expect(requiresBaseUrl("https://example.test/1.jpg")).toBe(false);
  });

  it("reports only forward Read Progress unless the Local Library Chapter changes", () => {
    const initial = { chapterId: "", highestReportedPage: 0 };
    const first = nextReadProgressReport(initial, "chapter-1", 0, 5);
    expect(first.report).toEqual({ completed: false, page: 1 });

    const duplicate = nextReadProgressReport(first.nextState, "chapter-1", 0, 5);
    expect(duplicate.report).toBeNull();

    const completed = nextReadProgressReport(duplicate.nextState, "chapter-1", 4, 5);
    expect(completed.report).toEqual({ completed: true, page: 5 });

    const nextChapter = nextReadProgressReport(completed.nextState, "chapter-2", 0, 5);
    expect(nextChapter.report).toEqual({ completed: false, page: 1 });
  });

  it("handles downloaded Chapter Page conflict probe URLs", () => {
    const origin = "https://reader.example.test";
    expect(isDownloadedPageRoute("/v1/library/chapters/ch-1/pages/3", origin)).toBe(true);
    expect(isDownloadedPageRoute("/v1/sources/demo/chapters/1/pages", origin)).toBe(false);
    expect(withSkipPageCache("/v1/library/chapters/ch-1/pages/3?x=1", origin)).toBe(
      "/v1/library/chapters/ch-1/pages/3?x=1&skip_page_cache=true",
    );
  });
});
