{
  description = "Rust development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          pkg-config
          gobject-introspection
          cargo
          nodejs
          pnpm
          typescript
          wrapGAppsHook4
        ];
        buildInputs = with pkgs; [
          at-spi2-atk
          atkmm
          cairo
          gdk-pixbuf
          glib
          gtk3
          harfbuzz
          librsvg
          libsoup_3
          pango
          webkitgtk_4_1
          pkg-config
          glib-networking
          openssl
          gcc
          rustc
          rust-analyzer
          wasm-pack
          lld
          rustfmt
          openssl
          pkg-config
          clippy
          sqlite
          libayatana-appindicator
        ];

        RUST_BACKTRACE = "1";
        RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        shellHook =
          ''
            # for svg
            export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS
            # for TLS:
            export GIO_MODULE_DIR=${pkgs.glib-networking}/lib/gio/modules/
            export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath [
              pkgs.libayatana-appindicator
              pkgs.libappindicator-gtk3  # optional fallback name the crate also probes
            ]}:$LD_LIBRARY_PATH
          '';
      };
    };
}
