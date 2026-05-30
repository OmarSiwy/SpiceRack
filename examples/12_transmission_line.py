"""
Example 12: Lossless Transmission Line

50-ohm line, 1ns delay, mismatched 100-ohm load.
Demonstrates T element. Pulse input shows reflections.
Reflection coefficient = (Zl - Z0)/(Zl + Z0) = 0.333.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_Ohm

# ── DUT ──

dut = ps.Subcircuit("tline", ["vs", "out_p"])
dut.R(name="src", positive="vs", negative="in_p", value=50 @ u_Ohm)
dut.T(name="line", input_positive="in_p", input_negative=dut.gnd,
      output_positive="out_p", output_negative=dut.gnd, Z0=50.0, TD=1e-9)
dut.R(name="load", positive="out_p", negative=dut.gnd, value=100 @ u_Ohm)

# ── Transient ──

tb = ps.Testbench(dut)
tb.PulseVoltageSource(
    name="src", positive="vs", negative=dut.gnd,
    initial_value=0.0, pulsed_value=1.0,
    pulse_width=5e-9, period=20e-9,
    rise_time=0.1e-9, fall_time=0.1e-9,
)
tb.with_backend("ngspice")

gamma = (100 - 50) / (100 + 50)

try:
    tran = tb.transient(step_time=0.01e-9, end_time=20e-9)
    vout = tran["out_p"]
    print(f"Reflection coefficient = {gamma:.3f}")
    print(f"V(out_p) peak = {max(vout):.3f} V")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
