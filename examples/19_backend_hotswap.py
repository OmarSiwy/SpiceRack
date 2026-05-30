"""
Example 19: Backend Hotswap

Same voltage divider simulated on ngspice and vacask.
The Testbench API generates backend-native netlists via IR codegen --
switching backends is a single `with_backend()` call.

No PDK needed (passive components only).
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

# ── DUT: voltage divider (backend-neutral) ──

dut = ps.Subcircuit("divider", ["vin", "vout"])
dut.R(name="top", positive="vin", negative="vout", value=2 @ u_kOhm)
dut.R(name="bot", positive="vout", negative=dut.gnd, value=1 @ u_kOhm)

expected = 10 * 1000 / (2000 + 1000)

# ── Run on each backend ──

for backend in ["ngspice", "vacask"]:
    print(f"\n{'='*40}")
    print(f"Backend: {backend}")
    print(f"{'='*40}")

    tb = ps.Testbench(dut)
    tb.V(name="supply", positive="vin", negative=dut.gnd, value=10 @ u_V)
    tb.with_backend(backend)  # <-- one-line swap

    try:
        op = tb.operating_point()
        vout = op["vout"]
        print(f"  V(vout)   = {vout:.4f} V")
        print(f"  Expected  = {expected:.4f} V")
        print(f"  Match: {'yes' if abs(vout - expected) < 0.01 else 'NO'}")
    except RuntimeError as e:
        print(f"  Skipped ({backend} not available): {e}")

print(f"\nKey point: same DUT, same testbench -- only with_backend() changed.")
