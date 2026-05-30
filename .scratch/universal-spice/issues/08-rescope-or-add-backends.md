# 08 — Honest backend target set (+ extensibility)

Status: done
Priority: P2
Theme: scope / extensibility

## Problem

Stated goal "ALL SPICE tools" vs reality: `docs/backends/README.md:18-23`
de-scopes HSPICE/PSpice/Qucsator. Qspice (modern free), Eldo, CppSim, SIMetrix
unmentioned. CONTEXT/README/PRD disagree on ambition.

Adding a backend touches ~12 scattered sites (BackendKind variants, 4 match
arms, detect, ~7 `matches!` capability lists, normalize branches, result parser)
because capabilities live outside the `Backend` trait (`src/backend/mod.rs:118-158`).

## Change

- Decide + document the honest target list. Candidates to add: **Qspice**
  (free, popular), HSPICE (`-i`/batch), PSpice. Or explicitly scope to the 5 and
  fix CONTEXT.md wording.
- Move capability flags onto the `Backend` trait (or one table from #05) so a new
  backend is one cohesive impl, not 12 edit sites.

## Tests (TDD)

- Adding a stub backend requires implementing one trait + registering once;
  compile-time exhaustiveness catches missing capability entries.

## Files

`docs/backends/README.md`, `CONTEXT.md`, `src/backend/mod.rs:39-158`,
`src/backend/detect.rs`.
</content>
