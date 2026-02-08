# COSMIC Panel Applet for Jasper AI Companion
# Standalone workspace — separate from the daemon to avoid vendoring libcosmic
# into daemon builds.
{
  lib,
  rustPlatform,
  libcosmicAppHook,
  pkg-config,
  dbus,
  wayland,
  libxkbcommon,
  fontconfig,
  freetype,
  libinput,
  glib,
}:

let
  fs = lib.fileset;
  filteredSrc = fs.toSource {
    root = ../cosmic-applet;
    fileset = fs.unions [
      ../cosmic-applet/Cargo.toml
      ../cosmic-applet/Cargo.lock
      ../cosmic-applet/src
      ../cosmic-applet/data
    ];
  };
in

rustPlatform.buildRustPackage {
  pname = "jasper-cosmic-applet";
  version = "0.2.0";

  src = filteredSrc;

  cargoLock = {
    lockFile = ../cosmic-applet/Cargo.lock;
    allowBuiltinFetchGit = true;
    outputHashes = {
      "libcosmic-1.0.0" = "sha256-pfT6/cYjA3CGrXr2d7aAwfW+7FUNdfQvAeOWkknu/Y8=";
    };
  };

  nativeBuildInputs = [
    pkg-config
    libcosmicAppHook
  ];

  buildInputs = [
    dbus
    wayland
    libxkbcommon
    fontconfig
    freetype
    libinput
    glib
  ];

  # We don't use just as our build system
  dontUseJustBuild = true;
  dontUseJustCheck = true;
  dontUseJustInstall = true;

  postInstall = ''
    install -Dm644 data/com.system76.CosmicAppletJasper.desktop \
      $out/share/applications/com.system76.CosmicAppletJasper.desktop
  '';

  meta = {
    description = "COSMIC panel applet for Jasper AI Companion";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}
