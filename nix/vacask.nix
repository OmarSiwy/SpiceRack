{ pkgs, openvafPkg }:

pkgs.stdenv.mkDerivation rec {
  pname = "vacask";
  version = "unstable-2026";

  src = pkgs.fetchFromGitHub {
    owner = "robtaylor";
    repo = "VACASK";
    rev = "bcd48e2dd25182f5aaa3392c4e27b4e198372744";
    hash = "sha256-/x6yJ+fklipvYbtI5rHx4d5YIpC9IJ5uhHCtWC5eJJg=";
  };

  nativeBuildInputs = with pkgs; [
    cmake
    ninja
    pkg-config
    python3
    bison
    flex
  ];

  buildInputs = with pkgs; [
    suitesparse
    openblas
    boost
    tomlplusplus
  ];

  postPatch = ''
    # --- Bypass all Darwin/Homebrew blocks ---
    # The CMakeLists.txt "Darwin" blocks (lines 47, 130, 280) assume Homebrew:
    # they call brew --prefix, set SuiteSparse_DIR/TOMLPP_DIR/Boost_ROOT to
    # Homebrew paths, etc. None of this works in Nix's sandbox.
    # By renaming "Darwin" the platform falls through to the Linux/generic
    # else blocks, where all our Nix patches already apply.
    sed -i 's/STREQUAL "Darwin"/STREQUAL "DarwinBrew"/g' CMakeLists.txt

    # --- Linux/generic block Boost fixes ---
    sed -i 's/set(Boost_NO_SYSTEM_PATHS TRUE)//' CMakeLists.txt
    sed -i 's/find_package(Boost 1.88 REQUIRED COMPONENTS filesystem process system)/find_package(Boost REQUIRED COMPONENTS filesystem process)/' CMakeLists.txt
    sed -i 's|set(Boost_EXTRA_LINK_DIR "''${Boost_INCLUDE_DIRS}/stage/lib")|set(Boost_EXTRA_LINK_DIR "''${Boost_LIBRARY_DIRS}")|' CMakeLists.txt
    sed -i 's/boost_system boost_filesystem boost_process/boost_filesystem boost_process/' CMakeLists.txt

    # --- Header fixes ---
    sed -i 's|suitesparse/klu.h|klu.h|g' include/klumatrix.h
  '';

  # Newer clang (macOS) promotes certain warnings to hard errors by default.
  env.NIX_CFLAGS_COMPILE = "-Wno-error -Wno-defaulted-function-deleted";

  cmakeFlags = [
    "-DCMAKE_BUILD_TYPE=Release"
    "-DOPENVAF_DIR=${openvafPkg}/bin"
    "-DTOMLPP_DIR=${pkgs.tomlplusplus}"
    "-DSuiteSparse_DIR=${pkgs.suitesparse}"
  ];

  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin
    # The simulator binary is built into the simulator/ subdirectory.
    cp simulator/vacask $out/bin/vacask
    runHook postInstall
  '';

  meta = {
    description = "VACASK – Verilog-A Circuit Analysis Kernel";
    homepage = "https://github.com/robtaylor/VACASK";
    license = pkgs.lib.licenses.gpl2Plus;
    platforms = pkgs.lib.platforms.linux ++ pkgs.lib.platforms.darwin;
  };
}
