{ config, ... }:

let
  cfg = config.infra;
in
{
  sops = {
    defaultSopsFile = cfg.secrets.sopsFile;
    age = {
      keyFile = cfg.secrets.ageKeyFile;
      generateKey = true;
      sshKeyPaths = [ ];
    };
    gnupg.sshKeyPaths = [ ];

    secrets = {
      "manga-downloader.env" = {
        key = "manga-downloader.env";
        mode = "0400";
        restartUnits = [ "manga-downloader.service" ];
      };

      "grafana.env" = {
        key = "grafana.env";
        mode = "0400";
        restartUnits = [ "grafana.service" ];
      };
    };
  };
}
