import type { ClientDownload } from "./clients";

type Fetcher = (input: string) => Promise<Response>;

interface ErrorEnvelope {
  error?: {
    message?: unknown;
  };
}

function errorMessageFromEnvelope(value: unknown): string | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const message = (value as ErrorEnvelope).error?.message;
  return typeof message === "string" && message.trim() ? message : null;
}

async function packageDownloadError(response: Response): Promise<string> {
  try {
    const message = errorMessageFromEnvelope(await response.clone().json());
    if (message) {
      return message;
    }
  } catch {
    // Fall back to status text below when the backend did not return JSON.
  }

  return response.statusText || `Package download failed with HTTP ${response.status}`;
}

export async function fetchClientPackage(
  download: Pick<ClientDownload, "href" | "filename">,
  fetcher: Fetcher = fetch,
): Promise<Blob> {
  const response = await fetcher(download.href);

  if (!response.ok) {
    throw new Error(await packageDownloadError(response));
  }

  const blob = await response.blob();
  if (blob.size === 0) {
    throw new Error(`${download.filename} was empty`);
  }

  return blob;
}

export function saveBlobAsFile(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");

  anchor.href = url;
  anchor.download = filename;
  anchor.rel = "noopener";
  anchor.style.display = "none";
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}
