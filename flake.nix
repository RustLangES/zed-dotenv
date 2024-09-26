{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix.url = "github:nix-community/fenix";
    flake-utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    flake-utils,
    crane,
    ...
  } @ inputs:
    flake-utils.lib.eachSystem (flake-utils.lib.defaultSystems) (
      system: let
          pkgs = nixpkgs.legacyPackages.${system};
          lib = pkgs.lib;
          fenix = inputs.fenix.packages;
          craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

          # fenix: rustup replacement for reproducible builds
          toolchain = fenix.${system}.fromToolchainFile {
            file = ./rust-toolchain.toml;
            sha256 = "sha256-VZZnlyP69+Y3crrLHQyJirqlHrTtGTsyiSnZB8jEvVo=";
          };

          # buildInputs for Examples
          buildInputs = with pkgs; [
            toolchain
            pkg-config
          ];

          zed-plugin = craneLib.buildPackage {
            doCheck = false;
            pname = "zed-dotenv";
            src = craneLib.cleanCargoSource (craneLib.path ./.);
            buildPhaseCargoCommand = "cargo build --release --target wasm32-wasip1";

            installPhaseCommand = ''
              mkdir -p $out
              cp target/wasm32-wasip1/release/zed_dotenv.wasm $out/extension.wasm
            '';

            inherit buildInputs;
          };
      in {
        # `nix build`
        packages.default = zed-plugin;
        # `nix develop`
        devShells.default = pkgs.mkShell {
          packages = with pkgs; buildInputs ++ [
            cargo-dist
            cargo-release
          ];
          LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
        };
      }
    );
}
