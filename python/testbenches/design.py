"""Reusable testbench recipes for common analog and mixed-signal blocks.

The functions in this module intentionally build ordinary ``pyspice_rs.Testbench``
objects. They are small recipes, not a second simulation framework.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class DesignBench:
    """A generated testbench plus the validation intent attached to it."""

    name: str
    category: str
    testbench: Any
    intent: str
    measurements: list[str] = field(default_factory=list)
    result_fields: list[str] = field(default_factory=list)
    validation: list[str] = field(default_factory=list)

    def netlist(self, backend: str = "ngspice") -> str:
        return self.testbench.netlist(backend)


def _save(tb: Any, *signals: str) -> None:
    if signals:
        tb.save(*signals)


def amplifier_voltage_gain(
    ps: Any,
    dut: Any,
    *,
    input_node: str = "vin",
    output_node: str = "vout",
    reference_node: str = "0",
    source_name: str = "in",
    load_resistance: float | None = None,
    ac_magnitude: float = 1.0,
    start_frequency: float = 1.0,
    stop_frequency: float = 1e9,
) -> DesignBench:
    tb = ps.Testbench(dut)
    tb.V(name=source_name, positive=input_node, negative=reference_node, value=0.0, ac=ac_magnitude)
    if load_resistance is not None:
        tb.R(name="load", positive=output_node, negative=reference_node, value=load_resistance)
    _save(tb, f"V({input_node})", f"V({output_node})")
    tb.add_operating_point()
    tb.add_ac(variation="dec", number_of_points=100, start_frequency=start_frequency, stop_frequency=stop_frequency)
    tb.measure("ac", "gain_midband", "find", f"vdb({output_node})", "at=1k")
    return DesignBench(
        name="voltage_gain",
        category="amplifier",
        testbench=tb,
        intent="Voltage-domain gain, bias, bandwidth, and load sensitivity.",
        measurements=["gain_midband", "bandwidth", "phase_margin_proxy"],
        result_fields=["op[node]", "ac.frequency", f"ac['{output_node}']"],
        validation=["output bias in range", "midband gain in budget", "bandwidth above target"],
    )


def amplifier_current_gain(
    ps: Any,
    dut: Any,
    *,
    input_node: str = "iin",
    output_node: str = "iout",
    reference_node: str = "0",
    source_name: str = "in",
    ac_magnitude: float = 1e-6,
) -> DesignBench:
    tb = ps.Testbench(dut)
    tb.I(name=source_name, positive=input_node, negative=reference_node, value=0.0, ac=ac_magnitude)
    tb.V(name="out_sense", positive=output_node, negative="out_sense_n", value=0.0)
    _save(tb, f"I(V{source_name})", "I(Vout_sense)", f"V({output_node})")
    tb.add_operating_point()
    tb.add_ac(variation="dec", number_of_points=80, start_frequency=1.0, stop_frequency=100e6)
    return DesignBench(
        name="current_gain",
        category="amplifier",
        testbench=tb,
        intent="Current-domain transfer, input compliance, and output current tracking.",
        measurements=["current_gain_midband", "current_gain_bandwidth"],
        result_fields=["ac.frequency", "branch current"],
        validation=["current gain in budget", "output compliance maintained"],
    )


def amplifier_transimpedance(
    ps: Any,
    dut: Any,
    *,
    input_node: str = "iin",
    output_node: str = "vout",
    reference_node: str = "0",
    source_name: str = "in",
    ac_current: float = 1e-6,
) -> DesignBench:
    tb = ps.Testbench(dut)
    tb.I(name=source_name, positive=input_node, negative=reference_node, value=0.0, ac=ac_current)
    _save(tb, f"V({output_node})", f"I(V{source_name})")
    tb.add_operating_point()
    tb.add_ac(variation="dec", number_of_points=100, start_frequency=1.0, stop_frequency=1e9)
    return DesignBench(
        name="transimpedance",
        category="amplifier",
        testbench=tb,
        intent="Current-to-voltage gain, noise-relevant bandwidth, and output swing.",
        measurements=["transimpedance_midband", "tia_bandwidth"],
        result_fields=["ac.frequency", f"ac['{output_node}']"],
        validation=["transimpedance in budget", "output bias in linear region"],
    )


def charge_amplifier(
    ps: Any,
    dut: Any,
    *,
    input_node: str = "qin",
    output_node: str = "vout",
    reference_node: str = "0",
    charge_coulombs: float = 1e-12,
    pulse_width: float = 10e-9,
) -> DesignBench:
    pulse_current = charge_coulombs / pulse_width
    tb = ps.Testbench(dut)
    tb.PulseCurrentSource(
        name="qstep",
        positive=input_node,
        negative=reference_node,
        initial_value=0.0,
        pulsed_value=pulse_current,
        pulse_width=pulse_width,
        period=10 * pulse_width,
        rise_time=pulse_width / 100,
        fall_time=pulse_width / 100,
    )
    _save(tb, f"V({output_node})", "I(Iqstep)")
    tb.add_transient(step_time=pulse_width / 100, end_time=20 * pulse_width)
    return DesignBench(
        name="charge_amplifier",
        category="amplifier",
        testbench=tb,
        intent="Charge-domain impulse response and droop after injected charge.",
        measurements=["q_injected", "delta_vout", "droop_rate"],
        result_fields=["tran.time", f"tran['{output_node}']"],
        validation=["delta_vout matches Q/C target", "droop below target"],
    )


def dac_static_linearity(
    ps: Any,
    dut: Any,
    *,
    code_nodes: list[str],
    output_node: str = "vout",
    reference_node: str = "0",
    v_high: float = 1.0,
    load_resistance: float | None = None,
) -> DesignBench:
    tb = ps.Testbench(dut)
    for idx, node in enumerate(code_nodes):
        tb.V(name=f"code{idx}", positive=node, negative=reference_node, value=0.0)
        tb.step(f"Vcode{idx}", 0.0, v_high, v_high)
    if load_resistance is not None:
        tb.R(name="load", positive=output_node, negative=reference_node, value=load_resistance)
    _save(tb, f"V({output_node})")
    tb.add_operating_point()
    tb.add_dc(**{f"Vcode0": slice(0.0, v_high, v_high)})
    return DesignBench(
        name="dac_static_linearity",
        category="dac",
        testbench=tb,
        intent="Code-to-output transfer, endpoint gain, monotonicity, DNL, and INL.",
        measurements=["offset_error", "gain_error", "dnl", "inl", "monotonic"],
        result_fields=["dc.sweep", f"dc['{output_node}']"],
        validation=["monotonic output", "endpoint error in budget", "DNL/INL in budget"],
    )


def adc_ramp(
    ps: Any,
    dut: Any,
    *,
    input_node: str = "vin",
    clock_node: str = "clk",
    output_nodes: list[str] | None = None,
    reference_node: str = "0",
    input_start: float = 0.0,
    input_stop: float = 1.0,
    conversion_period: float = 1e-6,
) -> DesignBench:
    output_nodes = output_nodes or ["d0"]
    tb = ps.Testbench(dut)
    tb.PieceWiseLinearVoltageSource(
        name="ramp",
        positive=input_node,
        negative=reference_node,
        values=[(0.0, input_start), (conversion_period * 128, input_stop)],
    )
    tb.PulseVoltageSource(
        name="clk",
        positive=clock_node,
        negative=reference_node,
        initial_value=0.0,
        pulsed_value=1.0,
        pulse_width=conversion_period / 2,
        period=conversion_period,
    )
    _save(tb, f"V({input_node})", f"V({clock_node})", *(f"V({node})" for node in output_nodes))
    tb.add_transient(step_time=conversion_period / 100, end_time=conversion_period * 128)
    return DesignBench(
        name="adc_ramp",
        category="adc",
        testbench=tb,
        intent="Ramp-code transition order, missing codes, latency, and metastability symptoms.",
        measurements=["missing_codes", "transition_levels", "latency_cycles"],
        result_fields=["tran.time", f"tran['{input_node}']", "digital output nodes"],
        validation=["codes increase monotonically", "no missing codes in target range"],
    )


def switch_characterization(
    ps: Any,
    dut: Any,
    *,
    input_node: str = "vin",
    output_node: str = "vout",
    control_node: str = "ctrl",
    reference_node: str = "0",
    load_capacitance: float = 1e-12,
) -> DesignBench:
    tb = ps.Testbench(dut)
    tb.V(name="sig", positive=input_node, negative=reference_node, value=1.0, ac=1.0)
    tb.PulseVoltageSource(
        name="ctrl",
        positive=control_node,
        negative=reference_node,
        initial_value=0.0,
        pulsed_value=1.0,
        pulse_width=1e-6,
        period=2e-6,
    )
    tb.C(name="hold", positive=output_node, negative=reference_node, value=load_capacitance)
    _save(tb, f"V({input_node})", f"V({output_node})", f"V({control_node})")
    tb.add_operating_point()
    tb.add_ac(variation="dec", number_of_points=50, start_frequency=1.0, stop_frequency=1e9)
    tb.add_transient(step_time=1e-9, end_time=2e-6)
    return DesignBench(
        name="switch_characterization",
        category="switch",
        testbench=tb,
        intent="On resistance proxy, off isolation, feedthrough, and charge injection.",
        measurements=["ron_est", "i_leak_off", "feedthrough_ratio", "q_injected"],
        result_fields=["op[node]", "ac.frequency", "tran.time"],
        validation=["selected path follows input", "off path stays isolated", "glitch below target"],
    )


def mux_routing(
    ps: Any,
    dut: Any,
    *,
    input_nodes: list[str],
    output_node: str = "vout",
    select_nodes: list[str] | None = None,
    reference_node: str = "0",
) -> DesignBench:
    select_nodes = select_nodes or ["sel"]
    tb = ps.Testbench(dut)
    for idx, node in enumerate(input_nodes):
        tb.V(name=f"in{idx}", positive=node, negative=reference_node, value=float(idx + 1))
    for idx, node in enumerate(select_nodes):
        tb.PulseVoltageSource(
            name=f"sel{idx}",
            positive=node,
            negative=reference_node,
            initial_value=0.0,
            pulsed_value=1.0,
            pulse_width=1e-6,
            period=2e-6 * (idx + 1),
        )
    _save(tb, f"V({output_node})", *(f"V({node})" for node in input_nodes + select_nodes))
    tb.add_transient(step_time=1e-9, end_time=4e-6)
    return DesignBench(
        name="mux_routing",
        category="mux",
        testbench=tb,
        intent="One-hot selection, selected-path gain, off-channel isolation, and select hazards.",
        measurements=["selected_vout", "unselected_feedthrough", "overlap_time"],
        result_fields=["tran.time", f"tran['{output_node}']"],
        validation=["one selected path at a time", "output equals selected input within tolerance"],
    )


def demux_routing(
    ps: Any,
    dut: Any,
    *,
    input_node: str = "vin",
    output_nodes: list[str],
    select_nodes: list[str] | None = None,
    reference_node: str = "0",
) -> DesignBench:
    select_nodes = select_nodes or ["sel"]
    tb = ps.Testbench(dut)
    tb.V(name="in", positive=input_node, negative=reference_node, value=1.0)
    for idx, node in enumerate(select_nodes):
        tb.PulseVoltageSource(
            name=f"sel{idx}",
            positive=node,
            negative=reference_node,
            initial_value=0.0,
            pulsed_value=1.0,
            pulse_width=1e-6,
            period=2e-6 * (idx + 1),
        )
    for idx, node in enumerate(output_nodes):
        tb.C(name=f"hold{idx}", positive=node, negative=reference_node, value=1e-12)
    _save(tb, f"V({input_node})", *(f"V({node})" for node in output_nodes + select_nodes))
    tb.add_transient(step_time=1e-9, end_time=4e-6)
    return DesignBench(
        name="demux_routing",
        category="demux",
        testbench=tb,
        intent="One input routed to one output while inactive outputs hold or reject feedthrough.",
        measurements=["selected_output", "inactive_output_droop", "feedthrough_ratio"],
        result_fields=["tran.time", "output node waveforms"],
        validation=["only selected output follows input", "inactive outputs stay bounded"],
    )


def sample_hold(
    ps: Any,
    dut: Any,
    *,
    input_node: str = "vin",
    output_node: str = "vhold",
    clock_node: str = "phi",
    reference_node: str = "0",
    input_frequency: float = 1e3,
    hold_capacitance: float = 1e-12,
) -> DesignBench:
    tb = ps.Testbench(dut)
    tb.SinusoidalVoltageSource(
        name="in",
        positive=input_node,
        negative=reference_node,
        offset=0.5,
        amplitude=0.5,
        frequency=input_frequency,
    )
    tb.PulseVoltageSource(
        name="phi",
        positive=clock_node,
        negative=reference_node,
        initial_value=0.0,
        pulsed_value=1.0,
        pulse_width=0.4 / input_frequency,
        period=1.0 / input_frequency,
    )
    tb.C(name="hold", positive=output_node, negative=reference_node, value=hold_capacitance)
    _save(tb, f"V({input_node})", f"V({output_node})", f"V({clock_node})")
    tb.add_transient(step_time=1.0 / input_frequency / 200, end_time=5.0 / input_frequency)
    return DesignBench(
        name="sample_hold",
        category="sample_hold",
        testbench=tb,
        intent="Acquisition error, hold droop, pedestal, and aperture sensitivity.",
        measurements=["acquisition_error", "hold_droop", "pedestal_step", "aperture_error"],
        result_fields=["tran.time", f"tran['{output_node}']"],
        validation=["settles during track", "droop during hold below target"],
    )


def pll_lock(
    ps: Any,
    dut: Any,
    *,
    reference_node: str = "ref",
    output_node: str = "vco",
    control_node: str = "vctrl",
    ground: str = "0",
    reference_frequency: float = 10e6,
) -> DesignBench:
    period = 1.0 / reference_frequency
    tb = ps.Testbench(dut)
    tb.PulseVoltageSource(
        name="ref",
        positive=reference_node,
        negative=ground,
        initial_value=0.0,
        pulsed_value=1.0,
        pulse_width=period / 2,
        period=period,
        rise_time=period / 100,
        fall_time=period / 100,
    )
    _save(tb, f"V({reference_node})", f"V({output_node})", f"V({control_node})")
    tb.add_transient(step_time=period / 100, end_time=period * 256)
    tb.add_fourier(reference_frequency, [f"V({output_node})"], num_harmonics=10)
    return DesignBench(
        name="pll_lock",
        category="pll",
        testbench=tb,
        intent="Lock acquisition, control settling, output periodicity, and spur proxy checks.",
        measurements=["lock_time", "frequency_error", "control_settling", "reference_spur"],
        result_fields=["tran.time", f"tran['{control_node}']", f"tran['{output_node}']"],
        validation=["control voltage settles", "output period matches reference ratio"],
    )


def bandgap_reference(
    ps: Any,
    dut: Any,
    *,
    supply_node: str = "vdd",
    output_node: str = "vref",
    ground: str = "0",
    supply_start: float = 1.0,
    supply_stop: float = 5.0,
) -> DesignBench:
    tb = ps.Testbench(dut)
    tb.V(name="dd", positive=supply_node, negative=ground, value=supply_stop)
    _save(tb, f"V({supply_node})", f"V({output_node})")
    tb.temperature = 27.0
    tb.nominal_temperature = 27.0
    tb.add_operating_point()
    tb.add_dc(Vdd=slice(supply_start, supply_stop, (supply_stop - supply_start) / 20))
    tb.step_sweep("temp", -40.0, 125.0, 5.0, "lin")
    return DesignBench(
        name="bandgap_reference",
        category="bandgap",
        testbench=tb,
        intent="Line regulation, temperature drift, startup state, and output impedance proxy.",
        measurements=["vref_27c", "line_regulation", "tempco_ppm_c", "startup_margin"],
        result_fields=["op[node]", "dc.sweep", f"dc['{output_node}']"],
        validation=["vref in target range", "line regulation below target", "temperature drift bounded"],
    )
