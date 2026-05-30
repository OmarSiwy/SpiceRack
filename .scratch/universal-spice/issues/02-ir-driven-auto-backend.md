# 02 — IR-driven automatic backend selection

Status: needs-triage
Priority: P0
Theme: auto-select

## Problem

Auto-select exists (`detect_and_select_with_features`, `src/backend/mod.rs:214`)
but is keyed on a hand-passed `analysis_type: &str` and a `CircuitFeatures`
struct that defaults to all-false. The rich `Analysis` enum (`src/ir/mod.rs:181`)
and `FeatureFlags` (`src/ir/mod.rs:247`) never reach it:

- No `Analysis -> kind_str()` mapping. Selector can't read what the user wrote.
- No `FeatureFlags -> CircuitFeatures` bridge; `detect_and_select` passes
  `CircuitFeatures::default()` (`mod.rs:209`) -> feature filtering effectively off.
- `element_count` declared (`mod.rs:113`) but never read -> no "big circuit ->
  xyce parallel" logic.
- `check_backend()` (`ir/mod.rs:337`) used only in tests; could rank candidates.

## Change

- `impl Analysis { fn kind_str(&self) -> &str }`.
- `impl From<&FeatureFlags> for CircuitFeatures` (or collapse the two — see #05).
- Drive `detect_and_select_with_features` from IR analysis + features.
- Auto-pick = first installed backend whose `check_backend()` yields zero errors,
  respecting analysis preference order.

## Tests (TDD)

- PSS analysis on IR -> selects spectre when present, errors helpfully if not.
- OSDI feature on IR -> excludes backends lacking OSDI.
- No backend named by user -> correct auto-pick; explicit override still wins.

## Files

`src/backend/mod.rs:174-270`, `src/ir/mod.rs:181-414`, `src/simulation.rs:746`.
</content>
