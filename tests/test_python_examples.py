"""
Tests that all Python examples execute without errors.

Each example generates a netlist and prints it. We import and run them
to verify no crashes in the circuit builder or unit system.

Run: maturin develop && pytest tests/test_python_examples.py -v
"""
import pytest
import subprocess
import sys
from pathlib import Path

EXAMPLES_DIR = Path(__file__).parent.parent / "examples"


def _skip_if_not_built():
    try:
        import pyspice_rs
    except ImportError:
        pytest.skip("pyspice_rs not built")


def run_example(filename):
    """Run an example script as subprocess, return (returncode, stdout, stderr)."""
    _skip_if_not_built()
    script = EXAMPLES_DIR / filename
    if not script.exists():
        pytest.skip(f"{filename} not found")
    result = subprocess.run(
        [sys.executable, str(script)],
        capture_output=True, text=True, timeout=30,
    )
    return result


class TestExamples:
    def test_01_voltage_divider(self):
        r = run_example("01_voltage_divider.py")
        assert r.returncode == 0, r.stderr
        assert "Voltage Divider" in r.stdout

    def test_02_rc_lowpass(self):
        r = run_example("02_rc_lowpass.py")
        assert r.returncode == 0, r.stderr
        assert "RC Low-Pass Filter" in r.stdout

    def test_03_rlc_bandpass(self):
        r = run_example("03_rlc_bandpass.py")
        assert r.returncode == 0, r.stderr
        assert "L1" in r.stdout

    def test_04_bjt_amplifier(self):
        r = run_example("04_bjt_amplifier.py")
        assert r.returncode == 0, r.stderr
        assert "Q1" in r.stdout

    def test_05_cmos_inverter(self):
        r = run_example("05_cmos_inverter.py")
        assert r.returncode == 0, r.stderr
        assert "Mp1" in r.stdout
        assert "Mn1" in r.stdout

    def test_06_opamp_inverting(self):
        r = run_example("06_opamp_inverting.py")
        assert r.returncode == 0, r.stderr
        assert "Bopamp" in r.stdout

    def test_07_diode_rectifier(self):
        r = run_example("07_diode_rectifier.py")
        assert r.returncode == 0, r.stderr
        assert "D1" in r.stdout
        assert "D4" in r.stdout

    def test_08_differential_pair(self):
        r = run_example("08_differential_pair.py")
        assert r.returncode == 0, r.stderr
        assert "M1" in r.stdout
        assert "M2" in r.stdout

    def test_09_controlled_sources(self):
        r = run_example("09_controlled_sources.py")
        assert r.returncode == 0, r.stderr
        assert "E1" in r.stdout
        assert "G1" in r.stdout
        assert "F1" in r.stdout
        assert "H1" in r.stdout

    def test_10_subcircuit(self):
        r = run_example("10_subcircuit.py")
        assert r.returncode == 0, r.stderr
        assert "Xinv1" in r.stdout
        assert "subcircuit" in r.stdout.lower()

    def test_11_jfet_amplifier(self):
        r = run_example("11_jfet_amplifier.py")
        assert r.returncode == 0, r.stderr
        assert "J1" in r.stdout

    def test_12_transmission_line(self):
        r = run_example("12_transmission_line.py")
        assert r.returncode == 0, r.stderr
        assert "Tline" in r.stdout
        assert "Z0=50" in r.stdout

    def test_13_switches(self):
        r = run_example("13_switches.py")
        assert r.returncode == 0, r.stderr
        assert "S1" in r.stdout or "W1" in r.stdout

    def test_14_pwl_dac(self):
        r = run_example("14_pwl_dac.py")
        assert r.returncode == 0, r.stderr
        assert "PWL(" in r.stdout

    def test_15_linting(self):
        r = run_example("15_linting.py")
        assert r.returncode == 0, r.stderr

    def test_16_simulator_config(self):
        r = run_example("16_simulator_config.py")
        assert r.returncode == 0, r.stderr
        assert "Simulator configured" in r.stdout

    def test_17_veriloga_inline(self):
        r = run_example("17_veriloga_inline.py")
        assert r.returncode == 0, r.stderr

    def test_18_verilog_digital(self):
        r = run_example("18_verilog_digital.py")
        assert r.returncode == 0, r.stderr

    def test_19_backend_hotswap(self):
        r = run_example("19_backend_hotswap.py")
        assert r.returncode == 0, r.stderr
        assert "Backend: ngspice" in r.stdout

    def test_20_pdk_hotswap(self):
        r = run_example("20_pdk_hotswap.py")
        assert r.returncode == 0, r.stderr
        assert "PDK: sky130" in r.stdout

    def test_21_combined_hotswap(self):
        r = run_example("21_combined_hotswap.py")
        assert r.returncode == 0, r.stderr
        assert "Combined Backend" in r.stdout
