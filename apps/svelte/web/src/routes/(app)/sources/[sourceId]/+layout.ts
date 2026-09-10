import { error } from "@sveltejs/kit";
import type { Source } from "$lib/types";
import type { LayoutLoad } from "./$types";

export const load: LayoutLoad = async ({ parent, params }) => {
  const { sources } = await parent();
  const sourceInfo =
    sources.items.find((source: Source) => source.name === params.sourceId) ?? null;

  if (!sourceInfo) {
    throw error(404, {
      message: `Source "${params.sourceId}" was not found.`,
    });
  }

  return {
    sourceInfo,
  };
};
