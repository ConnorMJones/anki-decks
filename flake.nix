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

  outputs = { nixpkgs, rust-overlay, anki-toolkit, ... }:
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
              # pkgs.pkg-config
              # pkgs.openssl
            ];

            RUST_BACKTRACE = "1";
          };
        });
    };
}
