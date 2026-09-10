{
  infraLib,
  lib,
  pkgs,
  ...
}:

let
  h = infraLib.hardening;
  rt = infraLib.runtime;
in
{
  services.timesyncd.enable = lib.mkForce false;

  environment.etc."ntpd-rs/ntp.toml".text = ''
    [source-defaults]
    initial-poll-interval = 4
    poll-interval-limits = { min = 4, max = 10 }

    # Google Public NTP and AWS Time Sync are leap-smeared, while Cloudflare
    # and NIST provide normal UTC time. Keep several non-smeared sources so
    # ntpd-rs can reject smeared offsets around a future leap second.
    [[source]]
    mode = "server"
    address = "time.google.com"

    [[source]]
    mode = "nts"
    address = "time.cloudflare.com"

    [[source]]
    mode = "server"
    address = "time.nist.gov"

    [[source]]
    mode = "server"
    address = "time-a-g.nist.gov"

    [[source]]
    mode = "server"
    address = "time-b-g.nist.gov"

    [[source]]
    mode = "server"
    address = "time-c-g.nist.gov"

    [[source]]
    mode = "server"
    address = "time.aws.com"

    [synchronization]
    minimum-agreeing-sources = 3

    [observability]
    observation-path = "/run/ntpd-rs/observe"
  '';

  systemd.services.ntpd-rs = {
    description = "ntpd-rs Network Time Protocol daemon";
    wantedBy = [ "multi-user.target" ];
    wants = [ "network-online.target" ];
    after = [
      "network-online.target"
      "systemd-resolved.service"
    ];
    conflicts = [ "systemd-timesyncd.service" ];
    unitConfig = rt.startLimit;
    serviceConfig =
      h.networkService
      // rt.restartAlways
      // {
        AmbientCapabilities = [
          "CAP_SYS_TIME"
        ];
        CapabilityBoundingSet = [
          "CAP_SYS_TIME"
        ];
        DynamicUser = true;
        Environment = "SYSTEMD_NSS_RESOLVE_VALIDATE=0";
        ExecStart = "${pkgs.ntpd-rs}/bin/ntp-daemon --config /etc/ntpd-rs/ntp.toml";
        IPAddressAllow = [
          "127.0.0.0/8"
          "::1/128"
          "129.6.15.0/24"
          "132.163.96.0/23"
          "162.159.200.0/24"
          "216.239.35.0/24"
          "3.94.91.31/32"
          "44.201.148.133/32"
          "52.207.222.50/32"
          "54.81.127.33/32"
          "54.210.225.137/32"
          "2001:4860:4806::/48"
          "2606:4700:f1::/48"
          "2600:1f18:4a3:6900::/56"
        ];
        IPAddressDeny = "any";
        ProtectClock = lib.mkForce false;
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_NETLINK"
          "AF_UNIX"
        ];
        RuntimeDirectory = "ntpd-rs";
        RuntimeDirectoryMode = "0750";
        StateDirectory = "ntpd-rs";
        StateDirectoryMode = "0750";
        SystemCallFilter = lib.mkForce [
          "@system-service"
          "@clock"
        ];
        UMask = lib.mkForce "0077";
      };
  };
}
