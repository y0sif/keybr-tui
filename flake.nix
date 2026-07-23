{
  description = "keybr-tui: a terminal typing trainer with adaptive learning";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable."1.75.0".default;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [ rustToolchain pkgs.pkg-config ];
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "keybr-tui";
          version = "0.2.2";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          meta = with pkgs.lib; {
            description = "Adaptive terminal (TUI) typing trainer in Rust that ports the keybr.com algorithm, runs offline, and imports your keybr.com data";
            homepage = "https://y0sif.github.io/keybr-tui/";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "keybr-tui";
          };
        };
      });
}
