import * as dashboard from "@grafana/grafana-foundation-sdk/dashboard";

import {
  base,
  gaugePanel,
  prom,
  queryVariable,
  row,
  statPanel,
  timeseriesPanel,
} from "./dashboard-helpers.js";

export function tokioRuntime() {
  const targetPresent =
    'max(manga_server_tokio_workers_count{service_name="$job"}) or max(build_info{service_name="$job"}) or on() vector(0)';

  const builder = base(
    "Manga Server Tokio Runtime",
    "manga-server-tokio-runtime",
    ["manga-server", "prometheus", "tokio", "runtime"],
    "now-1h",
  )
    .description("Tokio runtime health and scheduler pressure for the manga downloader backend.")
    .withVariable(
      queryVariable(
        "job",
        "Service",
        'label_values({__name__=~"manga_server_tokio_workers_count|build_info"}, service_name)',
      ),
    );

  [
    statPanel(1, "Up", { x: 0, y: 0, w: 3, h: 4 }, targetPresent, "none", 0),
    statPanel(
      2,
      "Workers",
      { x: 3, y: 0, w: 3, h: 4 },
      'manga_server_tokio_workers_count{service_name="$job"}',
      "short",
      0,
    ),
    statPanel(
      3,
      "Live tasks",
      { x: 6, y: 0, w: 3, h: 4 },
      'manga_server_tokio_live_tasks_count{service_name="$job"}',
      "short",
      0,
    ),
    gaugePanel(4, "Busy", { x: 9, y: 0, w: 3, h: 4 }, 'manga_server_tokio_busy_ratio{service_name="$job"}'),
    statPanel(
      5,
      "Blocking Q",
      { x: 12, y: 0, w: 3, h: 4 },
      'manga_server_tokio_blocking_queue_depth{service_name="$job"}',
      "short",
      0,
    ),
    statPanel(
      6,
      "Forced yields/s",
      { x: 15, y: 0, w: 3, h: 4 },
      'sum(rate(manga_server_tokio_budget_forced_yield_count{service_name="$job"}[$__rate_interval]))',
      "ops",
      2,
    ),
    statPanel(
      7,
      "Poll p95",
      { x: 18, y: 0, w: 3, h: 4 },
      'max(manga_server_tokio_poll_time_histogram{service_name="$job", quantile="0.95"})',
      "ns",
      0,
    ),
    statPanel(
      8,
      "Polls/park",
      { x: 21, y: 0, w: 3, h: 4 },
      'manga_server_tokio_mean_polls_per_park{service_name="$job"}',
      "short",
      2,
    ),
    row(10, "Scheduler Pressure", 4),
    timeseriesPanel(
      11,
      "Queue depths",
      { x: 0, y: 5, w: 12, h: 8 },
      [
        prom('manga_server_tokio_global_queue_depth{service_name="$job"}', "global"),
        prom('manga_server_tokio_total_local_queue_depth{service_name="$job"}', "local total", "B"),
        prom('manga_server_tokio_max_local_queue_depth{service_name="$job"}', "local max", "C"),
        prom('manga_server_tokio_min_local_queue_depth{service_name="$job"}', "local min", "D"),
        prom('manga_server_tokio_blocking_queue_depth{service_name="$job"}', "blocking", "E"),
      ],
      "short",
      0,
    ),
    timeseriesPanel(
      12,
      "Task and blocking pool counts",
      { x: 12, y: 5, w: 12, h: 8 },
      [
        prom('manga_server_tokio_live_tasks_count{service_name="$job"}', "live tasks"),
        prom('manga_server_tokio_blocking_threads_count{service_name="$job"}', "blocking threads", "B"),
        prom(
          'manga_server_tokio_idle_blocking_threads_count{service_name="$job"}',
          "idle blocking threads",
          "C",
        ),
      ],
      "short",
      0,
    ),
    timeseriesPanel(
      13,
      "Scheduling operations",
      { x: 0, y: 13, w: 12, h: 8 },
      [
        prom(
          'rate(manga_server_tokio_num_remote_schedules{service_name="$job"}[$__rate_interval])',
          "remote",
        ),
        prom(
          'rate(manga_server_tokio_total_local_schedule_count{service_name="$job"}[$__rate_interval])',
          "local total",
          "B",
        ),
        prom(
          'rate(manga_server_tokio_max_local_schedule_count{service_name="$job"}[$__rate_interval])',
          "local worker max",
          "C",
        ),
        prom(
          'rate(manga_server_tokio_min_local_schedule_count{service_name="$job"}[$__rate_interval])',
          "local worker min",
          "D",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      14,
      "Polls, forced yields, and I/O readiness",
      { x: 12, y: 13, w: 12, h: 8 },
      [
        prom('rate(manga_server_tokio_total_polls_count{service_name="$job"}[$__rate_interval])', "polls"),
        prom(
          'rate(manga_server_tokio_budget_forced_yield_count{service_name="$job"}[$__rate_interval])',
          "forced yields",
          "B",
        ),
        prom(
          'rate(manga_server_tokio_io_driver_ready_count{service_name="$job"}[$__rate_interval])',
          "io ready",
          "C",
        ),
      ],
      "ops",
    ),
    row(20, "Poll and Busy Time", 21),
    timeseriesPanel(
      21,
      "Poll duration summary",
      { x: 0, y: 22, w: 12, h: 8 },
      [
        prom('manga_server_tokio_mean_poll_duration{service_name="$job"}', "mean"),
        prom('manga_server_tokio_mean_poll_duration_worker_max{service_name="$job"}', "worker max", "B"),
        prom('manga_server_tokio_mean_poll_duration_worker_min{service_name="$job"}', "worker min", "C"),
      ],
      "ns",
      0,
    ),
    timeseriesPanel(
      22,
      "Poll time quantiles",
      { x: 12, y: 22, w: 12, h: 8 },
      [
        prom(
          'manga_server_tokio_poll_time_histogram{service_name="$job", quantile=~"0.5|0.9|0.95|0.99|1.0"}',
          "p{{quantile}}",
        ),
      ],
      "ns",
      0,
    ),
    timeseriesPanel(
      23,
      "Poll counts by worker extrema",
      { x: 0, y: 30, w: 12, h: 8 },
      [
        prom('rate(manga_server_tokio_total_polls_count{service_name="$job"}[$__rate_interval])', "polls"),
        prom(
          'rate(manga_server_tokio_max_polls_count{service_name="$job"}[$__rate_interval])',
          "worker max",
          "B",
        ),
        prom(
          'rate(manga_server_tokio_min_polls_count{service_name="$job"}[$__rate_interval])',
          "worker min",
          "C",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      24,
      "Busy duration rate",
      { x: 12, y: 30, w: 12, h: 8 },
      [
        prom(
          'rate(manga_server_tokio_total_busy_duration{service_name="$job"}[$__rate_interval])',
          "total busy",
        ),
        prom(
          'rate(manga_server_tokio_max_busy_duration{service_name="$job"}[$__rate_interval])',
          "worker max",
          "B",
        ),
        prom(
          'rate(manga_server_tokio_min_busy_duration{service_name="$job"}[$__rate_interval])',
          "worker min",
          "C",
        ),
      ],
      "ns",
      0,
    ),
    row(30, "Worker Balance", 38),
    timeseriesPanel(
      31,
      "Parking and no-op wakeups",
      { x: 0, y: 39, w: 12, h: 8 },
      [
        prom('rate(manga_server_tokio_total_park_count{service_name="$job"}[$__rate_interval])', "parks"),
        prom(
          'rate(manga_server_tokio_total_noop_count{service_name="$job"}[$__rate_interval])',
          "no-op",
          "B",
        ),
        prom('manga_server_tokio_mean_polls_per_park{service_name="$job"}', "polls per park", "C"),
      ],
      "ops",
    ),
    timeseriesPanel(
      32,
      "Park/no-op worker extrema",
      { x: 12, y: 39, w: 12, h: 8 },
      [
        prom(
          'rate(manga_server_tokio_max_park_count{service_name="$job"}[$__rate_interval])',
          "park worker max",
        ),
        prom(
          'rate(manga_server_tokio_min_park_count{service_name="$job"}[$__rate_interval])',
          "park worker min",
          "B",
        ),
        prom(
          'rate(manga_server_tokio_max_noop_count{service_name="$job"}[$__rate_interval])',
          "no-op worker max",
          "C",
        ),
        prom(
          'rate(manga_server_tokio_min_noop_count{service_name="$job"}[$__rate_interval])',
          "no-op worker min",
          "D",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      33,
      "Task steals",
      { x: 0, y: 47, w: 12, h: 8 },
      [
        prom(
          'rate(manga_server_tokio_total_steal_count{service_name="$job"}[$__rate_interval])',
          "steal count",
        ),
        prom(
          'rate(manga_server_tokio_total_steal_operations{service_name="$job"}[$__rate_interval])',
          "steal ops",
          "B",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      34,
      "Steal worker extrema",
      { x: 12, y: 47, w: 12, h: 8 },
      [
        prom(
          'rate(manga_server_tokio_max_steal_count{service_name="$job"}[$__rate_interval])',
          "count worker max",
        ),
        prom(
          'rate(manga_server_tokio_min_steal_count{service_name="$job"}[$__rate_interval])',
          "count worker min",
          "B",
        ),
        prom(
          'rate(manga_server_tokio_max_steal_operations{service_name="$job"}[$__rate_interval])',
          "ops worker max",
          "C",
        ),
        prom(
          'rate(manga_server_tokio_min_steal_operations{service_name="$job"}[$__rate_interval])',
          "ops worker min",
          "D",
        ),
      ],
      "ops",
    ),
    row(40, "Overflow and Blocking", 55),
    timeseriesPanel(
      41,
      "Overflow events",
      { x: 0, y: 56, w: 12, h: 8 },
      [
        prom(
          'rate(manga_server_tokio_total_overflow_count{service_name="$job"}[$__rate_interval])',
          "total",
        ),
        prom(
          'rate(manga_server_tokio_max_overflow_count{service_name="$job"}[$__rate_interval])',
          "worker max",
          "B",
        ),
        prom(
          'rate(manga_server_tokio_min_overflow_count{service_name="$job"}[$__rate_interval])',
          "worker min",
          "C",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      42,
      "Blocking pool detail",
      { x: 12, y: 56, w: 12, h: 8 },
      [
        prom('manga_server_tokio_blocking_threads_count{service_name="$job"}', "blocking threads"),
        prom(
          'manga_server_tokio_idle_blocking_threads_count{service_name="$job"}',
          "idle blocking threads",
          "B",
        ),
        prom('manga_server_tokio_blocking_queue_depth{service_name="$job"}', "queue", "C"),
      ],
      "short",
      0,
    ),
    timeseriesPanel(
      43,
      "Poll histogram sample flow",
      { x: 0, y: 64, w: 12, h: 8 },
      [
        prom(
          'rate(manga_server_tokio_poll_time_histogram_count{service_name="$job"}[$__rate_interval])',
          "samples",
        ),
        prom(
          'rate(manga_server_tokio_poll_time_histogram_sum{service_name="$job"}[$__rate_interval])',
          "sample sum",
          "B",
        ),
      ],
      "ops",
    ),
  ].forEach((panel) =>
    panel instanceof dashboard.RowBuilder ? builder.withRow(panel) : builder.withPanel(panel),
  );

  return builder.build();
}
