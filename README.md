# DeSpice

Python library for building SPICE circuits, generating netlists, running simulator backends, and reading results.

The Python package is named `pyspice-rs` and imported as `pyspice_rs`.

## Install for Development

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install maturin numpy pytest
maturin develop
```

If you use Nix:

```bash
nix develop
```

Install at least one simulator if you want to run analyses. Start with `ngspice` unless you need a specific backend.

## First Circuit

```python
import pyspice_rs as ps
from pyspice_rs.unit import u_V, u_kOhm

circuit = ps.Circuit("voltage_divider")
circuit.V(name="in", positive="vin", negative=circuit.gnd, value=10 @ u_V)
circuit.R(name="top", positive="vin", negative="vout", value=2 @ u_kOhm)
circuit.R(name="bot", positive="vout", negative=circuit.gnd, value=1 @ u_kOhm)

print(circuit)

sim = circuit.simulator(simulator="ngspice")
op = sim.operating_point()
print(op["vout"])
```

## Docs

- [User guide](docs/guide.html): install, build circuits, simulate, read results, choose backends, lint, PDKs, Verilog-A.
- [API reference](docs/reference.html): every class, element method, analysis, and result type.
- [Examples](docs/examples.html): index of the executable scripts in [examples/](examples/), ordered from basic circuits to backend and testbench workflows.

## Test

```bash
cargo test
maturin develop
python3 -m pytest -v
```

## Backends

DeSpice has support for `ngspice`, `xyce`, `ltspice`, `spectre`, and `vacask`. Backend availability depends on what is installed locally.
