{
  description = "comrak";

  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-26.05/nixexprs.tar.zst";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      fenix,
      ...
    }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];
      eachSystem = nixpkgs.lib.genAttrs systems;
    in
    {

      packages = eachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          cargoToml = pkgs.lib.importTOML ./Cargo.toml;

          mkComrak =
            pkgs:
            pkgs.rustPlatform.buildRustPackage {
              pname = "comrak";
              inherit (cargoToml.package) version;

              src = ./.;
              cargoLock.lockFile = ./Cargo.lock;

              doCheck = false;
            };
        in
        rec {
          default = comrak;

          comrak = mkComrak pkgs;
          comrak-static = mkComrak pkgs.pkgsStatic;
        }
      );

      formatter = eachSystem (system: nixpkgs.legacyPackages.${system}.nixfmt);

      devShells = eachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          fenixPkgs = fenix.packages.${system};

          mkShell =
            {
              name,
              toolchain,
              extraPkgs ? [ ],
            }:
            pkgs.mkShell {
              inherit name;

              packages = [
                (fenixPkgs.combine (
                  with toolchain;
                  [
                    cargo
                    rustc
                    rust-analyzer
                    clippy
                    rustfmt
                    rust-src
                    llvm-tools-preview
                  ]
                  ++ [
                    fenixPkgs.targets.wasm32-unknown-unknown.latest.rust-std
                  ]
                ))
              ]
              ++ (with pkgs; [
                re2c
                cargo-fuzz
                cargo-nextest
                cargo-flamegraph
                cargo-tarpaulin
                cargo-audit
                samply
                python3
                hyperfine
                bacon
              ])
              ++ extraPkgs;
            };
        in
        {
          default = mkShell {
            name = "comrak";
            toolchain = fenixPkgs.complete;
          };

          codspeed = mkShell {
            name = "comrak-codspeed";
            toolchain = fenixPkgs.complete;
            extraPkgs = [
              (pkgs.callPackage ./nix/codspeed.nix {
                rustPlatform = pkgs.makeRustPlatform {
                  cargo = fenixPkgs.complete.toolchain;
                  rustc = fenixPkgs.complete.toolchain;
                };
              })
            ];
          };

          stable = mkShell {
            name = "comrak-stable";
            toolchain = fenixPkgs.stable;
          };

          msrv = mkShell {
            name = "comrak-msrv";
            toolchain = fenixPkgs.toolchainOf {
              channel = "1.89.0";
              sha256 = "sha256-+9FmLhAOezBZCOziO0Qct1NOrfpjNsXxc/8I0c7BdKE=";
            };
          };
        }
      );
    };
}
