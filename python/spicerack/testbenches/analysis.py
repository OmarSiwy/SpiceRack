"""Validation, corner, and statistical-analysis helpers for design benches."""

from __future__ import annotations

import csv
import math
import re
from dataclasses import dataclass, field
from math import sqrt
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence


def _testbench(obj: Any) -> Any:
    return getattr(obj, "testbench", obj)


@dataclass(frozen=True)
class ValidationResult:
    name: str
    passed: bool
    actual: Any
    message: str


@dataclass(frozen=True)
class ValidationReport:
    results: list[ValidationResult]

    @property
    def passed(self) -> bool:
        return all(result.passed for result in self.results)

    @property
    def failures(self) -> list[ValidationResult]:
        return [result for result in self.results if not result.passed]


@dataclass(frozen=True)
class ValidationRule:
    name: str
    field: str
    minimum: float | None = None
    maximum: float | None = None
    expected: Any = None
    tolerance: float | None = None
    required: bool = True

    def check(self, metrics: Mapping[str, Any]) -> ValidationResult:
        if self.field not in metrics:
            passed = not self.required
            message = "optional metric missing" if passed else "required metric missing"
            return ValidationResult(self.name, passed, None, message)

        actual = metrics[self.field]
        failures: list[str] = []
        if self.minimum is not None and float(actual) < self.minimum:
            failures.append(f"{self.field}={actual} below {self.minimum}")
        if self.maximum is not None and float(actual) > self.maximum:
            failures.append(f"{self.field}={actual} above {self.maximum}")
        if self.expected is not None:
            if self.tolerance is None:
                if actual != self.expected:
                    failures.append(f"{self.field}={actual} expected {self.expected}")
            elif abs(float(actual) - float(self.expected)) > self.tolerance:
                failures.append(
                    f"{self.field}={actual} outside {self.expected} +/- {self.tolerance}"
                )

        return ValidationResult(
            self.name,
            not failures,
            actual,
            "; ".join(failures) if failures else "passed",
        )


def validate_metrics(metrics: Mapping[str, Any], rules: Sequence[ValidationRule]) -> ValidationReport:
    return ValidationReport([rule.check(metrics) for rule in rules])


@dataclass(frozen=True)
class MetricSpec:
    name: str
    source: str
    reducer: str = "last"
    at: float | None = None
    threshold: float | None = None
    rising: bool = True


def _get_signal(result: Any, name: str) -> Any:
    if isinstance(result, Mapping):
        return result[name]
    if hasattr(result, "measures") and name in result.measures:
        return result.measures[name]
    if hasattr(result, name):
        return getattr(result, name)
    return result[name]


def _as_series(value: Any) -> list[float]:
    if isinstance(value, (str, bytes)):
        raise TypeError("string values are not numeric waveforms")
    try:
        return [float(item) for item in value]
    except TypeError:
        return [float(value)]


def _interp_at(x_values: Sequence[float], y_values: Sequence[float], x_target: float) -> float:
    if not x_values or not y_values:
        raise ValueError("cannot interpolate an empty waveform")
    if x_target <= x_values[0]:
        return float(y_values[0])
    for idx in range(1, min(len(x_values), len(y_values))):
        x0 = float(x_values[idx - 1])
        x1 = float(x_values[idx])
        if x_target <= x1:
            y0 = float(y_values[idx - 1])
            y1 = float(y_values[idx])
            if x1 == x0:
                return y1
            return y0 + (y1 - y0) * ((x_target - x0) / (x1 - x0))
    return float(y_values[-1])


def crossings(
    x_values: Sequence[float],
    y_values: Sequence[float],
    threshold: float,
    rising: bool | None = None,
    log_x: bool = False,
) -> list[float]:
    """Every x where y crosses ``threshold``, linearly interpolated.

    ``rising=None`` returns both directions. ``log_x=True`` interpolates in
    log10(x), which matters on a decade-spaced frequency sweep: linear
    interpolation between decade points misplaces a -3 dB corner by ~1% at
    10 points/decade.
    """
    found: list[float] = []
    for idx in range(1, min(len(x_values), len(y_values))):
        y0, y1 = float(y_values[idx - 1]), float(y_values[idx])
        up = y0 <= threshold <= y1
        down = y0 >= threshold >= y1
        if rising is True and not up:
            continue
        if rising is False and not down:
            continue
        if rising is None and not (up or down):
            continue
        x0, x1 = float(x_values[idx - 1]), float(x_values[idx])
        if y1 == y0:
            found.append(x1)
            continue
        frac = (threshold - y0) / (y1 - y0)
        if log_x and x0 > 0.0 and x1 > 0.0:
            lo, hi = math.log10(x0), math.log10(x1)
            found.append(10.0 ** (lo + (hi - lo) * frac))
        else:
            found.append(x0 + (x1 - x0) * frac)
    return found


def _crossing_time(x_values: Sequence[float], y_values: Sequence[float], threshold: float, rising: bool) -> float:
    hits = crossings(x_values, y_values, threshold, rising=rising)
    if not hits:
        raise ValueError(f"waveform never crossed {threshold}")
    return hits[0]


def frequency_from_crossings(
    time: Sequence[float],
    values: Sequence[float],
    threshold: float | None = None,
    skip: float = 0.0,
) -> float:
    """Mean frequency from rising-edge crossings, ignoring the first ``skip`` seconds.

    Averaging over the whole edge list rather than one period is what makes this
    robust: a single period inherits the timestep quantisation of its two edges.
    """
    t = [float(x) for x in time]
    v = [float(x) for x in values]
    if threshold is None:
        threshold = (max(v) + min(v)) / 2.0
    edges = [x for x in crossings(t, v, threshold, rising=True) if x >= skip]
    if len(edges) < 2:
        raise ValueError(
            f"need at least 2 rising crossings of {threshold:g} after {skip:g}s, got {len(edges)}"
        )
    return (len(edges) - 1) / (edges[-1] - edges[0])


# ── AC-domain extractors ──
#
# These need magnitude and phase, which `ac[node]` does not provide (it is the
# real part). They go through `ac.magnitude()` / `ac.phase()`.

def _ac_db(ac: Any, node: str) -> tuple[list[float], list[float]]:
    return [float(f) for f in ac.frequency], list(ac.magnitude_db(node))


def ac_gain_db(ac: Any, node: str, at_hz: float) -> float:
    """Gain in dB at one frequency (log-interpolated between sweep points)."""
    freq, db = _ac_db(ac, node)
    lo = [math.log10(f) for f in freq]
    return _interp_at(lo, db, math.log10(at_hz))


def ac_passband_db(ac: Any, node: str) -> float:
    """Reference gain: the passband maximum.

    Referencing to the *peak* rather than to DC is what makes this correct for
    Chebyshev and bandpass responses, where the DC value is not the passband.
    """
    _, db = _ac_db(ac, node)
    return max(db)


#: The half-power point, 10*log10(2) = 3.0103 dB -- not 3.000 dB.
#: Using a round 3.0 puts the corner of a single-pole response 0.23% low,
#: a systematic error that does not shrink with sweep density.
HALF_POWER_DB = 10.0 * math.log10(2.0)


def ac_bandwidth_hz(ac: Any, node: str, drop_db: float = HALF_POWER_DB,
                    reference_db: float | None = None) -> float:
    """Frequency where the response falls ``drop_db`` below the passband.

    Defaults to the half-power point (3.0103 dB), which is what "-3 dB
    bandwidth" means. Pass ``drop_db=3.0`` for the literal-3 dB convention.
    """
    freq, db = _ac_db(ac, node)
    ref = ac_passband_db(ac, node) if reference_db is None else reference_db
    peak_idx = db.index(max(db)) if reference_db is None else 0
    hits = crossings(freq[peak_idx:], db[peak_idx:], ref - drop_db,
                     rising=False, log_x=True)
    if not hits:
        raise ValueError(
            f"'{node}' never falls {drop_db} dB below its passband "
            f"({ref:.3f} dB) within the swept range"
        )
    return hits[0]


def ac_unity_gain_hz(ac: Any, node: str) -> float:
    """Frequency where the magnitude crosses 0 dB going down."""
    freq, db = _ac_db(ac, node)
    hits = crossings(freq, db, 0.0, rising=False, log_x=True)
    if not hits:
        raise ValueError(f"'{node}' never crosses 0 dB within the swept range")
    return hits[0]


def ac_phase_margin_deg(ac: Any, node: str) -> float:
    """Phase margin = 180 + phase at the unity-gain crossing.

    Only meaningful on an open-loop response. On a closed-loop response this
    number is not a stability margin.
    """
    f_unity = ac_unity_gain_hz(ac, node)
    freq = [float(f) for f in ac.frequency]
    lo = [math.log10(f) for f in freq]
    phase = _interp_at(lo, list(ac.phase(node)), math.log10(f_unity))
    return 180.0 + phase


def extract_metrics(result: Any, specs: Sequence[MetricSpec]) -> dict[str, float]:
    metrics: dict[str, float] = {}
    for spec in specs:
        values = _as_series(_get_signal(result, spec.source))
        reducer = spec.reducer.lower()

        if reducer in {"first", "initial"}:
            metrics[spec.name] = values[0]
        elif reducer in {"last", "final"}:
            metrics[spec.name] = values[-1]
        elif reducer == "min":
            metrics[spec.name] = min(values)
        elif reducer == "max":
            metrics[spec.name] = max(values)
        elif reducer == "mean":
            metrics[spec.name] = sum(values) / len(values)
        elif reducer == "abs_max":
            metrics[spec.name] = max(abs(value) for value in values)
        elif reducer in {"peak_to_peak", "pp"}:
            metrics[spec.name] = max(values) - min(values)
        elif reducer == "at":
            if spec.at is None:
                raise ValueError(f"MetricSpec '{spec.name}' requires at=")
            if isinstance(result, Mapping):
                axis_name = "time" if "time" in result else "frequency"
            else:
                axis_name = "time" if hasattr(result, "time") else "frequency"
            axis = _as_series(_get_signal(result, axis_name))
            metrics[spec.name] = _interp_at(axis, values, spec.at)
        elif reducer == "crossing_time":
            if spec.threshold is None:
                raise ValueError(f"MetricSpec '{spec.name}' requires threshold=")
            axis = _as_series(_get_signal(result, "time"))
            metrics[spec.name] = _crossing_time(axis, values, spec.threshold, spec.rising)
        else:
            raise ValueError(f"unknown metric reducer '{spec.reducer}'")
    return metrics


@dataclass(frozen=True)
class ValidationRun:
    name: str
    metrics: Mapping[str, Any]
    report: ValidationReport
    metadata: Mapping[str, Any] = field(default_factory=dict)

    @property
    def passed(self) -> bool:
        return self.report.passed


@dataclass(frozen=True)
class MetricStats:
    count: int
    minimum: float
    maximum: float
    mean: float
    sigma: float


@dataclass(frozen=True)
class YieldSummary:
    runs: list[ValidationRun]

    @property
    def total(self) -> int:
        return len(self.runs)

    @property
    def passed(self) -> int:
        return sum(1 for run in self.runs if run.passed)

    @property
    def failed(self) -> int:
        return self.total - self.passed

    @property
    def pass_rate(self) -> float:
        return self.passed / self.total if self.total else 0.0

    @property
    def failures(self) -> list[ValidationRun]:
        return [run for run in self.runs if not run.passed]

    def metric_stats(self, field: str) -> MetricStats:
        values = [float(run.metrics[field]) for run in self.runs if field in run.metrics]
        if not values:
            raise KeyError(f"metric '{field}' not found in any run")
        mean = sum(values) / len(values)
        sigma = sqrt(sum((value - mean) ** 2 for value in values) / len(values))
        return MetricStats(len(values), min(values), max(values), mean, sigma)


def evaluate_metric_sets(
    metric_sets: Sequence[Mapping[str, Any]],
    rules: Sequence[ValidationRule],
    names: Sequence[str] | None = None,
) -> YieldSummary:
    runs: list[ValidationRun] = []
    for idx, metrics in enumerate(metric_sets):
        name = names[idx] if names is not None else f"run_{idx}"
        runs.append(ValidationRun(name, metrics, validate_metrics(metrics, rules)))
    return YieldSummary(runs)


def evaluate_corners(
    factory: Callable[[], Any],
    corners: Sequence["CornerCase"],
    rules: Sequence[ValidationRule],
    metric_extractor: Callable[[Any], Mapping[str, Any]],
    backend: str = "ngspice",
    runner: Callable[[Any], Any] | None = None,
) -> YieldSummary:
    runs: list[ValidationRun] = []
    for corner in corners:
        bench = corner.apply_to(factory(), backend)
        result_source = runner(bench) if runner is not None else bench
        metrics = metric_extractor(result_source)
        runs.append(
            ValidationRun(
                corner.name,
                metrics,
                validate_metrics(metrics, rules),
                metadata={"backend": corner.backend or backend},
            )
        )
    return YieldSummary(runs)


_NUMERIC_RE = re.compile(r"^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$")


def _parse_float(value: str) -> float | None:
    cleaned = value.strip().strip(",;")
    if not cleaned or cleaned.lower() in {"nan", "failed"}:
        return None
    if _NUMERIC_RE.match(cleaned):
        return float(cleaned)
    return None


def _coerce_metric_rows(rows: Sequence[Mapping[str, str]]) -> list[dict[str, float]]:
    metric_rows: list[dict[str, float]] = []
    for row in rows:
        metrics: dict[str, float] = {}
        for key, value in row.items():
            if key is None:
                continue
            number = _parse_float(str(value))
            if number is not None:
                metrics[key.strip()] = number
        if metrics:
            metric_rows.append(metrics)
    return metric_rows


def _parse_delimited_metric_rows(text: str) -> list[dict[str, float]]:
    lines = [
        line.strip()
        for line in text.splitlines()
        if line.strip() and not line.lstrip().startswith(("#", "*", "//"))
    ]
    if not lines:
        return []
    if lines[0].lower().startswith(".measure") or lines[0].lower().startswith("measure"):
        return []

    sample = "\n".join(lines[:10])
    try:
        dialect = csv.Sniffer().sniff(sample, delimiters=",\t;")
        reader = csv.DictReader(lines, dialect=dialect)
        rows = _coerce_metric_rows(list(reader))
        if rows:
            return rows
    except csv.Error:
        pass

    header = re.split(r"\s+", lines[0].strip())
    if len(header) < 2:
        return []
    if any(_parse_float(column) is not None for column in header):
        return []
    table_rows: list[dict[str, str]] = []
    for line in lines[1:]:
        parts = re.split(r"\s+", line.strip())
        if len(parts) != len(header):
            continue
        if not any(_parse_float(part) is not None for part in parts):
            continue
        table_rows.append(dict(zip(header, parts)))
    return _coerce_metric_rows(table_rows)


def _parse_numeric_matrix(text: str) -> list[list[float]]:
    rows: list[list[float]] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith(("#", "*", "//")):
            continue
        parts = [part for part in re.split(r"[\s,;]+", stripped) if part]
        values = [_parse_float(part) for part in parts]
        if not values or any(value is None for value in values):
            return []
        rows.append([float(value) for value in values if value is not None])
    if not rows:
        return []
    width = len(rows[0])
    if width == 0 or any(len(row) != width for row in rows):
        return []
    return rows


def _spectre_mcparam_candidates(path: Path) -> list[Path]:
    name = path.name
    lower = name.lower()
    candidates: list[Path] = []

    def add(candidate: Path) -> None:
        if candidate not in candidates:
            candidates.append(candidate)

    if path.suffix.lower() == ".mcdata":
        add(path.with_suffix(".mcparam"))
    if lower.endswith("mcdata"):
        add(path.with_name(f"{name[:-len('mcdata')]}mcparam"))
    if lower.endswith("data"):
        add(path.with_name(f"{name[:-len('data')]}param"))
        add(path.with_name(f"{name[:-len('data')]}Param"))
    if lower == "mcdata":
        add(path.with_name("mcparam"))
    if lower == "processdata":
        add(path.with_name("processParam"))
        add(path.with_name("processparam"))
    if lower == "mismatchdata":
        add(path.with_name("mismatchparam"))
        add(path.with_name("mismatchParam"))

    return candidates


def _spectre_metric_names_from_paramfile(path: Path, expected: int) -> list[str]:
    ignored = {
        "column",
        "columns",
        "expr",
        "expression",
        "expressions",
        "index",
        "run",
        "sweep",
        "title",
        "titles",
    }
    for candidate in _spectre_mcparam_candidates(path):
        if not candidate.exists():
            continue
        names: list[str] = []
        for line in candidate.read_text().splitlines():
            stripped = line.strip()
            if not stripped or stripped.startswith(("#", "*", "//")):
                continue
            tokens = re.findall(r"[A-Za-z_]\w*", stripped)
            if len(tokens) == expected and not any(token.lower() in ignored for token in tokens):
                return tokens
            if tokens and tokens[0].lower() not in ignored:
                names.append(tokens[0])
        if len(names) >= expected:
            return names[:expected]
    return []


def _parse_spectre_mcdata_file(path: Path) -> list[dict[str, float]]:
    matrix = _parse_numeric_matrix(path.read_text())
    if not matrix:
        return []

    column_count = len(matrix[0])
    names = _spectre_metric_names_from_paramfile(path, column_count)
    has_iteration_column = False
    if not names:
        names = _spectre_metric_names_from_paramfile(path, column_count - 1)
        has_iteration_column = bool(names)
    if not names:
        names = [f"value_{index + 1}" for index in range(column_count)]

    rows: list[dict[str, float]] = []
    for index, values in enumerate(matrix, start=1):
        row: dict[str, float] = {}
        metric_values = values
        if has_iteration_column:
            row["iteration"] = values[0]
            metric_values = values[1:]
        elif "iteration" not in {name.lower() for name in names}:
            row["iteration"] = float(index)
        row.update(dict(zip(names, metric_values)))
        rows.append(row)
    return rows


def _parse_measure_blocks(text: str) -> list[dict[str, float]]:
    rows: list[dict[str, float]] = []
    current: dict[str, float] = {}

    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            continue

        measure = re.search(
            r"(?:\.?measure\s+\S+\s+)?([A-Za-z_]\w*)\s*=\s*"
            r"([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?)",
            stripped,
            flags=re.IGNORECASE,
        )
        if not measure:
            continue

        name = measure.group(1)
        value = float(measure.group(2))
        if name in current:
            rows.append(current)
            current = {}
        current[name] = value

    if current:
        rows.append(current)
    return rows


def parse_metric_rows(text: str, backend: str = "auto") -> list[dict[str, float]]:
    rows = _parse_delimited_metric_rows(text)
    if rows:
        return rows
    return _parse_measure_blocks(text)


def load_metric_rows(path: str | Path, backend: str = "auto") -> list[dict[str, float]]:
    metric_path = Path(path)
    if backend.lower() == "spectre" and _is_spectre_mcdata_file(metric_path):
        rows = _parse_spectre_mcdata_file(metric_path)
        if rows:
            return rows
    return parse_metric_rows(metric_path.read_text(), backend)


def _is_spectre_mcdata_file(path: Path) -> bool:
    name = path.name.lower()
    return name in {"mcdata", "processdata", "mismatchdata"} or name.endswith(".mcdata")


def _is_spectre_metric_file(path: Path) -> bool:
    if _is_spectre_mcdata_file(path):
        return True
    suffix = path.suffix.lower()
    if suffix in {".measure", ".measurement"}:
        return True
    if suffix == ".dat":
        stem = path.stem.lower()
        return any(token in stem for token in ("mc", "measure", "metric", "result", "scalar"))
    return False


def find_metric_files(path: str | Path, backend: str = "auto") -> list[Path]:
    root = Path(path)
    if root.is_file():
        return [root]

    backend_lower = backend.lower()
    suffixes = {".csv", ".tsv", ".txt", ".log", ".mt0", ".ms0", ".ma0"}
    if backend_lower == "spectre":
        suffixes |= {".measure", ".measurement", ".mcdata"}

    files = [
        candidate
        for candidate in sorted(root.rglob("*"))
        if candidate.is_file()
        and (
            candidate.suffix.lower() in suffixes
            or (backend_lower == "spectre" and _is_spectre_metric_file(candidate))
        )
    ]
    return files


def load_monte_carlo_metrics(path: str | Path, backend: str = "auto") -> list[dict[str, float]]:
    rows: list[dict[str, float]] = []
    for metric_file in find_metric_files(path, backend):
        rows.extend(load_metric_rows(metric_file, backend))
    return rows


def evaluate_result_text(
    text: str,
    rules: Sequence[ValidationRule],
    backend: str = "auto",
    names: Sequence[str] | None = None,
) -> YieldSummary:
    return evaluate_metric_sets(parse_metric_rows(text, backend), rules, names)


def evaluate_result_file(
    path: str | Path,
    rules: Sequence[ValidationRule],
    backend: str = "auto",
    names: Sequence[str] | None = None,
) -> YieldSummary:
    return evaluate_metric_sets(load_metric_rows(path, backend), rules, names)


def evaluate_monte_carlo_file(
    path: str | Path,
    rules: Sequence[ValidationRule],
    backend: str = "auto",
    names: Sequence[str] | None = None,
) -> YieldSummary:
    return evaluate_metric_sets(load_monte_carlo_metrics(path, backend), rules, names)


@dataclass(frozen=True)
class CornerCase:
    name: str
    backend: str | None = None
    temperature: float | None = None
    nominal_temperature: float | None = None
    parameters: Mapping[str, Any] = field(default_factory=dict)
    model_libraries: Sequence[Any] = field(default_factory=tuple)

    def apply_to(self, bench_or_testbench: Any, backend: str = "ngspice") -> Any:
        tb = _testbench(bench_or_testbench)
        selected_backend = self.backend or backend
        if hasattr(tb, "with_backend"):
            tb.with_backend(selected_backend)
        if self.temperature is not None:
            tb.temperature = self.temperature
        if self.nominal_temperature is not None:
            tb.nominal_temperature = self.nominal_temperature
        for model_library in self.model_libraries:
            tb.use_pdk(model_library)
        for key, value in self.parameters.items():
            if selected_backend == "spectre":
                tb.extra_line(f"parameters {key}={value}")
            else:
                tb.extra_line(f".param {key}={value}")
        return bench_or_testbench


def corner_netlists(
    factory: Callable[[], Any],
    corners: Sequence[CornerCase],
    backend: str = "ngspice",
) -> dict[str, str]:
    netlists: dict[str, str] = {}
    for corner in corners:
        bench = corner.apply_to(factory(), backend)
        selected_backend = corner.backend or backend
        netlists[corner.name] = _testbench(bench).netlist(selected_backend)
    return netlists


@dataclass(frozen=True)
class MonteCarloPlan:
    """Statistical sampling plan. Spectre is the only backend with a
    Monte Carlo statement SpiceRack can emit."""

    backend: str = "spectre"
    samples: int = 100
    distributions: Mapping[str, str] = field(default_factory=dict)
    spectre_inner: str = "tran1"
    spectre_inner_type: str = "tran"
    seed: int | None = None

    def apply_to(self, bench_or_testbench: Any) -> Any:
        tb = _testbench(bench_or_testbench)
        if hasattr(tb, "with_backend"):
            tb.with_backend(self.backend)

        if self.backend.lower() != "spectre":
            raise ValueError(
                f"MonteCarloPlan supports backend='spectre'; got {self.backend!r}"
            )
        tb.add_spectre_monte_carlo(
            self.samples,
            self.spectre_inner,
            self.spectre_inner_type,
            self.seed,
        )
        return bench_or_testbench


def monte_carlo_netlist(bench_or_testbench: Any, plan: MonteCarloPlan) -> str:
    plan.apply_to(bench_or_testbench)
    return _testbench(bench_or_testbench).netlist(plan.backend)
