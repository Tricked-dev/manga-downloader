import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { stringify } from "yaml";

import {
  autometricsFunctionExplorer,
  autometricsOverview,
  mangaServerOverview,
  mangaServerSignals,
  tokioRuntime,
} from "./dashboards.js";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const outputRoot = process.env.OBSERVABILITY_OUT_DIR ?? root;

export interface GeneratedArtifact {
  format: "json" | "yaml";
  path: string;
  value: unknown;
}

const dashboardOutputs = [
  ["manga-server-overview.json", mangaServerOverview()],
  ["manga-server-signals.json", mangaServerSignals()],
  ["tokio-runtime.json", tokioRuntime()],
  ["autometrics-overview.json", autometricsOverview()],
  ["autometrics-function-explorer.json", autometricsFunctionExplorer()],
] as const;

const datasourceProvisioning = {
  apiVersion: 1,
  deleteDatasources: [
    { name: "Prometheus", orgId: 1 },
    { name: "Tempo", orgId: 1 },
    { name: "Loki", orgId: 1 },
  ],
  datasources: [
    {
      name: "Metrics",
      uid: "prometheus",
      type: "prometheus",
      access: "proxy",
      url: "http://victoriametrics:8428",
      isDefault: true,
      jsonData: {
        httpMethod: "POST",
        timeInterval: "10s",
        exemplarTraceIdDestinations: [{ name: "trace_id", datasourceUid: "tempo" }],
      },
    },
    {
      name: "Logs",
      uid: "victorialogs",
      type: "victoriametrics-logs-datasource",
      access: "proxy",
      url: "http://victorialogs:9428",
      jsonData: {},
    },
    {
      name: "Traces",
      uid: "tempo",
      type: "tempo",
      access: "proxy",
      url: "http://tempo:3200",
      jsonData: {
        nodeGraph: { enabled: true },
        serviceMap: { datasourceUid: "prometheus" },
        tracesToLogsV2: {
          datasourceUid: "victorialogs",
          spanStartTimeShift: "-5m",
          spanEndTimeShift: "5m",
          filterByTraceID: true,
          filterBySpanID: false,
          tags: [{ key: "service.name", value: "service_name" }],
        },
      },
    },
  ],
};

const dashboardProvisioning = {
  apiVersion: 1,
  providers: [
    {
      name: "manga-downloader",
      orgId: 1,
      folder: "Manga Downloader",
      type: "file",
      disableDeletion: false,
      allowUiUpdates: true,
      updateIntervalSeconds: 10,
      options: {
        path: "/var/lib/grafana/dashboards/manga",
      },
    },
  ],
};

const otelCollectorConfig = {
  receivers: {
    otlp: {
      protocols: {
        grpc: {
          endpoint: "0.0.0.0:4317",
        },
        http: {
          endpoint: "0.0.0.0:4318",
        },
      },
    },
  },
  processors: {
    resource: {
      attributes: [
        { key: "service_name", from_attribute: "service.name", action: "upsert" },
        { key: "service_namespace", from_attribute: "service.namespace", action: "upsert" },
        { key: "service_instance_id", from_attribute: "service.instance.id", action: "upsert" },
      ],
    },
    batch: {},
  },
  exporters: {
    "otlp_http/victoriametrics": {
      compression: "gzip",
      encoding: "proto",
      metrics_endpoint: "http://victoriametrics:8428/opentelemetry/v1/metrics",
    },
    "otlp_http/victorialogs": {
      logs_endpoint: "http://victorialogs:9428/insert/opentelemetry/v1/logs",
      headers: {
        "VL-Stream-Fields": "service_name,service_namespace,service_instance_id",
      },
    },
    "otlp_grpc/tempo": {
      endpoint: "tempo:4317",
      tls: {
        insecure: true,
      },
    },
  },
  service: {
    pipelines: {
      metrics: {
        receivers: ["otlp"],
        processors: ["resource", "batch"],
        exporters: ["otlp_http/victoriametrics"],
      },
      logs: {
        receivers: ["otlp"],
        processors: ["resource", "batch"],
        exporters: ["otlp_http/victorialogs"],
      },
      traces: {
        receivers: ["otlp"],
        processors: ["resource", "batch"],
        exporters: ["otlp_grpc/tempo"],
      },
    },
  },
};

const tempoConfig = {
  server: {
    http_listen_port: 3200,
  },
  distributor: {
    receivers: {
      otlp: {
        protocols: {
          grpc: {
            endpoint: "0.0.0.0:4317",
          },
          http: {
            endpoint: "0.0.0.0:4318",
          },
        },
      },
    },
  },
  storage: {
    trace: {
      backend: "local",
      local: {
        path: "/tmp/tempo/blocks",
      },
      wal: {
        path: "/tmp/tempo/wal",
      },
    },
  },
};

const composeConfig = {
  services: {
    grafana: {
      image: "grafana/grafana:latest",
      depends_on: ["victoriametrics", "victorialogs", "tempo"],
      ports: ["3000:3000"],
      environment: {
        GF_SECURITY_ADMIN_USER: "admin",
        GF_SECURITY_ADMIN_PASSWORD: "admin",
        GF_USERS_DEFAULT_THEME: "light",
        GF_INSTALL_PLUGINS: "victoriametrics-logs-datasource",
      },
      volumes: [
        "grafana-data:/var/lib/grafana",
        "./grafana/dashboards:/var/lib/grafana/dashboards/manga:ro",
        "./grafana/provisioning/dashboards/manga.yaml:/etc/grafana/provisioning/dashboards/manga.yaml:ro",
        "./grafana/provisioning/datasources/datasources.yaml:/etc/grafana/provisioning/datasources/manga.yaml:ro",
      ],
    },
    victoriametrics: {
      image: "victoriametrics/victoria-metrics:latest",
      command: ["-storageDataPath=/victoria-metrics-data", "-httpListenAddr=:8428"],
      ports: ["8428:8428"],
      volumes: ["victoriametrics-data:/victoria-metrics-data"],
    },
    victorialogs: {
      image: "victoriametrics/victoria-logs:latest",
      command: ["-storageDataPath=/victoria-logs-data", "-httpListenAddr=:9428"],
      ports: ["9428:9428"],
      volumes: ["victorialogs-data:/victoria-logs-data"],
    },
    tempo: {
      image: "grafana/tempo:latest",
      command: ["-config.file=/etc/tempo/tempo.yaml"],
      user: "0:0",
      ports: ["3200:3200"],
      volumes: ["./tempo/tempo.yaml:/etc/tempo/tempo.yaml:ro", "tempo-data:/tmp/tempo"],
    },
    "otel-collector": {
      image: "otel/opentelemetry-collector-contrib:latest",
      command: ["--config=/etc/otelcol/config.yaml"],
      depends_on: ["victoriametrics", "victorialogs", "tempo"],
      ports: ["4317:4317", "4318:4318"],
      volumes: ["./otel-collector/config.yaml:/etc/otelcol/config.yaml:ro"],
    },
  },
  volumes: {
    "grafana-data": null,
    "tempo-data": null,
    "victorialogs-data": null,
    "victoriametrics-data": null,
  },
};

export function observabilityArtifacts(rootDir: string): GeneratedArtifact[] {
  return [
    ...dashboardOutputs.map(
      ([fileName, builtDashboard]): GeneratedArtifact => ({
        format: "json",
        path: join(rootDir, "grafana/dashboards", fileName),
        value: builtDashboard,
      }),
    ),
    {
      format: "yaml",
      path: join(rootDir, "grafana/provisioning/datasources/datasources.yaml"),
      value: datasourceProvisioning,
    },
    {
      format: "yaml",
      path: join(rootDir, "grafana/provisioning/dashboards/manga.yaml"),
      value: dashboardProvisioning,
    },
    {
      format: "yaml",
      path: join(rootDir, "otel-collector/config.yaml"),
      value: otelCollectorConfig,
    },
    {
      format: "yaml",
      path: join(rootDir, "tempo/tempo.yaml"),
      value: tempoConfig,
    },
    {
      format: "yaml",
      path: join(rootDir, "docker-compose.yml"),
      value: composeConfig,
    },
  ];
}

async function writeJson(path: string, value: unknown) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
}

async function writeYaml(path: string, value: unknown) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, stringify(value, { nullStr: "" }));
}

export async function writeObservabilityArtifacts(rootDir: string) {
  for (const artifact of observabilityArtifacts(rootDir)) {
    if (artifact.format === "json") {
      await writeJson(artifact.path, artifact.value);
    } else {
      await writeYaml(artifact.path, artifact.value);
    }
  }
}

function isEntrypoint() {
  return process.argv[1] ? import.meta.url === pathToFileURL(process.argv[1]).href : false;
}

if (isEntrypoint()) {
  await writeObservabilityArtifacts(outputRoot);
}
