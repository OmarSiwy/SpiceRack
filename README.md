# PySpice

Rust-powered Python library for SPICE circuit simulation. Drop-in circuit description API with a backend-neutral IR, multi-backend support, and exports for Rust, Python (PyO3), and C (FFI).

Part of [Schemify](https://github.com/OmarSiwy).

## Features

- **5 backends supported** -- ngspice, Xyce, LTspice, Spectre, VacaSk
- **Code generators** -- SPICE3 and Spectre netlist emission from the IR
- **C ABI** -- link directly from C/C++/Zig

## Quick Start

### Prerequisites

- Rust (stable)
- Python 3.10+
- At least one SPICE backend on `$PATH` (e.g. `ngspice`)

Or use the Nix devshell which provides everything:

```bash
nix develop
```

### Build & Install

```bash
# Rust library
cargo build

# Python package
python3 -m venv .venv
source .venv/bin/activate
pip install maturin
maturin develop
```

### Usage

```python
from pyspice_rs import Circuit
from pyspice_rs.unit import u_V, u_kOhm

circuit = Circuit("Voltage Divider")

circuit.V(name="in", positive="input", negative=circuit.gnd, value=10 @ u_V)
circuit.R(name="1", positive="input", negative="output", value=2 @ u_kOhm)
circuit.R(name="2", positive="output", negative=circuit.gnd, value=1 @ u_kOhm)

print(circuit)

# Simulate
sim = circuit.simulator()
result = sim.operating_point()
print(result["output"])  # ~3.333V
```

See the [`examples/`](examples/) directory for 18 working examples covering voltage dividers, filters, BJT/MOSFET amplifiers, op-amps, subcircuits, Verilog-A, and digital co-simulation.

## Backends

| Backend | Status | Notes |
|---------|--------|-------|
| ngspice | Supported | Default backend, XSPICE extensions |
| Xyce | Supported | Sandia parallel simulator |
| LTspice | Supported | Windows/macOS/Wine |
| Spectre | Supported (needs testing if anyone can) | Cadence, native Spectre syntax codegen |
| VacaSk | Supported | Compact model testing via OpenVAF |

See [`docs/backends/`](docs/backends/) for per-backend details and the [analysis compatibility map](docs/backends/analysis-map.md).

## Exports

The library supports three output targets:

- **Rust crate** (`rlib`) -- use as a native Rust dependency
- **Python module** (`cdylib` + PyO3) -- `pip install` via maturin
- **C ABI** (`cdylib` + `--features cabi`) -- `#include` and link

```bash
# Rust
cargo build --release

# Python
maturin build --release

# C ABI
cargo build --release --features cabi --no-default-features
```

## Testing

```bash
# Rust tests
cargo test

# Python tests
maturin develop && python3 -m pytest -v
```

## Project Structure

```
src/
  circuit.rs      Circuit/Subcircuit construction
  ir/             Backend-neutral intermediate representation
  codegen/        Netlist emitters (SPICE3, Spectre)
  backend/        Simulator drivers (ngspice, xyce, ltspice, spectre, vacask)
  simulation.rs   Simulation orchestration
  result.rs       Normalized result parsing
  lint.rs         Circuit topology linting
  unit.rs         Engineering unit system
  python.rs       PyO3 bindings
  cabi.rs         C FFI exports
examples/         Python usage examples
tests/            Rust + Python test suites
docs/             mdBook documentation
schema/           JSON schema for serialized IR
```

## License

See repository for license details.
