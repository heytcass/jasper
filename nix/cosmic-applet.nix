# COSMIC Panel Applet for Jasper AI Companion
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

rustPlatform.buildRustPackage {
  pname = "jasper-cosmic-applet";
  version = "0.2.0";

  src = ./..;

  cargoBuildFlags = [ "-p" "jasper-cosmic-applet" ];
  cargoTestFlags = [ "-p" "jasper-cosmic-applet" ];

  cargoLock = {
    lockFile = ../Cargo.lock;
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
    install -Dm644 cosmic-applet/data/com.system76.CosmicAppletJasper.desktop \
      $out/share/applications/com.system76.CosmicAppletJasper.desktop
  '';

  meta = {
    description = "COSMIC panel applet for Jasper AI Companion";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}
