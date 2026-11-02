{
  description = "UtaTUI - terminal Now Playing dashboard for MPRIS media players";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
    in
    {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "utatui";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.makeWrapper ];
          # cava is spawned at runtime for the visualizer; put it on PATH.
          postInstall = ''
            wrapProgram $out/bin/utatui --suffix PATH : ${pkgs.lib.makeBinPath [ pkgs.cava ]}
          '';
          meta = {
            description = "Terminal media controller for MPRIS players";
            mainProgram = "utatui";
            platforms = pkgs.lib.platforms.linux;
          };
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.system}.default ];
          packages = [ pkgs.cava pkgs.playerctl pkgs.clippy pkgs.rustfmt ];
        };
      });
    };
}
