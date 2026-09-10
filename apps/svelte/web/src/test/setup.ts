import { apiMockServer } from "@manga-server/api-client/mocks";
import { afterAll, afterEach, beforeAll } from "vitest";

beforeAll(() => {
  apiMockServer.listen({ onUnhandledRequest: "error" });
});

afterEach(() => {
  apiMockServer.resetHandlers();
});

afterAll(() => {
  apiMockServer.close();
});
