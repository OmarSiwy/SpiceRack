"""
Example 06: Inverting Amplifier (Behavioral Opamp)

Ideal opamp modeled as BV (behavioral voltage source):
  Vout = 1e6 * (V(inp) - V(inn))
Configured as inverting amp: Gain = -Rf/Rin = -10k/1k = -10.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

# ── DUT ──

dut = ps.Subcircuit("inv_amp", ["sig_in", "output"])
dut.R(name="in", positive="sig_in", negative="inn", value=1 @ u_kOhm)
dut.R(name="f", positive="inn", negative="output", value=10 @ u_kOhm)
dut.R(name="ref", positive="inp", negative=dut.gnd, value=1 @ u_kOhm)
dut.BV(name="opamp", positive="output", negative=dut.gnd,
       expression="1e6 * (V(inp) - V(inn))")
dut.R(name="load", positive="output", negative=dut.gnd, value=10 @ u_kOhm)

# ── Simulate ──

tb = ps.Testbench(dut)
tb.V(name="in", positive="sig_in", negative=dut.gnd, value=0.5 @ u_V)
tb.with_backend("ngspice")

expected = 0.5 * -10.0

try:
    op = tb.operating_point()
    vout = op["output"]
    print(f"V(output) = {vout:.4f} V")
    print(f"Expected  = {expected:.4f} V")
    print(f"Match: {'yes' if abs(vout - expected) < 0.05 else 'NO'}")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
