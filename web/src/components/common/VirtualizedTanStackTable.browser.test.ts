import { expect, test } from "vitest";
import { render } from "vitest-browser-svelte";

import Fixture from "./VirtualizedTanStackTableFixture.svelte";

const ROW_COUNT = 200;
const ROW_HEIGHT = 40;

async function scrollHeightFor(estimateSize: number) {
  const screen = await render(Fixture, {
    estimateSize,
    rowCount: ROW_COUNT,
    rowHeight: ROW_HEIGHT,
  });
  // Measurement lands after the rows paint, and the estimate it feeds back after that.
  await new Promise((resolve) => setTimeout(resolve, 300));

  const container = screen.container.querySelector<HTMLElement>(".fixture-scroll");
  expect(container).not.toBeNull();
  return container!.scrollHeight;
}

/**
 * `estimateSize` is a first guess. When rows rendered shorter than it, the scroll area kept
 * the guess's height and the scrollbar ran past the last row into empty space that grew with
 * every extra row. The rows on screen decide the height now, so the guess cannot change it.
 */
test("sizes the scroll area from the rendered rows, not from estimateSize", async () => {
  const fromFairEstimate = await scrollHeightFor(ROW_HEIGHT);
  const fromTallEstimate = await scrollHeightFor(ROW_HEIGHT * 5);

  // Rows below the fold keep the estimate until they are scrolled to, so a few rows of
  // drift survive. A wrong estimate used to cost whole screens of it.
  const drift = Math.abs(fromTallEstimate - fromFairEstimate) / fromFairEstimate;
  expect(drift).toBeLessThan(0.1);
  expect(fromTallEstimate).toBeLessThan(ROW_COUNT * ROW_HEIGHT * 1.5);
});
