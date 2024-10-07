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
          extensionToml = builtins.fromTOML (builtins.readFile ./extension.toml);
          # Template Variables
          plugin-id = builtins.replaceStrings ["-"] ["_"] extensionToml.id;
          isGrammar = builtins.hasAttr "grammars" extensionToml;
          lang = if isGrammar
                 then builtins.head (builtins.attrNames extensionToml.grammars)
                 else "";
          lsp-name = if isGrammar && builtins.hasAttr "language_servers" extensionToml
                     then builtins.head (builtins.attrNames extensionToml.language_servers)
                     else "";

          pkgs = nixpkgs.legacyPackages.${system};
          lib = pkgs.lib;
          fenix = inputs.fenix.packages.${system};
          craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

          grammar = if isGrammar then
            pkgs.stdenv.mkDerivation rec {
              pname = lang + "_grammar";
              version = extensionToml.grammars.${lang}.commit;
              src = builtins.fetchGit {
                rev = version;
                url = extensionToml.grammars.${lang}.repository;
              };
              buildInputs = with pkgs; [ docker tree-sitter ];
              # FIX: not compile
              buildPhase = ''
                HOME=$(mktemp -d fake-homeXXXX)
                tree-sitter build --wasm -o $out
              '';
            } else null;

          # fenix: rustup replacement for reproducible builds
          toolchain = fenix.fromToolchainFile {
            file = ./rust-toolchain.toml;
            sha256 = "sha256-VZZnlyP69+Y3crrLHQyJirqlHrTtGTsyiSnZB8jEvVo=";
          };

          buildInputs = with pkgs; [ pkg-config ];

          extension-wasm = craneLib.buildPackage {
            doCheck = false;
            pname = plugin-id;
            src = craneLib.cleanCargoSource (craneLib.path ./.);
            cargoExtraArgs = "--target wasm32-wasip1";
            installPhaseCommand = ''
              mkdir -p $out
              cp target/wasm32-wasip1/release/${plugin-id}.wasm $out/extension.wasm
            '';
            inherit buildInputs;
          };

          extension-lsp = if lsp-name != "" then craneLib.buildPackage {
            doCheck = false;
            pname = plugin-id;
            src = craneLib.cleanCargoSource (craneLib.path ./.);
            buildPhaseCargoCommand = "cargo build --release -p ${lsp-name}";
            installPhaseCommand = ''
              mkdir -p $out
              cp target/release/${lsp-name} $out/
            '';
            inherit buildInputs;
          } else null;

          # Lista de paths que siempre incluiremos
          basePaths = [ extension-wasm ]
            ++ lib.optional (extension-lsp != null) extension-lsp
            ++ lib.optional (grammar != null) grammar;

          all = pkgs.symlinkJoin {
            name = plugin-id;
            paths = basePaths;
            postBuild = ''
              cp ${./extension.toml} $out/extension.toml
              ${lib.optionalString (builtins.pathExists ./languages) ''
                cp -r ${./languages} $out/languages
              ''}
            '';
          };
      in {
        # `nix build`
        packages = rec {
          default = extension;
          extension = all;
          wasm = extension-wasm;
        } // lib.optionalAttrs (extension-lsp != null) {
          lsp = extension-lsp;
        } // lib.optionalAttrs (grammar != null) {
          inherit grammar;
        };

        # `nix develop`
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo-dist
            cargo-release
            docker
            tree-sitter
          ];
          LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
        };
      }
    );
}
