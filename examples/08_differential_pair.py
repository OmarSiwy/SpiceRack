"""
Example 08: MOSFET Differential Pair

Matched NMOS pair with 200uA tail current source, resistive loads.
VDD=3.3V, Vcm=1.65V, Vdiff=10mV.
Demonstrates I (current source) element.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm, u_uA

# ── DUT ──

dut = ps.Subcircuit("diff_pair", ["vdd", "inp", "inn", "out_p", "out_n"])
dut.model("NMOS_DIFF", "NMOS", LEVEL=1, VTO=0.7, KP=110e-6, LAMBDA=0.04, TOX=9e-9)

dut.M(name="1", drain="out_p", gate="inp", source="tail", bulk=dut.gnd, model="NMOS_DIFF")
dut.M(name="2", drain="out_n", gate="inn", source="tail", bulk=dut.gnd, model="NMOS_DIFF")
dut.R(name="d1", positive="vdd", negative="out_p", value=10 @ u_kOhm)
dut.R(name="d2", positive="vdd", negative="out_n", value=10 @ u_kOhm)
dut.I(name="tail", positive="tail", negative=dut.gnd, value=200 @ u_uA)

# ── Simulate ──

tb = ps.Testbench(dut)
tb.V(name="dd", positive="vdd", negative=dut.gnd, value=3.3 @ u_V)
tb.V(name="cm", positive="vcm", negative=dut.gnd, value=1.65 @ u_V)
tb.V(name="dp", positive="inp", negative="vcm", value=0.005 @ u_V)
tb.V(name="dn", positive="inn", negative="vcm", value=-0.005 @ u_V)
tb.with_backend("ngspice")

try:
    op = tb.operating_point()
    vp = op["out_p"]
    vn = op["out_n"]
    print(f"V(out_p) = {vp:.4f} V")
    print(f"V(out_n) = {vn:.4f} V")
    print(f"Vdiff    = {vp - vn:.4f} V")
    print(f"Vcm_out  = {(vp + vn) / 2:.4f} V  (expected ~ {3.3 - 0.1*10:.2f} V)")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
