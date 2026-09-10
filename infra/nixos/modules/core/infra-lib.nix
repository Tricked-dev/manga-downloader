{
  config,
  lib,
  ...
}:

let
  default = lib.mkDefault;
  domain = config.infra.domain;
  hostName = config.networking.hostName;

  dangerousSyscallDeny = "~@clock @cpu-emulation @debug @module @mount @obsolete @privileged @raw-io @reboot @resources @swap";
  resourceSyscallDeny = "~@clock @cpu-emulation @debug @module @mount @obsolete @privileged @raw-io @reboot @swap";

  baseService = {
    KeyringMode = default "private";
    LockPersonality = default true;
    MemoryDenyWriteExecute = default true;
    NoNewPrivileges = default true;
    PrivateDevices = default true;
    PrivateTmp = default true;
    ProtectClock = default true;
    ProtectControlGroups = default true;
    ProtectHome = default true;
    ProtectHostname = default true;
    ProtectKernelLogs = default true;
    ProtectKernelModules = default true;
    ProtectKernelTunables = default true;
    ProtectProc = default "invisible";
    ProtectSystem = default "strict";
    ProcSubset = default "pid";
    RemoveIPC = default true;
    RestrictNamespaces = default true;
    RestrictRealtime = default true;
    RestrictSUIDSGID = default true;
    SystemCallArchitectures = default "native";
    SystemCallFilter = default [
      "@system-service"
      "~@privileged"
      "~@resources"
    ];
    UMask = default "0077";
  };

  networkService = baseService // {
    RestrictAddressFamilies = default [
      "AF_INET"
      "AF_INET6"
      "AF_UNIX"
    ];
    SystemCallFilter = default dangerousSyscallDeny;
  };

  netAdminOneshot = baseService // {
    CapabilityBoundingSet = default [
      "CAP_NET_ADMIN"
      "CAP_NET_RAW"
    ];
    RestrictAddressFamilies = default [
      "AF_INET"
      "AF_INET6"
      "AF_NETLINK"
      "AF_UNIX"
    ];
    SystemCallFilter = default [
      "@system-service"
      "@network-io"
      "~@resources"
    ];
    Type = default "oneshot";
    UMask = default "0077";
  };

  localOneshot = baseService // {
    RestrictAddressFamilies = default [
      "AF_UNIX"
    ];
    Type = default "oneshot";
    UMask = default "0077";
  };

  stateDir =
    {
      path,
      mode,
      user,
      group ? user,
    }:
    "d ${path} ${mode} ${user} ${group} - -";

  cloudflareIPv4Ranges = [
    "173.245.48.0/20"
    "103.21.244.0/22"
    "103.22.200.0/22"
    "103.31.4.0/22"
    "141.101.64.0/18"
    "108.162.192.0/18"
    "190.93.240.0/20"
    "188.114.96.0/20"
    "197.234.240.0/22"
    "198.41.128.0/17"
    "162.158.0.0/15"
    "104.16.0.0/13"
    "104.24.0.0/14"
    "172.64.0.0/13"
    "131.0.72.0/22"
  ];
  cloudflareIPv6Ranges = [
    "2400:cb00::/32"
    "2606:4700::/32"
    "2803:f800::/32"
    "2405:b500::/32"
    "2405:8100::/32"
    "2a06:98c0::/29"
    "2c0f:f248::/32"
  ];

  topology = rec {
    inherit hostName;

    endpoints = {
      alloyOtlpGrpc = "127.0.0.1:4317";
      alloyOtlpHttp = "127.0.0.1:4318";
      grafana = "127.0.0.1:3000";
      victorialogs = "127.0.0.1:9428";
      victoriametrics = "127.0.0.1:8428";
      victoriatraces = "127.0.0.1:10428";
    };

    jobs = {
      ferron = "ferron";
      grafana = "grafana";
      mangaServer = "manga-server";
      victorialogs = "victorialogs";
      victoriametrics = "victoriametrics";
      victoriatraces = "victoriatraces";
    };

    criticalUnits = [
      "alloy.service"
      "ferron.service"
      "grafana.service"
      "kanidm.service"
      "manga-downloader.service"
      "tailscaled.service"
      "victorialogs.service"
      "victoriametrics.service"
      "victoriatraces.service"
    ];

    grafanaStackTargets = [
      {
        address = endpoints.grafana;
        job = jobs.grafana;
      }
      {
        address = endpoints.victoriametrics;
        job = jobs.victoriametrics;
      }
      {
        address = endpoints.victorialogs;
        job = jobs.victorialogs;
      }
      {
        address = endpoints.victoriatraces;
        job = jobs.victoriatraces;
      }
    ];
  };
in
{
  config._module.args.infraLib = {
    hardening = {
      inherit
        baseService
        dangerousSyscallDeny
        localOneshot
        netAdminOneshot
        networkService
        resourceSyscallDeny
        ;
    };

    runtime = {
      systemUser =
        {
          name,
          uid,
          gid ? uid,
          home ? null,
          homeMode ? "0750",
          extra ? { },
        }:
        {
          users.groups.${name}.gid = gid;
          users.users.${name} = {
            isSystemUser = true;
            inherit uid;
            group = name;
          }
          // lib.optionalAttrs (home != null) {
            inherit home homeMode;
            createHome = true;
          }
          // extra;
        };

      inherit stateDir;

      credential = name: path: "${name}:${path}";

      startLimit = {
        StartLimitBurst = default 5;
        StartLimitIntervalSec = default "5min";
      };

      restartAlways = {
        Restart = default "always";
        RestartSec = default "10s";
      };

      restartOnFailure = {
        Restart = default "on-failure";
        RestartSec = default "10s";
      };
    };

    publicEdgeRoutes = {
      publicIpHost = "http://217.77.4.104";

      hosts = {
        auth = {
          public = "https://auth.${domain}";
          local = "http://auth.${domain}:8080";
          upstream = "kanidm";
        };

        grafana = {
          public = "https://grafana.${domain}";
          local = "http://grafana.${domain}:8080";
          upstream = "grafana";
        };

        mangaApi = {
          public = "https://manga-api.${domain}";
          local = "http://manga-api.${domain}:8080";
          upstream = "manga-downloader";
        };
      };

      upstreams = {
        grafana = {
          address = topology.endpoints.grafana;
          traceNamespace = "edge";
          traceUpstream = "grafana";
        };

        kanidm = {
          address = "https://127.0.0.1:8443";
          traceNamespace = "edge";
          traceUpstream = "kanidm";
        };

        mangaDownloader = {
          address = "127.0.0.1:4000";
          traceNamespace = "edge";
          traceUpstream = "manga-downloader";
        };
      };
    };

    publicEdgeFirewall = {
      inherit cloudflareIPv4Ranges cloudflareIPv6Ranges;
      cloudflareRanges = cloudflareIPv4Ranges ++ cloudflareIPv6Ranges;
    };

    observabilityTopology = topology;
  };
}
