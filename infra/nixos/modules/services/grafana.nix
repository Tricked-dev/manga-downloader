{
  config,
  infraLib,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.infra;
  h = infraLib.hardening;
  topology = infraLib.observabilityTopology;
  hostName = topology.hostName;
  grafanaDomain = "grafana.${cfg.domain}";

  promTargetRef = refId: expr: legend: {
    datasource = {
      type = "victoriametrics-metrics-datasource";
      uid = "victoriametrics";
    };
    editorMode = "code";
    inherit expr;
    legendFormat = legend;
    range = true;
    inherit refId;
  };

  promTarget = promTargetRef "A";

  logsTarget = expr: queryType: {
    datasource = {
      type = "victoriametrics-logs-datasource";
      uid = "victorialogs";
    };
    inherit expr;
    inherit queryType;
    refId = "A";
  };

  ferronSelector = ''job="${topology.jobs.ferron}"'';
  criticalUnitMatcher = lib.concatStringsSep "|" topology.criticalUnits;
  stackJobMatcher = lib.concatStringsSep "|" (
    (map (target: target.job) topology.grafanaStackTargets)
    ++ [
      "integrations/self"
      "integrations/unix"
    ]
  );

  mangaStorageUsedRatioExpr = jobSelector: ''
    (
      manga_server_download_storage_bytes{job="${jobSelector}", kind="used"}
      / on(job, instance) manga_server_download_storage_bytes{job="${jobSelector}", kind="limit"}
    )
    and on(job, instance) (manga_server_download_storage_bytes{job="${jobSelector}", kind="limit"} > 0)
    or on() vector(0)
  '';

  mangaMetricsPresentExpr = jobSelector: ''
    max(manga_server_download_storage_bytes{job="${jobSelector}", kind="limit"} >= bool 0)
    or on() vector(0)
  '';

  gridPos = x: y: w: h: {
    inherit
      x
      y
      w
      h
      ;
  };

  thresholds = steps: {
    mode = "absolute";
    inherit steps;
  };

  neutralThresholds = thresholds [
    {
      color = "green";
      value = null;
    }
  ];

  healthThresholds = thresholds [
    {
      color = "red";
      value = null;
    }
    {
      color = "green";
      value = 1;
    }
  ];

  percentThresholds = thresholds [
    {
      color = "green";
      value = null;
    }
    {
      color = "orange";
      value = 70;
    }
    {
      color = "red";
      value = 85;
    }
  ];

  errorRateThresholds = thresholds [
    {
      color = "green";
      value = null;
    }
    {
      color = "orange";
      value = 0.01;
    }
    {
      color = "red";
      value = 0.1;
    }
  ];

  rowPanel = id: title: y: {
    inherit id title;
    collapsed = false;
    datasource = {
      type = "datasource";
      uid = "-- Mixed --";
    };
    gridPos = gridPos 0 y 24 1;
    panels = [ ];
    type = "row";
  };

  timeseriesPanel = id: title: grid: targets: unit: {
    inherit id title targets;
    datasource = {
      type = "datasource";
      uid = "-- Mixed --";
    };
    fieldConfig.defaults = {
      inherit unit;
      noValue = "0";
      custom = {
        drawStyle = "line";
        fillOpacity = 6;
        lineInterpolation = "linear";
        lineWidth = 2;
        pointSize = 5;
        showPoints = "never";
        spanNulls = false;
      };
    };
    fieldConfig.overrides = [ ];
    gridPos = grid;
    options = {
      legend = {
        calcs = [ "lastNotNull" ];
        displayMode = "table";
        placement = "bottom";
        showLegend = true;
      };
      tooltip = {
        mode = "multi";
        sort = "none";
      };
    };
    type = "timeseries";
  };

  logsTimeseriesPanel = id: title: grid: expr: unit: {
    inherit id title;
    datasource = {
      type = "victoriametrics-logs-datasource";
      uid = "victorialogs";
    };
    fieldConfig.defaults = {
      inherit unit;
      noValue = "0";
      custom = {
        drawStyle = "bars";
        fillOpacity = 25;
        lineWidth = 1;
        showPoints = "never";
        spanNulls = false;
      };
    };
    fieldConfig.overrides = [ ];
    gridPos = grid;
    options = {
      legend = {
        calcs = [ "lastNotNull" ];
        displayMode = "table";
        placement = "bottom";
        showLegend = true;
      };
      tooltip = {
        mode = "multi";
        sort = "desc";
      };
    };
    targets = [ (logsTarget expr "statsRange") ];
    type = "timeseries";
  };

  statPanel = id: title: grid: targets: unit: panelThresholds: {
    inherit id title targets;
    datasource = {
      type = "datasource";
      uid = "-- Mixed --";
    };
    fieldConfig = {
      defaults = {
        inherit unit;
        color.mode = "thresholds";
        noValue = "0";
        thresholds = panelThresholds;
      };
      overrides = [ ];
    };
    gridPos = grid;
    options = {
      colorMode = "value";
      graphMode = "area";
      justifyMode = "auto";
      orientation = "auto";
      reduceOptions = {
        calcs = [ "lastNotNull" ];
        fields = "";
        values = false;
      };
      textMode = "auto";
    };
    type = "stat";
  };

  logsPanel = id: title: grid: expr: {
    inherit id title;
    datasource = {
      type = "victoriametrics-logs-datasource";
      uid = "victorialogs";
    };
    gridPos = grid;
    options = {
      dedupStrategy = "none";
      enableLogDetails = true;
      prettifyLogMessage = false;
      showLabels = true;
      showTime = true;
      sortOrder = "Descending";
      wrapLogMessage = true;
    };
    targets = [ (logsTarget ''${expr} !~"missing _msg field"'' "instant") ];
    type = "logs";
  };

  baseDashboard = {
    annotations.list = [
      {
        builtIn = 1;
        datasource = {
          type = "grafana";
          uid = "-- Grafana --";
        };
        enable = true;
        hide = true;
        iconColor = "rgba(0, 211, 255, 1)";
        name = "Annotations & Alerts";
        target = {
          limit = 100;
          matchAny = false;
          tags = [ ];
          type = "dashboard";
        };
        type = "dashboard";
      }
    ];
    editable = true;
    fiscalYearStartMonth = 0;
    graphTooltip = 1;
    links = [ ];
    liveNow = false;
    refresh = "1m";
    schemaVersion = 39;
    style = "dark";
    time = {
      from = "now-3h";
      to = "now";
    };
    timezone = "browser";
    version = 1;
    weekStart = "";
  };

  infraDashboard = pkgs.writeText "infra-observability.json" (
    builtins.toJSON (
      baseDashboard
      // {
        title = "Infra Observability";
        uid = "infra-observability";
        tags = [
          "infra"
          "alloy"
          "victoriametrics"
        ];
        templating.list = [
          {
            allValue = ".*";
            current = {
              selected = true;
              text = "All";
              value = "$__all";
            };
            datasource = {
              type = "victoriametrics-metrics-datasource";
              uid = "victoriametrics";
            };
            definition = "label_values(node_uname_info, instance)";
            includeAll = true;
            label = "Instance";
            multi = true;
            name = "instance";
            query = {
              query = "label_values(node_uname_info, instance)";
              refId = "StandardVariableQuery";
            };
            refresh = 1;
            regex = "";
            sort = 1;
            type = "query";
          }
        ];
        panels = [
          (rowPanel 100 "At a glance" 0)
          (statPanel 1 "Critical Services Active" (gridPos 0 1 6 4)
            [
              (promTarget ''sum(node_systemd_unit_state{name=~"${criticalUnitMatcher}", state="active"})'' "active")
            ]
            "short"
            (thresholds [
              {
                color = "red";
                value = null;
              }
              {
                color = "green";
                value = builtins.length topology.criticalUnits;
              }
            ])
          )
          (statPanel 2 "Manga Storage Used" (gridPos 6 1 6 4) [
            (promTarget "100 * (${mangaStorageUsedRatioExpr "manga-server"})" "used")
          ] "percent" percentThresholds)
          (statPanel 3 "HTTP 5xx Rate" (gridPos 12 1 6 4) [
            (promTarget ''sum(rate(http_server_request_duration_seconds_count{http_response_status_code=~"5.."}[$__rate_interval]))'' "5xx/s")
          ] "reqps" errorRateThresholds)
          (statPanel 4 "Root Filesystem Used" (gridPos 18 1 6 4) [
            (promTarget ''100 * (1 - (node_filesystem_avail_bytes{instance=~"$instance", mountpoint="/", fstype!~"tmpfs|overlay"} / node_filesystem_size_bytes{instance=~"$instance", mountpoint="/", fstype!~"tmpfs|overlay"}))'' "{{instance}}")
          ] "percent" percentThresholds)
          (rowPanel 101 "Host USE" 5)
          (timeseriesPanel 5 "CPU Utilization" (gridPos 0 6 12 7) [
            (promTarget ''100 * (1 - avg by (instance) (rate(node_cpu_seconds_total{instance=~"$instance", mode="idle"}[$__rate_interval])))'' "{{instance}}")
          ] "percent")
          (timeseriesPanel 6 "Memory Utilization" (gridPos 12 6 12 7) [
            (promTarget ''100 * (1 - (node_memory_MemAvailable_bytes{instance=~"$instance"} / node_memory_MemTotal_bytes{instance=~"$instance"}))'' "{{instance}}")
          ] "percent")
          (timeseriesPanel 7 "Root Filesystem Utilization" (gridPos 0 13 12 7) [
            (promTarget ''100 * (1 - (node_filesystem_avail_bytes{instance=~"$instance", mountpoint="/", fstype!~"tmpfs|overlay"} / node_filesystem_size_bytes{instance=~"$instance", mountpoint="/", fstype!~"tmpfs|overlay"}))'' "{{instance}}")
          ] "percent")
          (timeseriesPanel 8 "Disk IO Throughput" (gridPos 12 13 12 7) [
            (promTargetRef "A"
              ''sum by (instance) (rate(node_disk_read_bytes_total{instance=~"$instance"}[$__rate_interval]))''
              "read {{instance}}"
            )
            (promTargetRef "B"
              ''sum by (instance) (rate(node_disk_written_bytes_total{instance=~"$instance"}[$__rate_interval]))''
              "write {{instance}}"
            )
          ] "Bps")
          (rowPanel 102 "Service RED" 20)
          (timeseriesPanel 9 "HTTP Request Rate" (gridPos 0 21 12 7) [
            (promTarget "sum by (job, http_route, http_response_status_code) (rate(http_server_request_duration_seconds_count[$__rate_interval]))" "{{job}} {{http_route}} {{http_response_status_code}}")
          ] "reqps")
          (timeseriesPanel 10 "HTTP Latency p95" (gridPos 12 21 12 7) [
            (promTarget "histogram_quantile(0.95, sum by (job, le, http_route) (rate(http_server_request_duration_seconds_bucket[$__rate_interval])))" "{{job}} {{http_route}}")
          ] "s")
          (rowPanel 103 "Service health" 28)
          (timeseriesPanel 11 "Systemd Unit State" (gridPos 0 29 24 7) [
            (promTarget ''max by (name, state) (node_systemd_unit_state{name=~"${criticalUnitMatcher}"})'' "{{name}} {{state}}")
          ] "short")
        ];
      }
    )
  );

  logsDashboard = pkgs.writeText "logs.json" (
    builtins.toJSON (
      baseDashboard
      // {
        title = "Logs";
        uid = "logs";
        tags = [
          "infra"
          "victorialogs"
        ];
        templating.list = [
          {
            allValue = ".*";
            current = {
              selected = true;
              text = "All";
              value = "$__all";
            };
            datasource = {
              type = "victoriametrics-logs-datasource";
              uid = "victorialogs";
            };
            definition = ''label_values({host="${hostName}"}, unit)'';
            includeAll = true;
            label = "Unit";
            multi = true;
            name = "unit";
            query = {
              field = "unit";
              limit = 1000;
              query = ''{host="${hostName}"}'';
              refId = "VictoriaLogsVariableQueryEditor-VariableQuery";
              type = "fieldValue";
            };
            refresh = 1;
            regex = "";
            sort = 1;
            type = "query";
          }
        ];
        panels = [
          (rowPanel 100 "Signal" 0)
          (statPanel 1 "Errors and Warnings" (gridPos 0 1 8 4)
            [
              {
                datasource = {
                  type = "victoriametrics-logs-datasource";
                  uid = "victorialogs";
                };
                expr = ''{host="${hostName}", unit=~"$unit"} ~"(?i)error|warn|failed|panic" | stats count()'';
                queryType = "statsRange";
                refId = "A";
              }
            ]
            "short"
            (thresholds [
              {
                color = "green";
                value = null;
              }
              {
                color = "orange";
                value = 1;
              }
              {
                color = "red";
                value = 10;
              }
            ])
          )
          (logsTimeseriesPanel 2 "Problem Log Events by Unit" (gridPos 8 1 16 4)
            ''{host="${hostName}", unit=~"$unit"} ~"(?i)error|warn|failed|panic|timeout|refused" | stats by (unit) count()''
            "short"
          )
          (rowPanel 101 "Logs" 5)
          (logsPanel 3 "Selected Unit Logs" (gridPos 0 6 24 9) ''{host="${hostName}", unit=~"$unit"}'')
          (logsPanel 4 "Manga Downloader Warnings" (gridPos 0 15 12 8)
            ''{host="${hostName}", unit="manga-downloader.service"} ~"(?i)warn|error|failed|panic|timeout|refused"''
          )
          (logsPanel 5 "Ferron Access and Errors" (gridPos 12 15 12
            8
          ) ''{host="${hostName}", unit="ferron.service"}'')
        ];
      }
    )
  );

  ferronEdgeDashboard = pkgs.writeText "ferron-edge.json" (
    builtins.toJSON (
      baseDashboard
      // {
        title = "Ferron Edge";
        uid = "ferron-edge";
        tags = [
          "infra"
          "ferron"
          "edge"
        ];
        templating.list = [ ];
        panels = [
          (rowPanel 100 "Edge RED" 0)
          (statPanel 1 "Request Rate" (gridPos 0 1 6 4) [
            (promTarget "sum(rate(ferron_http_server_request_count_total{${ferronSelector}}[$__rate_interval])) or sum(rate(ferron_http_server_request_count{${ferronSelector}}[$__rate_interval]))" "requests/s")
          ] "reqps" neutralThresholds)
          (statPanel 2 "5xx Rate" (gridPos 6 1 6 4) [
            (promTarget ''sum(rate(ferron_http_server_request_count_total{${ferronSelector}, http_response_status_code=~"5.."}[$__rate_interval])) or sum(rate(ferron_http_server_request_count{${ferronSelector}, http_response_status_code=~"5.."}[$__rate_interval]))'' "5xx/s")
          ] "reqps" errorRateThresholds)
          (statPanel 3 "Active Requests" (gridPos 12 1 6 4) [
            (promTarget "sum(http_server_active_requests{${ferronSelector}})" "active")
          ] "short" neutralThresholds)
          (statPanel 4 "Unit Active" (gridPos 18 1 6 4) [
            (promTarget ''max(node_systemd_unit_state{name="ferron.service", state="active"})'' "active")
          ] "short" healthThresholds)
          (timeseriesPanel 5 "Request Rate by Method" (gridPos 0 5 12 7) [
            (promTarget "sum by (http_request_method) (rate(ferron_http_server_request_count_total{${ferronSelector}}[$__rate_interval])) or sum by (http_request_method) (rate(ferron_http_server_request_count{${ferronSelector}}[$__rate_interval]))" "{{http_request_method}}")
          ] "reqps")
          (timeseriesPanel 6 "Request Rate by Status" (gridPos 12 5 12 7) [
            (promTarget "sum by (http_response_status_code) (rate(ferron_http_server_request_count_total{${ferronSelector}}[$__rate_interval])) or sum by (http_response_status_code) (rate(ferron_http_server_request_count{${ferronSelector}}[$__rate_interval]))" "{{http_response_status_code}}")
          ] "reqps")
          (rowPanel 101 "Latency and proxying" 12)
          (timeseriesPanel 7 "Request Latency p95/p99" (gridPos 0 13 12 7) [
            (promTargetRef "A"
              "histogram_quantile(0.95, sum by (le) (rate(http_server_request_duration_seconds_bucket{${ferronSelector}}[$__rate_interval])))"
              "p95"
            )
            (promTargetRef "B"
              "histogram_quantile(0.99, sum by (le) (rate(http_server_request_duration_seconds_bucket{${ferronSelector}}[$__rate_interval])))"
              "p99"
            )
          ] "s")
          (timeseriesPanel 8 "Proxy Requests" (gridPos 12 13 12 7) [
            (promTargetRef "A"
              "sum(rate(ferron_proxy_requests_total{${ferronSelector}}[$__rate_interval])) or sum(rate(ferron_proxy_requests{${ferronSelector}}[$__rate_interval]))"
              "proxy requests/s"
            )
          ] "reqps")
          (timeseriesPanel 9 "Selected Backends" (gridPos 0 20 12 7) [
            (promTargetRef "A"
              "sum by (ferron_proxy_backend_url) (rate(ferron_proxy_backends_selected_total{${ferronSelector}}[$__rate_interval])) or sum by (ferron_proxy_backend_url) (rate(ferron_proxy_backends_selected{${ferronSelector}}[$__rate_interval]))"
              "{{ferron_proxy_backend_url}}"
            )
          ] "short")
          (rowPanel 102 "Runtime and dependencies" 27)
          (timeseriesPanel 10 "Ferron Process Memory" (gridPos 0 28 8 7) [
            (promTarget "process_memory_usage_bytes{${ferronSelector}} or process_resident_memory_bytes{${ferronSelector}}" "memory")
          ] "bytes")
          (timeseriesPanel 11 "Ferron Process CPU" (gridPos 8 28 8 7) [
            (promTargetRef "A"
              "sum(rate(process_cpu_time_seconds_total{${ferronSelector}}[$__rate_interval])) or sum(rate(process_cpu_seconds_total{${ferronSelector}}[$__rate_interval]))"
              "cpu"
            )
          ] "short")
          (timeseriesPanel 12 "Ferron Restarts" (gridPos 16 28 8 7) [
            (promTarget ''node_systemd_service_restart_total{name="ferron.service"} or on() vector(0)'' "restarts")
          ] "short")
          (logsPanel 13 "Ferron Warnings and Errors" (gridPos 0 35 24 8)
            ''{host="${hostName}", unit="ferron.service"} ~"(?i)warn|error|failed|panic|timeout|refused|acme"''
          )
        ];
      }
    )
  );

  observabilityStackDashboard = pkgs.writeText "observability-stack.json" (
    builtins.toJSON (
      baseDashboard
      // {
        title = "Observability Stack";
        uid = "observability-stack";
        tags = [
          "infra"
          "grafana"
          "victoriametrics"
          "victorialogs"
          "victoriatraces"
        ];
        templating.list = [
          {
            allValue = ".*";
            current = {
              selected = true;
              text = "All";
              value = "$__all";
            };
            datasource = {
              type = "victoriametrics-metrics-datasource";
              uid = "victoriametrics";
            };
            definition = ''label_values(up{job=~"${stackJobMatcher}"}, job)'';
            includeAll = true;
            label = "Job";
            multi = true;
            name = "job";
            query = {
              query = ''label_values(up{job=~"${stackJobMatcher}"}, job)'';
              refId = "StandardVariableQuery";
            };
            refresh = 1;
            regex = "";
            sort = 1;
            type = "query";
          }
        ];
        panels = [
          (rowPanel 100 "Stack health" 0)
          (statPanel 1 "Selected Targets Healthy" (gridPos 0 1 6 4) [
            (promTarget ''min(up{job=~"$job"})'' "healthy")
          ] "short" healthThresholds)
          (statPanel 2 "Grafana 5xx Rate" (gridPos 6 1 6 4) [
            (promTarget ''sum(rate(grafana_http_request_duration_seconds_count{job="grafana", status_code=~"5.."}[$__rate_interval]))'' "5xx/s")
          ] "reqps" errorRateThresholds)
          (statPanel 3 "Scrape Samples" (gridPos 12 1 6 4) [
            (promTarget ''sum(scrape_samples_scraped{job=~"$job"})'' "samples")
          ] "short" neutralThresholds)
          (statPanel 4 "Stack Processes" (gridPos 18 1 6 4) [
            (promTarget ''count(process_start_time_seconds{job=~"$job"})'' "processes")
          ] "short" neutralThresholds)
          (timeseriesPanel 5 "Target Up" (gridPos 0 5 12 7) [
            (promTarget ''max by (job) (up{job=~"$job"})'' "{{job}}")
          ] "short")
          (timeseriesPanel 6 "Scrape Duration" (gridPos 12 5 12 7) [
            (promTarget ''max by (job) (scrape_duration_seconds{job=~"$job"})'' "{{job}}")
          ] "s")
          (rowPanel 101 "Collector load" 12)
          (timeseriesPanel 7 "Samples Scraped" (gridPos 0 13 12 7) [
            (promTarget ''sum by (job) (scrape_samples_scraped{job=~"$job"})'' "{{job}}")
          ] "short")
          (timeseriesPanel 8 "Process Memory" (gridPos 12 13 12 7) [
            (promTarget ''max by (job) (process_resident_memory_bytes{job=~"$job"})'' "{{job}}")
          ] "bytes")
          (timeseriesPanel 9 "Process CPU" (gridPos 0 20 12 7) [
            (promTarget ''sum by (job) (rate(process_cpu_seconds_total{job=~"$job"}[$__rate_interval]))'' "{{job}}")
          ] "short")
          (timeseriesPanel 10 "Open File Descriptors" (gridPos 12 20 12 7) [
            (promTarget ''max by (job) (process_open_fds{job=~"$job"})'' "{{job}}")
          ] "short")
          (rowPanel 102 "Grafana RED" 27)
          (timeseriesPanel 11 "Grafana Request Rate by Status" (gridPos 0 28 12 7) [
            (promTarget ''sum by (handler, status_code) (rate(grafana_http_request_duration_seconds_count{job="grafana"}[$__rate_interval]))'' "{{handler}} {{status_code}}")
          ] "reqps")
          (timeseriesPanel 12 "Grafana Request Latency p95" (gridPos 12 28 12 7) [
            (promTarget ''histogram_quantile(0.95, sum by (handler, le) (rate(grafana_http_request_duration_seconds_bucket{job="grafana"}[$__rate_interval])))'' "{{handler}}")
          ] "s")
          (rowPanel 103 "Stack logs" 35)
          (logsPanel 13 "Stack Warnings and Errors" (gridPos 0 36 24 8)
            ''{host="${hostName}", unit=~"alloy.service|grafana.service|victorialogs.service|victoriametrics.service|victoriatraces.service"} ~"(?i)warn|error|failed|panic|timeout|refused"''
          )
        ];
      }
    )
  );

  dashboardDir =
    pkgs.runCommand "grafana-dashboards" { nativeBuildInputs = [ pkgs.buildPackages.jq ]; }
      ''
        mkdir -p \
          "$out/applications" \
          "$out/edge" \
          "$out/infra-overview" \
          "$out/observability-stack" \
          "$out/troubleshooting"

        job_query='label_values({__name__=~"manga_server_.*|http_server_request_duration_seconds_count|function_calls_total|manga_server_tokio_.*"}, job)'
        storage_ratio_query=${lib.escapeShellArg (mangaStorageUsedRatioExpr "$job")}
        liveness_query=${lib.escapeShellArg (mangaMetricsPresentExpr "$job")}
        for dashboard in ${./manga-dashboards}/*.json; do
          name="$(basename "$dashboard")"
          jq \
            --arg name "$name" \
            --arg job_query "$job_query" \
            --arg storage_ratio_query "$storage_ratio_query" \
            --arg liveness_query "$liveness_query" \
            '
              .refresh = "1m" |
              .graphTooltip = 1 |
              .timezone = "browser" |
              .tags = ((.tags // []) + ["manga", "imported"] | unique) |
              if (.time? | type) == "object" then
                .time.from = "now-3h" |
                .time.to = "now"
              else
                .time = {"from": "now-3h", "to": "now"}
              end |
              (.. | objects | select(.expr? == "manga_server:download_storage_used_ratio{job=\"$job\"}") | .expr) = $storage_ratio_query |
              (.. | objects | select(.expr? == "min(up{job=\"$job\"})") | .expr) = $liveness_query |
              (.. | objects | select(.datasource?.uid? == "prometheus") | .datasource) = {
                type: "victoriametrics-metrics-datasource",
                uid: "victoriametrics"
              } |
              (.. | objects | select(.datasource?.uid? == "''${datasource}") | .datasource.type) = "victoriametrics-metrics-datasource" |
              (.templating.list[]? | select(.name == "datasource" and .type == "datasource") | .query) = "victoriametrics-metrics-datasource" |
              (.templating.list[]? | select(.name == "datasource" and .type == "datasource") | .current) = {
                selected: false,
                text: "VictoriaMetrics",
                value: "victoriametrics"
              } |
              if $name == "manga-server-overview.json" then
              (.templating.list[]? | select(.name == "job") | .definition) = $job_query |
              (.templating.list[]? | select(.name == "job") | .query.query) = $job_query
              else
                .
              end
            ' "$dashboard" > "$out/applications/$name"
        done

        cp ${infraDashboard} "$out/infra-overview/infra-observability.json"
        cp ${logsDashboard} "$out/troubleshooting/logs.json"
        cp ${ferronEdgeDashboard} "$out/edge/ferron-edge.json"
        cp ${observabilityStackDashboard} "$out/observability-stack/observability-stack.json"
      '';
in
{
  config = {
    services.grafana = {
      enable = true;
      dataDir = "/var/lib/grafana";
      declarativePlugins = with pkgs.grafanaPlugins; [
        grafana-metricsdrilldown-app
        victoriametrics-logs-datasource
        victoriametrics-metrics-datasource
      ];
      settings = {
        analytics = {
          reporting_enabled = false;
          check_for_updates = false;
          check_for_plugin_updates = false;
          feedback_links_enabled = false;
        };
        auth = {
          disable_login_form = true;
          oauth_auto_login = true;
        };
        "auth.anonymous".enabled = false;
        "auth.basic".enabled = false;
        "auth.generic_oauth" = {
          enabled = true;
          name = "Kanidm";
          allow_sign_up = false;
          auto_login = true;
          client_id = "grafana";
          client_secret = "$__env{GF_AUTH_GENERIC_OAUTH_CLIENT_SECRET}";
          scopes = "openid profile email groups_name";
          auth_url = "https://auth.${cfg.domain}/ui/oauth2";
          token_url = "https://auth.${cfg.domain}/oauth2/token";
          api_url = "https://auth.${cfg.domain}/oauth2/openid/grafana/userinfo";
          auth_style = "InHeader";
          login_attribute_path = "preferred_username || sub";
          name_attribute_path = "name || preferred_username || sub";
          email_attribute_path = "email";
          groups_attribute_path = "groups";
          role_attribute_path = "(contains(groups[*], 'grafana_admins') || contains(groups[*], 'grafana_admins@auth.${cfg.domain}') || contains(groups[*], 'idm_admins') || contains(groups[*], 'idm_admins@auth.${cfg.domain}')) && 'GrafanaAdmin' || (contains(groups[*], 'grafana_editors') || contains(groups[*], 'grafana_editors@auth.${cfg.domain}')) && 'Editor' || 'Viewer'";
          allow_assign_grafana_admin = true;
          role_attribute_strict = false;
          use_pkce = true;
          use_refresh_token = true;
        };
        database.wal = true;
        metrics = {
          enabled = true;
          disable_total_stats = false;
        };
        news.news_feed_enabled = false;
        security = {
          admin_password = "$__env{GF_SECURITY_ADMIN_PASSWORD}";
          secret_key = "$__env{GF_SECURITY_SECRET_KEY}";
          cookie_secure = true;
          disable_gravatar = true;
          strict_transport_security = true;
          strict_transport_security_max_age_seconds = 31536000;
          strict_transport_security_subdomains = true;
          x_content_type_options = true;
        };
        server = {
          domain = grafanaDomain;
          enforce_domain = true;
          enable_gzip = true;
          http_addr = "127.0.0.1";
          http_port = 3000;
          read_timeout = "30s";
          root_url = "https://${grafanaDomain}/";
        };
        snapshots.external_enabled = false;
        users = {
          allow_org_create = false;
          allow_sign_up = false;
          auto_assign_org = true;
          auto_assign_org_role = "Viewer";
        };
      };
      provision = {
        enable = true;
        datasources.settings = {
          apiVersion = 1;
          datasources = [
            {
              name = "VictoriaMetrics";
              type = "victoriametrics-metrics-datasource";
              uid = "victoriametrics";
              access = "proxy";
              url = "http://${topology.endpoints.victoriametrics}";
              isDefault = true;
              editable = false;
              jsonData = {
                httpMethod = "POST";
              };
            }
            {
              name = "VictoriaLogs";
              type = "victoriametrics-logs-datasource";
              uid = "victorialogs";
              access = "proxy";
              url = "http://${topology.endpoints.victorialogs}";
              editable = false;
            }
            {
              name = "VictoriaTraces";
              type = "jaeger";
              uid = "victoriatraces";
              access = "proxy";
              url = "http://${topology.endpoints.victoriatraces}/select/jaeger";
              editable = false;
            }
          ];
        };
        dashboards.settings = {
          apiVersion = 1;
          providers = [
            {
              name = "infra-overview";
              orgId = 1;
              folder = "Infra Overview";
              type = "file";
              disableDeletion = false;
              allowUiUpdates = false;
              updateIntervalSeconds = 30;
              options.path = "${dashboardDir}/infra-overview";
            }
            {
              name = "edge";
              orgId = 1;
              folder = "Edge";
              type = "file";
              disableDeletion = false;
              allowUiUpdates = false;
              updateIntervalSeconds = 30;
              options.path = "${dashboardDir}/edge";
            }
            {
              name = "observability-stack";
              orgId = 1;
              folder = "Observability Stack";
              type = "file";
              disableDeletion = false;
              allowUiUpdates = false;
              updateIntervalSeconds = 30;
              options.path = "${dashboardDir}/observability-stack";
            }
            {
              name = "troubleshooting";
              orgId = 1;
              folder = "Troubleshooting";
              type = "file";
              disableDeletion = false;
              allowUiUpdates = false;
              updateIntervalSeconds = 30;
              options.path = "${dashboardDir}/troubleshooting";
            }
            {
              name = "applications";
              orgId = 1;
              folder = "Applications";
              type = "file";
              disableDeletion = false;
              allowUiUpdates = false;
              updateIntervalSeconds = 30;
              options.path = "${dashboardDir}/applications";
            }
          ];
        };
      };
    };

    systemd.services.grafana = {
      after = [ "sops-install-secrets.service" ];
      requires = [ "sops-install-secrets.service" ];
      serviceConfig = h.networkService // {
        AmbientCapabilities = "";
        CapabilityBoundingSet = "";
        DeviceAllow = lib.mkForce [ "" ];
        DevicePolicy = lib.mkForce "closed";
        EnvironmentFile = [ config.sops.secrets."grafana.env".path ];
        ExecPaths = [ "/nix/store" ];
        IPAddressAllow = [
          "127.0.0.0/8"
          "::1/128"
        ];
        IPAddressDeny = "any";
        NoExecPaths = [ "/" ];
        PrivateIPC = true;
        PrivateMounts = true;
        PrivateUsers = true;
        ProtectSystem = lib.mkForce "strict";
        ReadWritePaths = [ "/var/lib/grafana" ];
        RestrictNetworkInterfaces = [ "lo" ];
        RuntimeDirectoryMode = lib.mkForce "0750";
        SystemCallFilter = lib.mkForce h.dangerousSyscallDeny;
        UMask = lib.mkForce "0077";
      };
    };
  };
}
