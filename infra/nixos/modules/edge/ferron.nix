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
  rt = infraLib.runtime;
  routes = infraLib.publicEdgeRoutes;
  topology = infraLib.observabilityTopology;
  publicHttpPort = 18080;
  publicHttpsPort = 18443;
  localHttpPort = 8080;
  hosts = {
    auth = "auth.${cfg.domain}";
    grafana = "grafana.${cfg.domain}";
    mangaApi = "manga-api.${cfg.domain}";
    publicIp = "152.53.83.190";
  };

  upstreams = {
    grafana = "http://${routes.upstreams.grafana.address}";
    kanidm = routes.upstreams.kanidm.address;
    mangaApi = "http://${routes.upstreams.mangaDownloader.address}";
  };

  indent =
    prefix: text:
    lib.concatMapStringsSep "\n" (line: "${prefix}${line}") (
      lib.filter (line: line != "") (lib.splitString "\n" text)
    );

  forwardedHeaders = scheme: ''
    proxy_request_header_replace "X-Real-IP" "{client_ip_canonical}"
    proxy_request_header_replace "X-Forwarded-For" "{client_ip_canonical}"
    proxy_request_header_replace "X-Forwarded-Host" "{header:Host}"
    proxy_request_header_replace "X-Forwarded-Proto" "${scheme}"
    proxy_request_header_replace "X-Forwarded-Uri" "{path_and_query}"
  '';
  publicProxyHeaders = forwardedHeaders "https";
  localProxyHeaders = forwardedHeaders "http";

  proxyTo = upstream: headers: ''
    proxy "${upstream}"
    ${headers}
  '';

  kanidmProxyForHost = host: headers: ''
    proxy "${upstreams.kanidm}"
    proxy_no_verification
    proxy_request_header_replace "Host" "${host}"
    ${headers}
  '';

  proxiedSite =
    host: backend:
    lib.concatStringsSep "\n" [
      ''"${host}:${toString localHttpPort}" {''
      "  use \"local_origin\""
      (indent "  " (backend localProxyHeaders))
      "}"
      ""
      ''"${host}" {''
      "  use \"public_origin\""
      (indent "  " (backend publicProxyHeaders))
      "}"
    ];

  siteBlocks = lib.concatStringsSep "\n\n" [
    (proxiedSite hosts.auth (kanidmProxyForHost hosts.auth))
    (proxiedSite hosts.mangaApi (proxyTo upstreams.mangaApi))
    (proxiedSite hosts.grafana (proxyTo upstreams.grafana))
  ];

  ferronConfig = pkgs.writeText "ferron.kdl" (
    ''
      globals {
        // Public 80/443 are redirected to these unprivileged listener ports
        // by nftables, so Ferron does not need CAP_NET_BIND_SERVICE.
        default_http_port ${toString publicHttpPort}
        default_https_port ${toString publicHttpsPort}
        listen_ip "::"
        protocols "h1" "h2"
        timeout 300000

        // Public DNS is Cloudflare-proxied. TLS-ALPN-01 cannot pass through
        // Cloudflare TLS termination, so origin certificates use HTTP-01.
        auto_tls_challenge "http-01"
        auto_tls_cache "/var/lib/ferron/acme"
        auto_tls_letsencrypt_production

        otlp_service_name "${topology.jobs.ferron}"
        otlp_logs "http://${topology.endpoints.alloyOtlpGrpc}/v1/logs" protocol="grpc"
        otlp_metrics "http://${topology.endpoints.alloyOtlpGrpc}/v1/metrics" protocol="grpc"

        log_format "{client_ip_canonical} - {auth_user} [{timestamp}] \"{method} {path} {version}\" {status_code} {content_length} \"{header:Referer}\" \"{header:User-Agent}\" request_id=\"{header:X-Request-Id}\" cf_ray=\"{header:CF-Ray}\""
      }

      * {
        log_stdout
        error_log_stderr
      }

      snippet "response_headers" {
        header_remove "Server"
        header_remove "X-Powered-By"
        header_replace "X-Content-Type-Options" "nosniff"
        header_replace "X-Frame-Options" "DENY"
        header_replace "Referrer-Policy" "strict-origin-when-cross-origin"
        header_replace "Permissions-Policy" "geolocation=(), microphone=(), camera=(), payment=(), usb=()"
        header_replace "X-Robots-Tag" "noindex,nofollow,nosnippet,noarchive"
      }

      snippet "https_response_headers" {
        use "response_headers"
        header_replace "Strict-Transport-Security" "max-age=31536000; includeSubDomains"
      }

      snippet "public_origin" {
        use "https_response_headers"
        // Public edge traffic is firewall-limited to Cloudflare ranges.
        trust_x_forwarded_for
        auto_tls
      }

      snippet "local_origin" {
        use "response_headers"
        auto_tls #false
      }

      "${hosts.publicIp}:${toString publicHttpPort}" {
        use "response_headers"
        auto_tls #false
        status 404 body="Not found"
      }
    ''
    + "\n"
    + siteBlocks
    + "\n"
  );
in
{
  users =
    (rt.systemUser {
      name = "ferron";
      uid = 903;
      home = "/var/lib/ferron";
      homeMode = "0750";
    }).users;

  environment.etc."ferron/ferron.kdl".source = ferronConfig;

  networking.hosts = {
    "127.0.0.1" = [
      hosts.auth
      hosts.grafana
      hosts.mangaApi
    ];
    "::1" = [
      hosts.auth
      hosts.grafana
      hosts.mangaApi
    ];
  };

  systemd.services.ferron = {
    description = "Ferron public reverse proxy";
    wantedBy = [ "multi-user.target" ];
    wants = [
      "network-online.target"
      "alloy.service"
    ];
    after = [
      "network-online.target"
      "alloy.service"
    ];
    serviceConfig =
      h.networkService
      // rt.restartAlways
      // {
        User = "ferron";
        Group = "ferron";
        ExecStart = "${lib.getExe pkgs.ferron} --config /etc/ferron/ferron.kdl";
        AmbientCapabilities = "";
        CapabilityBoundingSet = "";
        DeviceAllow = lib.mkForce [ "" ];
        DevicePolicy = lib.mkForce "closed";
        ExecPaths = [ "/nix/store" ];
        NoExecPaths = [ "/" ];
        PrivateIPC = true;
        PrivateMounts = true;
        PrivateUsers = true;
        RestrictAddressFamilies = lib.mkForce [
          "AF_INET"
          "AF_INET6"
        ];
        StateDirectory = "ferron";
        StateDirectoryMode = "0750";
        ReadWritePaths = [ "/var/lib/ferron" ];
        ProtectSystem = lib.mkForce "strict";
      };
  };
}
