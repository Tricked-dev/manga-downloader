{ ... }:

{
  imports = [
    ./core/options.nix
    ./core/secrets.nix
    ./core/infra-lib.nix
    ./core/base-security.nix
    ./core/performance-profiling.nix
    ./edge/tailscale.nix
    ./edge/ferron.nix
    ./image
    ./services/kanidm.nix
    ./services/manga-downloader.nix
    ./services/ntpd-rs.nix
    ./services/alloy.nix
    ./services/grafana.nix
    ./services/victorialogs.nix
    ./services/victoriametrics.nix
    ./services/victoriatraces.nix
  ];
}
