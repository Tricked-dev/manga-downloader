import { kanidmFormRequest, kanidmRequest } from "$lib/server/kanidm";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async ({ fetch, platform, request }) => {
  const contentType = request.headers.get("content-type") ?? "";

  try {
    if (contentType.includes("multipart/form-data")) {
      const formData = await request.formData();
      const metadata = JSON.parse(String(formData.get("json") ?? "{}")) as { path?: string };
      formData.delete("json");
      const result = await kanidmFormRequest(fetch, platform, metadata.path ?? "", formData);
      return Response.json({ body: result.body, status: result.status }, { status: 200 });
    }

    const body = (await request.json()) as {
      body?: unknown;
      method?: "GET" | "POST" | "PATCH" | "DELETE" | "PUT";
      path?: string;
    };
    const result = await kanidmRequest(fetch, platform, {
      body: body.body,
      method: body.method,
      path: body.path ?? "",
    });
    return Response.json({ body: result.body, status: result.status }, { status: 200 });
  } catch (error) {
    return Response.json(
      { body: error instanceof Error ? error.message : "Kanidm request failed", status: 500 },
      { status: 500 },
    );
  }
};
