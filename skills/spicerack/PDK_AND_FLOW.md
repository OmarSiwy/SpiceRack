# PDK, corners, Verilog-A, external netlists

Verified by running against the built module (`src/python.rs`, `src/codegen/*`,
`python/spicerack/testbenches/`).

## PDK model libraries

```python
import os, spicerack as ps
lib = ps.ModelLibrary(f"{os.environ['PDK_ROOT']}/sky130A/libs.tech/ngspice/sky130.lib.spice",
                      corner="tt")
tb = ps.Testbench(dut); tb.use_pdk(lib)
```

* `ModelLibrary(path, corner=None, setup_includes=None, **backend_paths)` —
  `backend_paths` maps a backend to its own file, e.g. `spectre="/pdk/x.scs"`.
* `corner` is passed through verbatim as the lib section. No tt/ss/ff table exists; the
  names are the PDK's own and vary by PDK version — `grep '^\.lib' <file>`. sky130A
  `sky130.lib.spice`: `tt sf ff ss fs ll hh hl lh`, `*_mm`, `mc`. gf180mcuD
  `sm141064.ngspice`: `typical ff ss fs sf statistical`.
* ngspice/ltspice emit `.lib <path> <corner>` (`.include` with no corner);
  spectre/vacask emit `include "<path>" section=<corner>`.
* `use_pdk` **appends** — each call adds another library.
* `Testbench` has no `lib`/`include`; put them on the DUT. A `.lib`, `.include` or
  `osdi` on a child subcircuit (added with `add_subcircuit`) is hoisted to the deck's
  top level, once.
* **Nothing reads PDK specs** (VDD, Lmin, Vth, device params). A spec reader is plain
  Python you write against `$PDK_ROOT/$PDK`.

## PDK devices with parameters

sky130 and gf180 FETs and passives are subcircuits in the ngspice libs, so they are `X`
cards with parameters — `M()` emits an `M` card those PDKs cannot resolve:

```python
dut.X("MN1", "sky130_fd_pr__nfet_01v8", "d", "g", "s", "b", W=1, L=0.15, nf=2, m=2, mult=2)
```

Parameter names and units differ per PDK — sky130 FETs take `W`/`L` in plain µm (its
`all.spice` sets `.option scale=1.0u`); gf180 FET wrappers take metre `w=`/`l=`, its MIM
caps `c_width`/`c_length`, its resistors `r_width`/`r_length`. sky130 multiplicity needs
both `m=` and `mult=` (gf180: `m=` and `par=`) so mismatch scales with area. Keep that
table per PDK in your own code, not in circuit code.

## Corners

```python
from spicerack.testbenches import pdk_corners, run_corners, CornerCase, corner_netlists
corners = pdk_corners(LIB, ["ss", "ff"], temperatures=[-40, 125], vdds=[1.62, 1.98])
rows = run_corners(build, measure, corners)    # list of {"corner", "temperature", "vdd", ...}
decks = corner_netlists(make_tb, corners)      # {name: deck}, no simulation
```

* `run_corners` is the one-call runner — see [`TESTBENCHES.md`](TESTBENCHES.md).
* `CornerCase(name, backend=None, temperature=None, nominal_temperature=None,
  parameters={}, model_libraries=(), vdd=None)` — frozen; `parameters` emits `.param k=v`;
  `vdd` is for your builder to read.
* Factories take a fresh bench per corner — build the whole bench inside them, including
  `use_pdk`, or libraries pile up.
* `evaluate_corners(factory, corners, rules, metric_extractor, backend="ngspice",
  runner=None)` scores against `ValidationRule`s: the extractor gets `runner(bench)` if
  given, else the bench — so the extractor must run the simulation itself.

## Monte Carlo

* **ngspice route:** `run_corners(build, measure, corners, seeds=N)` — one fresh bench
  per seed with `.options seed=<n>`, reproducible per seed. It only varies anything when
  the models are statistical. Mismatch is switched on differently per PDK: sky130 uses
  the `tt_mm` (`*_mm`) lib section; gf180 needs `.param sw_stat_mismatch=1` alongside
  the corner section, which loads `fets_mm` — pass it as `CornerCase(parameters=...)`.
* `MonteCarloPlan(backend="spectre", samples=100, distributions={},
  spectre_inner="tran1", spectre_inner_type="tran", seed=None)` is **Spectre only**
  (any other backend raises), and `distributions` is never read. Set `spectre_inner` /
  `spectre_inner_type` to an analysis you actually queued.
* `evaluate_monte_carlo_file(path, rules, backend="auto", names=None)` only parses
  existing result files (csv/tsv/log/.mt0/Spectre mcdata/`.measure` blocks).

## Verilog-A devices (OpenVAF / OSDI)

```python
dut = ps.Subcircuit("amp_va", ["inp", "inn", "out"])
dut.veriloga("amp.va")                     # runs `openvaf amp.va -o amp.osdi`, loads it
dut.model("ampmod", "amp", gain=1e4)
dut.raw_spice("N1 inp inn out ampmod")
```

* `veriloga(src_or_path)` compiles with **OpenVAF** (skips if `.osdi` is newer) and
  returns the `.osdi` path. `osdi(path)` loads a prebuilt one; `ps.compile_veriloga(src)`
  compiles without loading.
* No device method for VA instances — use an `N` line via `raw_spice`.
* Per backend: ngspice `.control / pre_osdi / .endc`; vacask `load "<p>"` (and needs
  VACASK instance syntax in `raw_spice`); spectre gets `ahdl_include` but **drops all
  `raw_spice`**; ltspice drops OSDI silently.
* `verilog(...)` on a `Subcircuit` DUT is applied when the Testbench runs, but
  `tb.netlist()` does not show it.

## External netlist as DUT (post-layout)

```python
top = ps.Subcircuit("tb_top"); top.include("/abs/path/block_pex.spice")
top.X("xdut", "block", "vdd", "0", "inp", "out")
tb = ps.Testbench(top)
```

## Printing a bare `.subckt`

No API returns just the subckt. The testbench deck **flattens** the DUT (no wrapper).
Wrap it in a `Circuit` and slice:

```python
c = ps.Circuit("x"); c.subcircuit(dut); deck = str(c)
body = deck[deck.index(".subckt"): deck.index("\n", deck.index(".ends"))]
```

Parameters come out as `PARAMS:` on the `.subckt` line.
