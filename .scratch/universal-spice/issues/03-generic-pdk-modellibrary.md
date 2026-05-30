# 03 — Generic PDK: wire ModelLibrary, kill PDK-specific leaks

Status: needs-triage
Priority: P0
Theme: generic PDK (goal 1)

## Problem

User is forced to hardcode PDK specifics 3 ways:

1. **Model names** — examples hand-write `circuit.model("NMOS_3V3","NMOS",...)`
   (`examples/05_cmos_inverter.py`). Real PDK = `circuit.include(
   "/path/sky130_fd_pr/nfet_01v8.spice")` then reference model name by hand
   (`docs/api/models.md:81`).
2. **File paths** — scattered string paths in `.include`/`.lib`, no single object.
3. **Dir layout** — `pdk="sky130_fd_sc_hd"` shortcut routes through
   `resolve_pdk_paths` which hardcodes Sky130/OpenLane `libs.ref` + `A`-suffix
   layout (`src/python.rs:2001-2068`). tsmc/proprietary layouts fail.

Root cause: `ModelLibrary` (`src/ir/mod.rs:235`) is the right abstraction
(opaque, `corner`, `backend_paths` per-backend map) but is **dead** — no API
accepts it; both IR builders hardcode `model_libraries: vec![]`
(`src/python.rs:3993`, `src/ir/mod.rs:429`). Only non-empty use is one unit test.

## Change

- Python API: `pdk = ModelLibrary(name, corner=..., backend_paths={...})` and
  `circuit.use_pdk(pdk)` / `simulator(pdk=...)`. Populate `CircuitIR.model_libraries`.
- **Generic device-type -> model-name resolution.** User writes a generic
  device (e.g. `circuit.M(..., type="nmos")`), PDK maps `nmos` ->
  PDK-specific model name + bin. No PDK model strings in user code.
- Central corner switch on the PDK object; emit `.lib f tt` (spice) vs
  `include "f" section=tt` (spectre) — both already exist in codegen
  (`spice3.rs:206`, `spectre.rs:188`), just wire to ModelLibrary.
- PDK layout resolution becomes pluggable (layout strategy), not Sky130-hardcoded.
  Sky130/OpenLane is one built-in strategy; others registerable.
- OSDI/Verilog-A models attach via ModelLibrary too (per-backend `.pre_osdi`
  ngspice vs `ahdl_include` spectre vs vacask `load`).

## Acceptance

Swap PDK = change one object/string. No sky130/gf180/tsmc literal anywhere in
user circuit code. `grep -i sky130 user_script.py` == empty.

## Tests (TDD)

- Build circuit with generic `nmos`/`pmos` + PDK_A -> emits PDK_A model names &
  includes. Swap to PDK_B object only -> emits PDK_B, circuit code unchanged.
- Per-backend path override resolves correctly for ngspice vs spectre.
- Corner switch tt->ss changes emitted section, nothing else.

## Files

`src/ir/mod.rs:235-242,429`, `src/python.rs:3993,4037,2001-2068`,
`src/codegen/spice3.rs:193-209`, `src/codegen/spectre.rs:169-205`.
</content>
