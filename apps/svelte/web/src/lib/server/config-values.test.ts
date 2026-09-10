import { describe, expect, it } from "vitest";

import {
  absoluteUrlOrNull,
  firstDefined,
  nonEmptyStringOrNull,
  normalizeRepositoryUrl,
  shortHash,
} from "./config-values";

describe("server config value helpers", () => {
  it.each([
    ["  value  ", "value"],
    ["   ", null],
    [123, null],
  ])("normalizes string-like values from %o", (value, expected) => {
    expect(nonEmptyStringOrNull(value)).toBe(expected);
  });

  it.each([
    [" https://example.com/path/ ", "https://example.com/path"],
    ["http://example.com", "http://example.com"],
    ["example.com", null],
    [null, null],
  ])("normalizes absolute URLs from %o", (value, expected) => {
    expect(absoluteUrlOrNull(value)).toBe(expected);
  });

  it.each([
    [[undefined, "", "fallback"], ""],
    [[undefined, "value", "fallback"], "value"],
    [[undefined, undefined], ""],
  ])("picks the first defined value from %o", (values, expected) => {
    expect(firstDefined(...values)).toBe(expected);
  });

  it.each([
    ["https://github.com/org/repo.git", "https://github.com/org/repo"],
    ["git@github.com:org/repo.git", "https://github.com/org/repo"],
    ["ssh://git@gitlab.com/org/repo.git", "https://gitlab.com/org/repo"],
    ["ftp://example.com/repo.git", null],
    [null, null],
  ])("converts repository remotes from %o", (value, expected) => {
    expect(normalizeRepositoryUrl(value)).toBe(expected);
  });

  it.each([
    ["abcdef123456", "abcdef1"],
    ["  123456789  ", "1234567"],
    ["", null],
    [undefined, null],
  ])("shortens hashes from %o", (value, expected) => {
    expect(shortHash(value)).toBe(expected);
  });
});
