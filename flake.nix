{
  description = "Development Nix";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    anki-toolkit = {
      url = "github:joshrotenberg/anki-toolkit";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, rust-overlay, anki-toolkit, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = f: nixpkgs.lib.genAttrs systems f;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
        in
        {
          default = self.packages.${system}.ptx;

          # Scrapes the PTX ISA HTML and emits the deck.
          ptx = pkgs.rustPlatform.buildRustPackage {
            pname = "ptx";
            version = "0.1.0";
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
              # ankit-builder is a git dependency, so its checkout needs a hash.
              # This is the same revision the anki-toolkit flake input pins, so
              # it matches that entry's narHash in flake.lock.
              outputHashes = {
                "ankit-0.1.0" = "sha256-vTedbJirZj4z9qdIdtRF0ygTGMIICOGqqoW2U5+TRdQ=";
              };
            };
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
          };

          ankit-mcp = pkgs.rustPlatform.buildRustPackage {
            pname = "ankit-mcp";
            version = "0.1.0";
            src = anki-toolkit;
            cargoBuildFlags = [ "-p" "ankit-mcp" ];
            cargoLock.lockFile = "${anki-toolkit}/Cargo.lock";
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
          };
        });

      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rust = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
              # ankit-builder's default features include "connect", whose
              # AnkiConnect client reaches openssl through reqwest.
              pkgs.pkg-config
              pkgs.openssl
            ];

            RUST_BACKTRACE = "1";
          };
        });
    };
}
