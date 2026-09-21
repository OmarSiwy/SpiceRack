"""The metric contract: a bench either computes what it declares, or says it can't."""
import math

import pytest

import spicerack as ps
from spicerack.testbenches import (
    DesignBench,
    amplifier_transimpedance,
    amplifier_voltage_gain,
)
from spicerack.testbenches.analysis import (
    HALF_POWER_DB,
    ac_bandwidth_hz,
    ac_passband_db,
    crossings,
)

FC = 159.1549430918953  # 1 / (2*pi*1k*1u)


def _rc_dut(name="rc"):
    dut = ps.Subcircuit(name, ["vin", "vout"])
    dut.R(name="r1", positive="vin", negative="vout", value=1000.0)
    dut.C(name="c1", positive="vout", negative="0", value=1e-6)
    return dut


# ── primitives ──

def test_crossings_returns_every_crossing_not_just_the_first():
    x = [0.0, 1.0, 2.0, 3.0, 4.0]
    y = [0.0, 2.0, 0.0, 2.0, 0.0]
    assert crossings(x, y, 1.0, rising=True) == [0.5, 2.5]
    assert crossings(x, y, 1.0, rising=False) == [1.5, 3.5]
    assert len(crossings(x, y, 1.0)) == 4


def test_crossings_empty_when_threshold_never_reached():
    assert crossings([0.0, 1.0], [0.0, 0.5], 10.0) == []


def test_half_power_is_not_three_db():
    """A round 3.0 dB puts a single-pole corner 0.23% low. Guard the constant."""
    assert HALF_POWER_DB == pytest.approx(3.0102999566, abs=1e-9)
    assert HALF_POWER_DB != 3.0


def test_bandwidth_converges_to_analytic_corner():
    """Error must shrink with sweep density -- otherwise it is a definition bug."""
    errors = []
    for ppd in (10, 100):
        freq = [10 ** (k / ppd) for k in range(0, 6 * ppd + 1)]
        db = [20 * math.log10(1 / math.sqrt(1 + (f / FC) ** 2)) for f in freq]
        got = crossings(freq, db, max(db) - HALF_POWER_DB, rising=False, log_x=True)[0]
        errors.append(abs(got / FC - 1))
    assert errors[1] < errors[0], f"not converging: {errors}"
    assert errors[1] < 1e-4


# ── contract ──

def test_voltage_gain_bench_computes_what_it_declares():
    bench = amplifier_voltage_gain(ps, _rc_dut(), input_node="vin", output_node="vout")
    assert bench.computed
    result = bench.testbench.ac(
        variation="dec", number_of_points=200, start_frequency=1, stop_frequency=1e6
    )
    metrics = bench.metrics(result)

    assert set(metrics) >= set(bench.measurements)
    assert metrics["gain_midband_db"] == pytest.approx(0.0, abs=0.01)
    assert metrics["bandwidth_hz"] == pytest.approx(FC, rel=1e-3)


def _tia_dut():
    """Transresistance stage: I_in into R||C, buffered to the output.

    Transimpedance = 1 kOhm at DC, rolling off at 1/(2*pi*R*C).
    """
    dut = ps.Subcircuit("tia", ["iin", "vout"])
    dut.R(name="f", positive="iin", negative="0", value=1000.0)
    dut.C(name="f", positive="iin", negative="0", value=1e-6)
    dut.E(name="buf", positive="vout", negative="0",
          control_positive="iin", control_negative="0", voltage_gain=1.0)
    return dut


def test_transimpedance_bench_computes_what_it_declares():
    bench = amplifier_transimpedance(ps, _tia_dut(), input_node="iin",
                                     output_node="vout", ac_current=1e-6)
    assert bench.computed
    result = bench.testbench.ac(
        variation="dec", number_of_points=100, start_frequency=1, stop_frequency=1e6
    )
    metrics = bench.metrics(result)
    assert set(metrics) >= set(bench.measurements)
    # 1 uA into a 1k resistor at DC -> 1 kOhm transimpedance.
    assert metrics["transimpedance_ohm"] == pytest.approx(1000.0, rel=1e-3)


def test_uncomputed_bench_refuses_rather_than_guessing():
    """A bench with no extractor must raise, not return a partial dict."""
    bench = DesignBench(
        name="stub", category="x", testbench=None, intent="",
        measurements=["a", "b"],
    )
    assert not bench.computed
    with pytest.raises(NotImplementedError, match="no extractor is wired"):
        bench.metrics({})


def test_extractor_that_underdelivers_is_caught():
    bench = DesignBench(
        name="stub", category="x", testbench=None, intent="",
        measurements=["a", "b"], extractor=lambda _: {"a": 1.0},
    )
    with pytest.raises(RuntimeError, match=r"declared \['b'\]"):
        bench.metrics({})


# ── every bench computes what it declares, against a closed-form DUT ──

SW_MODEL = ".model swmod SW(ron=100 roff=1e9 vt=0.5)"


def _sw(dut, name, p, n, c):
    dut.S(name=name, positive=p, negative=n, control_positive=c,
          control_negative="0", model="swmod")


def test_every_builtin_bench_is_computed():
    """No bench may advertise a metric with no extractor behind it."""
    import spicerack.testbenches as T

    dut = ps.Subcircuit("d", ["vin", "vout"])
    dut.R(name="r1", positive="vin", negative="vout", value=1e3)
    dut.C(name="c1", positive="vout", negative="0", value=1e-6)
    benches = [
        T.amplifier_voltage_gain(ps, dut),
        T.amplifier_transimpedance(ps, dut),
        T.amplifier_current_gain(ps, dut),
        T.charge_amplifier(ps, dut),
        T.sample_hold(ps, dut),
        T.bandgap_reference(ps, dut),
        T.bandgap_tempco(ps, dut),
        T.pll_lock(ps, dut),
        T.adc_ramp(ps, dut, output_nodes=["d0"]),
        T.dac_static_linearity(ps, dut, code_nodes=["d0", "d1"]),
        T.switch_characterization(ps, dut),
        T.mux_routing(ps, dut, input_nodes=["a", "b"]),
        T.demux_routing(ps, dut, output_nodes=["x", "y"]),
    ]
    uncomputed = [b.name for b in benches if not b.computed]
    assert not uncomputed, f"benches declaring metrics with no extractor: {uncomputed}"
    assert all(b.measurements for b in benches)


def test_switch_recovers_model_resistances():
    dut = ps.Subcircuit("sw", ["vin", "vout", "ctrl"])
    dut.raw_spice(SW_MODEL)
    _sw(dut, "sw", "vin", "vout", "ctrl")
    bench = T_switch = __import__("spicerack.testbenches", fromlist=["x"]).switch_characterization(ps, dut)
    m = bench.metrics(bench.testbench.dc(Vctrl=slice(0.0, 1.0, 0.05)))
    assert m["ron_ohm"] == pytest.approx(100.0, rel=1e-6)
    assert m["roff_ohm"] == pytest.approx(1e9, rel=1e-6)


def test_dac_detects_injected_dnl_and_non_monotonicity():
    import spicerack.testbenches as T

    nodes = ["d0", "d1", "d2", "d3"]
    n = 2 ** len(nodes) - 1
    delta = 0.5
    terms = "+".join(f"{w}*V({x})" for w, x in zip([1, 2, 4, 8 + delta], nodes))
    dut = ps.Subcircuit("dac", nodes + ["vout"])
    dut.BV(name="dac", positive="vout", negative="0", expression=f"({terms})/{n}")
    bench = T.dac_static_linearity(ps, dut, code_nodes=nodes)
    m = bench.metrics(bench.testbench.transient(step_time=2e-8, end_time=16e-6))
    # Endpoint-normalised DNL at the major carry: 14*delta/(15+delta)
    assert m["dnl_max_lsb"] == pytest.approx(14 * delta / (n + delta), rel=1e-4)
    assert m["monotonic"] == 1.0


def test_adc_ramp_detects_missing_codes():
    import spicerack.testbenches as T

    bits, fs = 3, 1.0
    lsb = fs / 2 ** bits
    outs = [f"d{i}" for i in range(bits)]
    dut = ps.Subcircuit("adc", ["vin", "clk"] + outs)
    for i, node in enumerate(outs):
        if i == 1:  # stuck low
            dut.BV(name=f"b{i}", positive=node, negative="0", expression="0")
        else:
            lo, hi = (2 ** i) * lsb, (2 ** (i + 1)) * lsb
            dut.BV(name=f"b{i}", positive=node, negative="0",
                   expression=f"floor(V(vin)/{lo})-2*floor(V(vin)/{hi})")
    bench = T.adc_ramp(ps, dut, output_nodes=outs, input_start=0.0,
                       input_stop=fs - lsb / 2, conversions=64)
    m = bench.metrics(bench.testbench.transient(step_time=2e-8, end_time=64e-6))
    assert m["codes_seen"] == 4.0
    assert m["missing_codes"] == 4.0
    assert m["code_monotonic"] == 0.0


# ── metrics verified against a known NON-ZERO value ──
# A metric exercised only in its degenerate case (0 or nan) is not verified.

def test_phase_margin_two_pole_loop():
    """A single pole always yields ~90 deg; two poles prove the extraction."""
    import math

    import spicerack.testbenches as T

    A, R1, C1, R2, C2 = 100.0, 1e3, 1e-6, 1e3, 1e-7
    f1, f2 = 1 / (2 * math.pi * R1 * C1), 1 / (2 * math.pi * R2 * C2)
    dut = ps.Subcircuit("op2", ["vin", "vout"])
    dut.E(name="g", positive="m1", negative="0", control_positive="vin",
          control_negative="0", voltage_gain=A)
    dut.R(name="a", positive="m1", negative="p1", value=R1)
    dut.C(name="a", positive="p1", negative="0", value=C1)
    dut.E(name="b", positive="m2", negative="0", control_positive="p1",
          control_negative="0", voltage_gain=1.0)
    dut.R(name="c", positive="m2", negative="vout", value=R2)
    dut.C(name="c", positive="vout", negative="0", value=C2)

    bench = T.amplifier_voltage_gain(ps, dut, start_frequency=0.1, stop_frequency=1e8)
    m = bench.metrics(bench.testbench.ac(variation="dec", number_of_points=400,
                                         start_frequency=0.1, stop_frequency=1e8))
    lo, hi = 0.1, 1e8
    for _ in range(200):
        mid = math.sqrt(lo * hi)
        gain = A / math.sqrt((1 + (mid / f1) ** 2) * (1 + (mid / f2) ** 2))
        lo, hi = (mid, hi) if gain > 1 else (lo, mid)
    fu = math.sqrt(lo * hi)
    exact = 180 - math.degrees(math.atan(fu / f1)) - math.degrees(math.atan(fu / f2))
    assert exact < 45.0, "test circuit must not be the degenerate ~90 deg case"
    assert m["phase_margin_deg"] == pytest.approx(exact, abs=0.01)


def test_dac_inl_and_offset_are_independent():
    import spicerack.testbenches as T

    nodes = ["d0", "d1", "d2", "d3"]
    n, delta, off = 15, 0.5, 0.02
    terms = "+".join(f"{w}*V({x})" for w, x in zip([1, 2, 4, 8 + delta], nodes))
    dut = ps.Subcircuit("dac", nodes + ["vout"])
    dut.BV(name="dac", positive="vout", negative="0",
           expression=f"({terms})/{n}+{off}")
    bench = T.dac_static_linearity(ps, dut, code_nodes=nodes)
    m = bench.metrics(bench.testbench.transient(step_time=2e-8, end_time=16e-6))
    # Endpoint-normalised INL peaks at 7*delta/(15+delta); offset is in LSB.
    assert m["inl_max_lsb"] == pytest.approx(7 * delta / (n + delta), rel=1e-4)
    assert m["offset_error_lsb"] == pytest.approx(off * n, rel=1e-4)


def test_pedestal_step_matches_capacitive_divider():
    import spicerack.testbenches as T

    ch, cc = 1e-12, 0.5e-12
    dut = ps.Subcircuit("cd", ["vin", "vhold", "phi"])
    dut.C(name="cpl", positive="phi", negative="vhold", value=cc)
    dut.R(name="bleed", positive="vhold", negative="0", value=1e15)
    bench = T.sample_hold(ps, dut, input_frequency=1e3, hold_capacitance=ch)
    period = 1 / (20 * 1e3)
    m = bench.metrics(bench.testbench.transient(step_time=period / 500, end_time=2e-3))
    assert m["pedestal_step"] == pytest.approx(-cc / (cc + ch), rel=1e-4)


def test_control_settling_tracks_a_known_ramp():
    import spicerack.testbenches as T

    fref, cycles, slope = 10e6, 128, 5e3
    dut = ps.Subcircuit("pll", ["ref", "vco", "vctrl"])
    dut.E(name="p", positive="vco", negative="0", control_positive="ref",
          control_negative="0", voltage_gain=1.0)
    dut.BV(name="ctl", positive="vctrl", negative="0",
           expression=f"0.5+{slope}*time")
    bench = T.pll_lock(ps, dut, reference_frequency=fref, cycles=cycles)
    result = bench.testbench.transient(step_time=1 / fref / 200, end_time=cycles / fref)
    m = bench.metrics(result)
    # Peak-to-peak of a ramp over the second half of the record.
    assert m["control_settling_v"] == pytest.approx(slope * (cycles / fref) / 2, rel=1e-3)
