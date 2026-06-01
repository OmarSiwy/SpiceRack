"""
Example 02: RC Low-Pass Filter

First-order RC filter: R=1k, C=1uF.
AC analysis shows -3dB at f = 1/(2*pi*R*C) ~ 159 Hz.
Step response shows exponential charging with tau = R*C = 1ms.
"""
import math
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm, u_uF

# ── DUT ──

dut = ps.Subcircuit("rc_lpf", ["vin", "vout"])
dut.R(name="r1", positive="vin", negative="vout", value=1 @ u_kOhm)
dut.C(name="c1", positive="vout", negative=dut.gnd, value=1 @ u_uF)

# ── AC analysis (frequency response) ──

tb_ac = ps.Testbench(dut)
tb_ac.V(name="in", positive="vin", negative=dut.gnd, value=0.0, ac=1.0)
tb_ac.with_backend("ngspice")

f_3db = 1.0 / (2 * math.pi * 1e3 * 1e-6)  # ~159.15 Hz

try:
    ac = tb_ac.ac(variation="dec", number_of_points=100, start_frequency=1.0, stop_frequency=1e6)
    freqs = ac.frequency
    vout = ac["vout"]
    try:
        vout_imag = ac["vout_imag"]
    except (AttributeError, KeyError):
        vout_imag = [0] * len(vout)
    vout_mag = [abs(complex(r, i)) for r, i in zip(vout, vout_imag)]

    print(f"f_3dB expected = {f_3db:.1f} Hz")
    print(f"DC gain = {vout_mag[0]:.4f}")
    print(f"Points: {len(freqs)}")
except RuntimeError as e:
    print(f"AC skipped (ngspice not available): {e}")

# ── Transient (step response) ──

tb_tran = ps.Testbench(dut)
tb_tran.PulseVoltageSource(
    name="step", positive="vin", negative=dut.gnd,
    initial_value=0.0, pulsed_value=1.0,
    pulse_width=10e-3, period=20e-3,
    rise_time=1e-9, fall_time=1e-9,
)
tb_tran.with_backend("ngspice")

tau = 1e3 * 1e-6  # R*C = 1ms

try:
    tran = tb_tran.transient(step_time=10e-6, end_time=5e-3)
    times = tran.time
    vout = tran["vout"]

    for t, v in zip(times, vout):
        if abs(t - tau) < 20e-6:
            print(f"V(vout) at t=tau ({tau*1000:.0f}ms) = {v:.4f} V  (expect ~0.632)")
            break

    print(f"V(vout) final = {vout[-1]:.4f} V")
    print(f"tau = R*C = {tau*1000:.1f} ms")
except RuntimeError as e:
    print(f"Transient skipped (ngspice not available): {e}")
