# 04 — Vacask: real codegen dialect + fix fabricated C API

Status: done
Priority: P1
Theme: backend coverage

## Problem

1. **Fabricated FFI.** `VacaskLibrary` shared-lib path dlsyms a guessed C API
   (`vacask_init/load_netlist/run/get_result/cleanup`) with a guessed
   `VacaskResult` layout — self-described "best-effort assumptions" that "will
   fail at runtime if signatures differ" (`src/backend/vacask.rs:563-601`).
   `is_available()` returns true if `libvacask.so` merely opens, so auto-select
   (`detect.rs:41`) can pick a backend that then hard-fails.
2. **Lossy translation.** No vacask codegen dialect at all
   (`src/codegen/spice3.rs:5-10` has only Ngspice/Xyce/Ltspice). Runtime
   `spice_to_vacask` drops unhandled lines to `// UNTRANSLATED:` comments
   (`vacask.rs:198,209,286`) -> silently simulates an altered circuit.
   `.ENDS` emits broken empty-name `ends ` (`vacask.rs:130`).

## Change

- Add a real vacask `CodeGen` (or dialect) emitting native vacask syntax (depends
  on #01). No string translation, no silent UNTRANSLATED drops — unsupported
  constructs become hard `CodeGenError`.
- Verify vacask C API against real headers/docs; gate shared-lib `is_available()`
  on actual symbol resolution, else fall back to subprocess.

## Tests (TDD)

- Every IR component either emits valid vacask or errors loudly (no silent drop).
- Subcircuit `.ends` round-trips with correct name.
- Shared-lib unavailable/symbol-missing -> graceful subprocess fallback, no panic.

## Files

`src/backend/vacask.rs:82-300,563-601`, `src/backend/detect.rs:41`,
`src/codegen/` (new vacask emitter).
</content>
