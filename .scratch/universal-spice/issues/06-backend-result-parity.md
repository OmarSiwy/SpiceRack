# 06 — Result parity across backends (Normalized Result contract)

Status: done
Priority: P1
Theme: backend coverage / results

## Problem

CONTEXT.md promises a Normalized Result (lowercase nodes, canonical currents,
complex-always-complex, `.device` namespace). Reality diverges:

- **`.device` namespace does not exist** anywhere (`AnalysisBase` has only
  `nodes`+`branches`, `src/result.rs:85`). Device op points (`@m1[id]`, gm, vth)
  leak into `branches`/`nodes`. Either implement or amend the contract.
- **Spectre normalized with ngspice rules**: PSF parser hardcodes
  `backend_hint: ""` (`src/psf.rs:442`), `AnalysisBase::from_raw` defaults empty
  -> "ngspice" (`result.rs:93`). Spectre `V1:p` currents misclassified.
- **PSF parser heuristic/fragile** (`psf.rs:215-341`): TYPE section skipped,
  property scan guessed, unknown sections "skip 4 bytes and hope". Real Cadence
  PSF likely mis-parses; only `nutbin` Nutmeg reliable.
- **Xyce advanced outputs not parsed**: HB (`.HB.FD/.TD`) and S-param
  (Touchstone/`.LIN`) routed but reader always reads `<cir>.raw` (`xyce.rs:29`).
- **pz/disto run but return empty** — `PoleZeroAnalysis::from_raw` stub
  (`result.rs:278`), no distortion parsing.

## Change

- Add `.device` namespace to result types + per-backend population; or update
  CONTEXT.md/ADR if descoped.
- Stamp real `backend_hint` in PSF parser; fix current classification per backend.
- Harden or replace PSF parser (TYPE/property sections); keep nutbin path.
- Parse Xyce HB/Touchstone outputs; locate correct output file per analysis.
- Implement pz/disto result parsing.

## Tests (TDD)

- Spectre AC result: `i(v1)` recognized as current (not node).
- Same circuit, ngspice vs xyce vs spectre -> identical normalized key set.
- pz analysis returns non-empty poles/zeros.
- Device op point readable via `.device` namespace, not polluting `.branches`.

## Files

`src/result.rs:85,93,278`, `src/psf.rs:215-341,442`, `src/normalize.rs`,
`src/backend/xyce.rs:29`.
</content>
