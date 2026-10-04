# Built-in testbenches

`from spicerack.testbenches import ...` — recipes that build ordinary
`Testbench` objects. They are recipes, not a second framework: anything a
recipe does, you can do by hand.

Each returns a `DesignBench`:

```python
bench = amplifier_voltage_gain(ps, dut, output_node="vout")
print(bench.netlist("ngspice"))         # the deck
result  = bench.testbench.ac(variation="dec", number_of_points=200,
                             start_frequency=1, stop_frequency=1e6)
metrics = bench.metrics(result)          # {'gain_midband_db': ..., 'bandwidth_hz': ...}
```

## The metric contract

`DesignBench.measurements` lists the metric names the bench claims. `bench.metrics(result)`
either returns all of them or raises:

- `bench.computed` is `False` → `metrics()` raises `NotImplementedError`. The names in
  `measurements` are documented intent that nothing computes yet.
- An extractor that returns fewer keys than declared raises `RuntimeError`.

A metric dict missing a key it advertised is worse than an error, because callers index it.
When you wire a new extractor, the contract test in `tests/test_bench_metrics.py` is what
holds it honest.

## Catalog

Every bench computes its declared metrics; `tests/test_bench_metrics.py` fails if
any of them stops doing so.

| Bench | Category | Analysis | Metrics |
|---|---|---|---|
| `amplifier_voltage_gain` | amplifier | op + ac | `gain_midband_db`, `bandwidth_hz`, `phase_margin_deg` |
| `amplifier_transimpedance` | amplifier | op + ac | `transimpedance_ohm`, `tia_bandwidth_hz` |
| `amplifier_current_gain` | amplifier | op + ac | `current_gain_midband`, `current_gain_bandwidth_hz` |
| `charge_amplifier` | amplifier | tran | `q_injected`, `delta_vout`, `droop_rate` |
| `sample_hold` | sample_hold | tran | `acquisition_error`, `hold_droop`, `pedestal_step` |
| `dac_static_linearity` | dac | tran | `offset_error_lsb`, `gain_error`, `dnl_max_lsb`, `inl_max_lsb`, `monotonic` |
| `adc_ramp` | adc | tran | `codes_seen`, `missing_codes`, `transition_count`, `code_monotonic` |
| `switch_characterization` | switch | dc | `ron_ohm`, `roff_ohm`, `off_isolation_db` |
| `mux_routing` | mux | tran | `selected_gain_min/max`, `channel_gain_mismatch`, `all_channels_routed` |
| `demux_routing` | demux | tran | `selected_gain_min`, `inactive_output_max`, `off_isolation_db` |
| `pll_lock` | pll | tran | `output_frequency_hz`, `frequency_error_ratio`, `control_settling_v`, `control_final_v` |
| `bandgap_reference` | bandgap | dc | `vref`, `line_regulation_ppm_v` |
| `bandgap_tempco` | bandgap | dc(temp) | `vref_27c`, `tempco_box_ppm_c`, `tempco_endpoint_ppm_c` |

`phase_margin_deg` is `nan` when the response never crosses 0 dB, and is a stability
margin only on an open-loop response.

`ac_bandwidth_hz` raises when the response never falls below its passband within the
swept range. That is a bench-configuration signal — widen the sweep — not a value to
paper over with a default.

### What these benches deliberately do not measure

Each is unreachable with the analyses available, and a bench that reported a number
anyway would be reporting its own setup:

- **Charge injection** (`switch_characterization`) — a behavioural ngspice `S`/`W`
  element carries no channel charge and would report exactly zero, passing every time.
  Needs a transistor-level switch.
- **Aperture delay** (`sample_hold`) — inseparable from the pedestal in a single
  record; needs two runs with opposite input slopes.
- **Reference spur, phase noise, jitter** (`pll_lock`) — ngspice injects no device
  noise into a transient, and the `.four` table is not parsed.
- **Startup margin** (`bandgap_reference`) — a DC sweep is a continuation and can
  march along the degenerate zero-current branch; only a transient from a genuinely
  off state proves startup.
- **Latency** (`adc_ramp`) — needs a DUT with a known pipeline depth to align against.

## Validation and corners

```python
from spicerack.testbenches import (
    ValidationRule, validate_metrics, MetricSpec, extract_metrics,
    CornerCase, corner_netlists, MonteCarloPlan, monte_carlo_netlist,
)

report = validate_metrics(metrics, [
    ValidationRule("gain target", "gain_midband_db", expected=40.0, tolerance=0.5),
    ValidationRule("bandwidth floor", "bandwidth_hz", minimum=1e6),
])
assert report.passed
```

`DesignBench.validation` is a `list[str]` of English intent, **not** `ValidationRule`
objects — build the rules yourself as above.

`extract_metrics(result, specs)` reduces waveforms with `MetricSpec(name, source, reducer)`,
reducers: `first last min max mean abs_max peak_to_peak at crossing_time`.

`corner_netlists(bench_factory, [CornerCase(...)])` returns `{corner_name: netlist}`.
`monte_carlo_netlist` is Spectre-only — real signatures and the ngspice route are in [`PDK_AND_FLOW.md`](PDK_AND_FLOW.md).

## Corner / Monte Carlo runner

```python
from spicerack.testbenches import pdk_corners, run_corners

def build(corner):                      # fresh bench per run; reads corner.vdd itself
    tb = ps.Testbench(dut)
    tb.V(name="dd", positive="vdd", negative="0", value=corner.vdd or 1.8)
    return tb

def measure(tb):                        # runs the analysis, returns {metric: value}
    return {"vout_v": tb.operating_point()["vout"]}

corners = pdk_corners(LIB, ["tt", "ss", "ff"], temperatures=[-40, 27, 125], vdds=[1.62, 1.98])
rows = run_corners(build, measure, corners)                  # one row per corner
rows = run_corners(build, measure, pdk_corners(LIB, ["tt_mm"]), seeds=100)   # mismatch MC
```

* `run_corners(build, measure, corners=(), *, seeds=None, backend="ngspice") -> list[dict]`.
  Each row is `{"corner", "seed", "temperature", "vdd", **metrics}`; a run that raises gets
  `"error": "<Type>: <message>"` instead of metrics, so filter on it.
* After `build(corner)` it applies the corner's temperature, model libraries and
  `.param` values, and `.options seed=<n>` per seed. `seeds=N` means `1..N`.
* A seed changes nothing unless the models are statistical (a PDK mismatch section or
  `agauss`). ngspice draws are reproducible per seed.
* `pdk_corners(library, sections, temperatures=(27,), vdds=(None,))` crosses them into
  `CornerCase`s named `ss_125C_1.62V`. `CornerCase.vdd` is read by your builder; nothing
  else applies it, because only the builder knows which source is the supply.
* Runs are sequential. A sky130 run spends ~7 s loading the model library, so
  45 corners x 100 seeds is hours.

## Cell characterization (for Liberty)

`from spicerack.testbenches import input_capacitance, delay_table, setup_time, hold_time, bisect_boundary`

The DUT is a `Subcircuit` whose ports are the cell pins. `bias={pin: volts}` drives the
pins a recipe does not (supplies, other inputs); `loads={pin: farads}` adds caps to
ground. Everything returns SI values with the unit in the key.

| Call | Returns |
|---|---|
| `input_capacitance(ps, dut, *, pin, vdd, slew=100e-12, settle=None, bias=, loads=)` | `{"c_rise_f", "c_fall_f"}` |
| `delay_table(ps, dut, *, input_pin, output_pin, vdd, slews, loads, window=10e-9, slew_lower=0.2, slew_upper=0.8, delay_threshold=0.5, bias=)` | `{"index_1_s", "index_2_f", "cell_rise_s", "cell_fall_s", "rise_transition_s", "fall_transition_s"}` |
| `setup_time(ps, dut, *, data_pin, clock_pin, vdd, passes, search=(-1e-9, 1e-9), tol=1e-12, data_edge="rise", clock_edge="rise", slew=50e-12, clock_time=5e-9, window=5e-9, bias=, loads=)` | `{"setup_s"}` |
| `hold_time(...)` (same arguments, `data_edge="fall"`) | `{"hold_s"}` |
| `bisect_boundary(passes, lo, hi, tol)` | smallest passing `x` |

* **Pin cap** is the charge method: `C = -∫i dt / ΔV` over each ramp plus `settle`
  (default one `slew`). Charge behind a series resistance arrives late; widen `settle`
  if the answer moves with it. DC leakage counts as charge.
* **NLDM tables** are `table[i][j]` at `slews[i]`, `loads[j]`, keyed by **output** edge
  (Liberty's convention), so inverting cells need no flag. `slew` is the
  `slew_lower`–`slew_upper` time; the ramp itself lasts `slew / (upper - lower)`.
  `window` must cover the slowest output edge, or it raises `widen window=`.
* **Setup/hold** bisect the data-to-clock skew on `passes(tran) -> bool`, which you
  write (e.g. "q equals the new data at the end"). Setup: data makes `data_edge` at
  `clock_time - skew`. Hold: data leaves its old value at `clock_time + skew` (negative
  hold is fine). Edges are referenced at 50 %. `passes` must be true at `search[1]` and
  false at `search[0]`, or it raises rather than return a bracket end.

## Writing a new bench

**The load is the specification.** One netlist cannot measure several specs, because each
spec mandates a different termination — charge injection wants a 1 nF hold cap and no
resistive load; off-isolation wants 50 Ω ∥ 5 pF; timing wants 100 Ω ∥ 35 pF; on-resistance
wants a forced current and no load at all. Benches that try to serve several specs at once
end up declaring metrics that have no observable in the deck. Prefer several small
single-purpose benches; each is smaller than the composite would have to be, and each has
one unambiguous extraction.

Two concrete failures of this kind in the current catalog, worth reading before you add one:
`switch_characterization` drives its input from an ideal 0 Ω source into a capacitor-only
load, so no DC current flows and on-resistance is unobservable for any value; `mux_routing`
drives every input from an ideal source, so a momentary two-channel overlap produces no
measurable current anywhere.

The shape:

```python
def my_bench(ps, dut, *, output_node="vout") -> DesignBench:
    tb = ps.Testbench(dut)
    tb.V(name="in", positive="vin", negative="0", value=0.0, ac=1.0)
    tb.save(f"V({output_node})")
    tb.add_ac(variation="dec", number_of_points=100,
              start_frequency=1.0, stop_frequency=1e9)

    def extract(ac):
        from .analysis import ac_bandwidth_hz, ac_passband_db
        return {
            "gain_db": ac_passband_db(ac, output_node),
            "bw_hz": ac_bandwidth_hz(ac, output_node),
        }

    return DesignBench(
        name="my_bench", category="amplifier", testbench=tb,
        intent="What this bench is for.",
        measurements=["gain_db", "bw_hz"],       # must match extract()'s keys
        result_fields=["ac.frequency"],
        validation=["gain in budget"],
        extractor=extract,
    )
```

Validate a new extractor against a circuit with a closed-form answer — an RC low-pass
gives an exact corner at `1/(2*pi*R*C)` — and check that the error *shrinks* as you raise
sweep density. An error that stays put under refinement is a definition bug, not a
numerical one.
