"""
Example 15: Backend Compatibility Check

Builds a circuit with a deliberate missing-model reference and
calls check_backend() to surface issues before simulation.
"""
import pyspice_rs as ps

# ── DUT with intentional issue ──

dut = ps.Subcircuit("lint_demo", ["input"])
dut.R(name="r1", positive="input", negative="orphan", value=1000.0)
dut.D(name="d1", anode="input", cathode=dut.gnd, model="MISSING_MODEL")

# ── Check each backend ──

tb = ps.Testbench(dut)
tb.V(name="in", positive="input", negative=dut.gnd, value=5.0)

for backend in ["ngspice", "vacask"]:
    issues = tb.check_backend(backend)
    print(f"\n{backend}: {len(issues)} issue(s)")
    for issue in issues:
        print(f"  {issue}")

print("\ncheck_backend() catches problems before you run the sim.")
