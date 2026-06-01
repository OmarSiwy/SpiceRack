Anonymized Spectre Monte Carlo scalar fixture.

This mirrors the documented standalone/ADE convention where a Monte Carlo
analysis writes scalar expression values to `scalarfile`, commonly
`monteCarlo/mcdata`, and column labels to the paired `paramfile`, commonly
`monteCarlo/mcparam`.

The `psf/` and `input.raw/` siblings are intentionally tiny placeholders for
waveform-heavy outputs that should not be selected by the scalar metric
scanner.
