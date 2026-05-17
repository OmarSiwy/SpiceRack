{
  description = "PySpice-rs: PySpice core rewritten in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [
          (import rust-overlay)
          (import ./nix/ngspice.nix)
        ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
        pythonEnv = pkgs.python312.withPackages (ps: with ps; [
          numpy
          pytest
        ]);
        openvaf = import ./nix/openvaf.nix { inherit pkgs; };
        vacask = import ./nix/vacask.nix { inherit pkgs; openvafPkg = openvaf; };
        xyce = import ./nix/xyce.nix { inherit pkgs; };

        srcFiltered = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let baseName = baseNameOf (toString path); in
            !(baseName == "target" || baseName == "target_nix" || baseName == "result"
              || baseName == ".git");
        };

        pyspiceRs = pkgs.python312Packages.buildPythonPackage {
          pname = "pyspice-rs";
          version = "0.1.0";
          format = "pyproject";
          src = srcFiltered;

          cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
            name = "pyspice-rs-vendor";
            hash = "sha256-SBJFkUj7mqqcZ1tmDQXejj7NkPbvO6c85nqpYH9O6n0=";
            src = srcFiltered;
          };

          nativeBuildInputs = [
            pkgs.rustPlatform.cargoSetupHook
            pkgs.rustPlatform.maturinBuildHook
            pkgs.cargo
            rustToolchain
            pkgs.pkg-config
          ];

          buildInputs = [
            pkgs.libngspice
          ];

          propagatedBuildInputs = [
            pkgs.python312Packages.numpy
          ];

          pythonImportsCheck = [ "pyspice_rs" ];
        };
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.cargo
            pkgs.maturin
            pkgs.ngspice
            pkgs.libngspice
            pythonEnv
            pkgs.pkg-config
            openvaf
            vacask
            xyce
          ];

          shellHook = ''
            echo "PySpice-rs dev shell"
            echo "  rust: $(rustc --version)"
            echo "  python: $(python3 --version)"
            echo "  ngspice: $(ngspice --version 2>&1 | head -1)"
          '';

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };

        packages = {
          default = pyspiceRs;
          inherit openvaf vacask xyce;
        };
      }
    );
}
