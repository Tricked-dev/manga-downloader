{
  config,
  infraLib,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.infra;
  ts = cfg.tailscale;
  edgeFirewall = infraLib.publicEdgeFirewall;
  nftSet = values: lib.concatStringsSep ", " values;
  ferronPublicHttpPort = 18080;
  ferronPublicHttpsPort = 18443;
  ferronAcceptedPorts = [
    "80"
    "443"
    (toString ferronPublicHttpPort)
    (toString ferronPublicHttpsPort)
  ];
in
{
  config = lib.mkIf ts.enable {
    boot.kernel.sysctl = {
      # Ferron can bind/listen cleanly while tailscale0 comes up during boot.
      "net.ipv4.ip_nonlocal_bind" = 1;
      "net.ipv6.ip_nonlocal_bind" = 1;
    };

    services.tailscale = {
      enable = true;
      package = pkgs.tailscale;
      interfaceName = ts.interfaceName;
      openFirewall = true;
      useRoutingFeatures = ts.useRoutingFeatures;
      authKeyFile = lib.mkIf (ts.authKeyFile != null) ts.authKeyFile;
      extraUpFlags = ts.extraUpFlags;
    };

    systemd.services.tailscaled.serviceConfig = {
      AmbientCapabilities = "";
      CapabilityBoundingSet = [
        "CAP_NET_ADMIN"
        "CAP_NET_BIND_SERVICE"
        "CAP_NET_RAW"
      ];
      DeviceAllow = [ "/dev/net/tun rw" ];
      DevicePolicy = "closed";
      LockPersonality = true;
      MemoryDenyWriteExecute = true;
      NoNewPrivileges = true;
      PrivateDevices = true;
      PrivateMounts = true;
      PrivateTmp = true;
      BindPaths = [ "/dev/net/tun" ];
      ProcSubset = "pid";
      ProtectClock = true;
      ProtectControlGroups = true;
      ProtectHome = true;
      ProtectHostname = true;
      ProtectKernelLogs = true;
      ProtectKernelModules = true;
      ProtectKernelTunables = true;
      ProtectProc = "invisible";
      ProtectSystem = "strict";
      ReadWritePaths = [
        "/run/tailscale"
        "/var/lib/tailscale"
      ];
      RemoveIPC = true;
      RestrictAddressFamilies = [
        "AF_INET"
        "AF_INET6"
        "AF_NETLINK"
        "AF_UNIX"
      ];
      RestrictNamespaces = true;
      RestrictRealtime = true;
      RestrictSUIDSGID = true;
      SystemCallArchitectures = "native";
      SystemCallFilter = "~@clock @cpu-emulation @debug @module @mount @obsolete @privileged @raw-io @reboot @resources @swap";
      UMask = "0077";
    };

    networking.firewall = {
      backend = "nftables";
      allowedUDPPorts = [
        41641
      ];
      interfaces.${ts.interfaceName}.allowedTCPPorts = [ 22 ];
      checkReversePath = "loose";
      extraCommands = lib.mkForce "";
      extraInputRules = ''
        ip saddr { ${nftSet edgeFirewall.cloudflareIPv4Ranges} } tcp dport { ${nftSet ferronAcceptedPorts} } accept comment "allow Cloudflare edge to public Ferron"
        ip6 saddr { ${nftSet edgeFirewall.cloudflareIPv6Ranges} } tcp dport { ${nftSet ferronAcceptedPorts} } accept comment "allow Cloudflare edge to public Ferron"
        iifname "${ts.interfaceName}" ip saddr 100.64.0.0/10 tcp dport 22 accept comment "allow Tailscale IPv4 SSH"
        iifname "${ts.interfaceName}" ip6 saddr fd7a:115c:a1e0::/48 tcp dport 22 accept comment "allow Tailscale IPv6 SSH"
      '';
      extraStopCommands = lib.mkForce "";
    };

    networking.nftables.tables."ferron-edge-redirect" = {
      family = "inet";
      content = ''
        chain prerouting {
          type nat hook prerouting priority dstnat; policy accept;
          ip saddr { ${nftSet edgeFirewall.cloudflareIPv4Ranges} } tcp dport 80 redirect to :${toString ferronPublicHttpPort}
          ip saddr { ${nftSet edgeFirewall.cloudflareIPv4Ranges} } tcp dport 443 redirect to :${toString ferronPublicHttpsPort}
          ip6 saddr { ${nftSet edgeFirewall.cloudflareIPv6Ranges} } tcp dport 80 redirect to :${toString ferronPublicHttpPort}
          ip6 saddr { ${nftSet edgeFirewall.cloudflareIPv6Ranges} } tcp dport 443 redirect to :${toString ferronPublicHttpsPort}
        }
      '';
    };
  };
}
