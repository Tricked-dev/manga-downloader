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
  kanidmTheme = pkgs.runCommand "kanidm-trashcan-theme" { } ''
    install -Dm0444 ${./kanidm-theme/override.css} $out/override.css
    install -Dm0444 ${./kanidm-theme/logo.svg} $out/logo.svg
  '';
in
{
  services.kanidm = {
    package = pkgs.kanidm_1_10;
    server = {
      enable = true;
      settings = {
        bindaddress = "127.0.0.1:8443";
        domain = "auth.${cfg.domain}";
        origin = "https://auth.${cfg.domain}";
        tls_chain = "/var/lib/kanidm/certs/chain.pem";
        tls_key = "/var/lib/kanidm/certs/key.pem";
        log_level = "info";
      };
    };
  };

  users =
    (rt.systemUser {
      name = "kanidm";
      uid = 900;
    }).users;

  systemd = {
    tmpfiles.rules = [
      (rt.stateDir {
        path = "/var/lib/kanidm";
        mode = "0700";
        user = "kanidm";
      })
      (rt.stateDir {
        path = "/var/lib/kanidm/data";
        mode = "0750";
        user = "kanidm";
      })
      (rt.stateDir {
        path = "/var/lib/kanidm/certs";
        mode = "0750";
        user = "root";
        group = "kanidm";
      })
    ];

    services = {
      kanidm-cert-init = {
        description = "Prepare Kanidm local TLS material";
        before = [ "kanidm.service" ];
        requiredBy = [ "kanidm.service" ];
        path = [
          pkgs.coreutils
          pkgs.openssl
        ];
        serviceConfig = h.localOneshot // {
          CapabilityBoundingSet = [
            "CAP_CHOWN"
            "CAP_DAC_OVERRIDE"
            "CAP_FOWNER"
          ];
          ReadWritePaths = [ "/var/lib/kanidm" ];
          SystemCallFilter = lib.mkForce [ ];
        };
        script = ''
          install -d -m 0700 -o kanidm -g kanidm /var/lib/kanidm
          install -d -m 0750 -o kanidm -g kanidm /var/lib/kanidm/data
          install -d -m 0750 -o root -g kanidm /var/lib/kanidm/certs

          if [ -f /var/lib/kanidm/data/kanidm.db ] && [ ! -e /var/lib/kanidm/kanidm.db ]; then
            for db_file in /var/lib/kanidm/data/kanidm.db*; do
              [ -e "$db_file" ] || continue
              target="/var/lib/kanidm/$(basename "$db_file")"
              [ -e "$target" ] || install -m 0600 -o kanidm -g kanidm "$db_file" "$target"
            done
          fi

          if [ ! -s /var/lib/kanidm/certs/key.pem ] || [ ! -s /var/lib/kanidm/certs/chain.pem ]; then
            openssl req -x509 -newkey rsa:4096 -sha256 -days 3650 -nodes \
              -keyout /var/lib/kanidm/certs/key.pem \
              -out /var/lib/kanidm/certs/chain.pem \
              -subj "/CN=auth.${cfg.domain}" \
              -addext "subjectAltName=DNS:auth.${cfg.domain}"
          fi

          chown root:kanidm /var/lib/kanidm/certs/key.pem /var/lib/kanidm/certs/chain.pem
          chmod 0440 /var/lib/kanidm/certs/key.pem /var/lib/kanidm/certs/chain.pem
          chown -R kanidm:kanidm /var/lib/kanidm/data
        '';
      };

      kanidm = {
        after = [ "kanidm-cert-init.service" ];
        unitConfig = rt.startLimit;
        serviceConfig =
          h.networkService
          // rt.restartAlways
          // {
            AmbientCapabilities = lib.mkForce "";
            BindReadOnlyPaths = lib.mkAfter [
              "${kanidmTheme}/override.css:${config.services.kanidm.package}/ui/hpkg/override.css"
            ];
            CapabilityBoundingSet = lib.mkForce "";
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
            PrivateUsers = lib.mkOverride 10 true;
            ReadWritePaths = [ "/var/lib/kanidm" ];
            RestrictNetworkInterfaces = [ "lo" ];
            SystemCallFilter = lib.mkForce h.dangerousSyscallDeny;
            UMask = lib.mkForce "0077";
          };
      };
    };
  };

}
