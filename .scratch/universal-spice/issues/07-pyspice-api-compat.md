# 07 — PySpice API compatibility + generic-API leaks

Status: needs-triage
Priority: P1
Theme: Python API

## Problem

Module claims "identical API surface to original PySpice" (`src/python.rs:1`) —
inaccurate. Breaks real PySpice scripts:

- **Element factories keyword-only** (`#[pyo3(signature=(*, name, ...))]`,
  `python.rs:166+`). PySpice is positional: `circuit.R(1,'in','out',1@u_kOhm)`
  raises TypeError here. Breaks ~100% of existing scripts.
- **AC drops imaginary part** (correctness): `PyAcAnalysis.__getitem__` returns
  `wf.data` = real part only (`python.rs:1378`); `complex_data` never exposed.
  Wrong Bode/magnitude/phase. Docs mislabel as complex.
- **Unicode units missing**: only ASCII `u_kOhm` etc. (`python.rs:4099-4139`);
  PySpice canonical `u_kΩ`/`u_Ω`/`u_µF` -> ImportError.
- **No `.nodes`/`.branches`** on result objects (maps exist `result.rs:86` but
  unexposed) -> can't enumerate results.
- **Subcircuit divergence**: no `SubCircuitFactory`; docs steer to
  `raw_spice(str(subckt))` (`examples/10_subcircuit.py:70`) instead of portable
  `circuit.subcircuit()` (`python.rs:485`) — defeats IR portability.
- **Backend leaks into generic API**: `xyce_*`/`spectre_*` methods on the generic
  simulator hardcode a backend (`python.rs:1077-1234`, `simulation.rs:454`);
  fail on machines without that backend.
- Missing `sim.fourier()` (documented `simulation.md:234`); `save_currents`
  getter stub (`python.rs:858`).

## Change

- Accept positional element args (keep kwargs).
- AC results return complex (numpy), expose magnitude/phase.
- Register Unicode unit aliases.
- Expose `.nodes`/`.branches`.
- Add `SubCircuitFactory`; document `circuit.subcircuit()` as the blessed path.
- Move `xyce_*`/`spectre_*` behind `sim.xyce.*`/`sim.spectre.*` namespaces with
  availability checks (Tier-3 vendor namespace per ADR).

## Tests (TDD)

- Positional `circuit.R('1','in','out',1e3)` works.
- AC `ac['out']` is complex; `abs()`/angle correct vs known RC.
- `10@u_kΩ` imports and equals `10@u_kOhm`.
- `analysis.nodes` enumerates node names.

## Files

`src/python.rs:1,166,858,1077-1234,1378,4099-4139`, `src/result.rs:66,86`,
`src/unit.rs`, examples/docs.
</content>
