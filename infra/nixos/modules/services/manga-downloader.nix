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
  topology = infraLib.observabilityTopology;
  mangaServerArtifact = ../../artifacts/manga-server;
  mangaServerPackage =
    if builtins.pathExists mangaServerArtifact then
      pkgs.stdenvNoCC.mkDerivation {
        pname = "manga-server-bazel";
        version = "0";
        src = mangaServerArtifact;
        dontUnpack = true;
        installPhase = ''
          runHook preInstall
          install -Dm755 "$src" "$out/bin/manga-server"
          runHook postInstall
        '';
        meta.mainProgram = "manga-server";
      }
    else
      pkgs.writeShellScriptBin "manga-server" ''
        echo "Missing Bazel-built manga-server artifact in infra/nixos/artifacts/manga-server." >&2
        echo "Build sysupdate bundles with infra/nixos/scripts/build-bazel-sysupdate.sh." >&2
        exit 70
      '';
  mangaPluginsDir = "${cfg.mangaDownloader.dataDir}/plugins";
  mangaTmpDir = "${cfg.mangaDownloader.dataDir}/tmp";
  mangaCacheDir = "${cfg.mangaDownloader.dataDir}/data/cache";
  mangaFontPackages = with pkgs; [
    dejavu_fonts.minimal
    noto-fonts
  ];
  mangaServerStart = pkgs.writeShellScript "manga-downloader-start" ''
    set -euo pipefail

    if [ -n "''${CREDENTIALS_DIRECTORY:-}" ] && [ -r "$CREDENTIALS_DIRECTORY/manga-downloader.env" ]; then
      while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
          "" | \#*) continue ;;
        esac

        key="''${line%%=*}"
        value="''${line#*=}"

        if [ "$key" = "$line" ] || [ -z "$key" ]; then
          continue
        fi

        case "$key" in
          *[!A-Za-z0-9_]* | [0-9]*)
            echo "Ignoring invalid environment key in manga-downloader credentials: $key" >&2
            continue
            ;;
        esac

        export "$key=$value"
      done < "$CREDENTIALS_DIRECTORY/manga-downloader.env"
    fi

    exec ${lib.getExe mangaServerPackage}
  '';
in
{
  fonts.packages = mangaFontPackages;

  users =
    (rt.systemUser {
      name = "manga-downloader";
      uid = 902;
      home = cfg.mangaDownloader.dataDir;
      homeMode = "0750";
    }).users;

  systemd = {
    tmpfiles.rules = [
      (rt.stateDir {
        path = cfg.mangaDownloader.dataDir;
        mode = "0750";
        user = "manga-downloader";
      })
      (rt.stateDir {
        path = "${cfg.mangaDownloader.dataDir}/.cache";
        mode = "0750";
        user = "manga-downloader";
      })
      (rt.stateDir {
        path = "${cfg.mangaDownloader.dataDir}/.config";
        mode = "0750";
        user = "manga-downloader";
      })
      (rt.stateDir {
        path = "${cfg.mangaDownloader.dataDir}/data";
        mode = "0750";
        user = "manga-downloader";
      })
      (rt.stateDir {
        path = mangaCacheDir;
        mode = "0750";
        user = "manga-downloader";
      })
      (rt.stateDir {
        path = mangaPluginsDir;
        mode = "0750";
        user = "manga-downloader";
      })
      (rt.stateDir {
        path = "${mangaPluginsDir}/.wasmtime-cache";
        mode = "0750";
        user = "manga-downloader";
      })
      (rt.stateDir {
        path = mangaTmpDir;
        mode = "0750";
        user = "manga-downloader";
      })
      (rt.stateDir {
        path = cfg.mangaDownloader.downloadsDir;
        mode = "0750";
        user = "manga-downloader";
      })
    ];

    sockets.manga-downloader = {
      description = "Manga Downloader backend socket";
      wantedBy = [ "sockets.target" ];
      listenStreams = [ "127.0.0.1:4000" ];
      socketConfig = {
        Backlog = 4096;
        NoDelay = true;
        Service = "manga-downloader.service";
        UMask = "0077";
      };
    };

    services = {
      manga-downloader-prepare = {
        description = "Prepare manga-downloader native service state";
        before = [ "manga-downloader.service" ];
        requiredBy = [ "manga-downloader.service" ];
        after = [ "systemd-tmpfiles-setup.service" ];
        path = [ pkgs.coreutils ];
        serviceConfig = h.localOneshot // {
          CapabilityBoundingSet = [
            "CAP_CHOWN"
            "CAP_DAC_OVERRIDE"
            "CAP_FOWNER"
          ];
          ReadWritePaths = [
            cfg.mangaDownloader.dataDir
            cfg.mangaDownloader.downloadsDir
          ];
        };
        script = ''
          set -euo pipefail

          install -d -m 0750 -o manga-downloader -g manga-downloader ${cfg.mangaDownloader.dataDir}
          install -d -m 0750 -o manga-downloader -g manga-downloader ${cfg.mangaDownloader.dataDir}/.cache
          install -d -m 0750 -o manga-downloader -g manga-downloader ${cfg.mangaDownloader.dataDir}/.config
          install -d -m 0750 -o manga-downloader -g manga-downloader ${cfg.mangaDownloader.dataDir}/data
          install -d -m 0750 -o manga-downloader -g manga-downloader ${mangaCacheDir}
          install -d -m 0750 -o manga-downloader -g manga-downloader ${mangaPluginsDir}
          install -d -m 0750 -o manga-downloader -g manga-downloader ${mangaPluginsDir}/.wasmtime-cache
          install -d -m 0750 -o manga-downloader -g manga-downloader ${mangaTmpDir}
          install -d -m 0750 -o manga-downloader -g manga-downloader ${cfg.mangaDownloader.downloadsDir}

        ''
        + lib.optionalString (cfg.mangaDownloader.comixPluginDir != null) ''

          for plugin in ${cfg.mangaDownloader.comixPluginDir}/*.wasm; do
            [ -e "$plugin" ] || continue
            install -Dm0640 -o manga-downloader -g manga-downloader "$plugin" "${mangaPluginsDir}/$(basename "$plugin")"
          done
        '';
      };

      manga-downloader = {
        description = "Manga Downloader backend";
        wants = [
          "network-online.target"
          "manga-downloader-prepare.service"
          "alloy.service"
          "sops-install-secrets.service"
        ];
        after = [
          "network-online.target"
          "manga-downloader-prepare.service"
          "alloy.service"
          "sops-install-secrets.service"
        ];
        unitConfig = rt.startLimit;
        environment = {
          AUTO_DOWNLOAD_NEW_CHAPTERS = "false";
          AVIF_CONVERSION_WORKERS = "5";
          CACHE_DISK_PATH = mangaCacheDir;
          CACHE_MAX_MEMORY_BYTES = "268435456";
          DB_PATH = "${cfg.mangaDownloader.dataDir}/manga.db";
          DOWNLOAD_PATH = cfg.mangaDownloader.downloadsDir;
          FONTCONFIG_FILE = "/etc/fonts/fonts.conf";
          HOME = cfg.mangaDownloader.dataDir;
          LIBRARY_CATEGORIES = "default,downloaded";
          OIDC_PROVIDER_ID = "oidc";
          OIDC_SCOPES = "openid profile email";
          PLUGINS_PATH = mangaPluginsDir;
          RUST_LOG = "info,chromiumoxide=error";
          SERVER_ADDR = "127.0.0.1:4000";
          SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          TMPDIR = mangaTmpDir;
          TZ = cfg.timezone;
          TZDIR = "${pkgs.tzdata}/share/zoneinfo";
          UPDATE_INTERVAL_HOURS = "1";
          XDG_CACHE_HOME = "${cfg.mangaDownloader.dataDir}/.cache";
          XDG_CONFIG_HOME = "${cfg.mangaDownloader.dataDir}/.config";
        }
        // {
          OTEL_EXPORTER_OTLP_ENDPOINT = "http://127.0.0.1:4318";
          OTEL_EXPORTER_OTLP_PROTOCOL = "http/protobuf";
          OTEL_LOGS_EXPORTER = "otlp";
          OTEL_METRICS_EXPORTER = "otlp";
          OTEL_RESOURCE_ATTRIBUTES = "deployment.environment=production,service.namespace=manga,service.instance.id=${topology.hostName}";
          OTEL_SERVICE_NAME = topology.jobs.mangaServer;
          OTEL_TRACES_EXPORTER = "otlp";
        }
        // lib.optionalAttrs (cfg.mangaDownloader.aidokuPackagePath != null) {
          AIDOKU_PACKAGE_PATH = cfg.mangaDownloader.aidokuPackagePath;
        }
        // lib.optionalAttrs (cfg.mangaDownloader.sourcePluginRegistryUrl != null) {
          SOURCE_PLUGIN_REGISTRY_URL = cfg.mangaDownloader.sourcePluginRegistryUrl;
        }
        // lib.optionalAttrs (cfg.mangaDownloader.tachiyomiPackagePath != null) {
          TACHIYOMI_PACKAGE_PATH = cfg.mangaDownloader.tachiyomiPackagePath;
        };
        serviceConfig =
          h.networkService
          // rt.restartOnFailure
          // {
            AmbientCapabilities = "";
            CapabilityBoundingSet = "";
            ExecStart = mangaServerStart;
            Group = "manga-downloader";
            # Let the server drain work and close browser children before systemd escalates.
            KillMode = "mixed";
            KillSignal = "SIGTERM";
            LoadCredential = [
              (rt.credential "manga-downloader.env" config.sops.secrets."manga-downloader.env".path)
            ];
            MemoryDenyWriteExecute = lib.mkForce false;
            PrivateUsers = true;
            ProtectKernelLogs = lib.mkForce true;
            RestrictAddressFamilies = lib.mkForce [
              "AF_INET"
              "AF_INET6"
            ];
            SystemCallFilter = lib.mkForce h.dangerousSyscallDeny;
            ReadWritePaths = [
              cfg.mangaDownloader.dataDir
              cfg.mangaDownloader.downloadsDir
            ];
            RuntimeDirectory = "manga-downloader";
            RuntimeDirectoryMode = "0750";
            TimeoutStopSec = "5min";
            UMask = lib.mkForce "0077";
            User = "manga-downloader";
            WorkingDirectory = cfg.mangaDownloader.dataDir;
          };
      };
    };
  };
}
