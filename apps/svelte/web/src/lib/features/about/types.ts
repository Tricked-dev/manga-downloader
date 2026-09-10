export type StatusTone = "ok" | "warn" | "muted";

export interface HealthCheckItem {
  component: string;
  detail: string | null;
  ok: boolean;
}

export interface DetailItem {
  label: string;
  value: string;
  detail?: string;
  icon?: "branch" | "lastCommit" | "remoteCommit";
  mono?: boolean;
}

export interface LinkItem {
  href: string;
  id: "repository";
  label: string;
  value: string;
}

export interface CapabilityItem {
  title: string;
  description: string;
}

export interface StatusItem {
  id: "server" | "git" | "upstream";
  label: string;
  value: string;
  detail: string;
  checks?: HealthCheckItem[];
  tone: StatusTone;
}
