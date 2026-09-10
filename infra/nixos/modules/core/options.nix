{ lib, ... }:

let
  inherit (lib) mkOption types;
  absolutePath = types.strMatching "^/.*";
in
{
  options.infra = {
    domain = mkOption {
      type = types.str;
      example = "trashcan.ing";
      description = "Root DNS name served by the public Ferron edge.";
    };

    timezone = mkOption {
      type = types.str;
      default = "UTC";
      description = "System timezone and application TZ value.";
    };

    bootstrap = {
      directSsh = mkOption {
        type = types.bool;
        default = false;
        description = "Temporarily allow direct SSH while Tailscale SSH/admin access is being bootstrapped.";
      };

    };

    tailscale = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = "Enable Tailscale for private administration and service-to-service access.";
      };

      interfaceName = mkOption {
        type = types.str;
        default = "tailscale0";
        description = "Tailscale network interface name.";
      };

      authKeyFile = mkOption {
        type = types.nullOr absolutePath;
        default = null;
        description = "Optional file containing a reusable or ephemeral Tailscale auth key.";
      };

      extraUpFlags = mkOption {
        type = types.listOf types.str;
        default = [
          "--ssh=false"
          "--accept-routes=false"
          "--accept-dns=false"
          "--shields-up=false"
        ];
        description = "Flags passed to tailscale up by the NixOS tailscale module.";
      };

      useRoutingFeatures = mkOption {
        type = types.enum [
          "none"
          "client"
          "server"
          "both"
        ];
        default = "none";
        description = "Routing mode for the Tailscale NixOS module.";
      };
    };

    imageRuntime = {
      version = mkOption {
        type = types.str;
        default = "1";
        description = "Monotonic system image version used by UKI and sysupdate transfer matching.";
      };

      ukiName = mkOption {
        type = types.str;
        default = "vps-83190";
        description = "UKI filename stem used by systemd-boot and systemd-sysupdate.";
      };

      legacyUkiNames = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Additional UKI filename stems emitted for sysupdate transitions from older images.";
      };

      diskDevice = mkOption {
        type = absolutePath;
        default = "/dev/sda";
        description = "Whole-disk device where sysupdate writes the inactive store partition.";
      };

      updatePath = mkOption {
        type = absolutePath;
        default = "/var/updates/";
        description = "Local directory where sysupdate reads compressed UKI and store update artifacts.";
      };
    };

    secrets = {
      sopsFile = mkOption {
        type = types.path;
        default = ../../secrets/vps-83190.yaml;
        description = "Encrypted sops-nix YAML file committed with the flake.";
      };

      ageKeyFile = mkOption {
        type = absolutePath;
        default = "/var/lib/sops-nix/key.txt";
        description = "Persistent age identity used by sops-nix for secret decryption.";
      };

    };

    mangaDownloader = {
      dataDir = mkOption {
        type = absolutePath;
        default = "/var/lib/manga-server";
        description = "Persistent manga-server application data.";
      };

      downloadsDir = mkOption {
        type = absolutePath;
        default = "/srv/manga-downloads";
        description = "Persistent manga download storage.";
      };

      comixPluginDir = mkOption {
        type = types.nullOr absolutePath;
        default = null;
        description = "Optional directory containing Comix .wasm plugin artifacts to install into PLUGINS_PATH.";
      };

      sourcePluginRegistryUrl = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Optional source plugin registry manifest URL. When set, manga-server installs the Comix plugin from this registry at startup.";
      };

      aidokuPackagePath = mkOption {
        type = types.nullOr absolutePath;
        default = null;
        description = "Optional path to the Aidoku client package artifact.";
      };

      tachiyomiPackagePath = mkOption {
        type = types.nullOr absolutePath;
        default = null;
        description = "Optional path to the Tachiyomi client package artifact.";
      };

    };

    performanceProfiling = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = "Install Linux perf, relax kernel profiling restrictions, and apply conservative throughput tuning.";
      };
    };

    deployment.githubActions = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = "Create a dedicated GitHub Actions deploy user for flake switches over SSH.";
      };

      user = mkOption {
        type = types.str;
        default = "infra-deploy";
        description = "SSH user used by GitHub Actions deployments.";
      };

      authorizedKeys = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Public SSH keys allowed to log in as the GitHub Actions deploy user.";
      };
    };

  };
}
