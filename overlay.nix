final: prev: {
  gnomeExtensions = prev.gnomeExtensions // {
    jasper = final.stdenv.mkDerivation {
      pname = "gnome-shell-extension-jasper";
      version = "1.0";
      
      src = ./gnome-extension;
      
      dontBuild = true;
      
      installPhase = ''
        runHook preInstall
        mkdir -p $out/share/gnome-shell/extensions/jasper@tom.local
        cp -r * $out/share/gnome-shell/extensions/jasper@tom.local/
        runHook postInstall
      '';
      
      passthru.extensionUuid = "jasper@tom.local";
      
      meta = with final.lib; {
        description = "Display AI-generated calendar insights in GNOME Shell panel";
        homepage = "https://github.com/tom/jasper";
        license = licenses.mit;
        platforms = platforms.linux;
      };
    };
  };
}