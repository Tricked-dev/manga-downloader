import { describe, expect, it, vi } from "vitest";

import { load } from "./+page";

describe("app home page load", () => {
  it("does not block initial render on library prefetch", () => {
    const fetch = vi.fn(async () => Response.json({ items: [] }));

    expect(load({ fetch } as never)).toEqual({});
    expect(fetch).not.toHaveBeenCalled();
  });
});
