"""
Example 16: Simulator Configuration

Demonstrates Testbench config: temperature, options, save, measure,
step sweep, initial_condition, node_set.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm, u_uF

# ── DUT ──

dut = ps.Subcircuit("rc_demo", ["input", "output"])
dut.R(name="r1", positive="input", negative="output", value=1 @ u_kOhm)
dut.C(name="c1", positive="output", negative=dut.gnd, value=1 @ u_uF)
dut.R(name="load", positive="output", negative=dut.gnd, value=10 @ u_kOhm)

# ── Testbench with all config knobs ──

tb = ps.Testbench(dut)
tb.V(name="in", positive="input", negative=dut.gnd, value=5 @ u_V)

tb.temperature = 27.0
tb.nominal_temperature = 25.0

tb.options(RELTOL="1e-4", ABSTOL="1e-12", VNTOL="1e-6", GMIN="1e-12")
tb.save("V(output)")
tb.measure("TRAN", "v_peak", "MAX", "V(output)")
tb.step("R1", 500, 2000, 500)
tb.initial_condition(output=0.0)
tb.node_set(output=2.5)

tb.with_backend("ngspice")

print("Testbench configured:")
print(f"  temperature       = 27 C")
print(f"  options           = RELTOL=1e-4, ABSTOL=1e-12, ...")
print(f"  save              = V(output)")
print(f"  measure           = TRAN v_peak MAX V(output)")
print(f"  step              = R1: 500..2000, step 500")
print(f"  initial_condition = V(output)=0")
print(f"  node_set          = V(output)=2.5 (DC guess)")
print(f"\nReady for tb.transient() / tb.operating_point() / tb.ac() / tb.dc()")
