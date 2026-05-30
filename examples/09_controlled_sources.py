"""
Example 09: All Four Controlled Source Types

E (VCVS), G (VCCS), F (CCCS), H (CCVS) driven from the same
1V input.  Each produces output on a separate node.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

# ── DUT ──

dut = ps.Subcircuit("ctrl_src", ["input", "out_vcvs", "out_vccs", "out_cccs", "out_ccvs"])

dut.V(name="sense", positive="input", negative="isense", value=0.0)
dut.R(name="sense_load", positive="isense", negative=dut.gnd, value=1 @ u_kOhm)

dut.E(name="1", positive="out_vcvs", negative=dut.gnd,
      control_positive="input", control_negative=dut.gnd, voltage_gain=5.0)
dut.R(name="e_load", positive="out_vcvs", negative=dut.gnd, value=10 @ u_kOhm)

dut.G(name="1", positive="out_vccs", negative=dut.gnd,
      control_positive="input", control_negative=dut.gnd, transconductance=1e-3)
dut.R(name="g_load", positive="out_vccs", negative=dut.gnd, value=10 @ u_kOhm)

dut.F(name="1", positive="out_cccs", negative=dut.gnd,
      vsense="Vsense", current_gain=3.0)
dut.R(name="f_load", positive="out_cccs", negative=dut.gnd, value=10 @ u_kOhm)

dut.H(name="1", positive="out_ccvs", negative=dut.gnd,
      vsense="Vsense", transresistance=2000.0)
dut.R(name="h_load", positive="out_ccvs", negative=dut.gnd, value=10 @ u_kOhm)

# ── Simulate ──

tb = ps.Testbench(dut)
tb.V(name="in", positive="input", negative=dut.gnd, value=1 @ u_V)
tb.with_backend("ngspice")

try:
    op = tb.operating_point()
    i_sense = 1.0 / 1000  # 1mA
    print(f"E (VCVS): {op['out_vcvs']:+.1f} V  (gain=5, expect +5.0)")
    print(f"G (VCCS): {op['out_vccs']:+.1f} V  (gm*Vin*Rload)")
    print(f"F (CCCS): {op['out_cccs']:+.1f} V  (Ai*Isense*Rload)")
    print(f"H (CCVS): {op['out_ccvs']:+.1f} V  (Rm*Isense, expect +2.0)")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
