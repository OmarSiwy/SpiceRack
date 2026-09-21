# SpiceRack reference

Signatures verified against the built module. Element arguments are keyword-only
except where noted.

## Elements

Available on `Circuit` and `Subcircuit`. `Testbench` carries only the rows marked **TB**.

| Method | Emits | Signature |
|---|---|---|
| `V` **TB** | `V` | `(name, positive, negative, value, ac=None, ac_phase=None)` |
| `I` **TB** | `I` | `(name, positive, negative, value, ac=None, ac_phase=None)` |
| `R` **TB** | `R` | `(name, positive, negative, value, raw_spice=None)` |
| `C` **TB** | `C` | `(name, positive, negative, value)` |
| `L` | `L` | `(name, positive, negative, value)` |
| `K` | `K` | `(name, inductor1, inductor2, coupling)` |
| `D` | `D` | `(name, anode, cathode, model)` |
| `Q` / `BJT` | `Q` | `(name, collector, base, emitter, model)` |
| `M` / `MOSFET` | `M` | `(name, drain, gate, source, bulk, model, **kwargs)` — e.g. `L=`, `W=` |
| `J` | `J` | `(name, drain, gate, source, model)` |
| `Z` | `Z` | `(name, drain, gate, source, model)` |
| `E` | `E` | `(name, positive, negative, control_positive, control_negative, voltage_gain)` |
| `G` | `G` | `(name, positive, negative, control_positive, control_negative, transconductance)` |
| `F` | `F` | `(name, positive, negative, vsense, current_gain)` — `vsense` names a V source |
| `H` | `H` | `(name, positive, negative, vsense, transresistance)` |
| `BV` / `BI` | `B` | `(name, positive, negative, expression)` |
| `T` | `T` | `(name, input_positive, input_negative, output_positive, output_negative, Z0, TD)` |
| `S` | `S` | `(name, positive, negative, control_positive, control_negative, model)` |
| `W` | `W` | `(name, positive, negative, vcontrol, model)` |
| `A` | `A` | `(name, connections, model)` — `connections` is a `list[str]` |
| `X` | `X` | `X(name, subcircuit_name, *nodes)` — **fully positional**, including `name` |

Current-controlled sources (`F`, `H`) sense through a named voltage source; add a
0 V source in the branch you want to measure.

Waveform sources, all **TB**:

```python
tb.SinusoidalVoltageSource(name=, positive=, negative=, offset=, amplitude=, frequency=,
                           delay=, damping=, phase=, ac=, ac_phase=)
tb.PulseVoltageSource(name=, positive=, negative=, initial_value=, pulsed_value=,
                      pulse_width=, period=, delay_time=, rise_time=, fall_time=)
tb.PieceWiseLinearVoltageSource(name=, positive=, negative=, values=[(t, v), ...])
```
Current variants replace `Voltage` with `Current`.

**Set `rise_time` and `fall_time` explicitly on a pulse.** Unspecified, ngspice
defaults them to the transient `step_time`, which silently makes edge-rate-dependent
results (charge injection, propagation delay, pedestal) track your timestep.

## Directives

`Circuit` / `Subcircuit` only: `model(name, kind, **params)`, `include(path)`,
`lib(path, section)`, `parameter(name, value)`, `raw_spice(line)`, `veriloga(...)`,
`osdi(...)`, `verilog(...)`. `temp(value)` and `options(**kw)` are `Circuit` only.

`Testbench`: `extra_line(text)`, `options(**kw)`, `temperature`, `nominal_temperature`,
`initial_condition(**nodes)`, `node_set(**nodes)`, `save(*signals)`, `use_pdk(...)`.

## Analyses

Queue on a `Testbench` with `add_*`; run on a `Testbench` or `CircuitSimulator` directly.

| Run method | Queue method | Signature | Result |
|---|---|---|---|
| `operating_point()` | `add_operating_point()` | — | `OperatingPoint` |
| `dc(**sweeps)` | `add_dc(**sweeps)` | `Vin=slice(start, stop, step)` | `DcAnalysis` |
| `ac(...)` | `add_ac(...)` | `(variation, number_of_points, start_frequency, stop_frequency)` | `AcAnalysis` |
| `transient(...)` | `add_transient(...)` | `(step_time, end_time, start_time=, max_time=)` | `TransientAnalysis` |
| `noise(...)` | `add_noise(...)` | `(output_node, ref_node, src, variation, points, start_frequency, stop_frequency)` | `NoiseAnalysis` |
| — | `add_fourier(...)` | `(fundamental_frequency, [vars], num_harmonics=)` | see note |
| `polezero(...)` | — | — | `PoleZeroAnalysis` |
| `transfer_function(...)` | — | — | `TransferFunctionAnalysis` |
| `distortion(...)` | — | — | `DistortionAnalysis` |

`variation` is `"dec"`, `"oct"` or `"lin"`.

`add_fourier` emits ngspice's `.four`, whose output table is not parsed — the
harmonics are unreachable. For THD, compute a Goertzel over the transient
waveform.

Backend-specific, `CircuitSimulator` only: `spectre_sweep`, `spectre_montecarlo`,
`spectre_pac`, `spectre_pnoise`, `spectre_pstb`, `spectre_pxf`, `network_params`.
`Testbench` has the `add_*` builder forms of the sampling and sweep variants.

## Results

Every result type has `result[node]`, `result.measures`.
`DcAnalysis` additionally has `.nodes` and `.branches`.

| Type | Axis | Extra |
|---|---|---|
| `OperatingPoint` | — | scalar per node |
| `DcAnalysis` | `.sweep` | `.nodes`, `.branches` |
| `AcAnalysis` | `.frequency` | `.magnitude(n)`, `.magnitude_db(n)`, `.phase(n)` |
| `TransientAnalysis` | `.time` | |
| `NoiseAnalysis` | `.frequency` | `inoise_spectrum`, `onoise_spectrum` |

`ac[node]` is the real part. Use `.magnitude()` for `|H|`.

## Units

`from spicerack.unit import u_V, u_kOhm, ...` then `10 @ u_V`.

The set is curated, not a full prefix x base cross product — there is no `u_kV`,
`u_pA` or `u_nW`.

| Quantity | Constants |
|---|---|
| Voltage | `u_V u_mV u_uV` |
| Current | `u_A u_mA u_uA u_nA` |
| Resistance | `u_Ohm u_kOhm u_MOhm` and `u_Ω u_kΩ u_MΩ` |
| Capacitance | `u_F u_mF u_uF u_nF u_pF u_fF` |
| Inductance | `u_H u_mH u_uH u_nH` |
| Time | `u_s u_ms u_us u_ns u_ps` |
| Frequency | `u_Hz u_kHz u_MHz u_GHz` |
| Power | `u_W u_mW u_uW` |
| Angle | `u_Degree` |

Micro is ASCII `u_uF`. Names spelled with U+00B5 MICRO SIGN (`u_µF`, `u_µs`, …)
do exist in the module dict and show up in `dir()`, but Python normalises source
identifiers to U+03BC, so neither `import` nor attribute access can reach them.
The `u_Ω` family is registered with U+03A9 and does work.

`UnitValue` exposes `.value`, `.str_spice()`, and `float()`.

## Backend capabilities

Declared in `src/backend/*.rs`; these gate automatic backend selection.

| | ngspice | ltspice | vacask | spectre |
|---|---|---|---|---|
| XSPICE | yes | no | no | no |
| OSDI | yes | no | yes | yes |
| `.measure` | yes | yes | **no** | yes |
| `.step` params | **no** | yes | yes | yes |
| control blocks | yes | no | no | no |
| Laplace sources | yes | yes | no | no |
| Verilog co-sim | yes | no | no | yes |

`ps.lint(netlist, backend=None)` takes the **netlist text**, not a `Circuit` —
pass `str(circuit)` or `tb.netlist(backend)`. It returns
`{"errors": [...], "warnings": [...]}`, each entry a dict with `line`,
`message`, `suggestion` and `backends_affected`:

```python
report = ps.lint(str(circuit))
for issue in report["errors"] + report["warnings"]:
    print(issue["line"], issue["message"], "->", issue["suggestion"])
```

It catches missing models, dangling nodes and backend-incompatible constructs
before a run. Pass a backend name to scope the check to it.
