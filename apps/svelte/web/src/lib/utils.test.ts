import { describe, expect, it } from "vitest";

import {
  csvFromList,
  formatChapterNumber,
  getErrorMessage,
  normalizeAbsoluteUrl,
  parseCsvSetting,
  parseDateValue,
} from "./utils";

describe("getErrorMessage", () => {
  it.each([
    [new Error("boom"), "Something went wrong", "boom"],
    [{ reason: "bad request" }, "Something went wrong", "bad request"],
    [{ error: { message: "backend failed" } }, "Something went wrong", "backend failed"],
    [{ error: "backend unavailable" }, "Something went wrong", "backend unavailable"],
    [{ message: "route failed" }, "fallback", "route failed"],
    [" failed ", "Something went wrong", " failed "],
    [{ reason: "   " }, "fallback", "fallback"],
    ["<!doctype html><html><body>Application error</body></html>", "fallback", "fallback"],
    [{ reason: "<html><body>Application error</body></html>" }, "fallback", "fallback"],
    [null, "fallback", "fallback"],
  ])("normalizes %o", (error, fallback, expected) => {
    expect(getErrorMessage(error, fallback)).toBe(expected);
  });

  it("prefers structured API reasons over generic Error messages", () => {
    const error = Object.assign(new Error("An error has occurred"), {
      reason: "download archive not found",
    });

    expect(getErrorMessage(error)).toBe("download archive not found");
  });
});

describe("normalizeAbsoluteUrl", () => {
  it.each([
    ["//cdn.example.com/cover.jpg", undefined, "https://cdn.example.com/cover.jpg"],
    ["/series/1", "https://reader.example.com", "https://reader.example.com/series/1"],
    ["/series/1", undefined, "/series/1"],
    ["/series/1", "https://reader.example.com/", "https://reader.example.com/series/1"],
    ["example.com/manga", undefined, "https://example.com/manga"],
    [" http://example.com/manga ", undefined, "http://example.com/manga"],
    [" https://example.com/manga ", undefined, "https://example.com/manga"],
  ])("normalizes %s", (url, fallbackBaseUrl, expected) => {
    expect(normalizeAbsoluteUrl(url, fallbackBaseUrl)).toBe(expected);
  });
});

describe("csv helpers", () => {
  it.each([
    ["  alpha, beta ,, gamma  ", ["fallback"], ["alpha", "beta", "gamma"]],
    [" ,  , ", ["fallback"], ["fallback"]],
    [undefined, ["fallback"], ["fallback"]],
  ])("parses %s", (value, fallback, expected) => {
    expect(parseCsvSetting(value, fallback)).toEqual(expected);
  });

  it.each([
    [" alpha, beta ,, gamma ", "alpha,beta,gamma"],
    ["single", "single"],
    [" , ", ""],
  ])("serializes %s", (value, expected) => {
    expect(csvFromList(value)).toBe(expected);
  });
});

describe("parseDateValue", () => {
  it.each([
    ["1704067200", "2024-01-01T00:00:00.000Z"],
    ["1704067200000", "2024-01-01T00:00:00.000Z"],
    ["2024-01-01 12:34:56", "2024-01-01T12:34:56.000Z"],
    ["2024-01-01T12:34:56Z", "2024-01-01T12:34:56.000Z"],
  ])("parses %s", (value, expectedIsoString) => {
    expect(parseDateValue(value)?.toISOString()).toBe(expectedIsoString);
  });

  it.each([undefined, "   ", "not-a-date"])("returns null for %s", (value) => {
    expect(parseDateValue(value)).toBeNull();
  });
});

describe("formatChapterNumber", () => {
  it.each([
    [12, "12"],
    [12.5, "12.5"],
    [-4.25, "-4.25"],
  ])("formats %d", (value, expected) => {
    expect(formatChapterNumber(value)).toBe(expected);
  });

  it.each([Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY])(
    "returns an empty string for %s",
    (value) => {
      expect(formatChapterNumber(value)).toBe("");
    },
  );
});
