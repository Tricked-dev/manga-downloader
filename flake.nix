{
  description = "Nix launcher for Bazel-only Manga Downloader builds";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    {
      nixpkgs,
      ...
    }:
    let
      lib = nixpkgs.lib;

      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      forAllSystems = lib.genAttrs supportedSystems;

      pkgsFor = system: import nixpkgs { inherit system; };

      mkApp = drv: {
        type = "app";
        program = lib.getExe drv;
      };

      mkFetchedBinary =
        pkgs:
        {
          pname,
          version,
          url,
          hash,
          programName,
          meta ? { },
        }:
        pkgs.stdenvNoCC.mkDerivation {
          inherit pname version;

          src = pkgs.fetchurl { inherit url hash; };
          dontUnpack = true;

          installPhase = ''
            runHook preInstall
            install -Dm755 "$src" "$out/bin/${programName}"
            runHook postInstall
          '';

          meta = meta // {
            mainProgram = programName;
          };
        };

      aspectLauncherFor =
        pkgs:
        let
          version = "2026.22.44";
          asset =
            {
              aarch64-darwin = {
                name = "aspect-launcher-aarch64-apple-darwin";
                hash = "sha256-rDfwmWdEVbcck805lYsChsNWMN4RJy28nNSnt+nJocU=";
              };
              x86_64-darwin = {
                name = "aspect-launcher-x86_64-apple-darwin";
                hash = "sha256-FxbD3yoNMXQoBRb+mRaNVjE+Cwb74z4yvKDxqyUSFpk=";
              };
              aarch64-linux = {
                name = "aspect-launcher-aarch64-unknown-linux-musl";
                hash = "sha256-rjKpa1Kj4ocMfeU9vpD1xWfzvU2jgU3LQU8cq1aIVgs=";
              };
              x86_64-linux = {
                name = "aspect-launcher-x86_64-unknown-linux-musl";
                hash = "sha256-sduBajjZOn8sO2/jSo+9vJC/Hdq7q89WJKG9h+XwsXU=";
              };
            }
            .${pkgs.stdenv.hostPlatform.system};
        in
        mkFetchedBinary pkgs {
          pname = "aspect-launcher";
          inherit version;
          url = "https://github.com/aspect-build/aspect-cli/releases/download/v${version}/${asset.name}";
          hash = asset.hash;
          programName = "aspect";
        };

      expectedCargoFiles = lib.concatStringsSep "\n" [
        "./third_party/rust/Cargo.lock"
        "./third_party/rust/Cargo.toml"
      ];

      manifestPolicyFor =
        pkgs:
        pkgs.runCommand "manga-downloader-cargo-manifest-policy"
          {
            src = lib.cleanSourceWith {
              src = ./.;
              filter =
                path: _type:
                let
                  rel = lib.removePrefix ((toString ./.) + "/") (toString path);
                in
                !lib.hasPrefix ".git/" rel
                && !lib.hasPrefix "bazel-" rel
                && !lib.hasPrefix ".direnv/" rel
                && !lib.hasPrefix ".cache/" rel;
            };
          }
          ''
            cd "$src"

            actual="$(
              find . \( -name Cargo.toml -o -name Cargo.lock -o -path '*/.cargo/*' \) -print | sort
            )"
            expected='${expectedCargoFiles}'

            if [ "$actual" != "$expected" ]; then
              echo "Cargo files must live only under third_party/rust:" >&2
              printf '%s\n' "$actual" >&2
              exit 1
            fi

            mkdir -p "$out"
            printf '%s\n' "$actual" > "$out/allowed-cargo-files.txt"
          '';

      formatterFor =
        pkgs:
        pkgs.writeShellApplication {
          name = "manga-downloader-nixfmt";
          runtimeInputs = [ pkgs.nixfmt ];
          text = ''
            if [ "$#" -eq 0 ]; then
              exit 0
            fi

            exec nixfmt "$@"
          '';
        };

      bazelLauncherFor =
        pkgs:
        let
          aspectLauncher = aspectLauncherFor pkgs;
        in
        pkgs.writeShellApplication {
          name = "manga-downloader-bazel";
          runtimeInputs = [
            aspectLauncher
            pkgs.bazelisk
          ];
          text = ''
            exec aspect "$@"
          '';
        };

      perSystem =
        system:
        let
          pkgs = pkgsFor system;
          aspectLauncher = aspectLauncherFor pkgs;
          bazelLauncher = bazelLauncherFor pkgs;
          cargoManifestPolicy = manifestPolicyFor pkgs;
        in
        {
          inherit
            aspectLauncher
            bazelLauncher
            cargoManifestPolicy
            pkgs
            ;

          formatter = formatterFor pkgs;
        };

      systemAttrs = forAllSystems perSystem;
    in
    {
      packages = forAllSystems (
        system:
        let
          ps = systemAttrs.${system};
        in
        {
          default = ps.bazelLauncher;
          bazel = ps.bazelLauncher;
          cargo-manifest-policy = ps.cargoManifestPolicy;
        }
      );

      checks = forAllSystems (
        system:
        let
          ps = systemAttrs.${system};
        in
        {
          cargo-manifest-policy = ps.cargoManifestPolicy;
        }
      );

      apps = forAllSystems (
        system:
        let
          ps = systemAttrs.${system};
        in
        {
          default = mkApp ps.bazelLauncher;
          bazel = mkApp ps.bazelLauncher;
        }
      );

      devShells = forAllSystems (
        system:
        let
          ps = systemAttrs.${system};
        in
        {
          default = ps.pkgs.mkShell {
            packages = [
              ps.aspectLauncher
              ps.pkgs.apko
              ps.pkgs.bazelisk
              ps.pkgs.buildifier
              ps.pkgs.nushell
            ];
          };
        }
      );

      formatter = forAllSystems (
        system:
        let
          ps = systemAttrs.${system};
        in
        ps.formatter
      );
    };
}
