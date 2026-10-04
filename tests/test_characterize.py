"""Characterization recipes against circuits with closed-form answers.

Run with: maturin develop && pytest tests/test_characterize.py
"""
import math
import shutil

import pytest

ps = pytest.importorskip("spicerack")
from spicerack.testbenches import (  # noqa: E402
    bisect_boundary, delay_table, hold_time, input_capacitance, setup_time)
from spicerack.testbenches.analysis import _interp_at  # noqa: E402

needs_ngspice = pytest.mark.skipif(not shutil.which("ngspice"), reason="ngspice not installed")

TAU = 1e-9  # 1 kOhm x 1 pF


def ramp_rc_half(T, tau=TAU):
    """Time after a 0-100 % ramp of length T starts until an RC output crosses 50 %.

    Past the ramp, v/V = 1 - (tau/T)(e^{T/tau} - 1) e^{-t/tau}."""
    t = -tau * math.log(0.5 * T / (tau * (math.exp(T / tau) - 1.0)))
    assert t >= T
    return t


def rc_cell(invert):
    dut = ps.Subcircuit("rc_cell", ["a", "y"])
    expr = "1.0 - V(a)" if invert else "V(a)"
    dut.BV(name="drv", positive="yi", negative=dut.gnd, expression=expr)
    dut.R(name="out", positive="yi", negative="y", value=1e3)
    return dut


class TestBisect:
    def test_finds_the_boundary_within_tol(self):
        x = bisect_boundary(lambda s: s >= 0.123456, 0.0, 1.0, 1e-6)
        assert 0.123456 <= x <= 0.123456 + 1e-6

    def test_refuses_a_bracket_without_a_boundary(self):
        with pytest.raises(ValueError, match="upper bound"):
            bisect_boundary(lambda s: False, 0.0, 1.0, 1e-3)
        with pytest.raises(ValueError, match="lower bound"):
            bisect_boundary(lambda s: True, 0.0, 1.0, 1e-3)


@needs_ngspice
class TestInputCapacitance:
    def test_linear_cap_is_recovered_on_both_edges(self):
        dut = ps.Subcircuit("cap", ["a"])
        dut.C(name="g", positive="a", negative=dut.gnd, value=1e-12)
        c = input_capacitance(ps, dut, pin="a", vdd=1.8)
        assert c["c_rise_f"] == pytest.approx(1e-12, rel=1e-3)
        assert c["c_fall_f"] == pytest.approx(1e-12, rel=1e-3)

    def test_charge_behind_a_resistor_needs_the_settle_window(self):
        dut = ps.Subcircuit("rcap", ["a"])
        dut.C(name="g", positive="a", negative=dut.gnd, value=1e-12)
        dut.R(name="s", positive="a", negative="b", value=1e3)
        dut.C(name="b", positive="b", negative=dut.gnd, value=0.5e-12)  # tau = 0.5 ns
        short = input_capacitance(ps, dut, pin="a", vdd=1.0)
        full = input_capacitance(ps, dut, pin="a", vdd=1.0, settle=10e-9)
        assert short["c_rise_f"] < 1.2e-12
        assert full["c_rise_f"] == pytest.approx(1.5e-12, rel=1e-3)
        assert full["c_fall_f"] == pytest.approx(1.5e-12, rel=1e-3)


@needs_ngspice
class TestDelayTable:
    SLEWS = [0.6e-9]                 # 20-80 % => a 1 ns 0-100 % ramp
    LOADS = [1e-12, 2e-12, 4e-12]     # tau >= ramp, so the 50 % crossing follows it

    def table(self, invert):
        return delay_table(ps, rc_cell(invert), input_pin="a", output_pin="y", vdd=1.0,
                           slews=self.SLEWS, loads=self.LOADS, window=40e-9)

    @pytest.mark.parametrize("invert", [True, False])
    def test_delay_matches_a_ramp_driven_rc(self, invert):
        tab = self.table(invert)
        assert tab["index_1_s"] == self.SLEWS and tab["index_2_f"] == self.LOADS
        T = 1e-9
        for j, load in enumerate(self.LOADS):
            tau = 1e3 * load
            want = ramp_rc_half(T, tau) - T / 2
            assert tab["cell_rise_s"][0][j] == pytest.approx(want, rel=5e-3)
            assert tab["cell_fall_s"][0][j] == pytest.approx(want, rel=5e-3)

    def test_transition_matches_rc_at_a_fast_edge(self):
        tab = delay_table(ps, rc_cell(True), input_pin="a", output_pin="y", vdd=1.0,
                          slews=[1e-12], loads=[1e-12], window=20e-9)
        # A step into RC: 20 -> 80 % takes tau ln 4, 50 % at tau ln 2.
        assert tab["cell_fall_s"][0][0] == pytest.approx(TAU * math.log(2), rel=5e-3)
        assert tab["fall_transition_s"][0][0] == pytest.approx(TAU * math.log(4), rel=5e-3)
        assert tab["rise_transition_s"][0][0] == pytest.approx(TAU * math.log(4), rel=5e-3)

    def test_output_that_never_switches_is_an_error(self):
        dut = ps.Subcircuit("dead", ["a", "y"])
        dut.R(name="y", positive="y", negative=dut.gnd, value=1e3)
        dut.R(name="a", positive="a", negative=dut.gnd, value=1e3)
        with pytest.raises(ValueError, match="widen window"):
            delay_table(ps, dut, input_pin="a", output_pin="y", vdd=1.0,
                        slews=[1e-10], loads=[1e-15], window=1e-9)


@needs_ngspice
class TestSetupHold:
    """A data -> RC -> q 'capture' node: q must cross 50 % before the clock."""

    SLEW = 50e-12
    T_CLK = 3e-9

    def dut(self):
        dut = ps.Subcircuit("rc_latch", ["d", "clk", "q"])
        dut.R(name="d", positive="d", negative="q", value=1e3)
        dut.C(name="q", positive="q", negative=dut.gnd, value=1e-12)
        dut.R(name="clk", positive="clk", negative=dut.gnd, value=1e6)
        return dut

    def test_setup_is_the_rc_mid_crossing(self):
        captured = lambda tran: _interp_at(tran.time, tran["q"], self.T_CLK) > 0.5
        got = setup_time(ps, self.dut(), data_pin="d", clock_pin="clk", vdd=1.0,
                         passes=captured, search=(0.0, 2e-9), tol=1e-13,
                         slew=self.SLEW, clock_time=self.T_CLK, window=2.5e-9)
        want = ramp_rc_half(self.SLEW) - self.SLEW / 2
        assert got["setup_s"] == pytest.approx(want, rel=5e-3)

    def test_hold_can_be_negative(self):
        # Pass when q is still above 50 % 0.5 ns after the clock.
        kept = lambda tran: _interp_at(tran.time, tran["q"], self.T_CLK + 0.5e-9) > 0.5
        got = hold_time(ps, self.dut(), data_pin="d", clock_pin="clk", vdd=1.0,
                        passes=kept, search=(-2e-9, 0.5e-9), tol=1e-13,
                        slew=self.SLEW, clock_time=self.T_CLK, window=2.5e-9)
        want = 0.5e-9 - (ramp_rc_half(self.SLEW) - self.SLEW / 2)
        assert got["hold_s"] == pytest.approx(want, rel=1e-2)
        assert got["hold_s"] < 0

    def test_skew_reaching_past_the_deck_is_refused(self):
        with pytest.raises(ValueError, match="widest skew"):
            setup_time(ps, self.dut(), data_pin="d", clock_pin="clk", vdd=1.0,
                       passes=lambda tran: True, search=(0.0, 5e-9), clock_time=3e-9)
