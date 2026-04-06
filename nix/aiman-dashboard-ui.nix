{
  lib,
  buildNpmPackage,
}:
buildNpmPackage {
  pname = "aiman-dashboard-ui";
  version = "0.1.0";

  src = lib.cleanSourceWith {
    src = ../dashboard;
    filter = path: type: let
      base = builtins.baseNameOf path;
    in
      lib.cleanSourceFilter path type
      && base != "node_modules"
      && base != "dist";
  };

  npmBuildScript = "build";
  npmInstallFlags = ["--include=dev"];
  npmDepsHash = "sha256-BfSlrED3EJgXBf8GXmSKdLG7Q2VEXWZKVZlL30dKRYM=";

  installPhase = ''
    runHook preInstall
    mkdir -p $out/share/aiman-dashboard-ui
    cp -r dist/ui $out/share/aiman-dashboard-ui/
    runHook postInstall
  '';

  meta = with lib; {
    description = "aiman dashboard Vue UI";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}
