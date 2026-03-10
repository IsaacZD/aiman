{ lib, buildNpmPackage, nodejs_20 }:

buildNpmPackage {
  pname = "aiman-dashboard";
  version = "0.1.0";

  src = lib.cleanSourceWith {
    src = ../dashboard;
    filter = path: type:
      let
        base = builtins.baseNameOf path;
      in
      lib.cleanSourceFilter path type
      && base != "node_modules"
      && base != "dist";
  };

  npmBuildScript = "build";
  npmInstallFlags = [ "--include=dev" ];
  npmDepsHash = "sha256-cj6S6WW8Dlw3Y9ddHidyaBKGjOmrtKuFGAhG0N6JmM0=";

  installPhase = ''
    runHook preInstall

    mkdir -p $out/lib/aiman-dashboard/dashboard
    cp -r dist $out/lib/aiman-dashboard/dashboard/
    mkdir -p $out/lib/aiman-dashboard/dashboard/src
    cp -r src/server $out/lib/aiman-dashboard/dashboard/src/
    cp -r node_modules $out/lib/aiman-dashboard/dashboard/

    mkdir -p $out/bin
    cat > $out/bin/aiman-dashboard <<SH
    #!/usr/bin/env bash
    set -euo pipefail

    export NODE_PATH="$out/lib/aiman-dashboard/dashboard/node_modules"

    exec ${nodejs_20}/bin/node \\
      "$out/lib/aiman-dashboard/dashboard/node_modules/.bin/tsx" \\
      "$out/lib/aiman-dashboard/dashboard/src/server/index.ts"
    SH
    chmod +x $out/bin/aiman-dashboard

    runHook postInstall
  '';

  meta = with lib; {
    description = "aiman dashboard server";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}
