{
  description = "Tailscale-backed NixOS configuration for the infra server";

  nixConfig = {
    extra-experimental-features = [
      "nix-command"
      "flakes"
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    sops-nix = {
      url = "github:Mic92/sops-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ self, nixpkgs, ... }:
    let
      inherit (nixpkgs) lib;
      formatterSystems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      linuxSystems = [ "aarch64-linux" ];
      forFormatterSystems = lib.genAttrs formatterSystems;
      forLinuxSystems = lib.genAttrs linuxSystems;
      pkgsFor = system: import nixpkgs { inherit system; };
    in
    {
      formatter = forFormatterSystems (system: (pkgsFor system).nixfmt);

      devShells = forFormatterSystems (system: {
        default =
          let
            pkgs = pkgsFor system;
          in
          pkgs.mkShell {
            packages =
              with pkgs;
              [
                age
                nil
                nixfmt
                nushell
                openssh
                sops
                tailscale
                xz
              ]
              ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [
                qemu
              ]
              ++ lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
                qemu
              ];
          };
      });

      nixosModules.default = ./modules;

      nixosConfigurations."vps-83190" = nixpkgs.lib.nixosSystem {
        system = "aarch64-linux";
        modules = [
          {
            nixpkgs.hostPlatform = "aarch64-linux";
          }
          inputs.sops-nix.nixosModules.sops
          ./hosts/vps-83190/configuration.nix
        ];
      };

      checks = forLinuxSystems (_system: {
        "vps-83190" = self.nixosConfigurations."vps-83190".config.system.build.toplevel;
      });
    };
}
