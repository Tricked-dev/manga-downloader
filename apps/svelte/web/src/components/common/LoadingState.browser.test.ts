import { expect, test } from "vitest";
import { render } from "vitest-browser-svelte";

import { loadTranslations } from "$lib/i18n";
import LoadingState from "./LoadingState.svelte";

test("renders the default loading message", async () => {
  await loadTranslations("en", "/");
  const screen = await render(LoadingState);

  await expect.element(screen.getByText("Loading...")).toBeInTheDocument();
});

test("renders a custom loading message", async () => {
  const screen = await render(LoadingState, {
    label: "Syncing library",
  });

  await expect.element(screen.getByText("Syncing library")).toBeInTheDocument();
});
