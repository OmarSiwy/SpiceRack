# SpiceRack

Build SPICE circuits in Python, generate netlists, run them on whichever simulator you have installed, and read the results back as plain lists of floats.

The core is a Rust crate (`spicerack`). Python bindings ship in the same package, imported as `spicerack`. The API is close enough to PySpice that most PySpice code ports with an import change, but the circuit model underneath is backend neutral: one description compiles to ngspice, Xyce, LTspice, Spectre, or VACASK dialects.

## Install

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install maturin pytest
maturin develop
```

Or with Nix: `nix develop`.

You also need at least one simulator to actually run an analysis. Start with `ngspice` unless you need something specific. Other tools are optional and only matter for particular features:

| Tool | Needed for |
| --- | --- |
| `ngspice` | Default backend, and the only one with XSPICE, control blocks, and Verilog co-simulation |
| `xyce`, `ltspice`, `spectre`, `vacask` | Alternative backends |
| `openvaf` | Compiling Verilog-A to OSDI |
| `iverilog` | Digital Verilog co-simulation |
| `yosys` | Synthesizing Verilog to gate level (`verilog(mode="synthesize")`) |
| `ciel` plus `PDK_ROOT` | Installing open PDKs (sky130, gf180mcu) |

## Two ways to build a circuit

The flat style builds one circuit and hands it to a simulator:

```python
import spicerack as ps
from spicerack.unit import u_V, u_kOhm

circuit = ps.Circuit("voltage_divider")
circuit.V(name="in", positive="vin", negative=circuit.gnd, value=10 @ u_V)
circuit.R(name="top", positive="vin", negative="vout", value=2 @ u_kOhm)
circuit.R(name="bot", positive="vout", negative=circuit.gnd, value=1 @ u_kOhm)

print(circuit)                 # the generated netlist

sim = circuit.simulator(simulator="ngspice")
print(sim.operating_point()["vout"])
```

The testbench style separates the design under test from the stimulus around it, which is what you want as soon as you run the same block under more than one set of conditions:

```python
dut = ps.Subcircuit("divider", ["vin", "vout"])
dut.R(name="top", positive="vin", negative="vout", value=2 @ u_kOhm)
dut.R(name="bot", positive="vout", negative=dut.gnd, value=1 @ u_kOhm)

tb = ps.Testbench(dut)
tb.V(name="supply", positive="vin", negative=dut.gnd, value=10 @ u_V)
tb.with_backend("ngspice")
op = tb.operating_point()
```

Subcircuits nest. Register a definition with `tb.add_subcircuit(inverter)` or `circuit.subcircuit(inverter)`, then instantiate it with `.X("inv1", "inverter", "vdd", "a", "mid")`.

Ground is `dut.gnd` or `circuit.gnd`, which is node `"0"`. Element and analysis arguments are keyword-only, except for `X`, `model`, `include`, `lib`, `parameter`, `temp`, and `raw_spice`, which take positional arguments.

## Units

Units live in `spicerack.unit` and attach to a number with the `@` operator:

```python
from spicerack.unit import u_V, u_kOhm, u_pF, u_uH

r = 10 @ u_kOhm
r.value            # 10000.0, always SI base units
r.str_spice()      # "10k", the netlist form
float(r)           # 10000.0
```

There are voltage, current, resistance, capacitance, inductance, frequency, time, and power families, plus `u_Degree`. Unicode aliases (`u_Ω`, `u_kΩ`, `u_MΩ`) exist for PySpice compatibility. Anywhere a `UnitValue` is accepted, a plain float works too and is read as SI, so `value=10000.0` also means 10 kΩ.

## Elements

Element methods mirror the SPICE element letters and exist on both `Circuit` and `Subcircuit`, with the source methods also on `Testbench`.

Passives are `R`, `C`, `L`, and `K` for mutual inductance. Independent sources are `V` and `I`, each taking an optional `ac=` magnitude for small-signal analyses. For time-varying stimulus there are `SinusoidalVoltageSource`, `PulseVoltageSource`, and `PieceWiseLinearVoltageSource`, along with sinusoidal and pulse current variants.

```python
tb.PulseVoltageSource(name="clk", positive="clk", negative=dut.gnd,
    initial_value=0.0, pulsed_value=3.3, pulse_width=25e-9, period=50e-9,
    rise_time=0.5e-9, fall_time=0.5e-9)
```

Behavioral sources `BV` and `BI` take a SPICE B-source expression:

```python
dut.BV(name="opamp", positive="out", negative=dut.gnd,
       expression="1e6 * (V(inp) - V(inn))")
```

Controlled sources cover all four combinations: `E` (VCVS), `G` (VCCS), `F` (CCCS), and `H` (CCVS). The current-controlled pair senses through a named voltage source, so put a 0 V source in the branch you want to measure.

Semiconductors are `D`, `Q` (aliased `BJT`), `M` (aliased `MOSFET`), `J` for JFETs, and `Z` for MESFETs. Switches are `S` (voltage controlled) and `W` (current controlled). `T` is a lossless transmission line taking `Z0` and `TD`. `A` instantiates an XSPICE code model, which is how the analog and digital bridges get wired up.

Circuit-level directives round it out:

```python
dut.model("2N2222", "NPN", IS=14.34e-15, BF=255.9, VAF=74.03)
dut.include("models.lib")
dut.lib("sky130.lib.spice", "tt")
dut.parameter("rload", "10k")
dut.raw_spice("R99 a b 1k")           # anything the builder does not cover
circuit.options(scale="1e-6")         # Circuit only
circuit.temp(85.0)                    # Circuit only
```

## Analyses

The same analysis methods exist on `Testbench` and on the `CircuitSimulator` returned by `circuit.simulator()`.

```python
op   = tb.operating_point()
dc   = tb.dc(Vin=slice(0, 5, 0.1))
ac   = tb.ac(variation="dec", number_of_points=100,
             start_frequency=1.0, stop_frequency=1e6)
tran = tb.transient(step_time=10e-6, end_time=5e-3)
```

Beyond those there is `noise`, `transfer_function` (aliased `tf`), `dc_sensitivity`, `ac_sensitivity`, `polezero`, and `distortion`. The RF and periodic steady-state group covers `pss`, `s_param`, `harmonic_balance`, `stability`, and `transient_noise`.

Some analyses only exist on one simulator, so they are exposed under a backend prefix. Spectre contributes `spectre_sweep`, `spectre_montecarlo`, `spectre_pac`, `spectre_pnoise`, `spectre_pxf`, and `spectre_pstb`. Xyce contributes `xyce_sampling`, `xyce_embedded_sampling`, `xyce_pce`, and `xyce_fft`.

```python
sim = circuit.simulator(simulator="xyce")

res = sim.xyce_sampling(100, [("R1", "normal(1000,50)"),
                              ("R2", "uniform(900,1100)")])

fft = sim.xyce_fft("V(vout)", np=1024, start=0.0, stop=1e-3, window="HANN")
fft.enob, fft.sfdr_db, fft.snr_db, fft.thd_db
```

`Testbench` has builder equivalents (`add_xyce_sampling`, `add_xyce_pce`, `add_spectre_sweep`, `add_spectre_monte_carlo`, and so on) that write the directive into the netlist instead of running it, which pairs with `tb.netlist("xyce")` when you want the text rather than the result.

## Reading results

Every result object uses the same access pattern, so you do not have to remember which analysis returns what shape:

```python
res["vout"]        # list[float], or a single float for operating_point
res.vout           # attribute access works too
res.measures       # .measure results by name
```

Each analysis exposes its own axis: `dc.sweep`, `ac.frequency`, `tran.time`. `DcAnalysis` additionally lists `nodes` and `branches`. Branch currents are not saved by default, so ask for them with `tb.save("I(Vsupply)")`, or set `sim.save_currents = True` on a `CircuitSimulator` to save all of them.

## Simulator configuration

```python
tb.temperature = 27.0
tb.nominal_temperature = 25.0
tb.options(RELTOL="1e-4", ABSTOL="1e-12", VNTOL="1e-6", GMIN="1e-12")

tb.save("V(output)")
tb.measure("TRAN", "v_peak", "MAX", "V(output)")   # lands in res.measures

tb.initial_condition(output=0.0)   # .ic
tb.node_set(output=2.5)            # .nodeset, helps DC convergence

tb.step("R1", 500, 2000, 500)
tb.step_sweep("R1", 100, 10000, 10, "dec")         # lin, oct, or dec
```

## Backends

```python
ps.CircuitSimulator.available_backends()   # what is actually installed
tb.with_backend("vacask")
sim = circuit.simulator(simulator="xyce")
```

Without an explicit choice, SpiceRack picks a backend by looking at what the circuit uses and what the analysis needs. A circuit with XSPICE elements or a control block narrows to ngspice; one with OSDI models can go to ngspice, VACASK, or Spectre.

| Feature | ngspice | xyce | ltspice | vacask | spectre |
| --- | --- | --- | --- | --- | --- |
| XSPICE (A-elements) | Yes | No | No | No | No |
| OSDI (Verilog-A) | Yes | No | No | Yes | Yes |
| `.measure` | Yes | Yes | Yes | No | No |
| `.step` parameters | No | Yes | Yes | No | Yes |
| Control blocks | Yes | No | No | No | No |
| Laplace sources | Yes | No | Yes | No | No |
| Verilog co-simulation | Yes | No | No | No | Yes |

Because the DUT is backend neutral, running the same design on several simulators is a loop:

```python
for backend in ["ngspice", "vacask"]:
    tb = ps.Testbench(dut)
    tb.V(name="supply", positive="vin", negative=dut.gnd, value=10 @ u_V)
    tb.with_backend(backend)
    print(backend, tb.operating_point()["vout"])
```

## PDKs and model libraries

```python
lib = ps.ModelLibrary(
    f"{os.environ['PDK_ROOT']}/sky130A/libs.tech/ngspice/sky130.lib.spice",
    corner="tt")

dut = ps.Subcircuit("cs_amp", ["vdd", "vin", "vout"])
dut.M(name="1", drain="vout", gate="vin", source=dut.gnd, bulk=dut.gnd,
      model="sky130_fd_pr__nfet_01v8", L=0.5, W=5)

tb = ps.Testbench(dut)
tb.use_pdk(lib)
```

`ModelLibrary(path, corner=None, setup_includes=None, **backend_paths)` takes per-backend model paths as keyword arguments, so a single library object can serve several simulators. Swapping PDKs means swapping the library and the model names while the topology stays put. Examples 20 and 21 do exactly that with sky130 and gf180mcu.

## Linting

Checks run against the target simulator before you spend time on a failed run:

```python
for issue in tb.check_backend("ngspice"):
    print(issue)

report = ps.lint(netlist_text, backend="ngspice")
report["errors"]     # [{"line": int, "message": str}, ...]
report["warnings"]   # same, plus "suggestion" and "backends_affected"
```

It catches missing models, dangling nodes, and constructs the chosen backend will not accept. Warnings name the other backends affected, which is useful when you are deciding where a design can run.

## Verilog-A

Requires `openvaf` on `$PATH` and a backend with OSDI support (ngspice, VACASK, or Spectre). The OSDI version has to match what your ngspice build expects.

```python
osdi_path = dut.veriloga(VERILOGA_SOURCE)     # source text or a .va path
dut.raw_spice("Nmyres1 mid 0 myres r=1000")   # instantiate the compiled model

dut.osdi("model.osdi")                        # or load a prebuilt binary
path = ps.compile_veriloga("model.va")        # or compile without attaching
```

## Digital Verilog co-simulation

Requires ngspice built with XSPICE, plus `iverilog` on `$PATH` (and `yosys` if you synthesize). At the low level you declare bridge models and place them as `A` elements:

```python
dut.model("adc_bridge_model", "adc_bridge", in_low=0.8, in_high=2.0)
dut.model("dac_bridge_model", "dac_bridge", out_low=0.0, out_high=3.3)

dut.A(name="adc1", connections=["[clk rst]", "[dclk drst]"],
      model="adc_bridge_model")
```

`circuit.verilog(source=..., mode="simulate" | "synthesize", instance_name=..., connections={...}, pdk=..., liberty=..., spice_models=...)` wires a Verilog module in directly and handles the bridging for you. In `simulate` mode it compiles with iverilog and drives ngspice's `d_cosim` XSPICE model. In `synthesize` mode it runs Yosys against a Liberty file and emits gate-level subcircuit calls into the netlist instead. Example 18 builds the bridges by hand rather than calling `verilog()`, so treat the high-level path as less exercised than the rest of the API.

## Design testbench recipes

`spicerack.testbenches` holds ready-made benches for amplifier voltage gain, ADC ramp, DAC linearity, PLL lock, and bandgap reference, plus the machinery for checking their output across corners:

```python
from spicerack.testbenches import (
    CornerCase, MetricSpec, ValidationRule,
    amplifier_voltage_gain, bandgap_reference,
    corner_netlists, extract_metrics, validate_metrics)

bench = amplifier_voltage_gain(ps, dut, load_resistance=10e3)
netlist = bench.netlist("ngspice")      # generate without simulating

corners = [CornerCase("tt_27", temperature=27, parameters={"vdd_nom": 3.3})]
cornered = corner_netlists(lambda: bandgap_reference(...), corners)

metrics = extract_metrics(waveforms, [MetricSpec("vout_final", "vout", "final")])
report = validate_metrics(metrics, [
    ValidationRule("output window", "vout_final", minimum=1.18, maximum=1.23),
])
```

`MonteCarloPlan` describes a statistical run the same way `CornerCase` describes a corner: a backend, a sample count, per-parameter distributions, and a mode of either `sampling` or `pce`. Apply it with `plan.apply_to(bench)` or get the text straight from `monte_carlo_netlist(bench, plan)`. One thing to know: `bench.validation` is a `list[str]` of plain-English intent, not executable rules. You write the `ValidationRule` objects yourself.

## Output file parsing

Reading results back is native Rust rather than a shell-out, which is most of why the round trip is quick. SpiceRack parses ngspice and Xyce `.raw` files in both ASCII and binary form, including the column-major FastAccess layout, LTspice raw files with their UTF-16-LE headers, and Cadence PSF binary files from Spectre. Simulator `.meas` output is parsed into `res.measures`, and node naming is normalized across backends so `res["vout"]` means the same thing no matter who ran the simulation.

## Circuit IR

Circuits lower to a backend-neutral intermediate representation before any netlist text is generated, and the code generators for SPICE3, Spectre, and VACASK dialects all read from it. The IR is documented as JSON Schema in [schema/circuit-ir.schema.json](schema/circuit-ir.schema.json), so other tools can produce circuits for SpiceRack without going through the Python API.

## Using the Rust crate directly

The Python bindings are a default feature. Turn them off to use the crate on its own:

```toml
[dependencies]
spicerack = { version = "0.1", default-features = false }
```

That gives you the circuit builder, the IR and code generators, backend drivers, and the raw and PSF parsers, with no Python or PyO3 in the build.

## Examples

[examples/](examples/) has 22 runnable scripts ordered roughly by difficulty, starting at a voltage divider and ending with the design testbench library. Along the way: RC and RLC filters, BJT and JFET amplifiers, a CMOS inverter, an op-amp, a rectifier, a differential pair, controlled sources, subcircuits, transmission lines, switches, a PWL DAC, linting, simulator configuration, inline Verilog-A, digital Verilog, and hot-swapping backends and PDKs.

## Docs

- [User guide](docs/guide.html)
- [API reference](docs/reference.html): every class, element method, analysis, and result type
- [Examples index](docs/examples.html)

## Tests

```bash
cargo test
maturin develop
python3 -m pytest -v
```

## License

MIT or Apache-2.0, at your option.
