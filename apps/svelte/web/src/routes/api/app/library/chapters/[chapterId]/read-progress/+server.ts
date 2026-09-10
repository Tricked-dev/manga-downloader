import { proxyApiRequest } from "$lib/server/api/backend-client";
import type { RequestHandler } from "./$types";

function readProgressPath(chapterId: string): string {
  return `/v1/library/chapters/${encodeURIComponent(chapterId)}/read-progress`;
}

export const PUT: RequestHandler = async (event) =>
  proxyApiRequest(event, readProgressPath(event.params.chapterId));

export const DELETE: RequestHandler = async (event) =>
  proxyApiRequest(event, readProgressPath(event.params.chapterId));
