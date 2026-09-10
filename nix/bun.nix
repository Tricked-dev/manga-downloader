{ pkgs }:
let
  release =
    {
      x86_64-linux = {
        archive = "bun-linux-x64-baseline";
        hash = "sha256-xngEDxT+BEDrg503y9DOTAUaMtpygGrJfeamqra/co8=";
      };
      aarch64-linux = {
        archive = "bun-linux-aarch64";
        hash = "sha256-VDKLvC2cjgyfiSxUTWbFeoO4QTnjSQnl7oF1jxrI/ac=";
      };
      aarch64-darwin = {
        archive = "bun-darwin-aarch64";
        hash = "sha256-kJh6OhbX21VtiGrD1VHnttPt8KHPQ6yu1iLoZ2vh0S8=";
      };
    }
    .${pkgs.stdenv.hostPlatform.system};
in
pkgs.bun.overrideAttrs (_: {
  version = "1.4.2";
  src = pkgs.fetchurl {
    url = "https://github.com/oven-sh/bun/releases/download/bun-v1.4.2/${release.archive}.zip";
    inherit (release) hash;
  };
})
