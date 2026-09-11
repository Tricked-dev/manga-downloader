import { expect, test, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import { request } from "@manga-server/api-client";
import SeriesUpscalingCard from "./SeriesUpscalingCard.svelte";

vi.mock("@manga-server/api-client", () => ({ request: vi.fn() }));

test("enables only this series and displays page progress", async () => {
  let automatic: boolean | null = null;
  vi.mocked(request).mockImplementation(async (_path, options) => {
    if (options?.method === "PUT") {
      automatic = JSON.parse(options.body as string).automatic;
      return undefined;
    }
    return {
      automatic, enabled: automatic === true,
      chapters: [{ download_id: "download-1", chapter_number: 1, upscaled_at: null,
        progress: { status: "running", completed_pages: 3, total_pages: 20, message: "Upscaling pages", updated_at: "2026-09-11" } }],
    };
  });
  const screen = await render(SeriesUpscalingCard, { seriesId: "series-1" });
  await expect.element(screen.getByText("Automatic upscaling: off", { exact: true })).toBeInTheDocument();
  await screen.getByRole("combobox", { name: "Series automatic upscaling" }).selectOptions("true");
  await expect.element(screen.getByText("Automatic upscaling: on", { exact: true })).toBeInTheDocument();
  await expect.element(screen.getByText("Upscaling pages · 3/20 pages", { exact: true })).toBeInTheDocument();
  expect(request).toHaveBeenCalledWith("/v1/library/series-1/upscaling", expect.objectContaining({
    method: "PUT", body: '{"automatic":true}',
  }));
});
