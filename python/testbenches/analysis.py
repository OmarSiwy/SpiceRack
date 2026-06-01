"""Validation, corner, and statistical-analysis helpers for design benches."""

from __future__ import annotations

import csv
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


def _crossing_time(x_values: Sequence[float], y_values: Sequence[float], threshold: float, rising: bool) -> float:
    for idx in range(1, min(len(x_values), len(y_values))):
        y0 = float(y_values[idx - 1])
        y1 = float(y_values[idx])
        crossed = y0 <= threshold <= y1 if rising else y0 >= threshold >= y1
        if crossed:
            x0 = float(x_values[idx - 1])
            x1 = float(x_values[idx])
            if y1 == y0:
                return x1
            return x0 + (x1 - x0) * ((threshold - y0) / (y1 - y0))
    raise ValueError(f"waveform never crossed {threshold}")


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
    return parse_metric_rows(Path(path).read_text(), backend)


def find_metric_files(path: str | Path, backend: str = "auto") -> list[Path]:
    root = Path(path)
    if root.is_file():
        return [root]

    backend_lower = backend.lower()
    suffixes = {".csv", ".tsv", ".txt", ".log", ".mt0", ".ms0", ".ma0"}
    if backend_lower.startswith("xyce"):
        suffixes |= {".prn", ".res"}
    if backend_lower == "spectre":
        suffixes |= {".measure", ".measurement"}

    files = [
        candidate
        for candidate in sorted(root.rglob("*"))
        if candidate.is_file() and candidate.suffix.lower() in suffixes
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
    backend: str = "xyce"
    samples: int = 100
    distributions: Mapping[str, str] = field(default_factory=dict)
    mode: str = "sampling"
    pce_order: int = 2
    spectre_inner: str = "tran1"
    spectre_inner_type: str = "tran"
    seed: int | None = None

    def apply_to(self, bench_or_testbench: Any) -> Any:
        tb = _testbench(bench_or_testbench)
        if hasattr(tb, "with_backend"):
            tb.with_backend(self.backend)

        backend = self.backend.lower()
        mode = self.mode.lower()
        if backend == "spectre":
            tb.add_spectre_monte_carlo(
                self.samples,
                self.spectre_inner,
                self.spectre_inner_type,
                self.seed,
            )
        elif mode == "embedded":
            tb.add_xyce_embedded_sampling(self.samples, dict(self.distributions))
        elif mode == "pce":
            tb.add_xyce_pce(self.samples, dict(self.distributions), self.pce_order)
        else:
            tb.add_xyce_sampling(self.samples, dict(self.distributions))
        return bench_or_testbench


def monte_carlo_netlist(bench_or_testbench: Any, plan: MonteCarloPlan) -> str:
    plan.apply_to(bench_or_testbench)
    return _testbench(bench_or_testbench).netlist(plan.backend)
