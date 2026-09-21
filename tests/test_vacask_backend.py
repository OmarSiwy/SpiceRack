"""VACASK from Python: the deck a Testbench emits, and the numbers it returns.

The netlist assertions run anywhere. The simulation tests skip when `vacask` is
not on PATH, so CI without the binary still passes.

Run: maturin develop && pytest tests/test_vacask_backend.py -v
"""
import math
import shutil

import pytest


def ps():
    try:
        import spicerack
        return spicerack
    except ImportError:
        pytest.skip("spicerack not built")


def needs_vacask():
    if shutil.which("vacask") is None:
        pytest.skip("vacask not on PATH")


def divider(mod, rtop=1e3, rbot=1e3):
    """A 1k/1k divider as a subcircuit with `vdd` and `out` ports."""
    sc = mod.Subcircuit("divider", ["vdd", "out"])
    sc.R(name="top", positive="vdd", negative="out", value=rtop)
    sc.R(name="bot", positive="out", negative=sc.gnd, value=rbot)
    return sc


def bench(mod, dut):
    tb = mod.Testbench(dut)
    tb.with_backend("vacask")
    return tb


# ── The emitted deck ──


class TestNetlist:
    def test_emits_vacask_not_spice_or_spectre(self):
        mod = ps()
        tb = bench(mod, divider(mod))
        tb.V(name="dd", positive="vdd", negative="0", value=3.0)
        tb.add_operating_point()
        deck = tb.netlist("vacask")

        # Native scaffolding VACASK needs and no SPICE dialect has.
        assert "ground 0" in deck
        assert 'load "spice/resistor.osdi"' in deck
        assert "model resistor sp_resistor" in deck
        assert "control" in deck and "endc" in deck
        assert "analysis op1 op" in deck

        # And no leftovers from the dialects it is not.
        assert ".op" not in deck
        assert ".end" not in deck
        assert "simulator lang" not in deck

    def test_transient_always_bounds_the_step(self):
        # VACASK's `step` is only a STARTING step; `maxstep` is the bound.
        mod = ps()
        tb = bench(mod, divider(mod))
        tb.V(name="dd", positive="vdd", negative="0", value=1.0)
        tb.add_transient(step_time=1e-6, end_time=1e-3)
        assert "maxstep=" in tb.netlist("vacask")

    def test_measures_refuse_because_vacask_has_no_measure_statement(self):
        mod = ps()
        tb = bench(mod, divider(mod))
        tb.V(name="dd", positive="vdd", negative="0", value=1.0)
        tb.add_operating_point()
        tb.measure("op", "vout", "FIND", "v(out)")
        with pytest.raises(Exception) as err:
            tb.netlist("vacask")
        assert "measure" in str(err.value).lower()

    def test_dc_becomes_a_sweep_around_an_operating_point(self):
        # VACASK has no `.dc`. `points` counts INTERVALS, so 0..10 by 0.5 is
        # 20, and the endpoint is recomputed so the grid matches SPICE's.
        #
        # Only the deck is checked here: `backend::mod::create_backend_by_name`
        # still lists "dc" as incompatible with vacask, so the run path is
        # blocked upstream of this backend. tests/test_vacask_runs.rs drives
        # the same deck through the binary and checks the 21 numbers.
        mod = ps()
        tb = bench(mod, divider(mod))
        tb.V(name="dd", positive="vdd", negative="0", value=0.0)
        tb.add_dc(Vdd=slice(0.0, 10.0, 0.5))
        deck = tb.netlist("vacask")
        assert 'sweep vsweep instance="vdd" parameter="dc"' in deck
        assert "points=20" in deck
        assert "analysis dc1 op" in deck

    def test_capability_table_reports_step_params_as_supported(self):
        # A control-block `var` is visible to the netlist body and sweepable,
        # so `.step param` has a faithful VACASK spelling.
        mod = ps()
        tb = bench(mod, divider(mod))
        tb.V(name="dd", positive="vdd", negative="0", value=1.0)
        tb.step("rtop", 1e3, 3e3, 1e3)
        tb.add_operating_point()
        assert tb.check_backend("vacask") == []


# ── Real simulations ──


class TestRun:
    def test_operating_point(self):
        needs_vacask()
        mod = ps()
        tb = bench(mod, divider(mod))
        tb.V(name="dd", positive="vdd", negative="0", value=3.0)
        op = tb.operating_point()
        assert op["out"] == pytest.approx(1.5, rel=1e-9)

    def test_ac_corner_of_an_rc(self):
        needs_vacask()
        mod = ps()
        sc = mod.Subcircuit("rc", ["vin", "out"])
        sc.R(name="1", positive="vin", negative="out", value=1e3)
        sc.C(name="1", positive="out", negative=sc.gnd, value=1e-6)
        tb = bench(mod, sc)
        tb.V(name="in", positive="vin", negative="0", value=0.0, ac=1.0)
        ac = tb.ac(variation="dec", number_of_points=200, start_frequency=1.0, stop_frequency=1e5)

        freq = ac.frequency
        mag = ac.magnitude("out")
        target = 1.0 / math.sqrt(2.0)
        i = min(range(len(mag)), key=lambda k: abs(mag[k] - target))
        f3db = 1.0 / (2.0 * math.pi * 1e3 * 1e-6)
        assert freq[i] == pytest.approx(f3db, rel=2e-2)

    def test_transient_step_response(self):
        needs_vacask()
        mod = ps()
        sc = mod.Subcircuit("rc", ["vin", "out"])
        sc.R(name="1", positive="vin", negative="out", value=1e3)
        sc.C(name="1", positive="out", negative=sc.gnd, value=1e-6)
        tb = bench(mod, sc)
        tb.PulseVoltageSource(
            name="in", positive="vin", negative="0",
            initial_value=0.0, pulsed_value=1.0,
            pulse_width=1.0, period=0.0, rise_time=1e-9, fall_time=1e-9,
        )
        tr = tb.transient(step_time=1e-5, end_time=5e-3, use_initial_condition=True)
        t = tr.time
        out = tr["out"]
        i = min(range(len(t)), key=lambda k: abs(t[k] - 1e-3))
        assert out[i] == pytest.approx(1.0 - math.exp(-1.0), rel=5e-3)

    def test_a_spice_string_deck_is_refused_rather_than_mistranslated(self):
        needs_vacask()
        mod = ps()
        c = mod.Circuit("spice_string")
        c.V(name="dd", positive="vdd", negative=c.gnd, value=3.0)
        c.R(name="1", positive="vdd", negative="out", value=1e3)
        c.R(name="2", positive="out", negative=c.gnd, value=1e3)
        sim = c.simulator(simulator="vacask")
        with pytest.raises(Exception) as err:
            sim.operating_point()
        assert "netlist language" in str(err.value)
