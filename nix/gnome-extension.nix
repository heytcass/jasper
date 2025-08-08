{ lib, stdenv, fetchFromGitHub, gnome }:

stdenv.mkDerivation rec {
  pname = "gnome-shell-extension-jasper";
  version = "1.0";

  src = ./gnome-extension;

  uuid = "jasper@tom.local";

  dontBuild = true;

  installPhase = ''
    runHook preInstall
    
    # Create extension directory
    mkdir -p $out/share/gnome-shell/extensions/${uuid}
    
    # Copy extension files
    cp -r * $out/share/gnome-shell/extensions/${uuid}/
    
    runHook postInstall
  '';

  passthru = {
    extensionUuid = uuid;
    extensionPortalSlug = "jasper-ai-insights";
  };

  meta = with lib; {
    description = "Display AI-generated calendar insights in GNOME Shell panel";
    homepage = "https://github.com/tom/jasper";
    license = licenses.mit;
    maintainers = [ ];
    platforms = platforms.linux;
  };
}