"""
Example 13: Voltage- and Current-Controlled Switches

S (voltage-controlled) and W (current-controlled) switches.
Demonstrates switch models with ON/OFF resistance.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

# ── DUT ──

dut = ps.Subcircuit("switches", ["vdd", "vctrl", "sw_v_out", "sw_i_out"])

dut.model("SW_VCTL", "SW", VT=2.5, VH=0.5, RON=1.0, ROFF=1e6)
dut.model("SW_ICTL", "CSW", IT=1e-3, IH=0.1e-3, RON=1.0, ROFF=1e6)

# Voltage-controlled switch
dut.S(name="1", positive="vdd", negative="sw_v_out",
      control_positive="vctrl", control_negative=dut.gnd, model="SW_VCTL")
dut.R(name="load_v", positive="sw_v_out", negative=dut.gnd, value=1 @ u_kOhm)

# Current-controlled switch
dut.V(name="sense_i", positive="i_ctrl_in", negative="i_ctrl_out", value=0.0)
dut.R(name="ctrl_i", positive="vdd", negative="i_ctrl_in", value=1 @ u_kOhm)
dut.R(name="sense_gnd", positive="i_ctrl_out", negative=dut.gnd, value=0.001)
dut.W(name="1", positive="vdd", negative="sw_i_out",
      vcontrol="Vsense_i", model="SW_ICTL")
dut.R(name="load_i", positive="sw_i_out", negative=dut.gnd, value=1 @ u_kOhm)

# ── Simulate (control voltage high -> S switch ON) ──

tb = ps.Testbench(dut)
tb.V(name="dd", positive="vdd", negative=dut.gnd, value=5 @ u_V)
tb.V(name="ctrl", positive="vctrl", negative=dut.gnd, value=5 @ u_V)
tb.with_backend("ngspice")

try:
    op = tb.operating_point()
    print(f"V(sw_v_out) = {op['sw_v_out']:.4f} V  (S switch ON, expect ~5V)")
    print(f"V(sw_i_out) = {op['sw_i_out']:.4f} V  (W switch ON, expect ~5V)")
    print(f"S: ON when Vctrl > VT+VH = 3.0V")
    print(f"W: ON when Isense > IT+IH = 1.1mA")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
