"""
Example 20: PDK Hotswap

Same NMOS common-source amplifier simulated with sky130 and gf180mcu PDKs.
The circuit topology is identical -- only the ModelLibrary changes.

Requires PDK_ROOT set and PDKs installed via ciel:
    ciel install sky130
    ciel install gf180mcu
"""
import os
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

PDK_ROOT = os.environ.get("PDK_ROOT", os.path.expanduser("~/.ciel"))

# ── PDK configurations ──
# Each maps generic concepts to PDK-specific values.

PDKS = {
    "sky130": {
        "lib": f"{PDK_ROOT}/sky130A/libs.tech/ngspice/sky130.lib.spice",
        "corner": "tt",
        "nmos": "sky130_fd_pr__nfet_01v8",
        "vdd": 1.8,
    },
    "gf180mcu": {
        "lib": f"{PDK_ROOT}/gf180mcuD/libs.tech/ngspice/sm141064.ngspice",
        "corner": "typical",
        "nmos": "nfet_03v3",
        "vdd": 3.3,
    },
}


def build_cs_amp(nmos_model):
    """NMOS common-source amplifier. Topology is PDK-independent."""
    dut = ps.Subcircuit("cs_amp", ["vdd", "vin", "vout"])
    dut.R(name="drain", positive="vdd", negative="vout", value=5 @ u_kOhm)
    dut.M(
        name="n1",
        drain="vout",
        gate="vin",
        source=dut.gnd,
        bulk=dut.gnd,
        model=nmos_model,
    )
    return dut


# ── Run with each PDK ──

for pdk_name, cfg in PDKS.items():
    print(f"\n{'='*50}")
    print(f"PDK: {pdk_name}  (corner={cfg['corner']}, VDD={cfg['vdd']}V)")
    print(f"{'='*50}")

    dut = build_cs_amp(cfg["nmos"])

    lib = ps.ModelLibrary(cfg["lib"], corner=cfg["corner"])  # <-- PDK swap

    tb = ps.Testbench(dut)
    tb.use_pdk(lib)
    tb.V(name="dd", positive="vdd", negative=dut.gnd, value=cfg["vdd"])
    tb.V(name="gate", positive="vin", negative=dut.gnd, value=cfg["vdd"] / 2)
    tb.with_backend("ngspice")

    print(f"  Model library: {lib}")

    try:
        if not os.path.exists(cfg["lib"]):
            raise RuntimeError(
                f"PDK not installed: {cfg['lib']}\n"
                f"  Install with: ciel install {pdk_name}"
            )
        op = tb.operating_point()
        print(f"  V(vout) = {op['vout']:.4f} V")
        print(f"  V(vin)  = {op['vin']:.4f} V")
    except RuntimeError as e:
        print(f"  Skipped: {e}")

print(f"\nKey point: same topology, same testbench -- only ModelLibrary changed.")
