# Reusable Testbenches

DeSpice includes a packaged Python helper library for reusable design-domain
testbench recipes. These helpers build normal `pyspice_rs.Testbench` objects;
they do not introduce a separate simulation path.

```python
import pyspice_rs as ps
from testbenches import amplifier_voltage_gain, sample_hold

dut = ps.Subcircuit("amp", ["vin", "vout"])
dut.R(name="load_path", positive="vout", negative=dut.gnd, value=10e3)

bench = amplifier_voltage_gain(ps, dut, input_node="vin", output_node="vout")
print(bench.netlist("ngspice"))
```

Each returned `DesignBench` carries:

- `testbench`: the generated `pyspice_rs.Testbench`
- `category`: design family such as `amplifier`, `adc`, `pll`, or `bandgap`
- `measurements`: scalar KPIs the bench is intended to extract
- `result_fields`: result axes and vectors consumers should inspect
- `validation`: pass/fail ideas layered above backend results

## Validation Helpers

Use `MetricSpec` and `extract_metrics` to turn operating-point, AC, transient,
raw-data, or plain mapping results into scalar metrics. Then validate those
scalars with `ValidationRule` and `validate_metrics`:

```python
from testbenches import MetricSpec, ValidationRule, extract_metrics, validate_metrics

metrics = extract_metrics(
    tran,
    [
        MetricSpec("vout_final", "vout", "final"),
        MetricSpec("vout_rise", "vout", "crossing_time", threshold=0.9),
    ],
)

report = validate_metrics(
    metrics | {"gain_db": 40.1, "vref": 1.205},
    [
        ValidationRule("gain target", "gain_db", expected=40.0, tolerance=0.5),
        ValidationRule("reference window", "vref", minimum=1.18, maximum=1.23),
    ],
)
assert report.passed
```

## Corners And Monte Carlo

`CornerCase` applies temperature, nominal temperature, model libraries, and
backend-specific parameter lines to fresh testbench instances. `MonteCarloPlan`
adds Xyce sampling/PCE or Spectre Monte Carlo analyses through the normal
`Testbench` API. `evaluate_metric_sets` and `evaluate_corners` summarize
pass-rate/yield and metric statistics across corners or sampled runs.

```python
from testbenches import (
    CornerCase, MonteCarloPlan, ValidationRule,
    corner_netlists, evaluate_metric_sets, monte_carlo_netlist,
)

corners = [
    CornerCase("tt_27", temperature=27, parameters={"vdd_nom": 3.3}),
    CornerCase("ss_hot", temperature=125, parameters={"vdd_nom": 3.0}),
]
netlists = corner_netlists(lambda: bench_factory(), corners, backend="ngspice")

mc = MonteCarloPlan(samples=200, distributions={"Rload": "normal(1000,50)"})
xyce_netlist = monte_carlo_netlist(bench_factory(), mc)

summary = evaluate_metric_sets(
    [{"gain_db": 40.0}, {"gain_db": 39.5}, {"gain_db": 36.0}],
    [ValidationRule("gain", "gain_db", minimum=38.0)],
)
print(summary.pass_rate)
```

For backend output files, use `parse_metric_rows` for text already in memory,
`load_metric_rows` for a known scalar table/log file, or
`load_monte_carlo_metrics` / `evaluate_monte_carlo_file` for a run directory.
The loader accepts common Xyce scalar files such as `.mt0`, `.ms0`, `.ma0`,
`.prn`, `.res`, logs containing repeated `.MEASURE ... = value` lines, and
Spectre-style CSV/TSV/whitespace scalar tables.

```python
from testbenches import evaluate_monte_carlo_file

summary = evaluate_monte_carlo_file(
    "runs/xyce_sampling",
    [ValidationRule("gain", "gain_db", minimum=38.0)],
    backend="xyce",
)
print(summary.total, summary.pass_rate)
```

For direct control, `pyspice_rs.Testbench` also exposes:

- `add_xyce_sampling(samples, distributions)`
- `add_xyce_embedded_sampling(samples, distributions)`
- `add_xyce_pce(samples, distributions, order=2)`
- `add_spectre_sweep(param, start, stop, step, inner_analysis, inner_type)`
- `add_spectre_monte_carlo(iterations, inner_analysis, inner_type, seed=None)`

## Supported Families

The initial library covers:

- Amplifiers: voltage gain, current gain, transimpedance, and charge amplifier
- Converters: DAC static linearity and ADC ramp-code tests
- Switching: switch characterization, mux routing, demux routing, sample/hold
- Timing/RF: PLL lock and spur-oriented transient/Fourier setup
- References: bandgap reference line/temperature-regulation setup

## Standard Contract

Reusable benches should follow the same contract:

- Define explicit stimulus with `V`, `I`, waveform sources, or fixture `R`/`C`
- Save every node or branch current that validation will use
- Include at least one static analysis (`op` or `dc`) when applicable
- Include at least one dynamic analysis (`tran`, `ac`, Fourier, or backend RF)
- Name intended scalar checks in `measurements`
- Keep validation backend-aware: Spectre is best for periodic RF analyses, Xyce
  for sweeps/statistics, and ngspice for baseline transient/DC/AC regression

Voltage-based benches use voltage sources and node assertions. Current-based
benches use current sources and explicit branch-current saves. Charge-based
benches express charge as current into a known capacitor and validate `Q = C*dV`
from transient waveforms.

See `examples/22_design_testbenches.py` for a no-backend-required validation
example that instantiates every recipe and checks generated netlists.
