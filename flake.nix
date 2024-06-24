{
  inputs = {
    systems.url = "github:nix-systems/default-linux";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = inputs @ {
    self,
    systems,
    nixpkgs,
    ...
  }: let
    eachSystem = f: nixpkgs.lib.genAttrs (import systems) (system: f nixpkgs.legacyPackages.${system});

    naersk = eachSystem (pkgs: pkgs.callPackage inputs.naersk {});

    treefmt-config = {pkgs, ...}: {
      projectRootFile = "flake.nix";
      programs = {
        # Nix
        alejandra.enable = true;
        # Rust
        rustfmt.enable = true;
        # Everything else
        prettier.enable = true;
      };
    };
    treefmtEval = eachSystem (pkgs: inputs.treefmt-nix.lib.evalModule pkgs treefmt-config);
  in {
    # For `nix build` & `nix run`:
    defaultPackage = eachSystem (pkgs: self.packages.${pkgs.system}.sse-tannoy);

    packages = eachSystem (pkgs: {
      sse-tannoy = naersk.${pkgs.system}.buildPackage {
        src =
          pkgs.nix-gitignore.gitignoreSourcePure [
            "flake.*"
            ".github"
          ]
          ./.;
      };
      image = pkgs.dockerTools.buildImage {
        name = "sse-tannoy";
        config = {
          Cmd = with self.packages.${pkgs.system}; ["${sse-tannoy}/bin/sse-tannoy"];
        };
      };
    });

    # For `nix develop`:
    devShell = eachSystem (pkgs:
      pkgs.mkShell {
        nativeBuildInputs = with pkgs; [rustc cargo];
      });

    # for `nix fmt`
    formatter = eachSystem (pkgs: treefmtEval.${pkgs.system}.config.build.wrapper);

    # for `nix flake check`
    checks = eachSystem (pkgs: {
      formatting = treefmtEval.${pkgs.system}.config.build.check self;
    });
  };
}
