import {
  boundedRatio,
  base,
  link,
  prom,
  queryVariable,
  statPanel,
  tablePanel,
  textVariable,
  timeseriesPanel,
} from "./dashboard-helpers.js";

export function autometricsOverview() {
  const selector = 'job="$job", service_name=~"$service_name", module=~"$module"';
  const callsRate = `rate(function_calls_total{${selector}}[$__rate_interval])`;
  const errorCallsRate = `rate(function_calls_total{${selector}, result="error"}[$__rate_interval])`;
  const callsByFunction = `sum by (function, module, service_name) (${callsRate})`;
  const errorsByFunction = `sum by (function, module, service_name) (${errorCallsRate}) or on(function, module, service_name) (0 * ${callsByFunction})`;
  const builder = base(
    "Autometrics Overview",
    "autometrics-overview",
    ["autometrics", "manga-server", "prometheus"],
    "now-6h",
    "30s",
  )
    .description(
      "Function-level request rate, error ratio, latency, and concurrency from Autometrics.",
    )
    .withVariable(queryVariable("job", "Job", "label_values(function_calls_total, job)"))
    .withVariable(
      queryVariable(
        "service_name",
        "Service",
        'label_values(function_calls_total{job="$job"}, service_name)',
        "All",
        true,
      ),
    )
    .withVariable(
      queryVariable(
        "module",
        "Module",
        'label_values(function_calls_total{job="$job", service_name=~"$service_name"}, module)',
        "All",
        true,
      ),
    )
    .withVariable(textVariable("limit", "Top N", "10"))
    .link(
      link(
        "Function Explorer",
        "/d/autometrics-function-explorer/autometrics-function-explorer?var-datasource=$datasource&var-job=$job",
        true,
      ),
    );

  [
    statPanel(1, "Function call rate", { x: 0, y: 0, w: 4, h: 4 }, `sum(${callsRate})`, "ops", 2),
    statPanel(
      2,
      "Error ratio",
      { x: 4, y: 0, w: 4, h: 4 },
      boundedRatio(`sum(${errorCallsRate}) or vector(0)`, `sum(${callsRate})`),
      "percentunit",
      3,
    ),
    statPanel(
      3,
      "p95 latency",
      { x: 8, y: 0, w: 4, h: 4 },
      `max(function_calls_duration_seconds{${selector}, quantile="0.95"})`,
      "s",
      4,
    ),
    statPanel(
      4,
      "Concurrent calls",
      { x: 12, y: 0, w: 4, h: 4 },
      `sum(function_calls_concurrent{${selector}})`,
      "short",
      0,
    ),
    statPanel(
      5,
      "Functions seen",
      { x: 16, y: 0, w: 4, h: 4 },
      `count(count by (function, module) (function_calls_total{${selector}}))`,
      "short",
      0,
    ),
    statPanel(
      6,
      "Modules seen",
      { x: 20, y: 0, w: 4, h: 4 },
      `count(count by (module) (function_calls_total{${selector}}))`,
      "short",
      0,
    ),
    timeseriesPanel(
      10,
      "Top function call rate",
      { x: 0, y: 4, w: 12, h: 8 },
      [prom(`topk($limit, ${callsByFunction})`, "{{module}}::{{function}}")],
      "ops",
    ),
    timeseriesPanel(
      11,
      "Top function error ratio",
      { x: 12, y: 4, w: 12, h: 8 },
      [
        prom(
          `100 * topk($limit, ${boundedRatio(errorsByFunction, callsByFunction)})`,
          "{{module}}::{{function}}",
        ),
      ],
      "percent",
      3,
    ),
    timeseriesPanel(
      12,
      "Function latency p95/p99",
      { x: 0, y: 12, w: 12, h: 8 },
      [
        prom(
          `topk($limit, max by (function, module, service_name) (function_calls_duration_seconds{${selector}, quantile="0.95"}))`,
          "p95 {{module}}::{{function}}",
        ),
        prom(
          `topk($limit, max by (function, module, service_name) (function_calls_duration_seconds{${selector}, quantile="0.99"}))`,
          "p99 {{module}}::{{function}}",
          "B",
        ),
      ],
      "s",
      4,
    ),
    timeseriesPanel(
      13,
      "Concurrent functions",
      { x: 12, y: 12, w: 12, h: 8 },
      [
        prom(
          `topk($limit, sum by (function, module, service_name) (function_calls_concurrent{${selector}}))`,
          "{{module}}::{{function}}",
        ),
      ],
      "short",
      0,
    ),
    tablePanel(
      14,
      "High volume functions",
      { x: 0, y: 20, w: 8, h: 8 },
      [
        prom(
          `topk($limit, sum by (function, module, service_name) (increase(function_calls_total{${selector}}[$__range])))`,
          "{{module}}::{{function}}",
          "A",
          true,
          "table",
        ),
      ],
      "short",
    ),
    tablePanel(
      15,
      "Slowest p95 functions",
      { x: 8, y: 20, w: 8, h: 8 },
      [
        prom(
          `topk($limit, max by (function, module, service_name) (function_calls_duration_seconds{${selector}, quantile="0.95"}))`,
          "{{module}}::{{function}}",
          "A",
          true,
          "table",
        ),
      ],
      "s",
    ),
    tablePanel(
      16,
      "Recent function errors",
      { x: 16, y: 20, w: 8, h: 8 },
      [
        prom(
          `topk($limit, sum by (function, module, service_name, result) (increase(function_calls_total{${selector}, result="error"}[$__range])) or on(function, module, service_name, result) label_replace(0 * sum by (function, module, service_name) (increase(function_calls_total{${selector}}[$__range])), "result", "none", "function", ".*"))`,
          "{{module}}::{{function}} {{result}}",
          "A",
          true,
          "table",
        ),
      ],
      "short",
    ),
  ].forEach((panel) => builder.withPanel(panel));

  return builder.build();
}

export function autometricsFunctionExplorer() {
  const selector =
    'job="$job", service_name=~"$service_name", module=~"$module", function=~"$function"';
  const callsRate = `rate(function_calls_total{${selector}}[$__rate_interval])`;
  const errorCallsRate = `rate(function_calls_total{${selector}, result="error"}[$__rate_interval])`;
  const callsByFunction = `sum by (function, module) (${callsRate})`;
  const errorsByFunction = `sum by (function, module) (${errorCallsRate}) or on(function, module) (0 * ${callsByFunction})`;
  const builder = base(
    "Autometrics Function Explorer",
    "autometrics-function-explorer",
    ["autometrics", "manga-server", "prometheus"],
    "now-6h",
    "30s",
  )
    .description("Drill-down dashboard for selected Autometrics functions.")
    .withVariable(queryVariable("job", "Job", "label_values(function_calls_total, job)"))
    .withVariable(
      queryVariable(
        "service_name",
        "Service",
        'label_values(function_calls_total{job="$job"}, service_name)',
        "All",
        true,
      ),
    )
    .withVariable(
      queryVariable(
        "module",
        "Module",
        'label_values(function_calls_total{job="$job", service_name=~"$service_name"}, module)',
        "All",
        true,
      ),
    )
    .withVariable(
      queryVariable(
        "function",
        "Function",
        'label_values(function_calls_total{job="$job", service_name=~"$service_name", module=~"$module"}, function)',
        "All",
        true,
      ),
    )
    .link(
      link(
        "Autometrics Overview",
        "/d/autometrics-overview/autometrics-overview?var-datasource=$datasource&var-job=$job",
        true,
      ),
    );

  [
    statPanel(1, "Call rate", { x: 0, y: 0, w: 4, h: 4 }, `sum(${callsRate})`, "ops", 2),
    statPanel(
      2,
      "Error ratio",
      { x: 4, y: 0, w: 4, h: 4 },
      boundedRatio(`sum(${errorCallsRate}) or vector(0)`, `sum(${callsRate})`),
      "percentunit",
      3,
    ),
    statPanel(
      3,
      "p95 latency",
      { x: 8, y: 0, w: 4, h: 4 },
      `max(function_calls_duration_seconds{${selector}, quantile="0.95"})`,
      "s",
      4,
    ),
    statPanel(
      4,
      "Concurrent",
      { x: 12, y: 0, w: 4, h: 4 },
      `sum(function_calls_concurrent{${selector}})`,
      "short",
      0,
    ),
    statPanel(
      5,
      "Calls in range",
      { x: 16, y: 0, w: 4, h: 4 },
      `sum(increase(function_calls_total{${selector}}[$__range]))`,
      "short",
      0,
    ),
    statPanel(
      6,
      "Selected functions",
      { x: 20, y: 0, w: 4, h: 4 },
      `count(count by (function, module) (function_calls_total{${selector}}))`,
      "short",
      0,
    ),
    timeseriesPanel(
      10,
      "Request rate",
      { x: 0, y: 4, w: 12, h: 8 },
      [
        prom(
          `sum by (function, module, result) (${callsRate})`,
          "{{module}}::{{function}} {{result}}",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      11,
      "Error ratio",
      { x: 12, y: 4, w: 12, h: 8 },
      [
        prom(
          `100 * ${boundedRatio(errorsByFunction, callsByFunction)}`,
          "{{module}}::{{function}}",
        ),
      ],
      "percent",
      3,
    ),
    timeseriesPanel(
      12,
      "Latency quantiles",
      { x: 0, y: 12, w: 12, h: 8 },
      [
        prom(
          `max by (function, module) (function_calls_duration_seconds{${selector}, quantile="0.5"})`,
          "p50 {{module}}::{{function}}",
        ),
        prom(
          `max by (function, module) (function_calls_duration_seconds{${selector}, quantile="0.95"})`,
          "p95 {{module}}::{{function}}",
          "B",
        ),
        prom(
          `max by (function, module) (function_calls_duration_seconds{${selector}, quantile="0.99"})`,
          "p99 {{module}}::{{function}}",
          "C",
        ),
      ],
      "s",
      4,
    ),
    timeseriesPanel(
      13,
      "Concurrent calls",
      { x: 12, y: 12, w: 12, h: 8 },
      [
        prom(
          `sum by (function, module) (function_calls_concurrent{${selector}})`,
          "{{module}}::{{function}}",
        ),
      ],
      "short",
      0,
    ),
    tablePanel(
      14,
      "Caller breakdown",
      { x: 0, y: 20, w: 12, h: 8 },
      [
        prom(
          `sum by (caller_module, caller_function, function, module) (increase(function_calls_total{${selector}}[$__range]))`,
          "{{caller_module}}::{{caller_function}} -> {{module}}::{{function}}",
          "A",
          true,
          "table",
        ),
      ],
      "short",
    ),
    tablePanel(
      15,
      "Result breakdown",
      { x: 12, y: 20, w: 12, h: 8 },
      [
        prom(
          `sum by (function, module, result) (increase(function_calls_total{${selector}}[$__range]))`,
          "{{module}}::{{function}} {{result}}",
          "A",
          true,
          "table",
        ),
      ],
      "short",
    ),
  ].forEach((panel) => builder.withPanel(panel));

  return builder.build();
}
