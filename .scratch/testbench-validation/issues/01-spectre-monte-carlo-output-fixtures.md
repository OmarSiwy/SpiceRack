# Spectre Monte Carlo Output Fixture Coverage

Status: needs-info

## Summary

The testbench validation helpers can ingest common scalar tables, repeated
measure logs, and Xyce-style Monte Carlo metric files. Spectre Monte Carlo
output naming and scalar-export layout can vary by command-line flags,
Spectre version, and whether users export CSV/TSV, PSF, or log-derived
measurements.

## Risk

`load_monte_carlo_metrics(..., backend="spectre")` supports common scalar
table and log formats, but it has not yet been verified against a real
Spectre-generated Monte Carlo run directory. A production Spectre run may use
an additional suffix or nested location that the directory scanner does not
currently include.

## Acceptance Criteria

- Capture at least one small real Spectre Monte Carlo output directory.
- Add it as a minimal fixture or document an anonymized equivalent layout.
- Confirm `find_metric_files(..., backend="spectre")` discovers the scalar
  result files without picking waveform-heavy raw/PSF data by default.
- Add parser tests for the discovered filename/layout.
- Update `docs/api/testbenches.md` with the verified Spectre export command
  or file convention.

## Comments

Created after landing the generic Monte Carlo result-ingestion layer. The
remaining work needs a representative real Spectre output sample rather than
more synthetic table parsing.
