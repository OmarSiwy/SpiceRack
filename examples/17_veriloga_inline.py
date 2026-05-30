"""
Example 17: Inline Verilog-A Model

Compiles a parameterized resistor in Verilog-A via veriloga(),
then instantiates it alongside a standard R.

Requires 'openvaf' (or 'openvaf-r') on $PATH, and ngspice with
matching OSDI version support (v0.4 for openvaf-r).
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

VERILOGA_RESISTOR = r"""
`include "disciplines.vams"
module myres(a, b);
    inout a, b;
    electrical a, b;
    parameter real r = 1000.0;
    analog V(a,b) <+ r * I(a,b);
endmodule
"""

# ── DUT ──

dut = ps.Subcircuit("va_demo", ["input"])

try:
    osdi_path = dut.veriloga(VERILOGA_RESISTOR)
    print(f"Compiled Verilog-A to: {osdi_path}")

    dut.R(name="std", positive="input", negative="mid", value=1 @ u_kOhm)
    dut.raw_spice("Nmyres1 mid 0 myres r=1000")

    tb = ps.Testbench(dut)
    tb.V(name="in", positive="input", negative=dut.gnd, value=5 @ u_V)
    tb.with_backend("ngspice")

    op = tb.operating_point()
    print(f"V(mid) = {op['mid']:.4f} V  (expect 2.5 -- two 1k in series)")

except RuntimeError as e:
    msg = str(e)
    if "OSDI" in msg:
        print(f"Skipped (OSDI version mismatch): {msg}")
    elif "openvaf" in msg.lower():
        print(f"Skipped (openvaf not on PATH): {msg}")
    else:
        print(f"Skipped: {msg}")
