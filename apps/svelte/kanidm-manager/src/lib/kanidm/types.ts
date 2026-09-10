import type { Component } from "svelte";

export type NotificationType = "success" | "error" | "info";
export type AddNotification = (type: NotificationType, message: string) => void;
export type KanidmManagerTab = "oauth2" | "users" | "groups" | "better-auth";

export interface Notification {
  id: string;
  message: string;
  type: NotificationType;
}

export interface KanidmEntry {
  attrs?: Record<string, string[]>;
}

export interface KanidmCollection {
  body: KanidmEntry[];
  status?: number;
}

export interface BetterAuthSettings {
  auth_enabled?: string;
  auth_oidc_client_id?: string;
  auth_oidc_client_secret?: string;
  auth_oidc_issuer_url?: string;
  auth_oidc_provider_id?: string;
  auth_oidc_scopes?: string;
}

export interface ConfigState {
  baseUrl: string;
  configured: boolean;
}

export interface MangaSettingsState {
  configured: boolean;
  error?: string;
  settings: BetterAuthSettings;
}

export interface KanidmManagerData {
  apps: KanidmCollection;
  groups: KanidmCollection;
  kanidm: ConfigState;
  mangaSettings: ConfigState;
  mangaSettingsState: MangaSettingsState;
  origin: string;
  users: KanidmCollection;
}

export interface KanidmNavItem {
  count?: number;
  icon: Component;
  id: KanidmManagerTab;
  label: string;
}

export interface KanidmProxyResponse<T = unknown> {
  body: T;
  status: number;
}

export interface KanidmProxyRequest {
  body?: unknown;
  method?: "GET" | "POST" | "PATCH" | "DELETE" | "PUT";
  path: string;
}
