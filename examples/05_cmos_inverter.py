"""
Example 05: CMOS Inverter

Complementary NMOS/PMOS inverter, VDD=3.3V.
Demonstrates MOSFET model definitions and transient analysis with
pulse input.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V

# ── DUT ──

dut = ps.Subcircuit("cmos_inv", ["vdd", "vin", "vout"])
dut.model("NMOS_3V3", "NMOS", LEVEL=1, VTO=0.7, KP=110e-6, LAMBDA=0.04,
          TOX=9e-9, CGSO=0.06e-9, CGDO=0.06e-9)
dut.model("PMOS_3V3", "PMOS", LEVEL=1, VTO=-0.7, KP=50e-6, LAMBDA=0.05,
          TOX=9e-9, CGSO=0.07e-9, CGDO=0.07e-9)

dut.M(name="p1", drain="vout", gate="vin", source="vdd", bulk="vdd", model="PMOS_3V3")
dut.M(name="n1", drain="vout", gate="vin", source=dut.gnd, bulk=dut.gnd, model="NMOS_3V3")
dut.C(name="load", positive="vout", negative=dut.gnd, value=50e-15)

# ── Transient ──

tb = ps.Testbench(dut)
tb.V(name="dd", positive="vdd", negative=dut.gnd, value=3.3 @ u_V)
tb.PulseVoltageSource(
    name="in", positive="vin", negative=dut.gnd,
    initial_value=0.0, pulsed_value=3.3,
    pulse_width=25e-9, period=50e-9,
    rise_time=0.5e-9, fall_time=0.5e-9,
)
tb.with_backend("ngspice")

try:
    tran = tb.transient(step_time=0.1e-9, end_time=100e-9)
    times = tran.time
    vout = tran["vout"]
    print(f"Simulated {len(times)} points over {times[-1]*1e9:.1f} ns")
    print(f"Vout range: {min(vout):.3f} V to {max(vout):.3f} V")
    print(f"Vin=0 -> Vout=VDD, Vin=VDD -> Vout=0")
except RuntimeError as e:
    print(f"Skipped (ngspice not available): {e}")
