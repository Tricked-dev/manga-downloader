{
  config,
  infraLib,
  lib,
  ...
}:

let
  h = infraLib.hardening;
  topology = infraLib.observabilityTopology;
  hostName = topology.hostName;

  alloyOtlpGrpcEndpoint = topology.endpoints.alloyOtlpGrpc;
  alloyOtlpHttpEndpoint = topology.endpoints.alloyOtlpHttp;

  journalUnits = topology.criticalUnits;

  alloySyscallDeny = h.dangerousSyscallDeny;

  alloyLabel =
    unit:
    lib.replaceStrings
      [
        "-"
        "."
        "@"
      ]
      [
        "_"
        "_"
        "_"
      ]
      unit;

  journalSourceBlocks = lib.concatMapStringsSep "\n" (unit: ''
    loki.source.journal "${alloyLabel unit}" {
      forward_to    = [loki.process.journal.receiver]
      labels        = {host = "${hostName}", unit = "${unit}"}
      matches       = "_SYSTEMD_UNIT=${unit}"
      max_age       = "12h"
      relabel_rules = loki.relabel.journal.rules
    }
  '') journalUnits;

  prometheusTarget = target: ''
    {
      "__address__" = "${target.address}",
      "job"         = "${target.job}",
      "instance"    = "${hostName}",
    },
  '';

  grafanaStackTargets = lib.concatMapStringsSep "\n" prometheusTarget topology.grafanaStackTargets;
in
{
  config = {
    environment.etc."alloy/config.alloy".text = ''
            logging {
              level  = "info"
              format = "logfmt"
            }

            otelcol.receiver.otlp "apps" {
              grpc {
                endpoint = "${alloyOtlpGrpcEndpoint}"
              }

              http {
                endpoint = "${alloyOtlpHttpEndpoint}"
              }

              output {
                logs    = [otelcol.processor.resourcedetection.default.input]
                metrics = [otelcol.processor.resourcedetection.default.input]
                traces  = [otelcol.processor.resourcedetection.default.input]
              }
            }

            otelcol.processor.resourcedetection "default" {
              detectors = ["env", "system"]

              output {
                logs    = [otelcol.processor.attributes.loki_labels.input]
                metrics = [otelcol.processor.transform.prometheus_labels.input]
                traces  = [otelcol.processor.batch.default.input]
              }
            }

            otelcol.processor.attributes "loki_labels" {
              action {
                action = "insert"
                key    = "loki.resource.labels"
                value  = "service.name,service.namespace,service.instance.id"
              }

              output {
                logs = [otelcol.processor.batch.default.input]
              }
            }

            otelcol.processor.transform "prometheus_labels" {
              error_mode = "ignore"

              metric_statements {
                context = "datapoint"
                statements = [
                  `set(datapoint.attributes["job"], resource.attributes["service.name"]) where resource.attributes["service.name"] != nil`,
                  `set(datapoint.attributes["instance"], "${hostName}")`,
                  `set(datapoint.attributes["service_namespace"], resource.attributes["service.namespace"]) where resource.attributes["service.namespace"] != nil`,
                ]
              }

              output {
                metrics = [otelcol.exporter.prometheus.local.input]
              }
            }

            otelcol.processor.batch "default" {
              output {
                logs   = [otelcol.exporter.loki.local.input]
                traces = [otelcol.exporter.otlphttp.victoriatraces.input]
              }
            }

            otelcol.exporter.prometheus "local" {
              forward_to = [prometheus.remote_write.local.receiver]
            }

            otelcol.exporter.loki "local" {
              forward_to = [loki.write.local.receiver]
            }

            otelcol.exporter.otlphttp "victoriatraces" {
              traces_endpoint = "http://${topology.endpoints.victoriatraces}/insert/opentelemetry/v1/traces"

              client {
                endpoint = "http://${topology.endpoints.victoriatraces}"
              }
            }

            prometheus.exporter.self "alloy" {}

            prometheus.exporter.unix "host" {
              enable_collectors        = ["processes", "systemd"]
              include_exporter_metrics = true

              filesystem {
                mount_points_exclude = "^/(dev|proc|run/credentials/.+|run/user/.+|sys|var/lib/docker/.+)($|/)"
              }

              systemd {
                enable_restarts = true
                start_time      = true
                task_metrics    = true
                unit_include    = "^(alloy|ferron|grafana|kanidm|manga-downloader|tailscaled|victorialogs|victoriametrics|victoriatraces)\\.service$"
              }
            }

            prometheus.remote_write "local" {
              endpoint {
                url = "http://${topology.endpoints.victoriametrics}/api/v1/write"
              }
            }

            prometheus.scrape "alloy" {
              targets         = prometheus.exporter.self.alloy.targets
              scrape_interval = "30s"
              forward_to      = [prometheus.remote_write.local.receiver]
            }

            prometheus.scrape "host" {
              targets         = prometheus.exporter.unix.host.targets
              scrape_interval = "30s"
              forward_to      = [prometheus.remote_write.local.receiver]
            }

            prometheus.scrape "grafana_stack" {
              targets = [
      ${grafanaStackTargets}
              ]

              scrape_interval = "30s"
              forward_to      = [prometheus.remote_write.local.receiver]
            }

            loki.write "local" {
              endpoint {
                url = "http://${topology.endpoints.victorialogs}/insert/loki/api/v1/push"
              }
            }

            loki.relabel "journal" {
              forward_to = []

              rule {
                source_labels = ["__journal_priority_keyword"]
                target_label  = "level"
              }

              rule {
                source_labels = ["__journal_syslog_identifier"]
                target_label  = "syslog_identifier"
              }
            }

            loki.process "journal" {
              stage.decolorize {}

              stage.static_labels {
                values = {
                  source = "systemd-journal",
                }
              }

              forward_to = [loki.write.local.receiver]
            }

            ${journalSourceBlocks}
    '';

    services.alloy = {
      enable = true;
      extraFlags = [
        "--disable-reporting"
        "--server.http.listen-addr=127.0.0.1:12345"
      ];
    };

    systemd.services.alloy = {
      wants = [
        "victorialogs.service"
        "victoriametrics.service"
        "victoriatraces.service"
      ];
      after = [
        "victorialogs.service"
        "victoriametrics.service"
        "victoriatraces.service"
      ];
      serviceConfig = h.networkService // {
        AmbientCapabilities = "";
        CapabilityBoundingSet = "";
        DeviceAllow = lib.mkForce [ "" ];
        DevicePolicy = lib.mkForce "closed";
        IPAddressAllow = [
          "127.0.0.0/8"
          "::1/128"
        ];
        IPAddressDeny = "any";
        MemoryDenyWriteExecute = lib.mkForce true;
        PrivateUsers = true;
        ProtectKernelLogs = lib.mkForce true;
        ProtectProc = lib.mkForce "invisible";
        ProcSubset = lib.mkForce "pid";
        RestartSec = lib.mkDefault "10s";
        SystemCallFilter = lib.mkForce alloySyscallDeny;
        UMask = lib.mkForce "0077";
      };
    };
  };
}
