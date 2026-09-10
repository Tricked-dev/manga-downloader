import { setupServer } from "msw/node";

import { getDownloadsMock } from "./generated/endpoints/downloads/downloads.msw";
import { getInfoMock } from "./generated/endpoints/info/info.msw";
import { getLibraryMock } from "./generated/endpoints/library/library.msw";
import { getMediaMock } from "./generated/endpoints/media/media.msw";
import { getSettingsMock } from "./generated/endpoints/settings/settings.msw";
import { getSourcesMock } from "./generated/endpoints/sources/sources.msw";

export * from "./generated/endpoints/downloads/downloads.msw";
export * from "./generated/endpoints/info/info.msw";
export * from "./generated/endpoints/library/library.msw";
export * from "./generated/endpoints/media/media.msw";
export * from "./generated/endpoints/settings/settings.msw";
export * from "./generated/endpoints/sources/sources.msw";

export const apiMockHandlers = [
  ...getInfoMock(),
  ...getLibraryMock(),
  ...getDownloadsMock(),
  ...getMediaMock(),
  ...getSettingsMock(),
  ...getSourcesMock(),
];

export function createApiMockServer(...handlers: Parameters<typeof setupServer>) {
  return setupServer(...(handlers.length > 0 ? handlers : apiMockHandlers));
}

export const apiMockServer = createApiMockServer();
