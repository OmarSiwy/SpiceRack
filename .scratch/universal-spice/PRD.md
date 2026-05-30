# PRD: Universal SPICE wrapper — generic PDK, all-dialect codegen, auto-backend

Status: needs-triage
Owner: omare
Date: 2026-05-30

## Goal

One generic API. User writes PySpice-style Python once. Library:

1. Swaps any PDK with **zero PDK-specific code** (no model names, paths, or dir
   layout in user code). Quick swap sky130 -> gf180 -> tsmc.
2. Emits **all** SPICE dialects, auto-selecting backend from the analysis +
   circuit features the user wrote.
3. Genuinely supports all 5 target backends end-to-end.

## How we found the gaps

4 parallel research agents audited: backend coverage, PDK/ModelLibrary
genericity, codegen dialects + auto-select, PySpice API compat. Findings below.

## Architecture reality (must read first)

The IR `CodeGen` trait (`src/codegen/`) is **dead relative to the run path**.
`emit_netlist` is called only from tests. Production netlists come from
`Circuit::Display` (`src/circuit.rs:1692`) — one hardcoded SPICE3 dialect — then
vacask/spectre do runtime **string-to-string** post-translation
(`spice_to_vacask`, `src/backend/vacask.rs:82`). So "2 generators for 5
backends" is really **one** dialect + two ad-hoc translators.

`ModelLibrary` (the PDK abstraction) is likewise dead: defined at
`src/ir/mod.rs:235`, exposed to Python at `src/python.rs:4037`, but no API
accepts it and both IR builders hardcode `model_libraries: vec![]`
(`src/python.rs:3993`, `src/ir/mod.rs:429`).

**These two unwired layers are the spine of all three goals. Wiring them is P0.**

## Themes / issues

- `issues/01-wire-ir-codegen-runpath.md` — P0. Make IR CodeGen the only path to text.
- `issues/02-ir-driven-auto-backend.md` — P0. Analysis+features from IR drive selection.
- `issues/03-generic-pdk-modellibrary.md` — P0. Wire ModelLibrary; generic device->model resolution.
- `issues/04-vacask-codegen-and-ffi.md` — P1. Real vacask dialect; fabricated C API.
- `issues/05-dialect-correctness.md` — P1. Xyce/.GLOBAL_PARAM/.PRINT, selector/emitter agreement.
- `issues/06-backend-result-parity.md` — P1. PSF, Xyce HB/Sparam, pz/disto, .device namespace.
- `issues/07-pyspice-api-compat.md` — P1. Positional factories, AC complex, units, .nodes/.branches.
- `issues/08-rescope-or-add-backends.md` — P2. Honest target set; Qspice/HSPICE.

## Non-goals (for now)

- Parsing PDK model file contents (ModelLibrary stays opaque by design).
- New simulators beyond a documented, honest target list.
</content>
</invoke>
