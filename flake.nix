{
  description = "Jasper Companion - Personal Digital Assistant for Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    {
      # NixOS module for system integration (must be outside eachDefaultSystem)
      nixosModules.default = import ./module.nix;
    } // flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
        
        buildInputs = with pkgs; [
          # System libraries
          dbus
          sqlite
          openssl
          pkg-config
          
          # Development tools
          rustToolchain
          cargo-edit
          cargo-watch
          cargo-flamegraph
          
          # D-Bus development
          bustle  # D-Bus debugger
          
          # Database tools
          sqlitebrowser
          
          # Documentation
          mdbook
          
          # Additional useful tools
          ripgrep
          fd
          tree
          
          # Config processing for dev-mode
          python3
          jq
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          inherit buildInputs;
          
          shellHook = ''
            echo "Jasper Companion Development Environment"
            echo "Rust: $(rustc --version)"
            echo "Run 'cargo build' to build the daemon"
            echo ""
            echo "Useful commands:"
            echo "  nix flake update    # Update dependencies"
            echo "  nix build           # Build the package"
            echo "  nix develop         # Enter dev shell"
            echo "  cargo watch -x run  # Auto-rebuild on changes"
            echo "  bustle              # D-Bus debugger"
            echo ""
            echo "Database:"
            echo "  export DATABASE_URL=sqlite:./dev.db"
          '';
          
          # Environment variables
          RUST_BACKTRACE = 1;
          DATABASE_URL = "sqlite:./dev.db";
        };
        
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "jasper-companion";
          version = "0.2.0";
          
          src = ./.;
          
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          
          buildInputs = [ pkgs.dbus pkgs.sqlite pkgs.openssl ];
          nativeBuildInputs = [ pkgs.pkg-config ];
        };
        
        packages.gnome-extension = pkgs.stdenv.mkDerivation {
          pname = "jasper-companion-gnome-extension";
          version = "0.2.0";
          
          src = ./gnome-extension;
          
          installPhase = ''
            mkdir -p $out/share/gnome-shell/extensions/jasper@tom.local
            cp -r * $out/share/gnome-shell/extensions/jasper@tom.local/
          '';
          
          passthru.extensionUuid = "jasper@tom.local";
        };
        
        packages.gnome-extension-dev = pkgs.stdenv.mkDerivation {
          pname = "jasper-companion-gnome-extension-dev";
          version = "0.2.0-dev-${self.shortRev or "dirty"}";
          
          src = ./gnome-extension;
          
          installPhase = ''
            mkdir -p $out/share/gnome-shell/extensions/jasper-dev-v2@tom.local
            cp -r * $out/share/gnome-shell/extensions/jasper-dev-v2@tom.local/
            
            # Update metadata.json with development UUID
            ${pkgs.jq}/bin/jq '.uuid = "jasper-dev-v2@tom.local" | .name = "Jasper AI Insights (Development)" | .description = "Development version - AI-generated calendar insights"' metadata.json > \
              $out/share/gnome-shell/extensions/jasper-dev-v2@tom.local/metadata.json
          '';
          
          passthru.extensionUuid = "jasper-dev-v2@tom.local";
        };
      });
}