"""ESPice from Python: the deck a Testbench emits, and the numbers it returns.

The netlist assertions run anywhere. The simulation tests skip when `espice`
is not on PATH. Verilog-A goes to ESPice as source (`.hdl`, compiled by VerA);
no OpenVAF and no OSDI on this path.

Run: maturin develop && pytest tests/test_espice_backend.py -v
"""
import math
import os
import shutil

import pytest

ps = pytest.importorskip("spicerack")
from spicerack.testbenches import (  # noqa: E402
    CornerCase, delay_table, input_capacitance, run_corners)

needs_espice = pytest.mark.skipif(not shutil.which("espice"), reason="espice not on PATH")

VRES = """`include "disciplines.vams"
module vres(a, b);
    inout a, b;
    electrical a, b;
    parameter real r = 1000.0;
    analog I(a, b) <+ V(a, b) / r;
endmodule
"""


def divider(rtop=1e3, rbot=1e3):
    sc = ps.Subcircuit("divider", ["vdd", "out"])
    sc.R(name="top", positive="vdd", negative="out", value=rtop)
    sc.R(name="bot", positive="out", negative=sc.gnd, value=rbot)
    return sc


def bench(dut):
    tb = ps.Testbench(dut)
    tb.with_backend("espice")
    return tb


def rc():
    sc = ps.Subcircuit("rc", ["vin", "out"])
    sc.R(name="1", positive="vin", negative="out", value=1e3)
    sc.C(name="1", positive="out", negative=sc.gnd, value=1e-6)
    return sc


@pytest.fixture
def vres_va(tmp_path):
    path = tmp_path / "vres.va"
    path.write_text(VRES)
    return str(path)


def va_dut(va):
    dut = ps.Subcircuit("vdiv", ["vdd", "out"])
    dut.veriloga(va)
    dut.R(name="top", positive="vdd", negative="out", value=1e3)
    dut.raw_spice("N1 out 0 vres r=3k")
    return dut


# ── The emitted deck ──


class TestNetlist:
    def test_backend_name_is_accepted(self):
        tb = bench(divider())
        tb.V(name="dd", positive="vdd", negative="0", value=1.0)
        tb.add_operating_point()
        assert ".op" in tb.netlist("espice")

    def test_veriloga_becomes_hdl_with_no_openvaf_or_osdi(self, vres_va):
        tb = bench(va_dut(vres_va))
        tb.V(name="dd", positive="vdd", negative="0", value=1.0)
        tb.add_operating_point()
        deck = tb.netlist("espice")
        assert f'.hdl "{os.path.realpath(vres_va)}"' in deck
        assert "pre_osdi" not in deck and ".control" not in deck
        assert not os.path.exists(vres_va[:-3] + ".osdi"), "no OpenVAF run on the ESPice path"

    def test_an_osdi_binary_is_refused(self, tmp_path):
        dut = ps.Subcircuit("o", ["a"])
        dut.osdi(str(tmp_path / "m.osdi"))
        tb = bench(dut)
        tb.add_operating_point()
        with pytest.raises(Exception) as err:
            tb.netlist("espice")
        assert "does not load OSDI" in str(err.value)

    def test_capabilities_are_declared_not_assumed(self):
        tb = bench(divider())
        tb.V(name="dd", positive="vdd", negative="0", value=1.0)
        tb.step("x", 1, 2, 1)
        tb.add_operating_point()
        assert any("step" in i for i in tb.check_backend("espice"))


# ── Real simulations ──


@needs_espice
class TestRun:
    def test_operating_point(self):
        tb = bench(divider())
        tb.V(name="dd", positive="vdd", negative="0", value=3.0)
        assert tb.operating_point()["out"] == pytest.approx(1.5, rel=1e-9)

    def test_dc_sweep(self):
        tb = bench(divider(rbot=3e3))
        tb.V(name="dd", positive="vdd", negative="0", value=0.0)
        r = tb.dc(Vdd=slice(0.0, 2.0, 0.5))
        assert list(r["out"]) == pytest.approx([0.0, 0.375, 0.75, 1.125, 1.5], abs=1e-12)

    def test_ac_corner_of_an_rc(self):
        tb = bench(rc())
        tb.V(name="in", positive="vin", negative="0", value=0.0, ac=1.0)
        ac = tb.ac(variation="dec", number_of_points=200, start_frequency=1.0, stop_frequency=1e5)
        mag = ac.magnitude("out")
        i = min(range(len(mag)), key=lambda k: abs(mag[k] - 1 / math.sqrt(2)))
        assert ac.frequency[i] == pytest.approx(1 / (2 * math.pi * 1e-3), rel=2e-2)

    def test_transient_step_response(self):
        tb = bench(rc())
        tb.PulseVoltageSource(name="in", positive="vin", negative="0", initial_value=0.0,
                              pulsed_value=1.0, pulse_width=1.0, period=0.0,
                              rise_time=1e-9, fall_time=1e-9)
        tr = tb.transient(step_time=1e-5, end_time=5e-3, use_initial_condition=True)
        t, out = tr.time, tr["out"]
        i = min(range(len(t)), key=lambda k: abs(t[k] - 1e-3))
        assert out[i] == pytest.approx(1.0 - math.exp(-1.0), rel=5e-3)

    def test_veriloga_model_runs_through_vera(self, vres_va):
        tb = bench(va_dut(vres_va))
        tb.V(name="dd", positive="vdd", negative="0", value=1.0)
        assert tb.operating_point()["out"] == pytest.approx(0.75, rel=1e-9)

    def test_misspelled_veriloga_parameter_is_an_error(self, vres_va):
        dut = ps.Subcircuit("bad", ["vdd", "out"])
        dut.veriloga(vres_va)
        dut.R(name="top", positive="vdd", negative="out", value=1e3)
        dut.raw_spice("N1 out 0 vres rr=3k")
        tb = bench(dut)
        tb.V(name="dd", positive="vdd", negative="0", value=1.0)
        with pytest.raises(Exception):
            tb.operating_point()

    def test_env_var_selects_the_backend(self, monkeypatch):
        monkeypatch.setenv("SPICERACK_BACKEND", "espice")
        tb = ps.Testbench(divider())
        tb.V(name="dd", positive="vdd", negative="0", value=2.0)
        assert tb.operating_point()["out"] == pytest.approx(1.0, rel=1e-9)
        monkeypatch.setenv("SPICERACK_BACKEND", "no-such-sim")
        with pytest.raises(Exception):
            tb.operating_point()

    def test_a_legacy_circuit_with_osdi_is_refused(self, tmp_path):
        c = ps.Circuit("legacy")
        c.osdi(str(tmp_path / "m.osdi"))
        c.V(name="dd", positive="a", negative=c.gnd, value=1.0)
        c.R(name="1", positive="a", negative=c.gnd, value=1e3)
        with pytest.raises(Exception) as err:
            c.simulator(simulator="espice").operating_point()
        assert "OSDI" in str(err.value)


# ── Recipes, as tests/test_run_corners.py and tests/test_characterize.py run them ──

LIB = """\
.lib lo
.param rbot=1k
.endl lo
.lib hi
.param rbot=3k
.endl hi
"""


@needs_espice
class TestRecipes:
    def test_run_corners_sections_and_temperature(self, tmp_path):
        lib = tmp_path / "proc.lib"
        lib.write_text(LIB)

        def build(corner):
            dut = ps.Subcircuit("div", ["vin", "vout"])
            dut.R(name="top", positive="vin", negative="vout", value=1e3)
            dut.raw_spice("Rbot vout 0 {rbot} tc1=0.01")
            tb = ps.Testbench(dut)
            tb.V(name="dd", positive="vin", negative="0", value=1.0)
            return tb

        corners = [CornerCase(f"{s}_{t}", temperature=t, model_libraries=(ps.ModelLibrary(str(lib), corner=s),))
                   for s in ("lo", "hi") for t in (27, 127)]
        rows = run_corners(build, lambda tb: {"v": tb.operating_point()["vout"]}, corners, backend="espice")
        for row in rows:
            rb = (1e3 if row["corner"].startswith("lo") else 3e3) * (1 + 0.01 * (row["temperature"] - 27))
            assert row["v"] == pytest.approx(rb / (1e3 + rb), rel=1e-6), row

    def test_run_corners_seeds_draw_distinct_reproducible_samples(self):
        def build(corner):
            dut = ps.Subcircuit("mc", ["a"])
            dut.raw_spice("R1 a 0 {agauss(1k, 100, 1)}")
            tb = ps.Testbench(dut)
            tb.I(name="s", positive="0", negative="a", value=1e-3)
            return tb

        measure = lambda tb: {"r": tb.operating_point()["a"] * 1e3}  # noqa: E731
        rows = run_corners(build, measure, seeds=4, backend="espice")
        assert len({round(r["r"], 6) for r in rows}) == 4, rows
        again = run_corners(build, measure, seeds=[3], backend="espice")
        assert again[0]["r"] == rows[2]["r"]

    def test_input_capacitance_of_a_linear_cap(self):
        dut = ps.Subcircuit("cap", ["a"])
        dut.C(name="g", positive="a", negative=dut.gnd, value=1e-12)
        c = input_capacitance(ps, dut, pin="a", vdd=1.8, backend="espice")
        assert c["c_rise_f"] == pytest.approx(1e-12, rel=1e-3)
        assert c["c_fall_f"] == pytest.approx(1e-12, rel=1e-3)

    def test_delay_table_of_an_rc_cell(self):
        # A step into RC: 50 % at tau ln 2, 20 -> 80 % in tau ln 4.
        dut = ps.Subcircuit("rc_cell", ["a", "y"])
        dut.BV(name="drv", positive="yi", negative=dut.gnd, expression="1.0 - V(a)")
        dut.R(name="out", positive="yi", negative="y", value=1e3)
        tab = delay_table(ps, dut, input_pin="a", output_pin="y", vdd=1.0, slews=[1e-12],
                          loads=[1e-12], window=20e-9, backend="espice")
        tau = 1e-9
        assert tab["cell_fall_s"][0][0] == pytest.approx(tau * math.log(2), rel=5e-3)
        assert tab["fall_transition_s"][0][0] == pytest.approx(tau * math.log(4), rel=5e-3)
