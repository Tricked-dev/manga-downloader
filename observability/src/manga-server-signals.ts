import * as dashboard from "@grafana/grafana-foundation-sdk/dashboard";

import {
  base,
  logStatPanel,
  logsPanel,
  logsTimeseriesPanel,
  row,
  tempoTraceql,
  textPanel,
  textVariable,
  tracesTablePanel,
  victoriaLogsStats,
} from "./dashboard-helpers.js";

export function mangaServerSignals() {
  const service = "$service";
  const logSelector = `{service_name=~"${service}"}`;
  const noisyLogSelector = `${logSelector} "error"`;
  const errorLogSelector = `${logSelector} "error"`;
  const serviceTraceSelector = `{ resource.service.name =~ "${service}" }`;
  const apiTraceSelector = `{ resource.service.name =~ "${service}" && name =~ "api[.].*" }`;
  const appTraceSelector = `{ resource.service.name =~ "${service}" && name =~ "app[.].*" }`;
  const cacheTraceSelector = `{ resource.service.name =~ "${service}" && name =~ "cache[.].*" }`;
  const pluginTraceSelector = `{ resource.service.name =~ "${service}" && name = "plugin.operation" }`;
  const dbTraceSelector = `{ resource.service.name =~ "${service}" && name = "db.operation" }`;
  const backgroundTraceSelector = `{ resource.service.name =~ "${service}" && name =~ "background[.].*" }`;
  const traceTableSort = {
    sortBy: "Duration",
    sortDesc: true,
    organize: {
      indexByName: {
        "Trace ID": 0,
        "Root Service": 1,
        "Root Span": 2,
        Duration: 3,
        "Start time": 4,
      },
    },
  };

  const builder = base(
    "Manga Server Signals",
    "manga-server-signals",
    ["manga-server", "loki", "tempo", "logs", "traces"],
    "now-1h",
    "10s",
    100,
  )
    .description(
      "Logs and traces for manga-server, with VictoriaLogs and Tempo queries tuned for OTLP.",
    )
    .withVariable(textVariable("service", "Service", "manga-server"));

  [
    logStatPanel(
      1,
      "Logs",
      { x: 0, y: 0, w: 4, h: 4 },
      `${logSelector} | stats count()`,
      "short",
      0,
    ),
    logStatPanel(
      2,
      "Errors",
      { x: 4, y: 0, w: 4, h: 4 },
      `${errorLogSelector} | stats count()`,
      "short",
      0,
    ),
    logStatPanel(
      3,
      "Noisy",
      { x: 8, y: 0, w: 4, h: 4 },
      `${noisyLogSelector} | stats count()`,
      "short",
      0,
    ),
    textPanel(
      4,
      "Explore",
      { x: 12, y: 0, w: 12, h: 4 },
      [
        "- Logs datasource: `victorialogs`, selector: `{service_name=~\"$service\"}`",
        "- Traces datasource: `tempo`, TraceQL: `{ resource.service.name =~ \"$service\" }`",
        "- Request/source/media logs emit `trace_id` and `span_id` fields when they run inside a trace.",
        "- Layer spans: `api.*`, `app.*`, `cache.lookup`, `cache.write`, `plugin.operation`, `db.operation`, `background.*`",
        "- Trace tables are client-sorted by `Duration` descending so slow/error traces float to the top.",
        "- Metrics datasource: `prometheus` points at VictoriaMetrics for MetricsQL/PromQL.",
      ].join("\n"),
    ),
    row(10, "Log Volume", 4),
    logsTimeseriesPanel(
      11,
      "Log rate",
      { x: 0, y: 5, w: 12, h: 8 },
      [
        victoriaLogsStats(`${logSelector} | stats count()`, "all logs"),
        victoriaLogsStats(`${errorLogSelector} | stats count()`, "errors", "B"),
      ],
      "logs/s",
      2,
    ),
    logsTimeseriesPanel(
      12,
      "Structured level volume",
      { x: 12, y: 5, w: 12, h: 8 },
      [
        victoriaLogsStats(
          `${logSelector} | stats by (severity_text) count()`,
          "{{severity_text}}",
        ),
      ],
      "short",
      2,
    ),
    row(20, "Recent Logs", 13),
    logsPanel(21, "Recent service logs", { x: 0, y: 14, w: 12, h: 12 }, logSelector, 250),
    logsPanel(22, "Error word matches", { x: 12, y: 14, w: 12, h: 12 }, noisyLogSelector, 250),
    row(30, "Trace Search", 26),
    tracesTablePanel(
      31,
      "Recent traces",
      { x: 0, y: 27, w: 8, h: 10 },
      [tempoTraceql(serviceTraceSelector, "A", 100)],
      traceTableSort,
    ),
    tracesTablePanel(
      32,
      "Slow traces",
      { x: 8, y: 27, w: 8, h: 10 },
      [tempoTraceql(`{ resource.service.name =~ "${service}" && duration > 500ms }`, "A", 100)],
      traceTableSort,
    ),
    tracesTablePanel(
      33,
      "Error traces",
      { x: 16, y: 27, w: 8, h: 10 },
      [tempoTraceql(`{ resource.service.name =~ "${service}" && status = error }`, "A", 100)],
      traceTableSort,
    ),
    row(40, "Layer Trace Drilldown", 37),
    tracesTablePanel(
      41,
      "API route spans",
      { x: 0, y: 38, w: 8, h: 9 },
      [tempoTraceql(apiTraceSelector, "A", 100)],
      traceTableSort,
    ),
    tracesTablePanel(
      42,
      "Application spans",
      { x: 8, y: 38, w: 8, h: 9 },
      [tempoTraceql(appTraceSelector, "A", 100)],
      traceTableSort,
    ),
    tracesTablePanel(
      43,
      "Cache spans",
      { x: 16, y: 38, w: 8, h: 9 },
      [tempoTraceql(cacheTraceSelector, "A", 100)],
      traceTableSort,
    ),
    tracesTablePanel(
      44,
      "Plugin spans",
      { x: 0, y: 47, w: 8, h: 9 },
      [tempoTraceql(pluginTraceSelector, "A", 100)],
      traceTableSort,
    ),
    tracesTablePanel(
      45,
      "DB spans",
      { x: 8, y: 47, w: 8, h: 9 },
      [tempoTraceql(dbTraceSelector, "A", 100)],
      traceTableSort,
    ),
    tracesTablePanel(
      46,
      "Background spans",
      { x: 16, y: 47, w: 8, h: 9 },
      [tempoTraceql(backgroundTraceSelector, "A", 100)],
      traceTableSort,
    ),
  ].forEach((panel) =>
    panel instanceof dashboard.RowBuilder ? builder.withRow(panel) : builder.withPanel(panel),
  );

  return builder.build();
}
