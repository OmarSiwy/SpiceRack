"""
Example 21: Combined Backend + PDK Hotswap

Same CMOS inverter across a matrix of PDKs and backends.
Demonstrates full portability: one circuit, any PDK, any backend.

ModelLibrary accepts per-backend paths via keyword arguments --
the codegen picks the right file automatically.  Backends without
a PDK-native model path are skipped (e.g. vacask can't parse
ngspice .lib section syntax).

Requires PDK_ROOT, PDKs via ciel, ngspice on PATH.
"""
import os
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

PDK_ROOT = os.environ.get("PDK_ROOT", os.path.expanduser("~/.ciel"))

# ── PDK configurations with per-backend model paths ──
# Only list backends for which the PDK ships native model files.
# vacask uses Spectre-format includes -- most open PDKs only ship ngspice.

PDKS = {
    "sky130": {
        "lib": f"{PDK_ROOT}/sky130A/libs.tech/ngspice/sky130.lib.spice",
        "corner": "tt",
        "nmos": "sky130_fd_pr__nfet_01v8",
        "pmos": "sky130_fd_pr__pfet_01v8",
        "vdd": 1.8,
        "backends": ["ngspice"],
    },
    "gf180mcu": {
        "lib": f"{PDK_ROOT}/gf180mcuD/libs.tech/ngspice/sm141064.ngspice",
        "corner": "typical",
        "nmos": "nfet_03v3",
        "pmos": "pfet_03v3",
        "vdd": 3.3,
        "backends": ["ngspice"],
    },
}


def build_inverter(nmos_model, pmos_model):
    """CMOS inverter -- topology is universal across PDKs."""
    dut = ps.Subcircuit("inverter", ["vdd", "vin", "vout"])
    dut.M(
        name="p1",
        drain="vout",
        gate="vin",
        source="vdd",
        bulk="vdd",
        model=pmos_model,
    )
    dut.M(
        name="n1",
        drain="vout",
        gate="vin",
        source=dut.gnd,
        bulk=dut.gnd,
        model=nmos_model,
    )
    return dut


# ── Full matrix: PDK x Backend ──

print("Combined Backend + PDK Hotswap Matrix")
print("=" * 60)

for pdk_name, cfg in PDKS.items():
    for backend in cfg["backends"]:
        print(f"\n--- {pdk_name} / {backend} ---")

        dut = build_inverter(cfg["nmos"], cfg["pmos"])

        lib = ps.ModelLibrary(cfg["lib"], corner=cfg["corner"])

        tb = ps.Testbench(dut)
        tb.use_pdk(lib)
        tb.V(name="dd", positive="vdd", negative=dut.gnd, value=cfg["vdd"])
        tb.V(name="in", positive="vin", negative=dut.gnd, value=cfg["vdd"] / 2)
        tb.R(name="load", positive="vout", negative=dut.gnd, value=10 @ u_kOhm)
        tb.with_backend(backend)

        try:
            if not os.path.exists(cfg["lib"]):
                raise RuntimeError(
                    f"PDK not installed. Run: ciel install {pdk_name}"
                )
            op = tb.operating_point()
            print(f"  V(vout) = {op['vout']:.4f} V")
        except RuntimeError as e:
            print(f"  Skipped: {e}")

print(f"\nKey point: one build_inverter(), swapped via ModelLibrary + with_backend().")
