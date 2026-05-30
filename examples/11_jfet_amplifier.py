"""
Example 11: JFET Common-Source Amplifier

Self-biased 2N5457 N-JFET, VDD=15V.
Demonstrates J element with .model.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm, u_uF

# ── DUT ──

dut = ps.Subcircuit("jfet_cs", ["vdd", "sig_in", "output"])
dut.model("2N5457", "NJF", VTO=-1.8, BETA=1.304e-3, LAMBDA=5e-3,
          IS=33.57e-15, RD=1.0, RS=1.0, CGS=2e-12, CGD=1.6e-12)

dut.C(name="cin", positive="sig_in", negative="gate", value=1 @ u_uF)
dut.R(name="rg", positive="gate", negative=dut.gnd, value=1000 @ u_kOhm)
dut.J(name="1", drain="drain", gate="gate", source="source", model="2N5457")
dut.R(name="rd", positive="vdd", negative="drain", value=3.3 @ u_kOhm)
dut.R(name="rs", positive="source", negative=dut.gnd, value=1 @ u_kOhm)
dut.C(name="cs", positive="source", negative=dut.gnd, value=100 @ u_uF)
dut.C(name="cout", positive="drain", negative="output", value=1 @ u_uF)
dut.R(name="load", positive="output", negative=dut.gnd, value=10 @ u_kOhm)

# ── Simulate ──

tb = ps.Testbench(dut)
tb.V(name="dd", positive="vdd", negative=dut.gnd, value=15 @ u_V)
tb.V(name="in", positive="sig_in", negative=dut.gnd, value=0 @ u_V)
tb.with_backend("ngspice")

try:
    op = tb.operating_point()
    vgs = -1.0
    beta = 1.304e-3
    i_d = beta * (1 - vgs / -1.8) ** 2
    print(f"Approximate: VGS~{vgs}V, ID~{i_d*1000:.2f}mA")
    print(f"Vdrain ~ {15 - i_d*3300:.2f}V")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
