{ config, ... }:

{
  imports = [
    ./hardware-configuration.nix
    ../../modules
  ];

  system.stateVersion = "25.11";

  networking.hostName = "vps-83190";

  services.qemuGuest.enable = true;

  infra = {
    domain = "trashcan.ing";
    timezone = "America/New_York";

    imageRuntime = {
      version = "42";
      ukiName = "vps-83190";
      legacyUkiNames = [ "experiment2" ];
      diskDevice = "/dev/vda";
      updatePath = "/var/updates/";
    };

    bootstrap = {
      directSsh = false;
    };

    tailscale = {
      enable = true;
      authKeyFile = null;
      extraUpFlags = [
        "--ssh=false"
        "--accept-routes=false"
        "--accept-dns=false"
        "--shields-up=false"
      ];
    };

    secrets = {
      ageKeyFile = "/var/lib/sops-nix/key.txt";
    };

    performanceProfiling.enable = false;

    deployment.githubActions = {
      enable = true;
      authorizedKeys = [
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPMHK1wByIGxceTMoQKincl8Y+oKqeOj66rOGLF7JmuI github-actions-infra-deploy"
      ];
    };

    mangaDownloader = {
      dataDir = "/var/lib/manga-server";
      downloadsDir = "/srv/manga-downloads";
    };
  };

  nix.settings = {
    substituters = [
      "http://100.68.106.29:8080/vps"
      "https://nix.aethor.xyz:8443/fleet"
      "https://cache.nixos.org"
    ];
    trusted-public-keys = [
      "vps:kGtlomXvtREiMQDVIapf7Kj5NRWN17ty3FnUexb6Iuo="
      "fleet:pPo9XWTATgYpXoridRoaEt6rq/e5KJrgHVRhzXAw5E4="
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    ];
  };

  networking = {
    useNetworkd = true;
    useDHCP = false;
    interfaces = { };
  };

  systemd.network.networks."10-netcup-uplink" = {
    matchConfig.MACAddress = "8a:74:31:f3:47:52";
    address = [
      "152.53.83.190/22"
      "2a0a:4cc0:2000:38cb::1/64"
    ];
    routes = [
      { Gateway = "152.53.80.1"; }
      {
        Gateway = "fe80::1";
        GatewayOnLink = true;
      }
    ];
    networkConfig = {
      DNS = [
        "1.1.1.1"
        "2606:4700:4700::1111"
      ];
      EmitLLDP = false;
      IPv6AcceptRA = false;
      LLDP = false;
    };
  };

  services.openssh.listenAddresses = [
    {
      addr = "100.69.47.77";
      port = 22;
    }
  ];
}
