# 01 — Wire IR CodeGen into the run path

Status: done
Priority: P0
Theme: codegen / all dialects

## Problem

IR `CodeGen` trait + `Spice3CodeGen`/`SpectreCodeGen` are dead. `emit_netlist`
called only from `#[cfg(test)]`. Production netlist = `Circuit::Display`
(`src/circuit.rs:1692`), one hardcoded SPICE3 dialect, then runtime string
translation for vacask (`src/backend/vacask.rs:82`) and spectre. ADR-0001 says
codegen is "the only path from IR to text" — violated.

## Why it blocks goals

Can't emit all dialects or auto-select if every backend gets the same string +
regex fixups. All dialect/PDK/auto-select work sits on top of this.

## Change

- `Backend::run` (or a pre-step) takes the **IR**, not a pre-rendered `&str`.
- Each backend names its `CodeGen`; dispatch builds IR -> emit -> run.
- Delete `spice_to_vacask` string translator and spectre post-transforms.
- Keep `Circuit::Display` only for human `print(circuit)` / `raw_spice` debug,
  not for execution.

## Tests (TDD)

- Same IR -> ngspice vs xyce vs spectre produce dialect-correct, diffable text.
- Round-trip: build IR, emit, assert no string-translation pass runs.
- Regression: existing examples still simulate on ngspice.

## Files

`src/codegen/mod.rs`, `src/backend/mod.rs`, `src/backend/*.rs`,
`src/circuit.rs:1692`, `src/backend/vacask.rs:82`.
</content>
