"""
Example 10: Subcircuit Instantiation

Defines an NMOS inverter subcircuit and instantiates it twice
(buffer = double inversion) using X().
Demonstrates Subcircuit definition + add_subcircuit + X element.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

# ── Inner subcircuit: resistive-load NMOS inverter ──

inverter = ps.Subcircuit("inverter", ["vdd", "in", "out"])
inverter.model("NMOS_INV", "NMOS", LEVEL=1, VTO=0.7, KP=110e-6)
inverter.R(name="rup", positive="vdd", negative="out", value=10 @ u_kOhm)
inverter.M(name="n1", drain="out", gate="in", source=inverter.gnd,
           bulk=inverter.gnd, model="NMOS_INV")

# ── DUT: two inverters in series ──

dut = ps.Subcircuit("buffer", ["vdd", "a", "output"])
dut.X("inv1", "inverter", "vdd", "a", "mid")
dut.X("inv2", "inverter", "vdd", "mid", "output")
dut.C(name="load", positive="output", negative=dut.gnd, value=10e-15)

# ── Simulate ──

tb = ps.Testbench(dut)
tb.add_subcircuit(inverter)
tb.V(name="dd", positive="vdd", negative=dut.gnd, value=3.3 @ u_V)
tb.V(name="in", positive="a", negative=dut.gnd, value=3.3 @ u_V)
tb.with_backend("ngspice")

try:
    op = tb.operating_point()
    print(f"Input = VDD -> mid ~ 0V -> output ~ VDD (buffer)")
    print(f"V(output) = {op['output']:.4f} V")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
