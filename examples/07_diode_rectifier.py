"""
Example 07: Full-Bridge Diode Rectifier

Four 1N4148 diodes, 10Vpk 60Hz AC input, 100uF filter cap.
Demonstrates D element with .model and transient analysis.
Expected Vdc ~ Vpeak - 2*Vfwd ~ 8.6V.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_kOhm, u_uF

# ── DUT ──

dut = ps.Subcircuit("bridge_rect", ["ac_p", "ac_n", "out_p"])
dut.model("1N4148", "D", IS=2.52e-9, RS=0.568, N=1.752,
          BV=100, IBV=100e-6, CJO=4e-12, TT=20e-9)

dut.D(name="1", anode="ac_p", cathode="out_p", model="1N4148")
dut.D(name="2", anode="ac_n", cathode="out_p", model="1N4148")
dut.D(name="3", anode=dut.gnd, cathode="ac_p", model="1N4148")
dut.D(name="4", anode=dut.gnd, cathode="ac_n", model="1N4148")
dut.C(name="filt", positive="out_p", negative=dut.gnd, value=100 @ u_uF)
dut.R(name="load", positive="out_p", negative=dut.gnd, value=1 @ u_kOhm)

# ── Transient ──

tb = ps.Testbench(dut)
tb.SinusoidalVoltageSource(
    name="ac", positive="ac_p", negative="ac_n",
    amplitude=10.0, frequency=60.0,
)
tb.with_backend("ngspice")

try:
    tran = tb.transient(step_time=10e-6, end_time=50e-3)
    vout = tran["out_p"]
    vdc_final = vout[-1]
    print(f"V(out_p) at t=50ms: {vdc_final:.2f} V")
    print(f"Expected ~ {10.0 - 2*0.7:.1f} V (after cap charges)")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
