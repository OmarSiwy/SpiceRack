---
name: spicerack
description: Build SPICE circuits, netlists and testbenches with SpiceRack (`import spicerack`). Use when writing or debugging circuit-simulation code that imports spicerack, when picking a backend (ngspice/Xyce/LTspice/Spectre/vacask), when extracting a metric such as gain, bandwidth or phase margin from a simulation result, or when reaching for the built-in analog testbench recipes.
---

SpiceRack builds a **deck** (SPICE netlist text), hands it to a simulator, and parses the result back into Python lists.

## Three objects

| Object | Holds | Build it with |
|---|---|---|
| `Circuit` | A standalone circuit. Every element type. | `ps.Circuit("name")` |
| `Subcircuit` | A reusable DUT with ports. Every element type. | `ps.Subcircuit("name", ["a","b"])` |
| `Testbench` | Stimulus wrapped around a DUT, plus queued analyses. | `ps.Testbench(dut)` |

`Circuit.simulator()` returns a `CircuitSimulator`; `Testbench` runs analyses directly.

**`Testbench` carries only stimulus: `V I R C` and the Sinusoidal/Pulse/PieceWiseLinear source builders.** Everything else — `L K M D Q J Z E F G H T S W A X BV BI`, plus `model include lib parameter raw_spice gnd veriloga osdi verilog` — lives on `Circuit` and `Subcircuit`. Put devices in the DUT, stimulus in the bench. Reaching for `tb.model(...)` or `tb.gnd` is the most common `AttributeError` here.

## Build and simulate

1. **Put the device under test in a `Subcircuit`** with explicit ports. Completion: `dut.instance(...)` or `ps.Testbench(dut)` accepts it, and `tb.netlist("ngspice")` shows every device you added.
2. **Wrap it in a `Testbench` and add stimulus.** AC analysis needs `ac=` on a source: `tb.V(name="in", positive="vin", negative="0", value=0.0, ac=1.0)`. A source without `ac=` contributes nothing to an AC sweep, and the sweep returns all zeros.
3. **Queue analyses** with `add_operating_point()`, `add_dc()`, `add_ac()`, `add_transient()`, `add_noise()`. Completion: `tb.netlist(backend)` contains the matching dot-card.
4. **Save what you will read**: `tb.save("V(vout)")`. Completion: `.save` appears in the deck.
5. **Run** — `tb.ac(...)`, `tb.transient(...)`, `tb.operating_point()`. Completion: the result object indexes without raising, and `len(result["vout"])` matches the expected point count.

Read the deck with `tb.netlist(backend)` before running. It is plain text, and it is the ground truth for what the simulator will do.

## Traps

Each of these returns a plausible wrong number rather than an error.

**`ac[node]` is the real part, not the magnitude.** For a Bode plot use `ac.magnitude(node)`, `ac.magnitude_db(node)` and `ac.phase(node)` (degrees). At 100 Hz on a 159 Hz RC, `ac["vout"]` reads 0.7170 while `|H|` is 0.8467. `.magnitude()` exists only on `AcAnalysis`.

**ngspice ignores `.step`.** The codegen emits it commented out (`* .step param temp ...`), so a swept bench runs once and silently reports one operating point. Sweep by rebuilding the testbench in a Python loop, one run per point. Xyce and LTspice support `.step` natively.

**Use ASCII micro units: `u_uF`, `u_uH`, `u_uA`, `u_uV`, `u_uW`, `u_us`.** Python normalises identifiers, so a `u_µF` spelled with U+00B5 cannot be imported. The `u_Ω` family does work.

**`.nodes` and `.branches` exist only on `DcAnalysis`.** `.measures` is on every result type. Elsewhere, index the result: `res["vout"]`.

**Results are `list[float]`, not numpy arrays.** Slice and iterate them; `.mean()` and stride-slicing raise.

**`.measure` reaches stdout only for `ac`, `dc` and `tran`.** That is ngspice's own limit (it refuses `.measure` under `-b -r rawfile`, which the backend works around by moving those three analyses into a `.control` block). A `.measure` attached to a noise or pole-zero run returns nothing.

## Metrics

`tb.measure("ac", "gain", "find", "vdb(vout)", "at=1k")` is a raw passthrough to the SPICE `.measure` card, so the full grammar is available — `TRIG/TARG`, `WHEN`, `FIND…AT`, `AVG`, `RMS`, `INTEG`, `DERIV`, `MAX/MIN/PP`. Results land in `result.measures`.

For extraction in Python, `spicerack.testbenches.analysis` provides `crossings`, `ac_gain_db`, `ac_passband_db`, `ac_bandwidth_hz`, `ac_unity_gain_hz`, `ac_phase_margin_deg`, and `extract_metrics` with `MetricSpec`.

Bandwidth defaults to the half-power point, `HALF_POWER_DB` = 3.0103 dB. A round 3.0 dB puts a single-pole corner 0.23 % low — a systematic error that does not shrink with sweep density.

## Backends

`ngspice` is the default and the only one assumed present. Declared features gate backend selection, so a bench that asks for `.measure` support will not route to vacask. Check with `tb.check_backend(name)` and `ps.CircuitSimulator.available_backends()`.

→ Element signatures per class, analysis signatures, the unit list, and the backend capability matrix: [`REFERENCE.md`](REFERENCE.md)

→ The 13 built-in benches, the declared-vs-computed metric contract, and how to write a new bench: [`TESTBENCHES.md`](TESTBENCHES.md)
