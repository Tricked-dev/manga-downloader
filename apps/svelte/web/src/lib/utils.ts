import { clsx } from "clsx";
import type { ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

export type WithElementRef<T> = T & { ref?: unknown };
export type WithoutChildren<T> = DistributiveOmit<T, "children">;
export type WithoutChild<T> = DistributiveOmit<T, "child">;
export type WithoutChildrenOrChild<T> = DistributiveOmit<T, "children" | "child">;

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function getErrorMessage(error: unknown, fallback = "Something went wrong"): string {
  const reason = readStringProperty(error, "reason");
  if (reason?.trim()) {
    return readableErrorText(reason, fallback);
  }

  const nestedError = readProperty(error, "error");
  const nestedMessage = readStringProperty(nestedError, "message");
  if (nestedMessage?.trim()) {
    return readableErrorText(nestedMessage, fallback);
  }

  if (typeof nestedError === "string" && nestedError.trim()) {
    return readableErrorText(nestedError, fallback);
  }

  const message = readStringProperty(error, "message");
  if (message?.trim()) {
    return readableErrorText(message, fallback);
  }

  if (error instanceof Error && error.message.trim()) {
    return readableErrorText(error.message, fallback);
  }

  if (typeof error === "string" && error.trim()) {
    return readableErrorText(error, fallback);
  }

  return fallback;
}

function readableErrorText(message: string, fallback: string): string {
  const trimmed = message.trim();
  if (!trimmed) {
    return fallback;
  }

  if (/^<!doctype html/i.test(trimmed) || /<html[\s>]/i.test(trimmed)) {
    return fallback;
  }

  return message;
}

function readProperty(value: unknown, key: string): unknown {
  return value && typeof value === "object" ? (value as Record<string, unknown>)[key] : undefined;
}

function readStringProperty(value: unknown, key: string): string | undefined {
  const property = readProperty(value, key);
  return typeof property === "string" ? property : undefined;
}

export function normalizeAbsoluteUrl(url: string, fallbackBaseUrl?: string | null): string {
  const trimmedUrl = url.trim();

  if (trimmedUrl.startsWith("//")) {
    return `https:${trimmedUrl}`;
  }
  if (trimmedUrl.startsWith("/")) {
    return fallbackBaseUrl ? `${fallbackBaseUrl.replace(/\/$/, "")}${trimmedUrl}` : trimmedUrl;
  }
  if (!/^https?:\/\//i.test(trimmedUrl)) {
    return `https://${trimmedUrl}`;
  }

  return trimmedUrl;
}

export function parseCsvSetting(value: string | undefined, fallback: string[]): string[] {
  if (!value) {
    return fallback;
  }

  const values = value
    .split(",")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);

  return values.length > 0 ? values : fallback;
}

export function csvFromList(value: string): string {
  return value
    .split(",")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0)
    .join(",");
}

const dateTimeFormatter = new Intl.DateTimeFormat("en-US", {
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
  month: "short",
});

export function parseDateValue(value?: string | null): Date | null {
  if (!value) {
    return null;
  }

  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }

  const numeric = Number(trimmed);
  if (Number.isFinite(numeric)) {
    const date = new Date(trimmed.length <= 10 ? numeric * 1000 : numeric);
    return Number.isNaN(date.getTime()) ? null : date;
  }

  const normalized = trimmed.includes("T") ? trimmed : `${trimmed.replace(" ", "T")}Z`;
  const date = new Date(normalized);
  return Number.isNaN(date.getTime()) ? null : date;
}

export function formatDateLabel(value?: string, fallback = ""): string {
  const parsed = parseDateValue(value);
  return parsed ? dateTimeFormatter.format(parsed) : (value?.trim() ?? fallback);
}

export function formatChapterNumber(value: number): string {
  if (!Number.isFinite(value)) {
    return "";
  }

  return Number.isInteger(value) ? value.toFixed(0) : value.toString();
}
