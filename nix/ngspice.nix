# Darwin needs llvmPackages_18 stdenv for ngspice to build.
# No version pin — nixpkgs-unstable tracks ngspice 44+ which
# supports OSDI v0.4 modules from openvaf-r.  The binned-model
# scoped-resolution issue (HANDOFF #10) is a fundamental ngspice
# limitation present in all versions, not a 44-specific regression.
final: prev: {
  libngspice = if prev.stdenv.isDarwin
    then prev.libngspice.override { stdenv = prev.llvmPackages_18.stdenv; }
    else prev.libngspice;
}
