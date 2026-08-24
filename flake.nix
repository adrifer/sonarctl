{
  description = "Control SteelSeries Sonar from the command line";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }: {
    packages.x86_64-linux =
      let
        pkgs = import nixpkgs { system = "x86_64-linux"; };
        windowsPkgs = pkgs.pkgsCross.mingwW64;
        manifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      in
      rec {
        sonarctl = windowsPkgs.rustPlatform.buildRustPackage {
          pname = "sonarctl";
          inherit (manifest.package) version;
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;
          doCheck = false;

          installPhase = ''
            runHook preInstall
            install -Dm755 \
              target/x86_64-pc-windows-gnu/release/sonarctl.exe \
              "$out/bin/sonarctl.exe"
            runHook postInstall
          '';
        };

        default = sonarctl;
      };
  };
}
