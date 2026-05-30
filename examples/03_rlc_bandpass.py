"""
Example 03: RLC Bandpass with Transformer

Two coupled inductors (K=0.9) forming a transformer, plus a tuning
capacitor on the secondary.  Demonstrates L and K elements.
Resonant f_0 = 1/(2*pi*sqrt(L*C)) ~ 5.03 MHz.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_kOhm, u_uH, u_pF

# ── DUT ──

dut = ps.Subcircuit("rlc_bp", ["input", "output"])
dut.R(name="src", positive="input", negative="n1", value=50.0)
dut.L(name="pri", positive="n1", negative=dut.gnd, value=10 @ u_uH)
dut.L(name="sec", positive="output", negative=dut.gnd, value=10 @ u_uH)
dut.K(name="xfmr", inductor1="pri", inductor2="sec", coupling=0.9)
dut.C(name="tune", positive="output", negative=dut.gnd, value=100 @ u_pF)
dut.R(name="load", positive="output", negative=dut.gnd, value=1 @ u_kOhm)

# ── AC sweep ──

tb = ps.Testbench(dut)
tb.V(name="src", positive="input", negative=dut.gnd, value=1.0)
tb.with_backend("ngspice")

try:
    ac = tb.ac(variation="dec", number_of_points=20,
               start_frequency=1e6, stop_frequency=20e6)
    freqs = ac.frequency
    mag = ac["output"]
    peak_idx = mag.index(max(mag))
    peak_f = freqs[peak_idx]
    print(f"Peak response at {peak_f/1e6:.2f} MHz")
    print(f"Expected resonance ~ 5.03 MHz")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
