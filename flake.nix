{
  description = "Adaptive TUI typing trainer in Rust, bringing keybr.com's algorithm to your terminal. Offline touch-typing practice that imports your keybr.com data export.";

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
        rustToolchain = pkgs.rust-bin.stable."1.88.0".default;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [ rustToolchain pkgs.pkg-config ];
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "keybr-tui";
          version = "0.3.2";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          meta = with pkgs.lib; {
            description = "Adaptive TUI typing trainer in Rust, bringing keybr.com's algorithm to your terminal. Offline touch-typing practice that imports your keybr.com data export.";
            homepage = "https://y0sif.github.io/keybr-tui/";
            license = licenses.agpl3Only;
            maintainers = [ ];
            mainProgram = "keybr-tui";
          };
        };
      });
}
