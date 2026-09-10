{
  infraLib,
  lib,
  ...
}:

let
  topology = infraLib.observabilityTopology;
in
{
  config = {
    services.victoriatraces = {
      enable = true;
      listenAddress = topology.endpoints.victoriatraces;
      retentionPeriod = "7d";
    };

    systemd.services.victoriatraces.serviceConfig = infraLib.hardening.networkService // {
      AmbientCapabilities = "";
      CapabilityBoundingSet = "";
      DeviceAllow = lib.mkForce [ "" ];
      DevicePolicy = lib.mkForce "closed";
      ExecPaths = [ "/nix/store" ];
      IPAddressAllow = [
        "127.0.0.0/8"
        "::1/128"
      ];
      IPAddressDeny = "any";
      NoExecPaths = [ "/" ];
      PrivateIPC = true;
      PrivateMounts = true;
      ProtectSystem = lib.mkForce "strict";
      ReadWritePaths = [ "/var/lib/victoriatraces" ];
      RestrictAddressFamilies = lib.mkForce [
        "AF_INET"
        "AF_INET6"
      ];
      RestrictNetworkInterfaces = [ "lo" ];
      SystemCallFilter = lib.mkForce infraLib.hardening.dangerousSyscallDeny;
      UMask = "0077";
    };
  };
}
