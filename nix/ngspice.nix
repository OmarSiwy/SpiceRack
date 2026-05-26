# Pin ngspice to 43 — versions 44+ have a regression where binned
# MOSFET models (.model nfet_01v8.0, .1, …) fail scoped resolution,
# breaking sky130 and gf180 PDK simulations.
#
# ngspice CLI derives from libngspice (withNgshared = false),
# so it picks up the pinned version automatically.
final: prev: {
  libngspice = let
    base = if prev.stdenv.isDarwin
      then prev.libngspice.override { stdenv = prev.llvmPackages_18.stdenv; }
      else prev.libngspice;
  in base.overrideAttrs (_old: {
    version = "43";
    src = prev.fetchurl {
      url = "mirror://sourceforge/ngspice/ngspice-43.tar.gz";
      hash = "sha256-FN1qbwhTHyBRwTrmN5CkVwi9Q/PneIamqEiYwpexNpk=";
    };
    patches = [ ];
  });
}
