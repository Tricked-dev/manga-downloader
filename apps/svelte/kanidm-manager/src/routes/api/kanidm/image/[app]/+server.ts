import { kanidmImage } from "$lib/server/kanidm";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = ({ fetch, params, platform }) => {
  return kanidmImage(fetch, platform, params.app);
};
