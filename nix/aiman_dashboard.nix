{ lib, rustPlatform, pkg-config, openssl }:

rustPlatform.buildRustPackage {
  pname = "aiman_dashboard";
  version = "0.1.0";
  src = lib.cleanSourceWith {
    src = ../.;
    filter = path: type: lib.cleanSourceFilter path type;
  };

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ openssl ];

  cargoBuildFlags = [ "-p" "aiman_dashboard" ];
  doCheck = false;

  meta = with lib; {
    description = "aiman dashboard backend";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}
