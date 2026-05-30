# 05 — Dialect correctness + single source of truth for capabilities

Status: done
Priority: P1
Theme: codegen / all dialects

## Problem

Shared spice3 emitter wrongly assumes one dialect in several spots:

- **Xyce silent-no-output**: emits `.save` (`spice3.rs:276`); Xyce needs `.PRINT`
  -> produces no output otherwise.
- **No `.GLOBAL_PARAM`** anywhere; Xyce needs it for hierarchical/swept params.
- Title line emitted as `* {name}` comment (`spice3.rs:189`); SPICE3 line 1 is
  the title -> shifts implicit title for ngspice/Xyce.
- Dead `hierarchy_separator` (`spice3.rs:89`, `#[allow(dead_code)]`).
- `.pz`/`.disto` emitted for all 3 dialects (`spice3.rs:522-537`) though
  ngspice-only -> invalid lines on Xyce/LTspice.
- **Selector/emitter disagree**: `hb/sp/stb/trannoise` route to xyce/vacask via
  `analysis_backend_preference` (`mod.rs:181-189`) but spice3 has no arm and
  errors (`spice3.rs:612-627`). Selector promises what codegen refuses.
- LTspice `.tran` option flags / unique `.options` keys absent.

**Three duplicated capability tables drift**: `backend/mod.rs:118-158`,
`ir/mod.rs:436-477`, `lint.rs` per-backend checks.

## Change

- Per-dialect emission for `.PRINT`/`.save`, `.GLOBAL_PARAM`, title line,
  hierarchy separator, pz/disto gating, hb/sp/stb arms (or correct the
  preference lists). Codegen must match capability tables exactly.
- Collapse the 3 capability tables into one source of truth consumed by
  selector, codegen, and lint.

## Tests (TDD)

- Xyce emit contains `.PRINT`, not `.save`; output parses back.
- Swept param across hierarchy emits `.GLOBAL_PARAM` for Xyce.
- For every (analysis, backend) the preference list allows, codegen emits a valid
  line (property test over the capability table) — no emitter/selector mismatch.

## Files

`src/codegen/spice3.rs:89,189,276,522-627`, `src/backend/mod.rs:118-202`,
`src/ir/mod.rs:436-477`, `src/lint.rs`.
</content>
