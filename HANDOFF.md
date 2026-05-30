# HANDOFF — Universal SPICE wrapper

All planned work complete. This file documents what was built and what
remains as known limitations.

## What was built

IR-based multi-backend architecture: circuits described as `CircuitIR`,
emitted to ngspice/xyce/ltspice (Spice3CodeGen), spectre (SpectreCodeGen),
and vacask (VacaskCodeGen) via a `CodeGen` trait. Auto backend selection
from IR feature flags. PDK hotswap via `ModelLibrary` with per-backend
paths, corner switching, and `setup_includes`. Normalized result contract
across backends. Python API (PyO3) with PySpice-compatible surface.

## Issues (all resolved)

| # | Title | Pri | Status |
|---|-------|-----|--------|
| 01 | Wire IR CodeGen into run path | P0 | **Done** |
| 02 | IR-driven auto backend selection | P0 | **Done** |
| 03 | Generic PDK: wire ModelLibrary, kill leaks | P0 | **Done** |
| 04 | Vacask: real codegen + fix fabricated C API | P1 | **Done** |
| 05 | Dialect correctness + one capability table | P1 | **Done** |
| 06 | Result parity / Normalized Result contract | P1 | **Done** |
| 07 | PySpice API compat + generic-API leaks | P1 | **Done** |
| 08 | Honest backend target set + extensibility | P2 | **Done** |
| 09 | OSDI version mismatch (ngspice 43 vs v0.4) | — | **Fixed** — removed ngspice 43 pin, nixpkgs default (44+) loads OSDI v0.4 |
| 10 | sky130 binned-model scoped resolution | — | **External** — ngspice limitation, not a DeSpice bug |
| 11 | gf180mcu Formula() errors | — | **Fixed** — `setup_includes` on `ModelLibrary`, all codegens + Python API |
| 12 | vacask can't parse ngspice `.lib` | — | **Fixed** — example-level fix (only list backends with native model files) |
| 13 | IR VoltageSource AC magnitude | — | **Fixed** — `ac_magnitude`/`ac_phase` on VoltageSource + CurrentSource |

Detail for 01–08 in `.scratch/universal-spice/issues/NN-*.md` (all `Status: done`).
Issues 09–13 were inline fixes without dedicated issue files.

## Known limitation: sky130 binned-model resolution (#10)

ngspice scopes model lookup inside `.subckt` wrappers. When sky130 models
reference binned variants (`sky130_fd_pr__nfet_01v8__model.0`, `.1`, …),
ngspice can't resolve them from within `X` instantiation scope. Affects
all ngspice versions (tested 43 and 44.2).

**Workarounds:** flatten the subcircuit wrapper, or use `nf=1` variants.
**Upstream fix:** ngspice patch or bug report.

## Acceptance check

```
grep -i 'sky130\|gf180\|tsmc' <any user circuit script>  # == empty
```

Same circuit runs on ngspice / xyce / spectre via auto-select with
normalized results.

## Git discipline (for future work)

1. One issue = one focused commit. Mark issue `Status: done` in same commit.
2. Clean tree before worktree fan-out (`git status` -> clean).
3. Branch per issue in worktrees: `issue-NN-<slug>`.
4. Re-sync `main` and re-confirm clean tree between merges.
