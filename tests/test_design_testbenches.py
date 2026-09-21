import json
from pathlib import Path

import pytest

from spicerack.testbenches import (
    CornerCase,
    MetricSpec,
    MonteCarloPlan,
    ValidationRule,
    adc_ramp,
    amplifier_voltage_gain,
    bandgap_reference,
    bandgap_tempco,
    charge_amplifier,
    dac_static_linearity,
    pll_lock,
    sample_hold,
    switch_characterization,
    corner_netlists,
    evaluate_corners,
    evaluate_metric_sets,
    evaluate_monte_carlo_file,
    evaluate_result_file,
    evaluate_result_text,
    extract_metrics,
    find_metric_files,
    load_metric_rows,
    load_monte_carlo_metrics,
    monte_carlo_netlist,
    parse_metric_rows,
    validate_metrics,
)


FIXTURES = Path(__file__).resolve().parent / "fixtures"


def ps():
    try:
        import spicerack
        return spicerack
    except ImportError:
        pytest.skip("spicerack not built")


def dut(name, ports):
    mod = ps()
    sc = mod.Subcircuit(name, ports)
    for idx, port in enumerate(ports):
        sc.R(name=f"bias{idx}", positive=port, negative=sc.gnd, value=1e6)
    return sc


def test_design_benches_emit_ngspice_netlists():
    mod = ps()
    benches = [
        amplifier_voltage_gain(mod, dut("amp", ["vin", "vout"])),
        charge_amplifier(mod, dut("charge", ["qin", "vout"])),
        dac_static_linearity(mod, dut("dac", ["b0", "b1", "vout"]), code_nodes=["b0", "b1"]),
        adc_ramp(mod, dut("adc", ["vin", "clk", "d0"]), output_nodes=["d0"]),
        switch_characterization(mod, dut("sw", ["vin", "vout", "ctrl"])),
        sample_hold(mod, dut("sh", ["vin", "vhold", "phi"])),
        pll_lock(mod, dut("pll", ["ref", "vco", "vctrl"])),
        bandgap_reference(mod, dut("bg", ["vdd", "vref"])),
    ]

    for bench in benches:
        netlist = bench.netlist("ngspice")
        assert netlist.startswith("* ")
        assert ".end" in netlist
        assert bench.measurements
        assert bench.validation


def test_design_benches_serialize_multi_analysis_ir():
    mod = ps()
    bench = amplifier_voltage_gain(mod, dut("amp_json", ["vin", "vout"]))
    data = json.loads(bench.testbench.to_json())

    analyses = data["testbench"]["analyses"]
    assert [analysis["type"] for analysis in analyses] == ["Op", "Ac"]
    assert data["testbench"]["measures"]
    assert data["testbench"]["saves"]


def test_current_waveform_and_spectre_step_support_for_charge_and_bandgap():
    mod = ps()
    charge = charge_amplifier(mod, dut("charge_json", ["qin", "vout"]))
    charge_data = json.loads(charge.testbench.to_json())
    stimulus = charge_data["testbench"]["stimulus"][0]
    assert stimulus["type"] == "CurrentSource"
    assert stimulus["waveform"]["type"] == "Pulse"

    bg = bandgap_reference(mod, dut("bg_spectre", ["vdd", "vref"]))
    spectre = bg.netlist("spectre")
    assert "mytnom options tnom=27" in spectre
    assert "dc1 dc" in spectre

    # Temperature drift rides on `.dc temp`, not `.step param temp`: the ngspice
    # codegen comments the step card out, so a step-based sweep would silently
    # run at one temperature.
    tc = bandgap_tempco(mod, dut("bg_tc", ["vdd", "vref"]))
    assert "dc temp -40 125 5" in tc.netlist("ngspice")
    assert "dc1 dc param=temp" in tc.netlist("spectre")


def test_validation_rules_report_failures_and_successes():
    report = validate_metrics(
        {"gain_db": 39.8, "vref": 1.32},
        [
            ValidationRule("gain", "gain_db", expected=40.0, tolerance=0.5),
            ValidationRule("reference", "vref", minimum=1.18, maximum=1.25),
        ],
    )

    assert not report.passed
    assert [result.name for result in report.failures] == ["reference"]
    assert "above 1.25" in report.failures[0].message


def test_extract_metrics_reduces_waveforms_and_measures():
    metrics = extract_metrics(
        {
            "time": [0.0, 1e-9, 2e-9, 3e-9],
            "vout": [0.0, 0.4, 0.9, 1.0],
            "gain_meas": 39.7,
        },
        [
            MetricSpec("vout_final", "vout", "final"),
            MetricSpec("vout_pp", "vout", "peak_to_peak"),
            MetricSpec("vout_1p5ns", "vout", "at", at=1.5e-9),
            MetricSpec("rise_0p5v", "vout", "crossing_time", threshold=0.5),
            MetricSpec("gain_db", "gain_meas"),
        ],
    )

    assert metrics["vout_final"] == 1.0
    assert metrics["vout_pp"] == 1.0
    assert metrics["vout_1p5ns"] == pytest.approx(0.65)
    assert metrics["rise_0p5v"] == pytest.approx(1.2e-9)
    assert metrics["gain_db"] == 39.7


def test_evaluate_metric_sets_summarizes_yield_and_stats():
    summary = evaluate_metric_sets(
        [
            {"gain_db": 40.0, "vref": 1.20},
            {"gain_db": 39.7, "vref": 1.22},
            {"gain_db": 36.0, "vref": 1.30},
        ],
        [
            ValidationRule("gain", "gain_db", minimum=38.0),
            ValidationRule("reference", "vref", minimum=1.18, maximum=1.25),
        ],
        names=["tt", "ff", "ss"],
    )

    assert summary.total == 3
    assert summary.passed == 2
    assert summary.pass_rate == pytest.approx(2 / 3)
    assert [run.name for run in summary.failures] == ["ss"]
    assert summary.metric_stats("gain_db").mean == pytest.approx((40.0 + 39.7 + 36.0) / 3)


def test_evaluate_result_file_validates_backend_metric_table(tmp_path):
    output = tmp_path / "spectre_mc.csv"
    output.write_text("iteration,gain_db,vref\n1,40.0,1.205\n2,39.0,1.210\n3,36.0,1.300\n")

    summary = evaluate_result_file(
        output,
        [
            ValidationRule("gain", "gain_db", minimum=38.0),
            ValidationRule("reference", "vref", minimum=1.18, maximum=1.23),
        ],
        backend="spectre",
    )

    assert summary.total == 3
    assert summary.passed == 2
    assert summary.failures[0].metrics["iteration"] == 3.0


def test_spectre_monte_carlo_fixture_finds_scalar_mcdata_without_waveforms():
    output_dir = FIXTURES / "spectre_mc_scalar"

    files = find_metric_files(output_dir, backend="spectre")

    assert files == [output_dir / "monteCarlo" / "mcdata"]
    assert output_dir / "psf" / "tran.tran" not in files
    assert output_dir / "input.raw" / "tran.raw" not in files


def test_spectre_mcdata_fixture_uses_mcparam_column_names():
    output_dir = FIXTURES / "spectre_mc_scalar"

    rows = load_monte_carlo_metrics(output_dir, backend="spectre")

    assert rows == [
        {"iteration": 1.0, "gain_db": 40.1, "vref": 1.204},
        {"iteration": 2.0, "gain_db": 39.4, "vref": 1.211},
        {"iteration": 3.0, "gain_db": 36.8, "vref": 1.287},
    ]

    summary = evaluate_monte_carlo_file(
        output_dir,
        [
            ValidationRule("gain", "gain_db", minimum=38.0),
            ValidationRule("reference", "vref", minimum=1.18, maximum=1.23),
        ],
        backend="spectre",
    )
    assert summary.total == 3
    assert summary.passed == 2


def test_spectre_default_instance_mcdata_uses_sibling_mcparam(tmp_path):
    scalar = tmp_path / "mc1.mcdata"
    scalar.write_text("40.1 1.204\n39.4 1.211\n")
    (tmp_path / "mc1.mcparam").write_text(
        "gain_db oceanEval(\"db20(v(\\\"vout\\\")/v(\\\"vin\\\"))\")\n"
        "vref oceanEval(\"value(v(\\\"vref\\\") 1u)\")\n"
    )

    files = find_metric_files(tmp_path, backend="spectre")
    rows = load_metric_rows(scalar, backend="spectre")

    assert files == [scalar]
    assert rows == [
        {"iteration": 1.0, "gain_db": 40.1, "vref": 1.204},
        {"iteration": 2.0, "gain_db": 39.4, "vref": 1.211},
    ]


def test_spectre_discovery_includes_named_dat_scalar_exports(tmp_path):
    output_dir = tmp_path / "spectre_run"
    output_dir.mkdir()
    scalar = output_dir / "mc_results.dat"
    scalar.write_text("gain_db vref\n40.1 1.204\n39.4 1.211\n")
    (output_dir / "waveform.dat").write_text("large waveform data")

    files = find_metric_files(output_dir, backend="spectre")

    assert files == [scalar]


def test_corner_netlists_apply_temperature_and_parameters():
    mod = ps()

    netlists = corner_netlists(
        lambda: bandgap_reference(mod, dut("bg_corner", ["vdd", "vref"])),
        [
            CornerCase("tt_27", temperature=27, parameters={"vdd_nom": 3.3}),
            CornerCase("ff_cold", temperature=-40, parameters={"vdd_nom": 3.6}),
        ],
    )

    assert set(netlists) == {"tt_27", "ff_cold"}
    assert ".temp 27" in netlists["tt_27"]
    assert ".param vdd_nom=3.3" in netlists["tt_27"]
    assert ".temp -40" in netlists["ff_cold"]
    assert ".param vdd_nom=3.6" in netlists["ff_cold"]


def test_evaluate_corners_connects_corner_setup_to_validation():
    mod = ps()
    corners = [
        CornerCase("tt_27", temperature=27, parameters={"vdd_nom": 3.3}),
        CornerCase("ss_hot", temperature=125, parameters={"vdd_nom": 3.0}),
    ]

    summary = evaluate_corners(
        lambda: bandgap_reference(mod, dut("bg_eval", ["vdd", "vref"])),
        corners,
        [ValidationRule("reference", "vref", minimum=1.18, maximum=1.25)],
        lambda bench: {"vref": 1.21 if ".temp 27" in bench.netlist("ngspice") else 1.31},
    )

    assert summary.total == 2
    assert summary.pass_rate == pytest.approx(0.5)
    assert summary.failures[0].name == "ss_hot"

