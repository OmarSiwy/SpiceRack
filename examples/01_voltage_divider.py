"""
Example 01: Voltage Divider

Resistive voltage divider: Vin=10V, R1=2k, R2=1k.
Expected Vout = 10 * 1k / (2k + 1k) = 3.333V.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

# ── DUT ──

dut = ps.Subcircuit("divider", ["vin", "vout"])
dut.R(name="top", positive="vin", negative="vout", value=2 @ u_kOhm)
dut.R(name="bot", positive="vout", negative=dut.gnd, value=1 @ u_kOhm)

expected = 10 * 1000 / (2000 + 1000)

# ── Simulate ──

tb = ps.Testbench(dut)
tb.V(name="supply", positive="vin", negative=dut.gnd, value=10 @ u_V)
tb.with_backend("ngspice")

try:
    op = tb.operating_point()
    vout = op["vout"]
    print(f"V(vout)   = {vout:.4f} V")
    print(f"Expected  = {expected:.4f} V")
    print(f"Match: {'yes' if abs(vout - expected) < 0.01 else 'NO'}")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
