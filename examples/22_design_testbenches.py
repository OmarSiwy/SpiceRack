"""
Example 22: Reusable Design Testbenches

Builds reusable testbench recipes for common analog and mixed-signal blocks.
This example validates generated netlists only; it does not require a simulator
backend to be installed.
"""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
PYTHON_SRC = ROOT / "python"
for path in (PYTHON_SRC, ROOT):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))

import pyspice_rs as ps
from testbenches import (
    CornerCase,
    MetricSpec,
    MonteCarloPlan,
    ValidationRule,
    adc_ramp,
    amplifier_current_gain,
    amplifier_transimpedance,
    amplifier_voltage_gain,
    bandgap_reference,
    charge_amplifier,
    dac_static_linearity,
    demux_routing,
    mux_routing,
    pll_lock,
    sample_hold,
    switch_characterization,
    corner_netlists,
    evaluate_metric_sets,
    evaluate_result_text,
    extract_metrics,
    monte_carlo_netlist,
    validate_metrics,
)


def make_dut(name, ports):
    dut = ps.Subcircuit(name, ports)
    for idx, port in enumerate(ports):
        if port != "0":
            dut.R(name=f"bias{idx}", positive=port, negative=dut.gnd, value=1e6)
    return dut


def main():
    benches = [
        amplifier_voltage_gain(ps, make_dut("vamp", ["vin", "vout"]), load_resistance=10e3),
        amplifier_current_gain(ps, make_dut("iamp", ["iin", "iout", "out_sense_n"])),
        amplifier_transimpedance(ps, make_dut("tia", ["iin", "vout"])),
        charge_amplifier(ps, make_dut("charge_amp", ["qin", "vout"])),
        dac_static_linearity(ps, make_dut("dac", ["b0", "b1", "b2", "vout"]), code_nodes=["b0", "b1", "b2"]),
        adc_ramp(ps, make_dut("adc", ["vin", "clk", "d0", "d1"]), output_nodes=["d0", "d1"]),
        switch_characterization(ps, make_dut("sw", ["vin", "vout", "ctrl"])),
        mux_routing(ps, make_dut("mux", ["in0", "in1", "vout", "sel"]), input_nodes=["in0", "in1"]),
        demux_routing(ps, make_dut("demux", ["vin", "out0", "out1", "sel"]), output_nodes=["out0", "out1"]),
        sample_hold(ps, make_dut("sh", ["vin", "vhold", "phi"])),
        pll_lock(ps, make_dut("pll", ["ref", "vco", "vctrl"])),
        bandgap_reference(ps, make_dut("bandgap", ["vdd", "vref"])),
    ]

    for bench in benches:
        netlist = bench.netlist("ngspice")
        assert bench.category
        assert ".end" in netlist
        assert bench.result_fields
        assert bench.validation
        print(f"{bench.category:12s} {bench.name:24s} analyses={len(bench.result_fields)}")

    spectre_bandgap = benches[-1].netlist("spectre")
    assert "mytnom options tnom=27" in spectre_bandgap
    assert "sweep" in spectre_bandgap

    corners = [
        CornerCase("tt_27", temperature=27, parameters={"vdd_nom": 3.3}),
        CornerCase("ss_hot", temperature=125, parameters={"vdd_nom": 3.0}),
    ]
    cornered = corner_netlists(
        lambda: bandgap_reference(ps, make_dut("bandgap_corner", ["vdd", "vref"])),
        corners,
    )
    assert set(cornered) == {"tt_27", "ss_hot"}
    assert all(".temp" in netlist and ".param vdd_nom=" in netlist for netlist in cornered.values())

    mc_netlist = monte_carlo_netlist(
        amplifier_voltage_gain(ps, make_dut("mc_amp", ["vin", "vout"])),
        MonteCarloPlan(samples=16, distributions={"Rload": "normal(1000,50)"}),
    )
    assert ".SAMPLING" in mc_netlist

    report = validate_metrics(
        {"vref": 1.205, "gain_db": 40.0},
        [
            ValidationRule("reference window", "vref", minimum=1.18, maximum=1.23),
            ValidationRule("gain target", "gain_db", expected=40.0, tolerance=0.5),
        ],
    )
    assert report.passed

    waveform_metrics = extract_metrics(
        {"time": [0.0, 1e-9, 2e-9], "vout": [0.0, 0.8, 1.2]},
        [
            MetricSpec("vout_final", "vout", "final"),
            MetricSpec("rise_0p6v", "vout", "crossing_time", threshold=0.6),
        ],
    )
    assert waveform_metrics["vout_final"] == 1.2

    yield_summary = evaluate_metric_sets(
        [{"vref": 1.205}, {"vref": 1.215}, {"vref": 1.260}],
        [ValidationRule("reference window", "vref", minimum=1.18, maximum=1.23)],
        names=["tt", "ff", "ss"],
    )
    assert yield_summary.pass_rate == 2 / 3

    parsed_summary = evaluate_result_text(
        ".MEASURE TRAN vref = 1.205\n.MEASURE TRAN vref = 1.260\n",
        [ValidationRule("reference window", "vref", minimum=1.18, maximum=1.23)],
        backend="xyce",
    )
    assert parsed_summary.pass_rate == 0.5

    print(f"\nValidated {len(benches)} reusable design testbench recipes.")


if __name__ == "__main__":
    main()
