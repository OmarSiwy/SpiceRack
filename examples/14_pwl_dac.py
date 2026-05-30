"""
Example 14: 3-bit DAC Staircase (PWL Source)

PieceWiseLinearVoltageSource generates 8 voltage steps (0..7/8 * Vref).
Vref=3.3V, each step 1us. Demonstrates PWL waveform with transient.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_kOhm

# ── DUT ──

dut = ps.Subcircuit("dac_out", ["dac", "buffered"])
dut.R(name="buf", positive="dac", negative="buffered", value=0.1 @ u_kOhm)
dut.R(name="load", positive="buffered", negative=dut.gnd, value=10 @ u_kOhm)

# ── Build PWL staircase ──

vref = 3.3
n_steps = 8
step_dur = 1e-6
pwl_values = []
for i in range(n_steps):
    v = vref * i / n_steps
    pwl_values.append((i * step_dur, v))
    pwl_values.append(((i + 1) * step_dur - 1e-9, v))
pwl_values.append((n_steps * step_dur, 0.0))

# ── Transient ──

tb = ps.Testbench(dut)
tb.PieceWiseLinearVoltageSource(
    name="dac", positive="dac", negative=dut.gnd, values=pwl_values,
)
tb.with_backend("ngspice")

try:
    tran = tb.transient(step_time=10e-9, end_time=n_steps * step_dur)
    vout = tran["buffered"]
    print(f"Simulated {len(tran.time)} points")
    print(f"LSB = {vref / n_steps:.4f} V")
    for i in range(n_steps):
        expected = vref * i / n_steps
        print(f"  Code {i}: {expected:.4f} V")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
