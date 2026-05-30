"""
Example 04: Common-Emitter BJT Amplifier

Voltage-divider biased 2N2222, Vcc=12V, Ic~1mA.
Demonstrates Q element with .model, coupling/bypass caps.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm, u_Ohm, u_uF

# ── DUT ──

dut = ps.Subcircuit("ce_amp", ["vcc", "sig_in", "output"])
dut.model("2N2222", "NPN", IS=14.34e-15, BF=255.9, VAF=74.03,
          RB=10.0, RC=1.0, CJE=22.01e-12, CJC=7.306e-12, TF=0.4111e-9)

dut.C(name="cin", positive="sig_in", negative="base", value=10 @ u_uF)
dut.R(name="r1", positive="vcc", negative="base", value=27 @ u_kOhm)
dut.R(name="r2", positive="base", negative=dut.gnd, value=4.7 @ u_kOhm)
dut.Q(name="1", collector="collector", base="base", emitter="emitter", model="2N2222")
dut.R(name="rc", positive="vcc", negative="collector", value=4.7 @ u_kOhm)
dut.R(name="re", positive="emitter", negative=dut.gnd, value=1 @ u_kOhm)
dut.C(name="ce", positive="emitter", negative=dut.gnd, value=100 @ u_uF)
dut.C(name="cout", positive="collector", negative="output", value=10 @ u_uF)
dut.R(name="load", positive="output", negative=dut.gnd, value=10 @ u_kOhm)

# ── Simulate ──

tb = ps.Testbench(dut)
tb.V(name="cc", positive="vcc", negative=dut.gnd, value=12 @ u_V)
tb.V(name="in", positive="sig_in", negative=dut.gnd, value=0 @ u_V)
tb.with_backend("ngspice")

try:
    op = tb.operating_point()
    vb = 12 * 4700 / (27000 + 4700)
    ve = vb - 0.7
    print(f"Approximate bias: Vb={vb:.2f}V, Ve={ve:.2f}V, Ic={ve/1000*1000:.2f}mA")
    print(f"Expected Av ~ -{4700/1000:.1f} (with bypass cap)")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
