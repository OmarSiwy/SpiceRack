"""Cell characterization recipes: pin capacitance, NLDM delay/transition tables,
setup/hold search.

Every function builds fresh ``Testbench(dut)`` objects around a ``Subcircuit``
whose ports are the cell pins, runs ngspice transients, and returns plain
numbers in SI units (seconds, farads) with the unit in the key name, ready for
a Liberty writer. ``bias`` holds DC voltages for the pins a recipe does not
drive (supplies, enables, other inputs); ``loads`` holds capacitances to ground.
"""

from __future__ import annotations

from typing import Any, Callable, Mapping, Sequence

from .analysis import _interp_at, crossings


def _bench(ps: Any, dut: Any, bias: Mapping[str, float], loads: Mapping[str, float],
           backend: str) -> Any:
    tb = ps.Testbench(dut)
    tb.with_backend(backend)
    for pin, volts in bias.items():
        tb.V(name=f"bias_{pin}", positive=pin, negative="0", value=float(volts))
    for pin, farads in loads.items():
        tb.C(name=f"load_{pin}", positive=pin, negative="0", value=float(farads))
    return tb


def _run(tb: Any, end: float, edge: float) -> Any:
    """Transient resolving an ``edge``-long transition with ~20 points."""
    return tb.transient(step_time=edge / 50.0, end_time=end, max_time=edge / 20.0)


def _integral(t: Sequence[float], y: Sequence[float], t0: float, t1: float) -> float:
    """Trapezoidal integral of y dt over [t0, t1] (sample-aligned)."""
    total = 0.0
    for k in range(1, len(t)):
        a, b = max(t[k - 1], t0), min(t[k], t1)
        if b <= a:
            continue
        # Interpolate the segment ends so a window edge inside a step is exact.
        span = t[k] - t[k - 1]
        ya = y[k - 1] + (y[k] - y[k - 1]) * (a - t[k - 1]) / span
        yb = y[k - 1] + (y[k] - y[k - 1]) * (b - t[k - 1]) / span
        total += (b - a) * (ya + yb) / 2.0
    return total


def input_capacitance(
    ps: Any,
    dut: Any,
    *,
    pin: str,
    vdd: float,
    slew: float = 100e-12,
    settle: float | None = None,
    bias: Mapping[str, float] | None = None,
    loads: Mapping[str, float] | None = None,
    backend: str = "ngspice",
) -> dict[str, float]:
    """Input pin capacitance by the charge method, C = -integral(i dt) / dV.

    ``pin`` is ramped 0 -> vdd and back over ``slew`` (0-100 %) from an ideal
    source; the charge that source delivers over each ramp plus ``settle``
    (default: one ``slew``) divided by the swing is the effective capacitance
    for that edge, Miller and gate-charge nonlinearity included.

    Returns ``{"c_rise_f": ..., "c_fall_f": ...}`` in farads. DC leakage into
    the pin over the window is counted as charge, so a leaky pin reads high.
    """
    settle = slew if settle is None else settle
    hold = 4.0 * (slew + settle)
    t_r = hold                       # rising ramp start
    t_f = t_r + slew + hold          # falling ramp start
    end = t_f + slew + hold
    src = f"char_{pin}"
    tb = _bench(ps, dut, bias or {}, loads or {}, backend)
    tb.PieceWiseLinearVoltageSource(
        name=src, positive=pin, negative="0",
        values=[(0.0, 0.0), (t_r, 0.0), (t_r + slew, vdd),
                (t_f, vdd), (t_f + slew, 0.0), (end, 0.0)])
    tb.save(f"V({pin})", f"I(V{src})")
    tran = _run(tb, end, slew)
    t = [float(x) for x in tran.time]
    i = [float(x) for x in tran[f"i(v{src})"]]
    # SPICE source current flows + -> -, so charging the pin is negative.
    q_rise = -_integral(t, i, t_r, t_r + slew + settle)
    q_fall = -_integral(t, i, t_f, t_f + slew + settle)
    return {"c_rise_f": q_rise / vdd, "c_fall_f": q_fall / -vdd}


def _first_crossing(t: Sequence[float], v: Sequence[float], level: float, after: float,
                    rising: bool) -> float:
    hits = [x for x in crossings(t, v, level, rising=rising) if x > after]
    if not hits:
        raise ValueError(f"no {'rising' if rising else 'falling'} crossing of {level:g} V "
                         f"after {after:g} s; widen window=")
    return hits[0]


def delay_table(
    ps: Any,
    dut: Any,
    *,
    input_pin: str,
    output_pin: str,
    vdd: float,
    slews: Sequence[float],
    loads: Sequence[float],
    window: float = 10e-9,
    slew_lower: float = 0.2,
    slew_upper: float = 0.8,
    delay_threshold: float = 0.5,
    bias: Mapping[str, float] | None = None,
    backend: str = "ngspice",
) -> dict[str, Any]:
    """NLDM-style delay and output-transition tables over input slew x load.

    For every ``(slew, load)`` one transient drives ``input_pin`` up then down;
    ``slew`` is the ``slew_lower``-``slew_upper`` transition time, so the ramp
    itself lasts ``slew / (slew_upper - slew_lower)``. ``load`` is a capacitor
    on ``output_pin``. Each input edge must finish its output edge within
    ``window`` seconds.

    Tables are keyed by **output** edge, as Liberty's ``cell_rise`` etc. are,
    so inverting and non-inverting cells need no flag. Returns::

        {"index_1_s": slews, "index_2_f": loads,
         "cell_rise_s": [[...]], "cell_fall_s": [[...]],
         "rise_transition_s": [[...]], "fall_transition_s": [[...]]}

    with ``table[i][j]`` at ``slews[i]``, ``loads[j]``. Delay is input
    ``delay_threshold`` crossing to output ``delay_threshold`` crossing.
    """
    names = ("cell_rise_s", "cell_fall_s", "rise_transition_s", "fall_transition_s")
    out: dict[str, Any] = {"index_1_s": list(slews), "index_2_f": list(loads)}
    out.update({n: [[0.0] * len(loads) for _ in slews] for n in names})
    src = f"char_{input_pin}"
    for i, slew in enumerate(slews):
        ramp = slew / (slew_upper - slew_lower)
        t_r = window
        t_f = t_r + ramp + window
        end = t_f + ramp + window
        for j, load in enumerate(loads):
            tb = _bench(ps, dut, bias or {}, {output_pin: load}, backend)
            tb.PieceWiseLinearVoltageSource(
                name=src, positive=input_pin, negative="0",
                values=[(0.0, 0.0), (t_r, 0.0), (t_r + ramp, vdd),
                        (t_f, vdd), (t_f + ramp, 0.0), (end, 0.0)])
            tb.save(f"V({input_pin})", f"V({output_pin})")
            # PWL corners are breakpoints, so a fast input ramp needs no tiny
            # global step; the output edge is resolved to window/1000.
            tran = _run(tb, end, max(min(ramp, window), window / 50.0))
            t = [float(x) for x in tran.time]
            vi = [float(x) for x in tran[input_pin]]
            vo = [float(x) for x in tran[output_pin]]
            for t_in, in_rising in ((t_r, True), (t_f, False)):
                # The output edge's direction is whatever the cell did across
                # this input edge, so inverting cells need no flag.
                rising = _interp_at(t, vo, t_in + ramp + window * 0.9) > _interp_at(t, vo, t_in)
                t50_in = _first_crossing(t, vi, delay_threshold * vdd, t_in, in_rising)
                t50_out = _first_crossing(t, vo, delay_threshold * vdd, t_in, rising)
                t_lo = _first_crossing(t, vo, slew_lower * vdd, t_in, rising)
                t_hi = _first_crossing(t, vo, slew_upper * vdd, t_in, rising)
                edge = "rise" if rising else "fall"
                out[f"cell_{edge}_s"][i][j] = t50_out - t50_in
                out[f"{edge}_transition_s"][i][j] = abs(t_hi - t_lo)
    return out


def bisect_boundary(passes: Callable[[float], bool], lo: float, hi: float,
                    tol: float) -> float:
    """Smallest x in [lo, hi] where ``passes(x)`` is true, to within ``tol``.

    Requires ``passes(hi)`` true and ``passes(lo)`` false (checked, since a
    search between two passing points returns ``lo`` and looks like a result),
    and a single pass/fail boundary in between.
    """
    if not passes(hi):
        raise ValueError(f"predicate fails at the upper bound {hi:g}; widen the search")
    if passes(lo):
        raise ValueError(f"predicate already passes at the lower bound {lo:g}; widen the search")
    while hi - lo > tol:
        mid = (lo + hi) / 2.0
        if passes(mid):
            hi = mid
        else:
            lo = mid
    return hi


def _constraint(ps: Any, dut: Any, kind: str, data_pin: str, clock_pin: str, vdd: float,
                passes: Callable[[Any], bool], search: tuple[float, float], tol: float,
                data_edge: str, clock_edge: str, slew: float, clock_time: float,
                window: float, bias: Mapping[str, float] | None,
                loads: Mapping[str, float] | None, backend: str) -> float:
    if data_edge not in ("rise", "fall") or clock_edge not in ("rise", "fall"):
        raise ValueError("data_edge and clock_edge are 'rise' or 'fall'")
    d0, d1 = (0.0, vdd) if data_edge == "rise" else (vdd, 0.0)
    c0, c1 = (0.0, vdd) if clock_edge == "rise" else (vdd, 0.0)
    end = clock_time + window
    reach = max(abs(search[0]), abs(search[1])) + slew
    if reach >= clock_time or reach >= window:
        raise ValueError("clock_time and window must each exceed the widest skew plus slew")

    def run(skew: float) -> bool:
        # setup: data settles to its new value `skew` before the clock edge.
        # hold:  data leaves its old value `skew` after the clock edge.
        t_d = clock_time - skew if kind == "setup" else clock_time + skew
        tb = _bench(ps, dut, bias or {}, loads or {}, backend)
        # Both edges are referenced to their 50 % points.
        tb.PieceWiseLinearVoltageSource(
            name=f"char_{clock_pin}", positive=clock_pin, negative="0",
            values=[(0.0, c0), (clock_time - slew / 2, c0), (clock_time + slew / 2, c1), (end, c1)])
        tb.PieceWiseLinearVoltageSource(
            name=f"char_{data_pin}", positive=data_pin, negative="0",
            values=[(0.0, d0), (t_d - slew / 2, d0), (t_d + slew / 2, d1), (end, d1)])
        return bool(passes(_run(tb, end, slew)))

    return bisect_boundary(run, search[0], search[1], tol)


def setup_time(
    ps: Any, dut: Any, *, data_pin: str, clock_pin: str, vdd: float,
    passes: Callable[[Any], bool], search: tuple[float, float] = (-1e-9, 1e-9),
    tol: float = 1e-12, data_edge: str = "rise", clock_edge: str = "rise",
    slew: float = 50e-12, clock_time: float = 5e-9, window: float = 5e-9,
    bias: Mapping[str, float] | None = None, loads: Mapping[str, float] | None = None,
    backend: str = "ngspice",
) -> dict[str, float]:
    """Setup time by bisection on ``passes(tran) -> bool``.

    Clock makes its ``clock_edge`` at ``clock_time`` (50 % point); data makes
    its ``data_edge`` at ``clock_time - skew``. ``passes`` judges one transient
    (e.g. "q settled to the new data by the end"); the result is the smallest
    passing skew in ``search``, within ``tol``. Returns ``{"setup_s": ...}``.
    """
    return {"setup_s": _constraint(ps, dut, "setup", data_pin, clock_pin, vdd, passes,
                                   search, tol, data_edge, clock_edge, slew, clock_time,
                                   window, bias, loads, backend)}


def hold_time(
    ps: Any, dut: Any, *, data_pin: str, clock_pin: str, vdd: float,
    passes: Callable[[Any], bool], search: tuple[float, float] = (-1e-9, 1e-9),
    tol: float = 1e-12, data_edge: str = "fall", clock_edge: str = "rise",
    slew: float = 50e-12, clock_time: float = 5e-9, window: float = 5e-9,
    bias: Mapping[str, float] | None = None, loads: Mapping[str, float] | None = None,
    backend: str = "ngspice",
) -> dict[str, float]:
    """Hold time by bisection on ``passes(tran) -> bool``.

    Data sits at its pre-``data_edge`` value through the clock edge and makes
    ``data_edge`` at ``clock_time + skew``; ``passes`` should be true when the
    old value was kept. The result is the smallest passing skew (negative means
    data may change before the clock). Returns ``{"hold_s": ...}``.
    """
    return {"hold_s": _constraint(ps, dut, "hold", data_pin, clock_pin, vdd, passes,
                                  search, tol, data_edge, clock_edge, slew, clock_time,
                                  window, bias, loads, backend)}
