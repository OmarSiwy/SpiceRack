# HANDOFF — Universal SPICE wrapper

Work tracked in `.scratch/universal-spice/` (PRD + 8 issues). This file =
execution contract for whoever (human or agent) picks it up.

## Git discipline (READ FIRST)

1. **Commit after every issue fix.** One issue = one focused commit (or a short
   series). Never batch multiple issues into one commit. Mark the issue
   `Status: done` in its file in the same commit.
2. **Commit before launching git worktrees / parallel worktree agents.** The
   working tree must be clean before any `git worktree add` or parallel-agent
   fan-out. Uncommitted changes + worktrees = lost/colliding work. Verify with
   `git status` -> clean, then spawn.
3. Branch per issue when working in a worktree: `issue-NN-<slug>`. Merge back to
   `main` only after that issue's TDD tests pass.
4. After merging a worktree, re-sync `main` and re-confirm clean tree before the
   next fan-out.

## Two dead layers block everything (do first)

- IR `CodeGen` is unwired (`emit_netlist` test-only); production text =
  `Circuit::Display` single dialect + string translators.
- `ModelLibrary` (PDK abstraction) is unwired; both IR builders hardcode
  `model_libraries: vec![]`.

Wiring these = the P0 spine. All other issues stack on top.

## Issues

| # | Title | Pri | Goal | Depends |
|---|-------|-----|------|---------|
| 01 | Wire IR CodeGen into run path | P0 | all dialects | — |
| 02 | IR-driven auto backend selection | P0 | auto-select | 01 |
| 03 | Generic PDK: wire ModelLibrary, kill leaks | P0 | PDK swap | 01 |
| 04 | Vacask: real codegen + fix fabricated C API | P1 | coverage | 01 |
| 05 | Dialect correctness + one capability table | P1 | all dialects | 01 |
| 06 | Result parity / Normalized Result contract | P1 | coverage | — |
| 07 | PySpice API compat + generic-API leaks | P1 | API | — |
| 08 | Honest backend target set + extensibility | P2 | scope | — |

Detail in `.scratch/universal-spice/issues/NN-*.md`. Each has Problem, Change,
TDD tests, Files.

## Recommended order

```
P0: 01 -> 02 -> 03      (sequential; 02 and 03 both need 01)
P1: 05, 04, 06, 07      (parallelizable in worktrees once 01 merged)
P2: 08
```

### Parallel fan-out plan (after 01 merged to main)

1. `git status` -> clean. Commit anything pending.
2. Spawn worktree agents for 05, 04, 06, 07 (06 and 07 are independent of 01 and
   could even start earlier).
3. Each agent: branch `issue-NN-<slug>`, TDD, commit per its own issue, return.
4. Merge sequentially, re-running the full test suite between merges.

## Definition of done (per issue)

- TDD tests in the issue file written first, then made to pass.
- `cargo test` + Python tests (`pytest tests/`) green.
- Issue file `Status: done`.
- Committed. Tree clean.

## Done-for-the-feature

All P0+P1 green, and the acceptance check holds:
`grep -i 'sky130\|gf180\|tsmc' <any user circuit script>` == empty, same circuit
runs on ngspice / xyce / spectre via auto-select with normalized results.
</content>
