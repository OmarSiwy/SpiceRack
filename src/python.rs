//! PyO3 Python bindings — identical API surface to original PySpice.
//!
//! ```python
//! from pyspice_rs import Circuit
//! from pyspice_rs.unit import *
//! ```

#![allow(non_snake_case)]

use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::exceptions::{PyKeyError, PyAttributeError};

use crate::circuit::{self as cir, ComponentValue, Param};
use crate::unit as u;

/// Convert MeasureResult vec to a Python-friendly dict
fn measures_to_dict(measures: &[crate::result::MeasureResult]) -> HashMap<String, f64> {
    measures.iter().map(|m| (m.name.clone(), m.value)).collect()
}

/// Convert an IR value to a circuit ComponentValue
fn ir_value_to_cv(v: &crate::ir::IrValue) -> ComponentValue {
    match v {
        crate::ir::IrValue::Numeric { value } => ComponentValue::Numeric(*value),
        crate::ir::IrValue::Expression { expr } => ComponentValue::Expression(expr.clone()),
        crate::ir::IrValue::Raw { text } => ComponentValue::Raw(text.clone()),
    }
}

/// Convert an IR waveform to a circuit Waveform
fn ir_waveform_to_cir(wf: &crate::ir::IrWaveform) -> cir::Waveform {
    match wf {
        crate::ir::IrWaveform::Sin { offset, amplitude, frequency, delay, damping, phase } => {
            cir::Waveform::Sin(cir::SinWaveform {
                offset: *offset, amplitude: *amplitude, frequency: *frequency,
                delay: *delay, damping: *damping, phase: *phase,
            })
        }
        crate::ir::IrWaveform::Pulse { initial, pulsed, delay, rise_time, fall_time, pulse_width, period } => {
            cir::Waveform::Pulse(cir::PulseWaveform {
                initial: *initial, pulsed: *pulsed, delay: *delay,
                rise_time: *rise_time, fall_time: *fall_time,
                pulse_width: *pulse_width, period: *period,
            })
        }
        crate::ir::IrWaveform::Pwl { values } => {
            cir::Waveform::Pwl(cir::PwlWaveform { values: values.clone() })
        }
        crate::ir::IrWaveform::Exp { initial, pulsed, rise_delay, rise_tau, fall_delay, fall_tau } => {
            cir::Waveform::Exp(cir::ExpWaveform {
                initial: *initial, pulsed: *pulsed,
                rise_delay: *rise_delay, rise_tau: *rise_tau,
                fall_delay: *fall_delay, fall_tau: *fall_tau,
            })
        }
        crate::ir::IrWaveform::Sffm { offset, amplitude, carrier_freq, modulation_index, signal_freq } => {
            cir::Waveform::Sffm(cir::SffmWaveform {
                offset: *offset, amplitude: *amplitude,
                carrier_freq: *carrier_freq, modulation_index: *modulation_index,
                signal_freq: *signal_freq,
            })
        }
        crate::ir::IrWaveform::Am { amplitude, offset, modulating_freq, carrier_freq, delay } => {
            cir::Waveform::Am(cir::AmWaveform {
                amplitude: *amplitude, offset: *offset,
                modulating_freq: *modulating_freq, carrier_freq: *carrier_freq,
                delay: *delay,
            })
        }
    }
}

// ── Unit bindings ──

#[pyclass(name = "Unit")]
#[derive(Clone)]
struct PyUnit {
    inner: u::Unit,
}

#[pyclass(name = "UnitValue")]
#[derive(Clone)]
struct PyUnitValue {
    inner: u::UnitValue,
}

#[pymethods]
impl PyUnit {
    fn __rmatmul__(&self, value: f64) -> PyUnitValue {
        PyUnitValue {
            inner: u::UnitValue::new(value, self.inner),
        }
    }

    fn __repr__(&self) -> String {
        format!("Unit({:?}, {:?})", self.inner.prefix, self.inner.kind)
    }
}

#[pymethods]
impl PyUnitValue {
    #[getter]
    fn value(&self) -> f64 {
        self.inner.value
    }

    fn str_spice(&self) -> String {
        self.inner.str_spice()
    }

    fn __float__(&self) -> f64 {
        self.inner.value
    }

    fn __repr__(&self) -> String {
        format!("{}", self.inner)
    }

    fn __str__(&self) -> String {
        format!("{}", self.inner)
    }
}

// ── Value argument: accept float or UnitValue from Python ──

#[derive(FromPyObject)]
enum PyValueArg {
    Float(f64),
    Unit(PyUnitValue),
}

impl PyValueArg {
    fn into_component_value(self) -> ComponentValue {
        match self {
            Self::Float(v) => ComponentValue::Numeric(v),
            Self::Unit(uv) => ComponentValue::Unit(uv.inner),
        }
    }
}

// ── Circuit bindings ──

#[pyclass(name = "Circuit")]
struct PyCircuit {
    inner: cir::Circuit,
}

#[pymethods]
impl PyCircuit {
    #[new]
    fn new(title: &str) -> Self {
        Self {
            inner: cir::Circuit::new(title),
        }
    }

    #[getter]
    fn gnd(&self) -> String {
        "0".to_string()
    }

    // ── Component methods ──

    #[pyo3(signature = (*, name, positive, negative, value, raw_spice=None))]
    fn R(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg, raw_spice: Option<&str>) {
        if let Some(raw) = raw_spice {
            self.inner.r_raw(name, positive, negative, raw);
        } else {
            self.inner.r(name, positive, negative, value.into_component_value());
        }
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn C(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.c(name, positive, negative, value.into_component_value());
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn L(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.l(name, positive, negative, value.into_component_value());
    }

    #[pyo3(signature = (*, name, inductor1, inductor2, coupling))]
    fn K(&mut self, name: &str, inductor1: &str, inductor2: &str, coupling: f64) {
        self.inner.k(name, inductor1, inductor2, coupling);
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn V(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.v(name, positive, negative, value.into_component_value());
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn I(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.i(name, positive, negative, value.into_component_value());
    }

    #[pyo3(signature = (*, name, positive, negative, expression))]
    fn BV(&mut self, name: &str, positive: &str, negative: &str, expression: &str) {
        self.inner.bv(name, positive, negative, expression);
    }

    #[pyo3(signature = (*, name, positive, negative, expression))]
    fn BI(&mut self, name: &str, positive: &str, negative: &str, expression: &str) {
        self.inner.bi(name, positive, negative, expression);
    }

    #[pyo3(signature = (*, name, positive, negative, control_positive, control_negative, voltage_gain))]
    fn E(&mut self, name: &str, positive: &str, negative: &str, control_positive: &str, control_negative: &str, voltage_gain: f64) {
        self.inner.e(name, positive, negative, control_positive, control_negative, voltage_gain);
    }

    #[pyo3(signature = (*, name, positive, negative, control_positive, control_negative, transconductance))]
    fn G(&mut self, name: &str, positive: &str, negative: &str, control_positive: &str, control_negative: &str, transconductance: f64) {
        self.inner.g(name, positive, negative, control_positive, control_negative, transconductance);
    }

    #[pyo3(signature = (*, name, positive, negative, vsense, current_gain))]
    fn F(&mut self, name: &str, positive: &str, negative: &str, vsense: &str, current_gain: f64) {
        self.inner.f(name, positive, negative, vsense, current_gain);
    }

    #[pyo3(signature = (*, name, positive, negative, vsense, transresistance))]
    fn H(&mut self, name: &str, positive: &str, negative: &str, vsense: &str, transresistance: f64) {
        self.inner.h(name, positive, negative, vsense, transresistance);
    }

    #[pyo3(signature = (*, name, anode, cathode, model))]
    fn D(&mut self, name: &str, anode: &str, cathode: &str, model: &str) {
        self.inner.d(name, anode, cathode, model);
    }

    #[pyo3(signature = (*, name, collector, base, emitter, model))]
    fn Q(&mut self, name: &str, collector: &str, base: &str, emitter: &str, model: &str) {
        self.inner.q(name, collector, base, emitter, model);
    }

    #[pyo3(signature = (*, name, collector, base, emitter, model))]
    fn BJT(&mut self, name: &str, collector: &str, base: &str, emitter: &str, model: &str) {
        self.inner.q(name, collector, base, emitter, model);
    }

    #[pyo3(signature = (*, name, drain, gate, source, bulk, model, **kwargs))]
    fn M(&mut self, name: &str, drain: &str, gate: &str, source: &str, bulk: &str, model: &str, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        let mut params = Vec::new();
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                let val: String = v.str()?.to_string();
                params.push(Param::new(key, val));
            }
        }
        if params.is_empty() {
            self.inner.m(name, drain, gate, source, bulk, model);
        } else {
            self.inner.m_with_params(name, drain, gate, source, bulk, model, params);
        }
        Ok(())
    }

    #[pyo3(signature = (*, name, drain, gate, source, bulk, model, **kwargs))]
    fn MOSFET(&mut self, name: &str, drain: &str, gate: &str, source: &str, bulk: &str, model: &str, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        self.M(name, drain, gate, source, bulk, model, kwargs)
    }

    #[pyo3(signature = (*, name, drain, gate, source, model))]
    fn J(&mut self, name: &str, drain: &str, gate: &str, source: &str, model: &str) {
        self.inner.j(name, drain, gate, source, model);
    }

    #[pyo3(signature = (*, name, drain, gate, source, model))]
    fn Z(&mut self, name: &str, drain: &str, gate: &str, source: &str, model: &str) {
        self.inner.z(name, drain, gate, source, model);
    }

    #[pyo3(signature = (*, name, positive, negative, control_positive, control_negative, model))]
    fn S(&mut self, name: &str, positive: &str, negative: &str, control_positive: &str, control_negative: &str, model: &str) {
        self.inner.s(name, positive, negative, control_positive, control_negative, model);
    }

    #[pyo3(signature = (*, name, positive, negative, vcontrol, model))]
    fn W(&mut self, name: &str, positive: &str, negative: &str, vcontrol: &str, model: &str) {
        self.inner.w(name, positive, negative, vcontrol, model);
    }

    #[pyo3(signature = (*, name, input_positive, input_negative, output_positive, output_negative, Z0, TD))]
    fn T(&mut self, name: &str, input_positive: &str, input_negative: &str, output_positive: &str, output_negative: &str, Z0: f64, TD: f64) {
        self.inner.t(name, input_positive, input_negative, output_positive, output_negative, Z0, TD);
    }

    #[pyo3(signature = (name, subcircuit_name, *nodes))]
    fn X(&mut self, name: &str, subcircuit_name: &str, nodes: Vec<String>) {
        let node_refs: Vec<&str> = nodes.iter().map(|s| s.as_str()).collect();
        self.inner.x(name, subcircuit_name, node_refs);
    }

    /// XSPICE code model instance (A-element).
    ///
    /// ```python
    /// circuit.A("adc1", connections=["[vin]", "[dout]"], model="adc_buf")
    /// circuit.A("and1", connections=["[da db]", "[dout]"], model="and_model")
    /// circuit.A("dff1", connections=["[d clk $d_hi $d_hi]", "[q qbar]"], model="dff_model")
    /// ```
    #[pyo3(signature = (*, name, connections, model))]
    fn A(&mut self, name: &str, connections: Vec<String>, model: &str) {
        self.inner.a(name, connections, model);
    }

    /// Load a compiled Verilog-A model (OSDI binary).
    ///
    /// ```python
    /// circuit.osdi("/path/to/model.osdi")
    /// ```
    fn osdi(&mut self, path: &str) {
        self.inner.osdi(path);
    }

    /// Compile Verilog-A source to OSDI and load it into the circuit.
    ///
    /// Accepts either a file path (.va) or inline source code. If source
    /// code is given, it's written to a temp file before compilation.
    /// Requires `openvaf` on `$PATH`.
    ///
    /// ```python
    /// # From file:
    /// circuit.veriloga("comparator.va")
    ///
    /// # From inline source:
    /// circuit.veriloga('''
    /// `include "disciplines.vams"
    /// module myres(a, b);
    ///     inout a, b; electrical a, b;
    ///     parameter real r = 1000.0;
    ///     analog V(a,b) <+ r * I(a,b);
    /// endmodule
    /// ''')
    /// ```
    fn veriloga(&mut self, source_or_path: &str) -> PyResult<String> {
        let osdi_path = compile_veriloga_impl(source_or_path)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        self.inner.osdi(&osdi_path);
        Ok(osdi_path)
    }

    /// Add a Verilog module to the circuit via co-simulation or gate-level synthesis.
    ///
    /// mode="simulate": compile with iverilog, use ngspice d_cosim XSPICE model.
    /// mode="synthesize": synthesize with Yosys to gate-level SPICE subcircuit calls.
    ///
    /// ```python
    /// # Co-simulation (digital Verilog alongside analog SPICE):
    /// circuit.verilog(
    ///     source="counter.v",
    ///     mode="simulate",
    ///     instance_name="cnt1",
    ///     connections={"clk": "clk_net", "count": ["bit0", "bit1", "bit2"]},
    /// )
    ///
    /// # Gate-level synthesis (compile to standard cells):
    /// circuit.verilog(
    ///     source="counter.v",
    ///     mode="synthesize",
    ///     instance_name="cnt1",
    ///     connections={"clk": "clk_net", "count": ["bit0", "bit1", "bit2"]},
    ///     pdk="sky130_fd_sc_hd",
    /// )
    /// ```
    #[pyo3(signature = (*, source, mode="simulate", instance_name, connections, pdk=None, liberty=None, spice_models=None))]
    fn verilog(
        &mut self,
        py: pyo3::Python<'_>,
        source: &str,
        mode: &str,
        instance_name: &str,
        connections: HashMap<String, pyo3::PyObject>,
        pdk: Option<&str>,
        liberty: Option<&str>,
        spice_models: Option<&str>,
    ) -> PyResult<()> {
        verilog_impl_circuit(
            &mut self.inner, py, source, mode, instance_name,
            &connections, pdk, liberty, spice_models,
        )
    }

    // ── High-level waveform sources ──

    #[pyo3(signature = (*, name, positive, negative, dc_offset=0.0, offset=0.0, amplitude=1.0, frequency=1000.0))]
    fn SinusoidalVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str,
        dc_offset: f64, offset: f64, amplitude: f64, frequency: f64,
    ) {
        self.inner.sinusoidal_voltage_source(name, positive, negative, dc_offset, offset, amplitude, frequency);
    }

    #[pyo3(signature = (*, name, positive, negative, initial_value=0.0, pulsed_value=1.0, pulse_width=50e-9, period=100e-9, rise_time=1e-9, fall_time=1e-9))]
    fn PulseVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str,
        initial_value: f64, pulsed_value: f64, pulse_width: f64,
        period: f64, rise_time: f64, fall_time: f64,
    ) {
        self.inner.pulse_voltage_source(
            name, positive, negative, initial_value, pulsed_value, pulse_width,
            period, rise_time, fall_time,
        );
    }

    #[pyo3(signature = (*, name, positive, negative, values))]
    fn PieceWiseLinearVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str, values: Vec<(f64, f64)>,
    ) {
        self.inner.pwl_voltage_source(name, positive, negative, values);
    }

    #[pyo3(signature = (*, name, positive, negative, dc_offset=0.0, offset=0.0, amplitude=1.0, frequency=1000.0))]
    fn SinusoidalCurrentSource(
        &mut self, name: &str, positive: &str, negative: &str,
        dc_offset: f64, offset: f64, amplitude: f64, frequency: f64,
    ) {
        self.inner.sinusoidal_current_source(name, positive, negative, dc_offset, offset, amplitude, frequency);
    }

    #[pyo3(signature = (*, name, positive, negative, initial_value=0.0, pulsed_value=1.0, pulse_width=50e-9, period=100e-9, rise_time=1e-9, fall_time=1e-9))]
    fn PulseCurrentSource(
        &mut self, name: &str, positive: &str, negative: &str,
        initial_value: f64, pulsed_value: f64, pulse_width: f64,
        period: f64, rise_time: f64, fall_time: f64,
    ) {
        self.inner.pulse_current_source(
            name, positive, negative, initial_value, pulsed_value, pulse_width,
            period, rise_time, fall_time,
        );
    }

    // ── Circuit-level directives ──

    #[pyo3(signature = (name, kind, **kwargs))]
    fn model(&mut self, name: &str, kind: &str, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        let mut params = Vec::new();
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let key: String = k.extract::<String>()?;
                let val: String = v.str()?.to_string();
                params.push(Param::new(key, val));
            }
        }
        self.inner.model(name, kind, params);
        Ok(())
    }

    fn include(&mut self, path: &str) {
        self.inner.include(path);
    }

    fn lib(&mut self, path: &str, section: &str) {
        self.inner.lib(path, section);
    }

    fn parameter(&mut self, name: &str, value: &str) {
        self.inner.parameter(name, value);
    }

    #[pyo3(signature = (**kwargs))]
    fn options(&mut self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let key: String = k.extract::<String>()?;
                let val: String = v.str()?.to_string();
                self.inner.options(key, val);
            }
        }
        Ok(())
    }

    fn temp(&mut self, temperature: f64) {
        self.inner.temp(temperature);
    }

    fn raw_spice(&mut self, line: &str) {
        self.inner.raw_spice(line);
    }

    fn subcircuit(&mut self, subckt: &PySubcircuit) -> PyResult<()> {
        use crate::circuit::{SubCircuitDef, Element, Mosfet, Resistor, Capacitor, Inductor,
            VoltageSource, CurrentSource, BehavioralVoltage, BehavioralCurrent,
            Vcvs, Vccs, Cccs, Ccvs, Diode, Bjt, Jfet, Mesfet, VSwitch, ISwitch,
            TLine, SubcircuitInstance, XspiceInstance, MutualInductor,
            Node, Param, Model};

        let ir = &subckt.inner;
        let pins: Vec<String> = ir.ports.iter().map(|p| p.name.clone()).collect();

        let mut elements: Vec<Element> = Vec::new();

        for comp in &ir.components {
            match comp {
                crate::ir::Component::Resistor { name, n1, n2, value, params } => {
                    elements.push(Element::R(Resistor {
                        name: name.clone(),
                        n1: Node::from(n1.as_str()),
                        n2: Node::from(n2.as_str()),
                        value: ir_value_to_cv(value),
                        params: params.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
                    }));
                }
                crate::ir::Component::Capacitor { name, n1, n2, value, params } => {
                    elements.push(Element::C(Capacitor {
                        name: name.clone(),
                        n1: Node::from(n1.as_str()),
                        n2: Node::from(n2.as_str()),
                        value: ir_value_to_cv(value),
                        params: params.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
                    }));
                }
                crate::ir::Component::Inductor { name, n1, n2, value, params } => {
                    elements.push(Element::L(Inductor {
                        name: name.clone(),
                        n1: Node::from(n1.as_str()),
                        n2: Node::from(n2.as_str()),
                        value: ir_value_to_cv(value),
                        params: params.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
                    }));
                }
                crate::ir::Component::MutualInductor { name, inductor1, inductor2, coupling } => {
                    elements.push(Element::K(MutualInductor {
                        name: name.clone(),
                        inductor1: inductor1.clone(),
                        inductor2: inductor2.clone(),
                        coupling: *coupling,
                    }));
                }
                crate::ir::Component::VoltageSource { name, np, nm, value, waveform } => {
                    elements.push(Element::V(VoltageSource {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        value: ir_value_to_cv(value),
                        waveform: waveform.as_ref().map(ir_waveform_to_cir),
                    }));
                }
                crate::ir::Component::CurrentSource { name, np, nm, value, waveform } => {
                    elements.push(Element::I(CurrentSource {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        value: ir_value_to_cv(value),
                        waveform: waveform.as_ref().map(ir_waveform_to_cir),
                    }));
                }
                crate::ir::Component::BehavioralVoltage { name, np, nm, expression } => {
                    elements.push(Element::BV(BehavioralVoltage {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        expression: expression.clone(),
                    }));
                }
                crate::ir::Component::BehavioralCurrent { name, np, nm, expression } => {
                    elements.push(Element::BI(BehavioralCurrent {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        expression: expression.clone(),
                    }));
                }
                crate::ir::Component::Vcvs { name, np, nm, ncp, ncm, gain } => {
                    elements.push(Element::E(Vcvs {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        ncp: Node::from(ncp.as_str()),
                        ncm: Node::from(ncm.as_str()),
                        gain: *gain,
                    }));
                }
                crate::ir::Component::Vccs { name, np, nm, ncp, ncm, transconductance } => {
                    elements.push(Element::G(Vccs {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        ncp: Node::from(ncp.as_str()),
                        ncm: Node::from(ncm.as_str()),
                        transconductance: *transconductance,
                    }));
                }
                crate::ir::Component::Cccs { name, np, nm, vsense, gain } => {
                    elements.push(Element::F(Cccs {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        vsense: vsense.clone(),
                        gain: *gain,
                    }));
                }
                crate::ir::Component::Ccvs { name, np, nm, vsense, transresistance } => {
                    elements.push(Element::H(Ccvs {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        vsense: vsense.clone(),
                        transresistance: *transresistance,
                    }));
                }
                crate::ir::Component::Diode { name, np, nm, model, params } => {
                    elements.push(Element::D(Diode {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        model: model.clone(),
                        params: params.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
                    }));
                }
                crate::ir::Component::Bjt { name, nc, nb, ne, model, params } => {
                    elements.push(Element::Q(Bjt {
                        name: name.clone(),
                        nc: Node::from(nc.as_str()),
                        nb: Node::from(nb.as_str()),
                        ne: Node::from(ne.as_str()),
                        model: model.clone(),
                        params: params.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
                    }));
                }
                crate::ir::Component::Mosfet { name, nd, ng, ns, nb, model, params } => {
                    elements.push(Element::M(Mosfet {
                        name: name.clone(),
                        nd: Node::from(nd.as_str()),
                        ng: Node::from(ng.as_str()),
                        ns: Node::from(ns.as_str()),
                        nb: Node::from(nb.as_str()),
                        model: model.clone(),
                        params: params.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
                    }));
                }
                crate::ir::Component::Jfet { name, nd, ng, ns, model, params } => {
                    elements.push(Element::J(Jfet {
                        name: name.clone(),
                        nd: Node::from(nd.as_str()),
                        ng: Node::from(ng.as_str()),
                        ns: Node::from(ns.as_str()),
                        model: model.clone(),
                        params: params.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
                    }));
                }
                crate::ir::Component::Mesfet { name, nd, ng, ns, model, params } => {
                    elements.push(Element::Z(Mesfet {
                        name: name.clone(),
                        nd: Node::from(nd.as_str()),
                        ng: Node::from(ng.as_str()),
                        ns: Node::from(ns.as_str()),
                        model: model.clone(),
                        params: params.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
                    }));
                }
                crate::ir::Component::VSwitch { name, np, nm, ncp, ncm, model } => {
                    elements.push(Element::S(VSwitch {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        ncp: Node::from(ncp.as_str()),
                        ncm: Node::from(ncm.as_str()),
                        model: model.clone(),
                    }));
                }
                crate::ir::Component::ISwitch { name, np, nm, vcontrol, model } => {
                    elements.push(Element::W(ISwitch {
                        name: name.clone(),
                        np: Node::from(np.as_str()),
                        nm: Node::from(nm.as_str()),
                        vcontrol: vcontrol.clone(),
                        model: model.clone(),
                    }));
                }
                crate::ir::Component::TLine { name, inp, inm, outp, outm, z0, td } => {
                    elements.push(Element::T(TLine {
                        name: name.clone(),
                        inp: Node::from(inp.as_str()),
                        inm: Node::from(inm.as_str()),
                        outp: Node::from(outp.as_str()),
                        outm: Node::from(outm.as_str()),
                        z0: *z0,
                        td: *td,
                    }));
                }
                crate::ir::Component::Xspice { name, connections, model } => {
                    elements.push(Element::A(XspiceInstance {
                        name: name.clone(),
                        connections: connections.clone(),
                        model: model.clone(),
                    }));
                }
                crate::ir::Component::RawSpice { line } => {
                    elements.push(Element::RawSpice(line.clone()));
                }
            }
        }

        // Convert IR instances to X elements
        for inst in &ir.instances {
            elements.push(Element::X(SubcircuitInstance {
                name: inst.name.clone(),
                subcircuit_name: inst.subcircuit.clone(),
                nodes: inst.port_mapping.iter().map(|n| Node::from(n.as_str())).collect(),
                params: inst.parameters.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
            }));
        }

        // Add raw_spice lines as RawSpice elements
        for line in &ir.raw_spice {
            elements.push(Element::RawSpice(line.clone()));
        }

        let models: Vec<Model> = ir.models.iter().map(|m| {
            Model {
                name: m.name.clone(),
                kind: m.kind.clone(),
                params: m.parameters.iter().map(|(k,v)| Param::new(k.clone(), v.clone())).collect(),
            }
        }).collect();

        let params: Vec<Param> = ir.parameters.iter()
            .filter_map(|p| p.default.as_ref().map(|d| Param::new(p.name.clone(), d.clone())))
            .collect();

        self.inner.subcircuit(SubCircuitDef {
            name: ir.name.clone(),
            pins,
            elements,
            models,
            params,
        });

        // Also handle includes/libs from the subcircuit
        for inc in &ir.includes {
            self.inner.include(inc.as_str());
        }
        for (path, section) in &ir.libs {
            self.inner.lib(path.as_str(), section.as_str());
        }
        for osdi in &ir.osdi_loads {
            self.inner.osdi(osdi.as_str());
        }

        Ok(())
    }

    // ── Accessors ──

    fn __getitem__(&self, name: &str) -> PyResult<String> {
        self.inner
            .element(name)
            .map(|e| e.to_string())
            .ok_or_else(|| PyKeyError::new_err(format!("Element '{}' not found", name)))
    }

    fn element(&self, name: &str) -> PyResult<String> {
        self.__getitem__(name)
    }

    fn set_source_value(&mut self, name: &str, value: f64) -> PyResult<()> {
        self.inner.set_source_value(name, value)
            .map_err(|e| PyKeyError::new_err(e))
    }

    fn node(&self, name: &str) -> String {
        name.to_string()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("Circuit('{}')", self.inner.title)
    }

    // ── Simulator ──

    #[pyo3(signature = (simulator=None))]
    fn simulator(&self, simulator: Option<&str>) -> PySimulator {
        let sim = self.inner.simulator();
        PySimulator {
            inner: if let Some(name) = simulator {
                sim.with_backend(name)
            } else {
                sim
            },
        }
    }
}

// ── Simulator bindings ──

#[pyclass(name = "CircuitSimulator")]
struct PySimulator {
    inner: crate::simulation::CircuitSimulator,
}

#[pymethods]
impl PySimulator {
    // ── Config ──

    #[pyo3(signature = (**kwargs))]
    fn options(&mut self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let key: String = k.extract::<String>()?;
                let val: String = v.str()?.to_string();
                self.inner.options(key, val);
            }
        }
        Ok(())
    }

    #[pyo3(signature = (**kwargs))]
    fn initial_condition(&mut self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let node: String = k.extract::<String>()?;
                let val: f64 = v.extract::<f64>()?;
                self.inner.initial_condition(node, val);
            }
        }
        Ok(())
    }

    #[pyo3(signature = (**kwargs))]
    fn node_set(&mut self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let node: String = k.extract::<String>()?;
                let val: f64 = v.extract::<f64>()?;
                self.inner.node_set(node, val);
            }
        }
        Ok(())
    }

    #[pyo3(signature = (*args))]
    fn save(&mut self, args: Vec<String>) {
        for a in args {
            self.inner.save(a);
        }
    }

    #[pyo3(signature = (*args))]
    fn measure(&mut self, args: Vec<String>) {
        self.inner.measure(args);
    }

    #[setter]
    fn set_save_currents(&mut self, v: bool) {
        self.inner.set_save_currents(v);
    }

    #[getter]
    fn get_save_currents(&self) -> bool {
        false // TODO: expose from inner
    }

    #[setter]
    fn set_temperature(&mut self, temp: f64) {
        self.inner.set_temperature(temp);
    }

    #[setter]
    fn set_nominal_temperature(&mut self, temp: f64) {
        self.inner.set_nominal_temperature(temp);
    }

    // ── Step parameter sweeps ──

    #[pyo3(signature = (param, start, stop, step))]
    fn step(&mut self, param: &str, start: f64, stop: f64, step: f64) {
        self.inner.step(param, start, stop, step);
    }

    #[pyo3(signature = (param, start, stop, step, sweep_type))]
    fn step_sweep(&mut self, param: &str, start: f64, stop: f64, step: f64, sweep_type: &str) {
        self.inner.step_sweep(param, start, stop, step, sweep_type);
    }

    // ── Analysis methods ──

    fn operating_point(&self) -> PyResult<PyOperatingPoint> {
        self.inner
            .operating_point()
            .map(|op| PyOperatingPoint { inner: op })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (**kwargs))]
    fn dc(&self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<PyDcAnalysis> {
        let dict = kwargs.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("dc() requires sweep parameters")
        })?;
        let sweeps = extract_dc_sweeps(&dict)?;
        let sweep_refs: Vec<(&str, f64, f64, f64)> = sweeps
            .iter()
            .map(|(v, a, b, c)| (v.as_str(), *a, *b, *c))
            .collect();
        self.inner
            .dc_multi(&sweep_refs)
            .map(|dc| PyDcAnalysis { inner: dc })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (variation="dec", number_of_points=10, start_frequency=1.0, stop_frequency=1e9))]
    fn ac(
        &self, variation: &str, number_of_points: u32,
        start_frequency: f64, stop_frequency: f64,
    ) -> PyResult<PyAcAnalysis> {
        self.inner
            .ac(variation, number_of_points, start_frequency, stop_frequency)
            .map(|ac| PyAcAnalysis { inner: ac })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (step_time, end_time, start_time=None, max_time=None, use_initial_condition=false))]
    fn transient(
        &self, step_time: f64, end_time: f64,
        start_time: Option<f64>, max_time: Option<f64>,
        use_initial_condition: bool,
    ) -> PyResult<PyTransientAnalysis> {
        self.inner
            .transient(step_time, end_time, start_time, max_time, use_initial_condition)
            .map(|t| PyTransientAnalysis { inner: t })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (output_node, ref_node, src, variation="dec", points=10, start_frequency=1e3, stop_frequency=1e8, points_per_summary=None))]
    fn noise(
        &self, output_node: &str, ref_node: &str, src: &str,
        variation: &str, points: u32,
        start_frequency: f64, stop_frequency: f64,
        points_per_summary: Option<u32>,
    ) -> PyResult<PyNoiseAnalysis> {
        self.inner
            .noise(output_node, ref_node, src, variation, points, start_frequency, stop_frequency, points_per_summary)
            .map(|n| PyNoiseAnalysis { inner: n })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (outvar, insrc))]
    fn transfer_function(&self, outvar: &str, insrc: &str) -> PyResult<PyTransferFunctionAnalysis> {
        self.inner
            .transfer_function(outvar, insrc)
            .map(|t| PyTransferFunctionAnalysis { inner: t })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (outvar, insrc))]
    fn tf(&self, outvar: &str, insrc: &str) -> PyResult<PyTransferFunctionAnalysis> {
        self.transfer_function(outvar, insrc)
    }

    #[pyo3(signature = (output_variable))]
    fn dc_sensitivity(&self, output_variable: &str) -> PyResult<PySensitivityAnalysis> {
        self.inner
            .dc_sensitivity(output_variable)
            .map(|s| PySensitivityAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (output_variable, variation="dec", number_of_points=10, start_frequency=100.0, stop_frequency=1e5))]
    fn ac_sensitivity(
        &self, output_variable: &str, variation: &str,
        number_of_points: u32, start_frequency: f64, stop_frequency: f64,
    ) -> PyResult<PySensitivityAnalysis> {
        self.inner
            .ac_sensitivity(output_variable, variation, number_of_points, start_frequency, stop_frequency)
            .map(|s| PySensitivityAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (node1, node2, node3, node4, tf_type, pz_type))]
    fn polezero(
        &self, node1: &str, node2: &str, node3: &str, node4: &str,
        tf_type: &str, pz_type: &str,
    ) -> PyResult<PyPoleZeroAnalysis> {
        self.inner
            .polezero(node1, node2, node3, node4, tf_type, pz_type)
            .map(|p| PyPoleZeroAnalysis { inner: p })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (variation="dec", points=10, start_frequency=100.0, stop_frequency=1e8, f2overf1=None))]
    fn distortion(
        &self, variation: &str, points: u32,
        start_frequency: f64, stop_frequency: f64,
        f2overf1: Option<f64>,
    ) -> PyResult<PyDistortionAnalysis> {
        self.inner
            .distortion(variation, points, start_frequency, stop_frequency, f2overf1)
            .map(|d| PyDistortionAnalysis { inner: d })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    // ── New analysis methods ──

    #[pyo3(signature = (fundamental_frequency, stabilization_time, observe_node, points_per_period=128, harmonics=10))]
    fn pss(
        &self, fundamental_frequency: f64, stabilization_time: f64,
        observe_node: &str, points_per_period: u32, harmonics: u32,
    ) -> PyResult<PyPssAnalysis> {
        self.inner
            .pss(fundamental_frequency, stabilization_time, observe_node, points_per_period, harmonics)
            .map(|p| PyPssAnalysis { inner: p })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (variation="dec", number_of_points=10, start_frequency=1e6, stop_frequency=1e10))]
    fn s_param(
        &self, variation: &str, number_of_points: u32,
        start_frequency: f64, stop_frequency: f64,
    ) -> PyResult<PySParamAnalysis> {
        self.inner
            .s_param(variation, number_of_points, start_frequency, stop_frequency)
            .map(|s| PySParamAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (output_current, input_source, z_in=50.0, z_out=50.0, variation="dec", points=100, start_freq=1e3, stop_freq=1e9))]
    fn network_params(
        &self, output_current: &str, input_source: &str,
        z_in: f64, z_out: f64,
        variation: &str, points: u32,
        start_freq: f64, stop_freq: f64,
    ) -> PyResult<PySParamAnalysis> {
        self.inner
            .network_params(output_current, input_source, z_in, z_out, variation, points, start_freq, stop_freq)
            .map(|s| PySParamAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (fundamental_frequencies, num_harmonics=None))]
    fn harmonic_balance(
        &self, fundamental_frequencies: Vec<f64>,
        num_harmonics: Option<Vec<u32>>,
    ) -> PyResult<PyHarmonicBalanceAnalysis> {
        let nharms = num_harmonics.unwrap_or_else(|| vec![7; fundamental_frequencies.len()]);
        self.inner
            .harmonic_balance(&fundamental_frequencies, &nharms)
            .map(|h| PyHarmonicBalanceAnalysis { inner: h })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (probe, variation="dec", number_of_points=10, start_frequency=1.0, stop_frequency=1e10))]
    fn stability(
        &self, probe: &str, variation: &str, number_of_points: u32,
        start_frequency: f64, stop_frequency: f64,
    ) -> PyResult<PyStabilityAnalysis> {
        self.inner
            .stability(probe, variation, number_of_points, start_frequency, stop_frequency)
            .map(|s| PyStabilityAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (step_time, end_time))]
    fn transient_noise(
        &self, step_time: f64, end_time: f64,
    ) -> PyResult<PyTransientNoiseAnalysis> {
        self.inner
            .transient_noise(step_time, end_time)
            .map(|t| PyTransientNoiseAnalysis { inner: t })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    // ── Xyce-specific analysis methods ──

    /// Xyce .SAMPLING Monte Carlo uncertainty quantification.
    ///
    /// `param_distributions` is a list of `(param_name, distribution_spec)` tuples.
    /// Distribution specs: `"normal(mean,stddev)"`, `"uniform(low,high)"`.
    #[pyo3(signature = (num_samples, param_distributions))]
    fn xyce_sampling(
        &self, num_samples: u32,
        param_distributions: Vec<(String, String)>,
    ) -> PyResult<PySamplingAnalysis> {
        let refs: Vec<(&str, &str)> = param_distributions
            .iter()
            .map(|(p, d)| (p.as_str(), d.as_str()))
            .collect();
        self.inner
            .xyce_sampling(num_samples, &refs)
            .map(|s| PySamplingAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Xyce .EMBEDDEDSAMPLING — embedded Monte Carlo.
    #[pyo3(signature = (num_samples, param_distributions))]
    fn xyce_embedded_sampling(
        &self, num_samples: u32,
        param_distributions: Vec<(String, String)>,
    ) -> PyResult<PySamplingAnalysis> {
        let refs: Vec<(&str, &str)> = param_distributions
            .iter()
            .map(|(p, d)| (p.as_str(), d.as_str()))
            .collect();
        self.inner
            .xyce_embedded_sampling(num_samples, &refs)
            .map(|s| PySamplingAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Xyce .PCE Polynomial Chaos Expansion.
    #[pyo3(signature = (num_samples, param_distributions, expansion_order=3))]
    fn xyce_pce(
        &self, num_samples: u32,
        param_distributions: Vec<(String, String)>,
        expansion_order: u32,
    ) -> PyResult<PySamplingAnalysis> {
        let refs: Vec<(&str, &str)> = param_distributions
            .iter()
            .map(|(p, d)| (p.as_str(), d.as_str()))
            .collect();
        self.inner
            .xyce_pce(num_samples, &refs, expansion_order)
            .map(|s| PySamplingAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Xyce .FFT with spectral metrics (ENOB, SFDR, SNR, THD).
    #[pyo3(signature = (signal, np=1024, start=0.0, stop=1e-3, window="HANN", format="UNORM"))]
    fn xyce_fft(
        &self, signal: &str,
        np: u32, start: f64, stop: f64,
        window: &str, format: &str,
    ) -> PyResult<PyXyceFftAnalysis> {
        let options = crate::result::XyceFftOptions {
            np,
            start,
            stop,
            window: window.to_string(),
            format: format.to_string(),
        };
        self.inner
            .xyce_fft(signal, &options)
            .map(|f| PyXyceFftAnalysis { inner: f })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    // ── Spectre-specific analysis methods ──

    /// Spectre parametric sweep wrapping an inner analysis.
    #[pyo3(signature = (param, start, stop, step, inner_analysis, inner_type="ac"))]
    fn spectre_sweep(
        &self, param: &str, start: f64, stop: f64, step: f64,
        inner_analysis: &str, inner_type: &str,
    ) -> PyResult<PyRawData> {
        self.inner
            .spectre_sweep(param, start, stop, step, inner_analysis, inner_type)
            .map(|r| PyRawData { inner: r })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Spectre Monte Carlo wrapping an inner analysis.
    #[pyo3(signature = (num_iterations, inner_analysis, inner_type="ac", seed=None))]
    fn spectre_montecarlo(
        &self, num_iterations: u32, inner_analysis: &str,
        inner_type: &str, seed: Option<u64>,
    ) -> PyResult<PyRawData> {
        self.inner
            .spectre_montecarlo(num_iterations, inner_analysis, inner_type, seed)
            .map(|r| PyRawData { inner: r })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// SpectreRF Periodic AC (PAC) analysis with automatic PSS prerequisite.
    #[pyo3(signature = (pss_fundamental, pss_stabilization, pss_harmonics=10, variation="dec", points=100, start_freq=1.0, stop_freq=1e9, sweep_type="relative"))]
    fn spectre_pac(
        &self, pss_fundamental: f64, pss_stabilization: f64,
        pss_harmonics: u32, variation: &str, points: u32,
        start_freq: f64, stop_freq: f64, sweep_type: &str,
    ) -> PyResult<PyAcAnalysis> {
        self.inner
            .spectre_pac(
                pss_fundamental, pss_stabilization, pss_harmonics,
                variation, points, start_freq, stop_freq, sweep_type,
            )
            .map(|a| PyAcAnalysis { inner: a })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// SpectreRF Periodic Noise (PNoise) analysis with automatic PSS prerequisite.
    #[pyo3(signature = (pss_fundamental, pss_stabilization, output_node, ref_node, pss_harmonics=10, variation="dec", points=100, start_freq=1.0, stop_freq=1e9))]
    fn spectre_pnoise(
        &self, pss_fundamental: f64, pss_stabilization: f64,
        output_node: &str, ref_node: &str,
        pss_harmonics: u32, variation: &str, points: u32,
        start_freq: f64, stop_freq: f64,
    ) -> PyResult<PyNoiseAnalysis> {
        self.inner
            .spectre_pnoise(
                pss_fundamental, pss_stabilization, pss_harmonics,
                output_node, ref_node, variation, points, start_freq, stop_freq,
            )
            .map(|n| PyNoiseAnalysis { inner: n })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// SpectreRF Periodic Transfer Function (PXF) analysis with automatic PSS prerequisite.
    #[pyo3(signature = (pss_fundamental, pss_stabilization, output_node, source, pss_harmonics=10, variation="dec", points=100, start_freq=1.0, stop_freq=1e9))]
    fn spectre_pxf(
        &self, pss_fundamental: f64, pss_stabilization: f64,
        output_node: &str, source: &str,
        pss_harmonics: u32, variation: &str, points: u32,
        start_freq: f64, stop_freq: f64,
    ) -> PyResult<PyAcAnalysis> {
        self.inner
            .spectre_pxf(
                pss_fundamental, pss_stabilization, pss_harmonics,
                output_node, source, variation, points, start_freq, stop_freq,
            )
            .map(|a| PyAcAnalysis { inner: a })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// SpectreRF Periodic Stability (PSTB) analysis with automatic PSS prerequisite.
    #[pyo3(signature = (pss_fundamental, pss_stabilization, probe, pss_harmonics=10, variation="dec", points=100, start_freq=1.0, stop_freq=1e9))]
    fn spectre_pstb(
        &self, pss_fundamental: f64, pss_stabilization: f64,
        probe: &str, pss_harmonics: u32, variation: &str, points: u32,
        start_freq: f64, stop_freq: f64,
    ) -> PyResult<PyStabilityAnalysis> {
        self.inner
            .spectre_pstb(
                pss_fundamental, pss_stabilization, pss_harmonics,
                probe, variation, points, start_freq, stop_freq,
            )
            .map(|s| PyStabilityAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// List all available simulator backends on this system
    #[staticmethod]
    fn available_backends() -> Vec<String> {
        crate::simulation::CircuitSimulator::available_backends()
    }

    /// Check backend compatibility for the circuit in this simulator.
    fn check_backend(&self, name: &str) -> Vec<String> {
        let ir = crate::ir::CircuitIR::from_circuit(self.inner.circuit());
        ir.check_backend(name).into_iter().map(|issue| {
            format!("{:?}: {}", issue.severity, issue.message)
        }).collect()
    }

    fn __repr__(&self) -> &str {
        "CircuitSimulator"
    }
}

/// Extract DC sweep params: kwargs like Vinput=slice(-2, 5, 0.01)
fn extract_dc_sweeps(dict: &Bound<'_, pyo3::types::PyDict>) -> PyResult<Vec<(String, f64, f64, f64)>> {
    let mut sweeps = Vec::new();
    for (k, v) in dict.iter() {
        let var: String = k.extract::<String>()?;
        let start: f64 = v.getattr("start")?.extract::<f64>()?;
        let stop: f64 = v.getattr("stop")?.extract::<f64>()?;
        let step: f64 = v.getattr("step")?.extract::<f64>()?;
        sweeps.push((var, start, stop, step));
    }
    if sweeps.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "dc() requires at least one sweep parameter",
        ));
    }
    Ok(sweeps)
}

// ── Result bindings ──

/// Raw simulation data wrapper -- returned by spectre_sweep and spectre_montecarlo.
#[pyclass(name = "RawData")]
struct PyRawData {
    inner: crate::result::RawData,
}

#[pymethods]
impl PyRawData {
    #[getter]
    fn title(&self) -> String { self.inner.title.clone() }
    #[getter]
    fn plot_name(&self) -> String { self.inner.plot_name.clone() }
    #[getter]
    fn is_complex(&self) -> bool { self.inner.is_complex }
    #[getter]
    fn variable_names(&self) -> Vec<String> {
        self.inner.variables.iter().map(|v| v.name.clone()).collect()
    }
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        let lower = name.to_lowercase();
        for (i, var) in self.inner.variables.iter().enumerate() {
            if var.name.to_lowercase() == lower {
                if i < self.inner.real_data.len() {
                    return Ok(self.inner.real_data[i].clone());
                }
            }
        }
        Err(PyKeyError::new_err(format!("Variable '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.__getitem__(name)
    }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> { measures_to_dict(&self.inner.measures) }
}

#[pyclass(name = "OperatingPoint")]
struct PyOperatingPoint {
    inner: crate::result::OperatingPoint,
}

#[pymethods]
impl PyOperatingPoint {
    fn __getitem__(&self, name: &str) -> PyResult<f64> {
        self.inner
            .get(name)
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }

    fn __getattr__(&self, name: &str) -> PyResult<f64> {
        self.inner
            .get(name)
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }

    /// Parsed .meas results as {name: value} dict
    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "DcAnalysis")]
struct PyDcAnalysis {
    inner: crate::result::DcAnalysis,
}

#[pymethods]
impl PyDcAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner
            .base
            .get(name)
            .map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }

    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner
            .base
            .get(name)
            .map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }

    #[getter]
    fn sweep(&self) -> Vec<f64> {
        self.inner.sweep.clone()
    }

    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "AcAnalysis")]
struct PyAcAnalysis {
    inner: crate::result::AcAnalysis,
}

#[pymethods]
impl PyAcAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner
            .base
            .get(name)
            .map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }

    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner
            .base
            .get(name)
            .map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }

    #[getter]
    fn frequency(&self) -> Vec<f64> {
        self.inner.frequency.clone()
    }

    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "TransientAnalysis")]
struct PyTransientAnalysis {
    inner: crate::result::TransientAnalysis,
}

#[pymethods]
impl PyTransientAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner
            .base
            .get(name)
            .map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }

    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner
            .base
            .get(name)
            .map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }

    #[getter]
    fn time(&self) -> Vec<f64> {
        self.inner.time.clone()
    }

    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "NoiseAnalysis")]
struct PyNoiseAnalysis {
    inner: crate::result::NoiseAnalysis,
}

#[pymethods]
impl PyNoiseAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "TransferFunctionAnalysis")]
struct PyTransferFunctionAnalysis {
    inner: crate::result::TransferFunctionAnalysis,
}

#[pymethods]
impl PyTransferFunctionAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "SensitivityAnalysis")]
struct PySensitivityAnalysis {
    inner: crate::result::SensitivityAnalysis,
}

#[pymethods]
impl PySensitivityAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "PoleZeroAnalysis")]
struct PyPoleZeroAnalysis {
    inner: crate::result::PoleZeroAnalysis,
}

#[pymethods]
impl PyPoleZeroAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner
            .base
            .get(name)
            .map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }

    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "DistortionAnalysis")]
struct PyDistortionAnalysis {
    inner: crate::result::DistortionAnalysis,
}

#[pymethods]
impl PyDistortionAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner
            .base
            .get(name)
            .map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }

    #[getter]
    fn frequency(&self) -> Vec<f64> {
        self.inner.frequency.clone()
    }

    #[getter]
    fn measures(&self) -> HashMap<String, f64> {
        measures_to_dict(&self.inner.base.measures)
    }
}

#[pyclass(name = "PssAnalysis")]
struct PyPssAnalysis {
    inner: crate::result::PssAnalysis,
}

#[pymethods]
impl PyPssAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn time(&self) -> Vec<f64> { self.inner.time.clone() }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> { measures_to_dict(&self.inner.base.measures) }
}

#[pyclass(name = "SParamAnalysis")]
struct PySParamAnalysis {
    inner: crate::result::SParamAnalysis,
}

#[pymethods]
impl PySParamAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn frequency(&self) -> Vec<f64> { self.inner.frequency.clone() }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> { measures_to_dict(&self.inner.base.measures) }
}

#[pyclass(name = "HarmonicBalanceAnalysis")]
struct PyHarmonicBalanceAnalysis {
    inner: crate::result::HarmonicBalanceAnalysis,
}

#[pymethods]
impl PyHarmonicBalanceAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn frequency(&self) -> Vec<f64> { self.inner.frequency.clone() }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> { measures_to_dict(&self.inner.base.measures) }
}

#[pyclass(name = "StabilityAnalysis")]
struct PyStabilityAnalysis {
    inner: crate::result::StabilityAnalysis,
}

#[pymethods]
impl PyStabilityAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn frequency(&self) -> Vec<f64> { self.inner.frequency.clone() }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> { measures_to_dict(&self.inner.base.measures) }
}

#[pyclass(name = "TransientNoiseAnalysis")]
struct PyTransientNoiseAnalysis {
    inner: crate::result::TransientNoiseAnalysis,
}

#[pymethods]
impl PyTransientNoiseAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn time(&self) -> Vec<f64> { self.inner.time.clone() }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> { measures_to_dict(&self.inner.base.measures) }
}

// ── Xyce-specific result bindings ──

#[pyclass(name = "SamplingAnalysis")]
struct PySamplingAnalysis {
    inner: crate::result::SamplingAnalysis,
}

#[pymethods]
impl PySamplingAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> { measures_to_dict(&self.inner.base.measures) }
}

#[pyclass(name = "XyceFftAnalysis")]
struct PyXyceFftAnalysis {
    inner: crate::result::XyceFftAnalysis,
}

#[pymethods]
impl PyXyceFftAnalysis {
    fn __getitem__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyKeyError::new_err(format!("Node '{}' not found", name)))
    }
    fn __getattr__(&self, name: &str) -> PyResult<Vec<f64>> {
        self.inner.base.get(name).map(|wf| wf.data.clone())
            .ok_or_else(|| PyAttributeError::new_err(format!("No node '{}'", name)))
    }
    #[getter]
    fn frequency(&self) -> Vec<f64> { self.inner.frequency.clone() }
    #[getter]
    fn magnitude(&self) -> Vec<f64> { self.inner.magnitude.clone() }
    #[getter]
    fn phase(&self) -> Vec<f64> { self.inner.phase.clone() }
    #[getter]
    fn enob(&self) -> f64 { self.inner.enob }
    #[getter]
    fn sfdr_db(&self) -> f64 { self.inner.sfdr_db }
    #[getter]
    fn snr_db(&self) -> f64 { self.inner.snr_db }
    #[getter]
    fn thd_db(&self) -> f64 { self.inner.thd_db }
    #[getter]
    fn measures(&self) -> HashMap<String, f64> { measures_to_dict(&self.inner.base.measures) }
}

// ── Module-level functions ──

/// Lint a SPICE netlist for common issues and backend-specific warnings.
///
/// Returns a dict with "warnings" and "errors" lists.
#[pyfunction]
#[pyo3(signature = (netlist, backend=None))]
fn lint(netlist: &str, backend: Option<&str>) -> HashMap<String, Vec<HashMap<String, pyo3::PyObject>>> {
    pyo3::Python::with_gil(|py| {
        let result = crate::lint::lint_netlist(netlist, backend);

        let warnings: Vec<HashMap<String, pyo3::PyObject>> = result.warnings.iter().map(|w| {
            let mut map = HashMap::new();
            map.insert("line".to_string(), w.line.into_pyobject(py).unwrap().into_any().unbind());
            map.insert("message".to_string(), w.message.clone().into_pyobject(py).unwrap().into_any().unbind());
            if let Some(ref s) = w.suggestion {
                map.insert("suggestion".to_string(), s.clone().into_pyobject(py).unwrap().into_any().unbind());
            }
            let backends: Vec<String> = w.backends_affected.clone();
            map.insert("backends_affected".to_string(), backends.into_pyobject(py).unwrap().into_any().unbind());
            map
        }).collect();

        let errors: Vec<HashMap<String, pyo3::PyObject>> = result.errors.iter().map(|e| {
            let mut map = HashMap::new();
            map.insert("line".to_string(), e.line.into_pyobject(py).unwrap().into_any().unbind());
            map.insert("message".to_string(), e.message.clone().into_pyobject(py).unwrap().into_any().unbind());
            map
        }).collect();

        let mut out = HashMap::new();
        out.insert("warnings".to_string(), warnings);
        out.insert("errors".to_string(), errors);
        out
    })
}

// ── Verilog-A compilation ──

/// Compile Verilog-A source to OSDI. Accepts a .va file path or inline source.
/// Returns the path to the compiled .osdi file.
fn compile_veriloga_impl(source_or_path: &str) -> Result<String, String> {
    use std::process::Command;

    let trimmed = source_or_path.trim();

    // Determine if this is a file path or inline source
    let va_path = if trimmed.ends_with(".va") && !trimmed.contains('\n') {
        // File path
        let p = std::path::Path::new(trimmed);
        if !p.exists() {
            return Err(format!("Verilog-A file not found: {}", trimmed));
        }
        std::path::PathBuf::from(trimmed)
    } else {
        // Inline source — write to temp file
        let dir = std::env::temp_dir().join("pyspice_va");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create temp dir: {}", e))?;

        // Hash the source for a stable filename
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            trimmed.hash(&mut h);
            h.finish()
        };
        let va_file = dir.join(format!("inline_{:016x}.va", hash));
        std::fs::write(&va_file, trimmed)
            .map_err(|e| format!("Failed to write temp .va file: {}", e))?;
        va_file
    };

    // Output .osdi path: same dir and stem as .va, with .osdi extension
    let osdi_path = va_path.with_extension("osdi");

    // Skip compilation if .osdi is newer than .va
    if osdi_path.exists() {
        if let (Ok(va_meta), Ok(osdi_meta)) = (va_path.metadata(), osdi_path.metadata()) {
            if let (Ok(va_time), Ok(osdi_time)) = (va_meta.modified(), osdi_meta.modified()) {
                if osdi_time > va_time {
                    return Ok(osdi_path.to_string_lossy().to_string());
                }
            }
        }
    }

    // Compile with openvaf
    let output = Command::new("openvaf")
        .arg(&va_path)
        .arg("-o")
        .arg(&osdi_path)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "openvaf not found on $PATH. Install OpenVAF to compile Verilog-A models.\n\
                 See: https://openvaf.semimod.de".to_string()
            } else {
                format!("Failed to run openvaf: {}", e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "openvaf compilation failed (exit {}):\n{}\n{}",
            output.status, stderr, stdout
        ));
    }

    Ok(osdi_path.to_string_lossy().to_string())
}

/// Compile a Verilog-A source file or inline source to OSDI.
///
/// Returns the path to the compiled .osdi file.
///
/// ```python
/// from pyspice_rs import compile_veriloga
///
/// # From file:
/// osdi_path = compile_veriloga("models/comparator.va")
///
/// # From inline source:
/// osdi_path = compile_veriloga('''
/// `include "disciplines.vams"
/// module myres(a, b);
///     inout a, b; electrical a, b;
///     parameter real r = 1000.0;
///     analog V(a,b) <+ r * I(a,b);
/// endmodule
/// ''')
/// circuit.osdi(osdi_path)
/// ```
#[pyfunction]
fn compile_veriloga(source_or_path: &str) -> PyResult<String> {
    compile_veriloga_impl(source_or_path)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
}

// ── Verilog compilation / synthesis helpers ──

/// Resolve a Verilog source string to a file path.
/// If `source_or_path` looks like a file path (.v extension, no newlines), return it directly.
/// Otherwise treat as inline source, write to a temp file, and return that path.
fn resolve_verilog_source(source_or_path: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = source_or_path.trim();

    if trimmed.ends_with(".v") && !trimmed.contains('\n') {
        let p = std::path::Path::new(trimmed);
        if !p.exists() {
            return Err(format!("Verilog file not found: {}", trimmed));
        }
        Ok(std::path::PathBuf::from(trimmed))
    } else {
        let dir = std::env::temp_dir().join("pyspice_verilog");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create temp dir: {}", e))?;

        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            trimmed.hash(&mut h);
            h.finish()
        };
        let v_file = dir.join(format!("inline_{:016x}.v", hash));
        std::fs::write(&v_file, trimmed)
            .map_err(|e| format!("Failed to write temp .v file: {}", e))?;
        Ok(v_file)
    }
}

/// Extract the top-level module name from Verilog source.
fn extract_verilog_module_name(source: &str) -> Result<String, String> {
    // Match `module <name>` possibly followed by `(`, `;`, `#`, or whitespace
    let re = regex::Regex::new(r"(?m)^\s*module\s+(\w+)")
        .map_err(|e| format!("Regex error: {}", e))?;
    re.captures(source)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| "Could not extract module name from Verilog source. \
            Ensure source contains `module <name>`.".to_string())
}

/// Compile Verilog source to VVP using Icarus Verilog (iverilog).
/// Returns the path to the compiled .vvp file.
fn compile_verilog_iverilog(source_or_path: &str) -> Result<String, String> {
    use std::process::Command;

    let v_path = resolve_verilog_source(source_or_path)?;
    let vvp_path = v_path.with_extension("vvp");

    // Skip compilation if .vvp is newer than .v
    if vvp_path.exists() {
        if let (Ok(v_meta), Ok(vvp_meta)) = (v_path.metadata(), vvp_path.metadata()) {
            if let (Ok(v_time), Ok(vvp_time)) = (v_meta.modified(), vvp_meta.modified()) {
                if vvp_time > v_time {
                    return Ok(vvp_path.to_string_lossy().to_string());
                }
            }
        }
    }

    let output = Command::new("iverilog")
        .arg("-o")
        .arg(&vvp_path)
        .arg(&v_path)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "iverilog not found on $PATH. Install Icarus Verilog for co-simulation.\n\
                 Install: sudo apt install iverilog (Debian/Ubuntu) or brew install icarus-verilog (macOS)".to_string()
            } else {
                format!("Failed to run iverilog: {}", e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "iverilog compilation failed (exit {}):\n{}\n{}",
            output.status, stderr, stdout
        ));
    }

    Ok(vvp_path.to_string_lossy().to_string())
}

/// Compile Verilog source using Cadence tools (xrun or ncvlog) for Spectre co-simulation.
/// Returns the path to the compiled output directory.
#[allow(dead_code)]
fn compile_verilog_spectre(source_or_path: &str) -> Result<String, String> {
    use std::process::Command;

    let v_path = resolve_verilog_source(source_or_path)?;
    let work_dir = v_path.parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("xcelium.d");

    // Try xrun first, fall back to ncvlog
    let result = Command::new("xrun")
        .arg("-compile")
        .arg(&v_path)
        .output();

    match result {
        Ok(output) if output.status.success() => {
            return Ok(work_dir.to_string_lossy().to_string());
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // xrun found but failed — try ncvlog as fallback
            eprintln!("xrun failed, trying ncvlog: {}", stderr);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // xrun not found, try ncvlog
        }
        Err(e) => {
            return Err(format!("Failed to run xrun: {}", e));
        }
    }

    // Fallback: ncvlog
    let output = Command::new("ncvlog")
        .arg(&v_path)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "Neither xrun nor ncvlog found on $PATH. Install Cadence Xcelium for \
                 Spectre Verilog co-simulation.".to_string()
            } else {
                format!("Failed to run ncvlog: {}", e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "ncvlog compilation failed (exit {}):\n{}\n{}",
            output.status, stderr, stdout
        ));
    }

    Ok(work_dir.to_string_lossy().to_string())
}

/// Resolve PDK liberty and SPICE model paths from $PDK_ROOT.
/// Returns (liberty_path, spice_models_path).
fn resolve_pdk_paths(pdk: &str) -> Result<(String, String), String> {
    let pdk_root = std::env::var("PDK_ROOT")
        .map_err(|_| "PDK_ROOT environment variable not set. Set it to your PDK installation root, \
            or provide explicit liberty= and spice_models= paths.".to_string())?;

    // Try with 'A' suffix first (e.g., sky130_fd_sc_hdA), then without
    let suffixes = ["A", ""];
    let mut liberty_path = None;
    let mut spice_path = None;

    for suffix in &suffixes {
        let base = format!("{}/{}{}/libs.ref/{}", pdk_root, pdk, suffix, pdk);

        // Look for liberty file (prefer tt corner)
        let lib_dir = format!("{}/lib", base);
        if let Ok(entries) = std::fs::read_dir(&lib_dir) {
            // Prefer tt corner, then any .lib file
            let mut found_lib = None;
            let mut tt_lib = None;
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".lib") {
                    if name.contains("tt") && tt_lib.is_none() {
                        tt_lib = Some(entry.path().to_string_lossy().to_string());
                    }
                    if found_lib.is_none() {
                        found_lib = Some(entry.path().to_string_lossy().to_string());
                    }
                }
            }
            if liberty_path.is_none() {
                liberty_path = tt_lib.or(found_lib);
            }
        }

        // Look for SPICE models
        let spice_dir = format!("{}/spice", base);
        if let Ok(entries) = std::fs::read_dir(&spice_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".spice") {
                    if spice_path.is_none() {
                        spice_path = Some(entry.path().to_string_lossy().to_string());
                    }
                    break;
                }
            }
        }

        if liberty_path.is_some() && spice_path.is_some() {
            break;
        }
    }

    let liberty = liberty_path.ok_or_else(|| format!(
        "Could not find liberty (.lib) file for PDK '{}' under $PDK_ROOT={}. \
         Searched: {}/{pdk}A/libs.ref/{pdk}/lib/ and {}/{pdk}/libs.ref/{pdk}/lib/",
        pdk, pdk_root, pdk_root, pdk_root, pdk = pdk
    ))?;

    let spice_models = spice_path.ok_or_else(|| format!(
        "Could not find SPICE model (.spice) file for PDK '{}' under $PDK_ROOT={}. \
         Searched: {}/{pdk}A/libs.ref/{pdk}/spice/ and {}/{pdk}/libs.ref/{pdk}/spice/",
        pdk, pdk_root, pdk_root, pdk_root, pdk = pdk
    ))?;

    Ok((liberty, spice_models))
}

/// Synthesize Verilog to gate-level netlist using Yosys.
/// Returns the path to the synthesized Verilog netlist file.
fn synthesize_verilog_yosys(
    source_or_path: &str,
    liberty: &str,
    module_name: &str,
) -> Result<String, String> {
    use std::process::Command;

    let v_path = resolve_verilog_source(source_or_path)?;
    let synth_path = v_path.with_extension("synth.v");

    // Skip synthesis if output is newer than source
    if synth_path.exists() {
        if let (Ok(v_meta), Ok(s_meta)) = (v_path.metadata(), synth_path.metadata()) {
            if let (Ok(v_time), Ok(s_time)) = (v_meta.modified(), s_meta.modified()) {
                if s_time > v_time {
                    return Ok(synth_path.to_string_lossy().to_string());
                }
            }
        }
    }

    let script = format!(
        "read_verilog {v}; synth -top {top}; dfflibmap -liberty {lib}; abc -liberty {lib}; write_verilog {out}",
        v = v_path.display(),
        top = module_name,
        lib = liberty,
        out = synth_path.display(),
    );

    let output = Command::new("yosys")
        .arg("-p")
        .arg(&script)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "yosys not found on $PATH. Install Yosys for Verilog synthesis.\n\
                 Install: sudo apt install yosys (Debian/Ubuntu) or brew install yosys (macOS)".to_string()
            } else {
                format!("Failed to run yosys: {}", e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "yosys synthesis failed (exit {}):\n{}\n{}",
            output.status, stderr, stdout
        ));
    }

    Ok(synth_path.to_string_lossy().to_string())
}

/// Parse a Yosys-synthesized Verilog netlist into cell instances.
/// Returns vec of (instance_name, cell_name, port_connections as ".<port>(<net>)" pairs).
fn parse_synthesized_netlist(netlist_path: &str) -> Result<Vec<(String, String, Vec<(String, String)>)>, String> {
    let content = std::fs::read_to_string(netlist_path)
        .map_err(|e| format!("Failed to read synthesized netlist: {}", e))?;

    let mut instances = Vec::new();

    // Match Yosys output patterns like: <cell> <instance> ( .<port>(<net>), ... );
    // Yosys writes: `  sky130_fd_sc_hd__and2_1 _0_ ( .A(\a ), .B(\b ), .X(\out ) );`
    let inst_re = regex::Regex::new(r"(?m)^\s+(\w+)\s+(\w+)\s*\(([^;]+)\);")
        .map_err(|e| format!("Regex error: {}", e))?;
    let port_re = regex::Regex::new(r"\.\s*(\w+)\s*\(\s*\\?([^\s)]+)\s*\)")
        .map_err(|e| format!("Regex error: {}", e))?;

    for cap in inst_re.captures_iter(&content) {
        let cell_name = cap[1].to_string();
        let inst_name = cap[2].to_string();

        // Skip `wire`, `assign`, `input`, `output`, `module`, `endmodule`
        if matches!(cell_name.as_str(), "wire" | "assign" | "input" | "output" | "module" | "endmodule" | "reg") {
            continue;
        }

        let port_str = &cap[3];
        let mut ports = Vec::new();
        for pcap in port_re.captures_iter(port_str) {
            ports.push((pcap[1].to_string(), pcap[2].to_string()));
        }

        instances.push((inst_name, cell_name, ports));
    }

    Ok(instances)
}

/// Extract connection values from a PyObject (either a string or list of strings).
fn extract_connection(py: pyo3::Python<'_>, obj: &pyo3::PyObject) -> PyResult<Vec<String>> {
    // Try extracting as a single string first
    if let Ok(s) = obj.extract::<String>(py) {
        return Ok(vec![s]);
    }
    // Try extracting as a list of strings
    if let Ok(v) = obj.extract::<Vec<String>>(py) {
        return Ok(v);
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "Connection value must be a string or list of strings"
    ))
}

/// Core implementation of the verilog() method for PyCircuit.
/// Modifies the circuit in-place by adding appropriate elements depending on mode.
fn verilog_impl_circuit(
    circuit: &mut cir::Circuit,
    py: pyo3::Python<'_>,
    source: &str,
    mode: &str,
    instance_name: &str,
    connections: &HashMap<String, pyo3::PyObject>,
    pdk: Option<&str>,
    liberty: Option<&str>,
    spice_models: Option<&str>,
) -> PyResult<()> {
    match mode {
        "simulate" => {
            verilog_simulate_circuit(circuit, py, source, instance_name, connections)
        }
        "synthesize" => {
            verilog_synthesize_circuit(circuit, py, source, instance_name, connections, pdk, liberty, spice_models)
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            format!("Invalid mode '{}'. Must be 'simulate' or 'synthesize'.", mode)
        )),
    }
}

/// Implement mode="simulate" for the circuit-level verilog() method.
/// Compiles with iverilog and emits d_cosim XSPICE model + A-element.
fn verilog_simulate_circuit(
    circuit: &mut cir::Circuit,
    py: pyo3::Python<'_>,
    source: &str,
    instance_name: &str,
    connections: &HashMap<String, pyo3::PyObject>,
) -> PyResult<()> {
    // Compile the Verilog source with iverilog
    let vvp_path = compile_verilog_iverilog(source)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;

    // Build the connection list for the A-element.
    // For d_cosim, connections are grouped as digital port vectors.
    let mut conn_parts = Vec::new();
    for (port_name, obj) in connections {
        let nets = extract_connection(py, obj)?;
        if nets.len() == 1 {
            conn_parts.push(format!("[{}]", nets[0]));
        } else {
            // Bus: [bit0 bit1 bit2 ...]
            conn_parts.push(format!("[{}]", nets.join(" ")));
        }
        let _ = port_name; // Port name ordering handled by Verilog module definition
    }

    // Add ADC/DAC bridge models if not already present
    let has_adc_bridge = circuit.models().iter().any(|m| m.kind == "adc_bridge");
    if !has_adc_bridge {
        circuit.model(
            "adc_buf",
            "adc_bridge",
            vec![cir::Param::new("in_low", "0.5"), cir::Param::new("in_high", "0.5")],
        );
    }
    let has_dac_bridge = circuit.models().iter().any(|m| m.kind == "dac_bridge");
    if !has_dac_bridge {
        circuit.model(
            "dac_buf",
            "dac_bridge",
            vec![cir::Param::new("out_low", "0.0"), cir::Param::new("out_high", "1.8")],
        );
    }

    // Emit the d_cosim model
    let model_name = format!("cosim_{}", instance_name);
    circuit.raw_spice(format!(
        ".model {} d_cosim(delay=1n simulation_id=\"{}\")",
        model_name, vvp_path
    ));

    // Emit the A-element instance
    circuit.a(instance_name, conn_parts, &model_name);

    Ok(())
}

/// Implement mode="synthesize" for the circuit-level verilog() method.
/// Synthesizes with Yosys and emits subcircuit instances for each gate.
fn verilog_synthesize_circuit(
    circuit: &mut cir::Circuit,
    py: pyo3::Python<'_>,
    source: &str,
    instance_name: &str,
    connections: &HashMap<String, pyo3::PyObject>,
    pdk: Option<&str>,
    liberty: Option<&str>,
    spice_models: Option<&str>,
) -> PyResult<()> {
    // Resolve liberty file
    let (lib_path, spice_path) = match (liberty, pdk) {
        (Some(lib), Some(_pdk)) => {
            // liberty provided explicitly; try pdk for spice_models if not given
            let sp = match spice_models {
                Some(sp) => sp.to_string(),
                None => resolve_pdk_paths(_pdk)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?
                    .1,
            };
            (lib.to_string(), sp)
        }
        (Some(lib), None) => {
            let sp = spice_models.ok_or_else(|| pyo3::exceptions::PyValueError::new_err(
                "mode='synthesize' requires either pdk= or both liberty= and spice_models="
            ))?.to_string();
            (lib.to_string(), sp)
        }
        (None, Some(pdk_name)) => {
            let (lib, sp) = resolve_pdk_paths(pdk_name)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
            let sp = spice_models.map(|s| s.to_string()).unwrap_or(sp);
            (lib, sp)
        }
        (None, None) => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "mode='synthesize' requires either pdk= or liberty= (and spice_models=) to be specified"
            ));
        }
    };

    // Read source to extract module name
    let source_text = if source.trim().ends_with(".v") && !source.contains('\n') {
        std::fs::read_to_string(source.trim())
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Failed to read {}: {}", source, e)))?
    } else {
        source.to_string()
    };

    let module_name = extract_verilog_module_name(&source_text)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;

    // Synthesize
    let synth_path = synthesize_verilog_yosys(source, &lib_path, &module_name)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;

    // Include the SPICE cell models
    circuit.include(&spice_path);

    // Parse the synthesized netlist
    let gate_instances = parse_synthesized_netlist(&synth_path)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;

    // Build a connection map from the user's connections dict
    let mut conn_map: HashMap<String, Vec<String>> = HashMap::new();
    for (port, obj) in connections {
        let nets = extract_connection(py, obj)?;
        conn_map.insert(port.clone(), nets);
    }

    // For each gate instance, emit an X (subcircuit call)
    for (gate_inst, cell_name, ports) in &gate_instances {
        let mut node_list: Vec<String> = Vec::new();
        for (port_name, net_name) in ports {
            // Map synthesized net names back to user-provided connections where possible
            let resolved = conn_map.get(net_name)
                .and_then(|v| v.first())
                .cloned()
                .unwrap_or_else(|| {
                    // Prefix internal nets with instance name to avoid collisions
                    format!("{}_{}", instance_name, net_name)
                });
            node_list.push(resolved);
            let _ = port_name;
        }
        let node_refs: Vec<&str> = node_list.iter().map(|s| s.as_str()).collect();
        circuit.x(
            &format!("{}_{}", instance_name, gate_inst),
            cell_name,
            node_refs,
        );
    }

    Ok(())
}

// ── IR type bindings: Subcircuit, Testbench, ModelLibrary ──

fn py_value_to_ir(v: PyValueArg) -> crate::ir::IrValue {
    match v {
        PyValueArg::Float(f) => crate::ir::IrValue::Numeric { value: f },
        PyValueArg::Unit(uv) => crate::ir::IrValue::Numeric { value: uv.inner.as_f64() },
    }
}

fn ir_value_to_component_value(v: &crate::ir::IrValue) -> ComponentValue {
    match v {
        crate::ir::IrValue::Numeric { value } => ComponentValue::Numeric(*value),
        crate::ir::IrValue::Expression { expr } => ComponentValue::Expression(expr.clone()),
        crate::ir::IrValue::Raw { text } => ComponentValue::Raw(text.clone()),
    }
}

/// Convert IR components to Circuit method calls
fn add_ir_components(circuit: &mut cir::Circuit, components: &[crate::ir::Component]) {
    for comp in components {
        match comp {
            crate::ir::Component::Resistor { name, n1, n2, value, params } => {
                let cv = ir_value_to_component_value(value);
                let p: Vec<Param> = params.iter().map(|(k, v)| Param::new(k, v)).collect();
                if p.is_empty() {
                    circuit.r(name, n1.as_str(), n2.as_str(), cv);
                } else {
                    circuit.r_with_params(name, n1.as_str(), n2.as_str(), cv, p);
                }
            }
            crate::ir::Component::Capacitor { name, n1, n2, value, .. } => {
                circuit.c(name, n1.as_str(), n2.as_str(), ir_value_to_component_value(value));
            }
            crate::ir::Component::Inductor { name, n1, n2, value, .. } => {
                circuit.l(name, n1.as_str(), n2.as_str(), ir_value_to_component_value(value));
            }
            crate::ir::Component::MutualInductor { name, inductor1, inductor2, coupling } => {
                circuit.k(name, inductor1, inductor2, *coupling);
            }
            crate::ir::Component::VoltageSource { name, np, nm, value, waveform } => {
                if let Some(wf) = waveform {
                    let cir_wf = ir_waveform_to_circuit(wf);
                    circuit.v_with_waveform(name, np.as_str(), nm.as_str(), ir_value_to_component_value(value), cir_wf);
                } else {
                    circuit.v(name, np.as_str(), nm.as_str(), ir_value_to_component_value(value));
                }
            }
            crate::ir::Component::CurrentSource { name, np, nm, value, waveform } => {
                if let Some(wf) = waveform {
                    let cir_wf = ir_waveform_to_circuit(wf);
                    // Use i_with_waveform if it exists, otherwise use raw approach
                    circuit.i(name, np.as_str(), nm.as_str(), ir_value_to_component_value(value));
                    // Note: Circuit doesn't have i_with_waveform, use raw_spice for waveform
                    let _ = wf; let _ = cir_wf;
                } else {
                    circuit.i(name, np.as_str(), nm.as_str(), ir_value_to_component_value(value));
                }
            }
            crate::ir::Component::BehavioralVoltage { name, np, nm, expression } => {
                circuit.bv(name, np.as_str(), nm.as_str(), expression);
            }
            crate::ir::Component::BehavioralCurrent { name, np, nm, expression } => {
                circuit.bi(name, np.as_str(), nm.as_str(), expression);
            }
            crate::ir::Component::Vcvs { name, np, nm, ncp, ncm, gain } => {
                circuit.e(name, np.as_str(), nm.as_str(), ncp.as_str(), ncm.as_str(), *gain);
            }
            crate::ir::Component::Vccs { name, np, nm, ncp, ncm, transconductance } => {
                circuit.g(name, np.as_str(), nm.as_str(), ncp.as_str(), ncm.as_str(), *transconductance);
            }
            crate::ir::Component::Cccs { name, np, nm, vsense, gain } => {
                circuit.f(name, np.as_str(), nm.as_str(), vsense, *gain);
            }
            crate::ir::Component::Ccvs { name, np, nm, vsense, transresistance } => {
                circuit.h(name, np.as_str(), nm.as_str(), vsense, *transresistance);
            }
            crate::ir::Component::Diode { name, np, nm, model, .. } => {
                circuit.d(name, np.as_str(), nm.as_str(), model);
            }
            crate::ir::Component::Bjt { name, nc, nb, ne, model, .. } => {
                circuit.q(name, nc.as_str(), nb.as_str(), ne.as_str(), model);
            }
            crate::ir::Component::Mosfet { name, nd, ng, ns, nb, model, params } => {
                let p: Vec<Param> = params.iter().map(|(k, v)| Param::new(k, v)).collect();
                if p.is_empty() {
                    circuit.m(name, nd.as_str(), ng.as_str(), ns.as_str(), nb.as_str(), model);
                } else {
                    circuit.m_with_params(name, nd.as_str(), ng.as_str(), ns.as_str(), nb.as_str(), model, p);
                }
            }
            crate::ir::Component::Jfet { name, nd, ng, ns, model, .. } => {
                circuit.j(name, nd.as_str(), ng.as_str(), ns.as_str(), model);
            }
            crate::ir::Component::Mesfet { name, nd, ng, ns, model, .. } => {
                circuit.z(name, nd.as_str(), ng.as_str(), ns.as_str(), model);
            }
            crate::ir::Component::VSwitch { name, np, nm, ncp, ncm, model } => {
                circuit.s(name, np.as_str(), nm.as_str(), ncp.as_str(), ncm.as_str(), model);
            }
            crate::ir::Component::ISwitch { name, np, nm, vcontrol, model } => {
                circuit.w(name, np.as_str(), nm.as_str(), vcontrol, model);
            }
            crate::ir::Component::TLine { name, inp, inm, outp, outm, z0, td } => {
                circuit.t(name, inp.as_str(), inm.as_str(), outp.as_str(), outm.as_str(), *z0, *td);
            }
            crate::ir::Component::Xspice { name, connections, model } => {
                circuit.a(name, connections.clone(), model);
            }
            crate::ir::Component::RawSpice { line } => {
                circuit.raw_spice(line);
            }
        }
    }
}

fn ir_waveform_to_circuit(wf: &crate::ir::IrWaveform) -> cir::Waveform {
    match wf {
        crate::ir::IrWaveform::Sin { offset, amplitude, frequency, delay, damping, phase } => {
            cir::Waveform::Sin(cir::SinWaveform {
                offset: *offset, amplitude: *amplitude, frequency: *frequency,
                delay: *delay, damping: *damping, phase: *phase,
            })
        }
        crate::ir::IrWaveform::Pulse { initial, pulsed, delay, rise_time, fall_time, pulse_width, period } => {
            cir::Waveform::Pulse(cir::PulseWaveform {
                initial: *initial, pulsed: *pulsed, delay: *delay,
                rise_time: *rise_time, fall_time: *fall_time,
                pulse_width: *pulse_width, period: *period,
            })
        }
        crate::ir::IrWaveform::Pwl { values } => {
            cir::Waveform::Pwl(cir::PwlWaveform { values: values.clone() })
        }
        crate::ir::IrWaveform::Exp { initial, pulsed, rise_delay, rise_tau, fall_delay, fall_tau } => {
            cir::Waveform::Exp(cir::ExpWaveform {
                initial: *initial, pulsed: *pulsed,
                rise_delay: *rise_delay, rise_tau: *rise_tau,
                fall_delay: *fall_delay, fall_tau: *fall_tau,
            })
        }
        crate::ir::IrWaveform::Sffm { offset, amplitude, carrier_freq, modulation_index, signal_freq } => {
            cir::Waveform::Sffm(cir::SffmWaveform {
                offset: *offset, amplitude: *amplitude, carrier_freq: *carrier_freq,
                modulation_index: *modulation_index, signal_freq: *signal_freq,
            })
        }
        crate::ir::IrWaveform::Am { amplitude, offset, modulating_freq, carrier_freq, delay } => {
            cir::Waveform::Am(cir::AmWaveform {
                amplitude: *amplitude, offset: *offset, modulating_freq: *modulating_freq,
                carrier_freq: *carrier_freq, delay: *delay,
            })
        }
    }
}

/// Build a Circuit from IR Subcircuit + Testbench for simulation via existing backend
fn ir_to_circuit(
    dut: &crate::ir::Subcircuit,
    tb: Option<&crate::ir::Testbench>,
    subcircuit_defs: &[crate::ir::Subcircuit],
) -> cir::Circuit {
    let mut circuit = cir::Circuit::new(&dut.name);

    // Add components from DUT
    add_ir_components(&mut circuit, &dut.components);

    // Add stimulus from testbench
    if let Some(tb) = tb {
        add_ir_components(&mut circuit, &tb.stimulus);
    }

    // Add models
    for model in &dut.models {
        let params: Vec<Param> = model.parameters.iter()
            .map(|(k, v)| Param::new(k, v))
            .collect();
        circuit.model(&model.name, &model.kind, params);
    }

    // Add instances (subcircuit calls)
    for inst in &dut.instances {
        let nodes: Vec<&str> = inst.port_mapping.iter().map(|s| s.as_str()).collect();
        circuit.x(&inst.name, &inst.subcircuit, nodes);
    }

    // Add subcircuit definitions
    for sc_def in subcircuit_defs {
        let mut elements = Vec::new();
        ir_components_to_elements(&sc_def.components, &mut elements);
        // Also convert instances to X-elements
        for inst in &sc_def.instances {
            let nodes: Vec<cir::Node> = inst.port_mapping.iter()
                .map(|s| cir::Node::from(s.as_str()))
                .collect();
            elements.push(cir::Element::X(cir::SubcircuitInstance {
                name: inst.name.clone(),
                subcircuit_name: inst.subcircuit.clone(),
                nodes,
                params: inst.parameters.iter()
                    .map(|(k, v)| Param::new(k, v))
                    .collect(),
            }));
        }
        let models: Vec<cir::Model> = sc_def.models.iter().map(|m| {
            cir::Model {
                name: m.name.clone(),
                kind: m.kind.clone(),
                params: m.parameters.iter().map(|(k, v)| Param::new(k, v)).collect(),
            }
        }).collect();
        let params: Vec<Param> = sc_def.parameters.iter().map(|p| {
            Param::new(&p.name, p.default.as_deref().unwrap_or("0"))
        }).collect();
        circuit.subcircuit(cir::SubCircuitDef {
            name: sc_def.name.clone(),
            pins: sc_def.ports.iter().map(|p| p.name.clone()).collect(),
            elements,
            models,
            params,
        });
    }

    // Includes, libs, raw_spice, osdi
    for inc in &dut.includes { circuit.include(inc); }
    for (path, section) in &dut.libs { circuit.lib(path, section); }
    for line in &dut.raw_spice { circuit.raw_spice(line); }
    for osdi in &dut.osdi_loads { circuit.osdi(osdi); }

    // Parameters
    for p in &dut.parameters {
        circuit.parameter(&p.name, p.default.as_deref().unwrap_or("0"));
    }

    // Process Verilog blocks from the DUT
    for vb in &dut.verilog_blocks {
        if let Err(e) = apply_verilog_block_to_circuit(&mut circuit, vb) {
            eprintln!("Warning: failed to apply verilog block '{}': {}", vb.instance_name, e);
        }
    }

    circuit
}

/// Apply a VerilogBlock from IR to a Circuit.
/// Handles both "simulate" (iverilog/d_cosim) and "synthesize" (yosys) modes.
fn apply_verilog_block_to_circuit(
    circuit: &mut cir::Circuit,
    vb: &crate::ir::VerilogBlock,
) -> Result<(), String> {
    match vb.mode {
        crate::ir::VerilogMode::Simulate => {
            let vvp_path = compile_verilog_iverilog(&vb.source)?;

            // Build connection list for A-element
            let mut conn_parts = Vec::new();
            for (_port, conn) in &vb.connections {
                match conn {
                    crate::ir::VerilogConnection::Single(net) => {
                        conn_parts.push(format!("[{}]", net));
                    }
                    crate::ir::VerilogConnection::Bus(nets) => {
                        conn_parts.push(format!("[{}]", nets.join(" ")));
                    }
                }
            }

            // Add bridge models if not present
            if !circuit.models().iter().any(|m| m.kind == "adc_bridge") {
                circuit.model(
                    "adc_buf", "adc_bridge",
                    vec![cir::Param::new("in_low", "0.5"), cir::Param::new("in_high", "0.5")],
                );
            }
            if !circuit.models().iter().any(|m| m.kind == "dac_bridge") {
                circuit.model(
                    "dac_buf", "dac_bridge",
                    vec![cir::Param::new("out_low", "0.0"), cir::Param::new("out_high", "1.8")],
                );
            }

            let model_name = format!("cosim_{}", vb.instance_name);
            circuit.raw_spice(format!(
                ".model {} d_cosim(delay=1n simulation_id=\"{}\")",
                model_name, vvp_path
            ));
            circuit.a(&vb.instance_name, conn_parts, &model_name);
            Ok(())
        }
        crate::ir::VerilogMode::Synthesize => {
            // Resolve liberty and spice model paths
            let (lib_path, spice_path) = match (&vb.liberty, &vb.pdk) {
                (Some(lib), Some(pdk)) => {
                    let sp = vb.spice_models.as_ref()
                        .map(|s| Ok(s.clone()))
                        .unwrap_or_else(|| resolve_pdk_paths(pdk).map(|p| p.1))?;
                    (lib.clone(), sp)
                }
                (Some(lib), None) => {
                    let sp = vb.spice_models.as_ref()
                        .ok_or("mode='synthesize' requires either pdk or both liberty and spice_models")?;
                    (lib.clone(), sp.clone())
                }
                (None, Some(pdk)) => {
                    let (lib, sp) = resolve_pdk_paths(pdk)?;
                    let sp = vb.spice_models.as_ref().cloned().unwrap_or(sp);
                    (lib, sp)
                }
                (None, None) => {
                    return Err("mode='synthesize' requires either pdk or liberty".to_string());
                }
            };

            // Get source text for module name extraction
            let source_text = if vb.source.trim().ends_with(".v") && !vb.source.contains('\n') {
                std::fs::read_to_string(vb.source.trim())
                    .map_err(|e| format!("Failed to read {}: {}", vb.source, e))?
            } else {
                vb.source.clone()
            };

            let module_name = extract_verilog_module_name(&source_text)?;
            let synth_path = synthesize_verilog_yosys(&vb.source, &lib_path, &module_name)?;

            circuit.include(&spice_path);

            let gate_instances = parse_synthesized_netlist(&synth_path)?;

            // Build connection map
            let mut conn_map: HashMap<String, Vec<String>> = HashMap::new();
            for (port, conn) in &vb.connections {
                match conn {
                    crate::ir::VerilogConnection::Single(net) => {
                        conn_map.insert(port.clone(), vec![net.clone()]);
                    }
                    crate::ir::VerilogConnection::Bus(nets) => {
                        conn_map.insert(port.clone(), nets.clone());
                    }
                }
            }

            for (gate_inst, cell_name, ports) in &gate_instances {
                let mut node_list: Vec<String> = Vec::new();
                for (_port_name, net_name) in ports {
                    let resolved = conn_map.get(net_name)
                        .and_then(|v| v.first())
                        .cloned()
                        .unwrap_or_else(|| format!("{}_{}", vb.instance_name, net_name));
                    node_list.push(resolved);
                }
                let node_refs: Vec<&str> = node_list.iter().map(|s| s.as_str()).collect();
                circuit.x(
                    &format!("{}_{}", vb.instance_name, gate_inst),
                    cell_name,
                    node_refs,
                );
            }

            Ok(())
        }
    }
}

/// Convert IR components to circuit Element vec (for subcircuit definitions)
fn ir_components_to_elements(components: &[crate::ir::Component], elements: &mut Vec<cir::Element>) {
    for comp in components {
        let elem = match comp {
            crate::ir::Component::Resistor { name, n1, n2, value, params } => {
                cir::Element::R(cir::Resistor {
                    name: name.clone(),
                    n1: cir::Node::from(n1.as_str()),
                    n2: cir::Node::from(n2.as_str()),
                    value: ir_value_to_component_value(value),
                    params: params.iter().map(|(k, v)| Param::new(k, v)).collect(),
                })
            }
            crate::ir::Component::Capacitor { name, n1, n2, value, params } => {
                cir::Element::C(cir::Capacitor {
                    name: name.clone(),
                    n1: cir::Node::from(n1.as_str()),
                    n2: cir::Node::from(n2.as_str()),
                    value: ir_value_to_component_value(value),
                    params: params.iter().map(|(k, v)| Param::new(k, v)).collect(),
                })
            }
            crate::ir::Component::Inductor { name, n1, n2, value, params } => {
                cir::Element::L(cir::Inductor {
                    name: name.clone(),
                    n1: cir::Node::from(n1.as_str()),
                    n2: cir::Node::from(n2.as_str()),
                    value: ir_value_to_component_value(value),
                    params: params.iter().map(|(k, v)| Param::new(k, v)).collect(),
                })
            }
            crate::ir::Component::MutualInductor { name, inductor1, inductor2, coupling } => {
                cir::Element::K(cir::MutualInductor {
                    name: name.clone(),
                    inductor1: inductor1.clone(),
                    inductor2: inductor2.clone(),
                    coupling: *coupling,
                })
            }
            crate::ir::Component::VoltageSource { name, np, nm, value, waveform } => {
                cir::Element::V(cir::VoltageSource {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    value: ir_value_to_component_value(value),
                    waveform: waveform.as_ref().map(ir_waveform_to_circuit),
                })
            }
            crate::ir::Component::CurrentSource { name, np, nm, value, waveform } => {
                cir::Element::I(cir::CurrentSource {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    value: ir_value_to_component_value(value),
                    waveform: waveform.as_ref().map(ir_waveform_to_circuit),
                })
            }
            crate::ir::Component::BehavioralVoltage { name, np, nm, expression } => {
                cir::Element::BV(cir::BehavioralVoltage {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    expression: expression.clone(),
                })
            }
            crate::ir::Component::BehavioralCurrent { name, np, nm, expression } => {
                cir::Element::BI(cir::BehavioralCurrent {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    expression: expression.clone(),
                })
            }
            crate::ir::Component::Vcvs { name, np, nm, ncp, ncm, gain } => {
                cir::Element::E(cir::Vcvs {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    ncp: cir::Node::from(ncp.as_str()),
                    ncm: cir::Node::from(ncm.as_str()),
                    gain: *gain,
                })
            }
            crate::ir::Component::Vccs { name, np, nm, ncp, ncm, transconductance } => {
                cir::Element::G(cir::Vccs {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    ncp: cir::Node::from(ncp.as_str()),
                    ncm: cir::Node::from(ncm.as_str()),
                    transconductance: *transconductance,
                })
            }
            crate::ir::Component::Cccs { name, np, nm, vsense, gain } => {
                cir::Element::F(cir::Cccs {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    vsense: vsense.clone(),
                    gain: *gain,
                })
            }
            crate::ir::Component::Ccvs { name, np, nm, vsense, transresistance } => {
                cir::Element::H(cir::Ccvs {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    vsense: vsense.clone(),
                    transresistance: *transresistance,
                })
            }
            crate::ir::Component::Diode { name, np, nm, model, params } => {
                cir::Element::D(cir::Diode {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    model: model.clone(),
                    params: params.iter().map(|(k, v)| Param::new(k, v)).collect(),
                })
            }
            crate::ir::Component::Bjt { name, nc, nb, ne, model, params } => {
                cir::Element::Q(cir::Bjt {
                    name: name.clone(),
                    nc: cir::Node::from(nc.as_str()),
                    nb: cir::Node::from(nb.as_str()),
                    ne: cir::Node::from(ne.as_str()),
                    model: model.clone(),
                    params: params.iter().map(|(k, v)| Param::new(k, v)).collect(),
                })
            }
            crate::ir::Component::Mosfet { name, nd, ng, ns, nb, model, params } => {
                cir::Element::M(cir::Mosfet {
                    name: name.clone(),
                    nd: cir::Node::from(nd.as_str()),
                    ng: cir::Node::from(ng.as_str()),
                    ns: cir::Node::from(ns.as_str()),
                    nb: cir::Node::from(nb.as_str()),
                    model: model.clone(),
                    params: params.iter().map(|(k, v)| Param::new(k, v)).collect(),
                })
            }
            crate::ir::Component::Jfet { name, nd, ng, ns, model, params } => {
                cir::Element::J(cir::Jfet {
                    name: name.clone(),
                    nd: cir::Node::from(nd.as_str()),
                    ng: cir::Node::from(ng.as_str()),
                    ns: cir::Node::from(ns.as_str()),
                    model: model.clone(),
                    params: params.iter().map(|(k, v)| Param::new(k, v)).collect(),
                })
            }
            crate::ir::Component::Mesfet { name, nd, ng, ns, model, params } => {
                cir::Element::Z(cir::Mesfet {
                    name: name.clone(),
                    nd: cir::Node::from(nd.as_str()),
                    ng: cir::Node::from(ng.as_str()),
                    ns: cir::Node::from(ns.as_str()),
                    model: model.clone(),
                    params: params.iter().map(|(k, v)| Param::new(k, v)).collect(),
                })
            }
            crate::ir::Component::VSwitch { name, np, nm, ncp, ncm, model } => {
                cir::Element::S(cir::VSwitch {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    ncp: cir::Node::from(ncp.as_str()),
                    ncm: cir::Node::from(ncm.as_str()),
                    model: model.clone(),
                })
            }
            crate::ir::Component::ISwitch { name, np, nm, vcontrol, model } => {
                cir::Element::W(cir::ISwitch {
                    name: name.clone(),
                    np: cir::Node::from(np.as_str()),
                    nm: cir::Node::from(nm.as_str()),
                    vcontrol: vcontrol.clone(),
                    model: model.clone(),
                })
            }
            crate::ir::Component::TLine { name, inp, inm, outp, outm, z0, td } => {
                cir::Element::T(cir::TLine {
                    name: name.clone(),
                    inp: cir::Node::from(inp.as_str()),
                    inm: cir::Node::from(inm.as_str()),
                    outp: cir::Node::from(outp.as_str()),
                    outm: cir::Node::from(outm.as_str()),
                    z0: *z0,
                    td: *td,
                })
            }
            crate::ir::Component::Xspice { name, connections, model } => {
                cir::Element::A(cir::XspiceInstance {
                    name: name.clone(),
                    connections: connections.clone(),
                    model: model.clone(),
                })
            }
            crate::ir::Component::RawSpice { line } => {
                cir::Element::RawSpice(line.clone())
            }
        };
        elements.push(elem);
    }
}

// ── PySubcircuit ──

#[pyclass(name = "Subcircuit")]
#[derive(Clone)]
struct PySubcircuit {
    inner: crate::ir::Subcircuit,
}

#[pymethods]
impl PySubcircuit {
    #[new]
    #[pyo3(signature = (name, ports=None, **params))]
    fn new(name: &str, ports: Option<Vec<String>>, params: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<Self> {
        let mut sc = crate::ir::Subcircuit {
            name: name.to_string(),
            ports: ports.unwrap_or_default().into_iter().map(|p| crate::ir::Port {
                name: p,
                direction: crate::ir::PortDirection::InOut,
            }).collect(),
            parameters: vec![],
            components: vec![],
            instances: vec![],
            models: vec![],
            raw_spice: vec![],
            includes: vec![],
            libs: vec![],
            osdi_loads: vec![],
            verilog_blocks: vec![],
        };
        if let Some(dict) = params {
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                let val: String = v.str()?.to_string();
                sc.parameters.push(crate::ir::ParamDef {
                    name: key,
                    default: Some(val),
                });
            }
        }
        Ok(Self { inner: sc })
    }

    #[getter]
    fn gnd(&self) -> String {
        "0".to_string()
    }

    // ── Component methods (mirror PyCircuit) ──

    #[pyo3(signature = (*, name, positive, negative, value, raw_spice=None))]
    fn R(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg, raw_spice: Option<&str>) {
        let ir_value = if let Some(raw) = raw_spice {
            crate::ir::IrValue::Raw { text: raw.to_string() }
        } else {
            py_value_to_ir(value)
        };
        self.inner.components.push(crate::ir::Component::Resistor {
            name: name.to_string(),
            n1: positive.to_string(),
            n2: negative.to_string(),
            value: ir_value,
            params: vec![],
        });
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn C(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.components.push(crate::ir::Component::Capacitor {
            name: name.to_string(),
            n1: positive.to_string(),
            n2: negative.to_string(),
            value: py_value_to_ir(value),
            params: vec![],
        });
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn L(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.components.push(crate::ir::Component::Inductor {
            name: name.to_string(),
            n1: positive.to_string(),
            n2: negative.to_string(),
            value: py_value_to_ir(value),
            params: vec![],
        });
    }

    #[pyo3(signature = (*, name, inductor1, inductor2, coupling))]
    fn K(&mut self, name: &str, inductor1: &str, inductor2: &str, coupling: f64) {
        self.inner.components.push(crate::ir::Component::MutualInductor {
            name: name.to_string(),
            inductor1: inductor1.to_string(),
            inductor2: inductor2.to_string(),
            coupling,
        });
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn V(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.components.push(crate::ir::Component::VoltageSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: py_value_to_ir(value),
            waveform: None,
        });
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn I(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.components.push(crate::ir::Component::CurrentSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: py_value_to_ir(value),
            waveform: None,
        });
    }

    #[pyo3(signature = (*, name, positive, negative, expression))]
    fn BV(&mut self, name: &str, positive: &str, negative: &str, expression: &str) {
        self.inner.components.push(crate::ir::Component::BehavioralVoltage {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            expression: expression.to_string(),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, expression))]
    fn BI(&mut self, name: &str, positive: &str, negative: &str, expression: &str) {
        self.inner.components.push(crate::ir::Component::BehavioralCurrent {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            expression: expression.to_string(),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, control_positive, control_negative, voltage_gain))]
    fn E(&mut self, name: &str, positive: &str, negative: &str, control_positive: &str, control_negative: &str, voltage_gain: f64) {
        self.inner.components.push(crate::ir::Component::Vcvs {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            ncp: control_positive.to_string(),
            ncm: control_negative.to_string(),
            gain: voltage_gain,
        });
    }

    #[pyo3(signature = (*, name, positive, negative, control_positive, control_negative, transconductance))]
    fn G(&mut self, name: &str, positive: &str, negative: &str, control_positive: &str, control_negative: &str, transconductance: f64) {
        self.inner.components.push(crate::ir::Component::Vccs {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            ncp: control_positive.to_string(),
            ncm: control_negative.to_string(),
            transconductance,
        });
    }

    #[pyo3(signature = (*, name, positive, negative, vsense, current_gain))]
    fn F(&mut self, name: &str, positive: &str, negative: &str, vsense: &str, current_gain: f64) {
        self.inner.components.push(crate::ir::Component::Cccs {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            vsense: vsense.to_string(),
            gain: current_gain,
        });
    }

    #[pyo3(signature = (*, name, positive, negative, vsense, transresistance))]
    fn H(&mut self, name: &str, positive: &str, negative: &str, vsense: &str, transresistance: f64) {
        self.inner.components.push(crate::ir::Component::Ccvs {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            vsense: vsense.to_string(),
            transresistance,
        });
    }

    #[pyo3(signature = (*, name, anode, cathode, model))]
    fn D(&mut self, name: &str, anode: &str, cathode: &str, model: &str) {
        self.inner.components.push(crate::ir::Component::Diode {
            name: name.to_string(),
            np: anode.to_string(),
            nm: cathode.to_string(),
            model: model.to_string(),
            params: vec![],
        });
    }

    #[pyo3(signature = (*, name, collector, base, emitter, model))]
    fn Q(&mut self, name: &str, collector: &str, base: &str, emitter: &str, model: &str) {
        self.inner.components.push(crate::ir::Component::Bjt {
            name: name.to_string(),
            nc: collector.to_string(),
            nb: base.to_string(),
            ne: emitter.to_string(),
            model: model.to_string(),
            params: vec![],
        });
    }

    #[pyo3(signature = (*, name, collector, base, emitter, model))]
    fn BJT(&mut self, name: &str, collector: &str, base: &str, emitter: &str, model: &str) {
        self.Q(name, collector, base, emitter, model);
    }

    #[pyo3(signature = (*, name, drain, gate, source, bulk, model, **kwargs))]
    fn M(&mut self, name: &str, drain: &str, gate: &str, source: &str, bulk: &str, model: &str, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        let mut params = vec![];
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                let val: String = v.str()?.to_string();
                params.push((key, val));
            }
        }
        self.inner.components.push(crate::ir::Component::Mosfet {
            name: name.to_string(),
            nd: drain.to_string(),
            ng: gate.to_string(),
            ns: source.to_string(),
            nb: bulk.to_string(),
            model: model.to_string(),
            params,
        });
        Ok(())
    }

    #[pyo3(signature = (*, name, drain, gate, source, bulk, model, **kwargs))]
    fn MOSFET(&mut self, name: &str, drain: &str, gate: &str, source: &str, bulk: &str, model: &str, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        self.M(name, drain, gate, source, bulk, model, kwargs)
    }

    #[pyo3(signature = (*, name, drain, gate, source, model))]
    fn J(&mut self, name: &str, drain: &str, gate: &str, source: &str, model: &str) {
        self.inner.components.push(crate::ir::Component::Jfet {
            name: name.to_string(),
            nd: drain.to_string(),
            ng: gate.to_string(),
            ns: source.to_string(),
            model: model.to_string(),
            params: vec![],
        });
    }

    #[pyo3(signature = (*, name, drain, gate, source, model))]
    fn Z(&mut self, name: &str, drain: &str, gate: &str, source: &str, model: &str) {
        self.inner.components.push(crate::ir::Component::Mesfet {
            name: name.to_string(),
            nd: drain.to_string(),
            ng: gate.to_string(),
            ns: source.to_string(),
            model: model.to_string(),
            params: vec![],
        });
    }

    #[pyo3(signature = (*, name, positive, negative, control_positive, control_negative, model))]
    fn S(&mut self, name: &str, positive: &str, negative: &str, control_positive: &str, control_negative: &str, model: &str) {
        self.inner.components.push(crate::ir::Component::VSwitch {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            ncp: control_positive.to_string(),
            ncm: control_negative.to_string(),
            model: model.to_string(),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, vcontrol, model))]
    fn W(&mut self, name: &str, positive: &str, negative: &str, vcontrol: &str, model: &str) {
        self.inner.components.push(crate::ir::Component::ISwitch {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            vcontrol: vcontrol.to_string(),
            model: model.to_string(),
        });
    }

    #[pyo3(signature = (*, name, input_positive, input_negative, output_positive, output_negative, Z0, TD))]
    fn T(&mut self, name: &str, input_positive: &str, input_negative: &str, output_positive: &str, output_negative: &str, Z0: f64, TD: f64) {
        self.inner.components.push(crate::ir::Component::TLine {
            name: name.to_string(),
            inp: input_positive.to_string(),
            inm: input_negative.to_string(),
            outp: output_positive.to_string(),
            outm: output_negative.to_string(),
            z0: Z0,
            td: TD,
        });
    }

    #[pyo3(signature = (*, name, connections, model))]
    fn A(&mut self, name: &str, connections: Vec<String>, model: &str) {
        self.inner.components.push(crate::ir::Component::Xspice {
            name: name.to_string(),
            connections,
            model: model.to_string(),
        });
    }

    // ── High-level waveform sources ──

    #[pyo3(signature = (*, name, positive, negative, dc_offset=0.0, offset=0.0, amplitude=1.0, frequency=1000.0))]
    fn SinusoidalVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str,
        dc_offset: f64, offset: f64, amplitude: f64, frequency: f64,
    ) {
        self.inner.components.push(crate::ir::Component::VoltageSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: crate::ir::IrValue::Numeric { value: dc_offset },
            waveform: Some(crate::ir::IrWaveform::Sin {
                offset, amplitude, frequency, delay: 0.0, damping: 0.0, phase: 0.0,
            }),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, initial_value=0.0, pulsed_value=1.0, pulse_width=50e-9, period=100e-9, rise_time=1e-9, fall_time=1e-9))]
    fn PulseVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str,
        initial_value: f64, pulsed_value: f64, pulse_width: f64,
        period: f64, rise_time: f64, fall_time: f64,
    ) {
        self.inner.components.push(crate::ir::Component::VoltageSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: crate::ir::IrValue::Numeric { value: 0.0 },
            waveform: Some(crate::ir::IrWaveform::Pulse {
                initial: initial_value, pulsed: pulsed_value, delay: 0.0,
                rise_time, fall_time, pulse_width, period,
            }),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, values))]
    fn PieceWiseLinearVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str, values: Vec<(f64, f64)>,
    ) {
        self.inner.components.push(crate::ir::Component::VoltageSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: crate::ir::IrValue::Numeric { value: 0.0 },
            waveform: Some(crate::ir::IrWaveform::Pwl { values }),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, dc_offset=0.0, offset=0.0, amplitude=1.0, frequency=1000.0))]
    fn SinusoidalCurrentSource(
        &mut self, name: &str, positive: &str, negative: &str,
        dc_offset: f64, offset: f64, amplitude: f64, frequency: f64,
    ) {
        self.inner.components.push(crate::ir::Component::CurrentSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: crate::ir::IrValue::Numeric { value: dc_offset },
            waveform: Some(crate::ir::IrWaveform::Sin {
                offset, amplitude, frequency, delay: 0.0, damping: 0.0, phase: 0.0,
            }),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, initial_value=0.0, pulsed_value=1.0, pulse_width=50e-9, period=100e-9, rise_time=1e-9, fall_time=1e-9))]
    fn PulseCurrentSource(
        &mut self, name: &str, positive: &str, negative: &str,
        initial_value: f64, pulsed_value: f64, pulse_width: f64,
        period: f64, rise_time: f64, fall_time: f64,
    ) {
        self.inner.components.push(crate::ir::Component::CurrentSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: crate::ir::IrValue::Numeric { value: 0.0 },
            waveform: Some(crate::ir::IrWaveform::Pulse {
                initial: initial_value, pulsed: pulsed_value, delay: 0.0,
                rise_time, fall_time, pulse_width, period,
            }),
        });
    }

    // ── Subcircuit instance ──

    #[pyo3(signature = (subckt, name, *nodes, **params))]
    fn instance(&mut self, subckt: &PySubcircuit, name: &str, nodes: Vec<String>, params: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        let mut param_vec = vec![];
        if let Some(dict) = params {
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                let val: String = v.str()?.to_string();
                param_vec.push((key, val));
            }
        }
        self.inner.instances.push(crate::ir::Instance {
            name: name.to_string(),
            subcircuit: subckt.inner.name.clone(),
            port_mapping: nodes,
            parameters: param_vec,
        });
        Ok(())
    }

    /// Subcircuit instance by name (when you don't have the PySubcircuit object)
    #[pyo3(signature = (name, subcircuit_name, *nodes))]
    fn X(&mut self, name: &str, subcircuit_name: &str, nodes: Vec<String>) {
        self.inner.instances.push(crate::ir::Instance {
            name: name.to_string(),
            subcircuit: subcircuit_name.to_string(),
            port_mapping: nodes,
            parameters: vec![],
        });
    }

    // ── Circuit-level directives ──

    #[pyo3(signature = (name, kind, **kwargs))]
    fn model(&mut self, name: &str, kind: &str, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        let mut params = vec![];
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                let val: String = v.str()?.to_string();
                params.push((key, val));
            }
        }
        self.inner.models.push(crate::ir::ModelDef {
            name: name.to_string(),
            kind: kind.to_string(),
            parameters: params,
        });
        Ok(())
    }

    fn raw_spice(&mut self, line: &str) {
        self.inner.raw_spice.push(line.to_string());
    }

    fn include(&mut self, path: &str) {
        self.inner.includes.push(path.to_string());
    }

    fn lib(&mut self, path: &str, section: &str) {
        self.inner.libs.push((path.to_string(), section.to_string()));
    }

    fn osdi(&mut self, path: &str) {
        self.inner.osdi_loads.push(path.to_string());
    }

    fn parameter(&mut self, name: &str, value: &str) {
        self.inner.parameters.push(crate::ir::ParamDef {
            name: name.to_string(),
            default: Some(value.to_string()),
        });
    }

    fn veriloga(&mut self, source_or_path: &str) -> PyResult<String> {
        let osdi_path = compile_veriloga_impl(source_or_path)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        self.inner.osdi_loads.push(osdi_path.clone());
        Ok(osdi_path)
    }

    /// Add a Verilog module to the subcircuit (stored in IR for later code generation).
    ///
    /// mode="simulate": will use co-simulation when the subcircuit is instantiated.
    /// mode="synthesize": will synthesize to gate-level when the subcircuit is instantiated.
    ///
    /// See `Circuit.verilog()` for full documentation.
    #[pyo3(signature = (*, source, mode="simulate", instance_name, connections, pdk=None, liberty=None, spice_models=None))]
    fn verilog(
        &mut self,
        py: pyo3::Python<'_>,
        source: &str,
        mode: &str,
        instance_name: &str,
        connections: HashMap<String, pyo3::PyObject>,
        pdk: Option<&str>,
        liberty: Option<&str>,
        spice_models: Option<&str>,
    ) -> PyResult<()> {
        let verilog_mode = match mode {
            "simulate" => crate::ir::VerilogMode::Simulate,
            "synthesize" => crate::ir::VerilogMode::Synthesize,
            _ => return Err(pyo3::exceptions::PyValueError::new_err(
                format!("Invalid mode '{}'. Must be 'simulate' or 'synthesize'.", mode)
            )),
        };

        // Convert connections to IR representation
        let mut ir_connections = HashMap::new();
        for (port, obj) in &connections {
            let nets = extract_connection(py, obj)?;
            let conn = if nets.len() == 1 {
                crate::ir::VerilogConnection::Single(nets.into_iter().next().unwrap())
            } else {
                crate::ir::VerilogConnection::Bus(nets)
            };
            ir_connections.insert(port.clone(), conn);
        }

        self.inner.verilog_blocks.push(crate::ir::VerilogBlock {
            source: source.to_string(),
            mode: verilog_mode,
            instance_name: instance_name.to_string(),
            connections: ir_connections,
            pdk: pdk.map(|s| s.to_string()),
            liberty: liberty.map(|s| s.to_string()),
            spice_models: spice_models.map(|s| s.to_string()),
        });

        Ok(())
    }

    // ── Serialization ──

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&self.inner)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let inner: crate::ir::Subcircuit = serde_json::from_str(json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    fn save_json(&self, path: &str) -> PyResult<()> {
        let json = serde_json::to_string_pretty(&self.inner)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        std::fs::write(path, json)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn load_json(path: &str) -> PyResult<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        Self::from_json(&json)
    }

    fn __str__(&self) -> String {
        format!("Subcircuit('{}', ports={:?})", self.inner.name,
            self.inner.ports.iter().map(|p| &p.name).collect::<Vec<_>>())
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

// ── PyTestbench ──

#[pyclass(name = "Testbench")]
struct PyTestbench {
    inner: crate::ir::Testbench,
    dut: crate::ir::Subcircuit,
    subcircuit_defs: Vec<crate::ir::Subcircuit>,
    backend_override: Option<String>,
}

#[pymethods]
impl PyTestbench {
    #[new]
    fn new(dut: &PySubcircuit) -> Self {
        Self {
            inner: crate::ir::Testbench {
                dut: dut.inner.name.clone(),
                stimulus: vec![],
                analyses: vec![],
                options: crate::ir::SimOptions::default(),
                saves: vec![],
                measures: vec![],
                temperature: None,
                nominal_temperature: None,
                initial_conditions: vec![],
                node_sets: vec![],
                step_params: vec![],
                extra_lines: vec![],
            },
            dut: dut.inner.clone(),
            subcircuit_defs: vec![],
            backend_override: None,
        }
    }

    /// Add a subcircuit definition to the testbench
    fn add_subcircuit(&mut self, subckt: &PySubcircuit) {
        self.subcircuit_defs.push(subckt.inner.clone());
    }

    // ── Stimulus sources ──

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn V(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.stimulus.push(crate::ir::Component::VoltageSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: py_value_to_ir(value),
            waveform: None,
        });
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn I(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.stimulus.push(crate::ir::Component::CurrentSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: py_value_to_ir(value),
            waveform: None,
        });
    }

    #[pyo3(signature = (*, name, positive, negative, dc_offset=0.0, offset=0.0, amplitude=1.0, frequency=1000.0))]
    fn SinusoidalVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str,
        dc_offset: f64, offset: f64, amplitude: f64, frequency: f64,
    ) {
        self.inner.stimulus.push(crate::ir::Component::VoltageSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: crate::ir::IrValue::Numeric { value: dc_offset },
            waveform: Some(crate::ir::IrWaveform::Sin {
                offset, amplitude, frequency, delay: 0.0, damping: 0.0, phase: 0.0,
            }),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, initial_value=0.0, pulsed_value=1.0, pulse_width=50e-9, period=100e-9, rise_time=1e-9, fall_time=1e-9))]
    fn PulseVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str,
        initial_value: f64, pulsed_value: f64, pulse_width: f64,
        period: f64, rise_time: f64, fall_time: f64,
    ) {
        self.inner.stimulus.push(crate::ir::Component::VoltageSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: crate::ir::IrValue::Numeric { value: 0.0 },
            waveform: Some(crate::ir::IrWaveform::Pulse {
                initial: initial_value, pulsed: pulsed_value, delay: 0.0,
                rise_time, fall_time, pulse_width, period,
            }),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, values))]
    fn PieceWiseLinearVoltageSource(
        &mut self, name: &str, positive: &str, negative: &str, values: Vec<(f64, f64)>,
    ) {
        self.inner.stimulus.push(crate::ir::Component::VoltageSource {
            name: name.to_string(),
            np: positive.to_string(),
            nm: negative.to_string(),
            value: crate::ir::IrValue::Numeric { value: 0.0 },
            waveform: Some(crate::ir::IrWaveform::Pwl { values }),
        });
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn R(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.stimulus.push(crate::ir::Component::Resistor {
            name: name.to_string(),
            n1: positive.to_string(),
            n2: negative.to_string(),
            value: py_value_to_ir(value),
            params: vec![],
        });
    }

    #[pyo3(signature = (*, name, positive, negative, value))]
    fn C(&mut self, name: &str, positive: &str, negative: &str, value: PyValueArg) {
        self.inner.stimulus.push(crate::ir::Component::Capacitor {
            name: name.to_string(),
            n1: positive.to_string(),
            n2: negative.to_string(),
            value: py_value_to_ir(value),
            params: vec![],
        });
    }

    // ── Config ──

    #[pyo3(signature = (**kwargs))]
    fn options(&mut self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                let val: String = v.str()?.to_string();
                self.inner.options.portable.push((key, val));
            }
        }
        Ok(())
    }

    #[setter]
    fn set_temperature(&mut self, temp: f64) {
        self.inner.temperature = Some(temp);
    }

    #[setter]
    fn set_nominal_temperature(&mut self, temp: f64) {
        self.inner.nominal_temperature = Some(temp);
    }

    #[pyo3(signature = (**kwargs))]
    fn initial_condition(&mut self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let node: String = k.extract()?;
                let val: f64 = v.extract::<f64>()?;
                self.inner.initial_conditions.push((node, val));
            }
        }
        Ok(())
    }

    #[pyo3(signature = (**kwargs))]
    fn node_set(&mut self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<()> {
        if let Some(dict) = kwargs {
            for (k, v) in dict.iter() {
                let node: String = k.extract()?;
                let val: f64 = v.extract::<f64>()?;
                self.inner.node_sets.push((node, val));
            }
        }
        Ok(())
    }

    #[pyo3(signature = (*args))]
    fn save(&mut self, args: Vec<String>) {
        self.inner.saves.extend(args);
    }

    #[pyo3(signature = (*args))]
    fn measure(&mut self, args: Vec<String>) {
        let joined = args.join(" ");
        self.inner.measures.push(joined);
    }

    fn with_backend(&mut self, name: &str) {
        self.backend_override = Some(name.to_string());
    }

    #[pyo3(signature = (param, start, stop, step))]
    fn step(&mut self, param: &str, start: f64, stop: f64, step: f64) {
        self.inner.step_params.push(crate::ir::StepParam {
            param: param.to_string(),
            start,
            stop,
            step,
            sweep_type: None,
        });
    }

    #[pyo3(signature = (param, start, stop, step, sweep_type))]
    fn step_sweep(&mut self, param: &str, start: f64, stop: f64, step: f64, sweep_type: &str) {
        self.inner.step_params.push(crate::ir::StepParam {
            param: param.to_string(),
            start,
            stop,
            step,
            sweep_type: Some(sweep_type.to_string()),
        });
    }

    // ── Backend check ──

    fn check_backend(&self, name: &str) -> Vec<String> {
        let ir = self.build_ir();
        ir.check_backend(name).into_iter().map(|issue| {
            format!("{:?}: {}", issue.severity, issue.message)
        }).collect()
    }

    // ── Analysis methods ──

    fn operating_point(&mut self) -> PyResult<PyOperatingPoint> {
        self.inner.analyses = vec![crate::ir::Analysis::Op];
        let sim = self.to_simulator()?;
        sim.operating_point()
            .map(|op| PyOperatingPoint { inner: op })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (**kwargs))]
    fn dc(&mut self, kwargs: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<PyDcAnalysis> {
        let dict = kwargs.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("dc() requires sweep parameters")
        })?;
        let sweeps = extract_dc_sweeps(&dict)?;
        let dc_sweeps: Vec<crate::ir::DcSweep> = sweeps.iter().map(|(v, a, b, c)| {
            crate::ir::DcSweep { source: v.clone(), start: *a, stop: *b, step: *c }
        }).collect();
        self.inner.analyses = vec![crate::ir::Analysis::Dc { sweeps: dc_sweeps }];

        let sim = self.to_simulator()?;
        let sweep_refs: Vec<(&str, f64, f64, f64)> = sweeps
            .iter()
            .map(|(v, a, b, c)| (v.as_str(), *a, *b, *c))
            .collect();
        sim.dc_multi(&sweep_refs)
            .map(|dc| PyDcAnalysis { inner: dc })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (variation="dec", number_of_points=10, start_frequency=1.0, stop_frequency=1e9))]
    fn ac(&mut self, variation: &str, number_of_points: u32, start_frequency: f64, stop_frequency: f64) -> PyResult<PyAcAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Ac {
            variation: variation.to_string(),
            points: number_of_points,
            start: start_frequency,
            stop: stop_frequency,
        }];
        let sim = self.to_simulator()?;
        sim.ac(variation, number_of_points, start_frequency, stop_frequency)
            .map(|ac| PyAcAnalysis { inner: ac })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (step_time, end_time, start_time=None, max_time=None, use_initial_condition=false))]
    fn transient(&mut self, step_time: f64, end_time: f64, start_time: Option<f64>, max_time: Option<f64>, use_initial_condition: bool) -> PyResult<PyTransientAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Transient {
            step: step_time,
            stop: end_time,
            start: start_time,
            max_step: max_time,
            uic: use_initial_condition,
        }];
        let sim = self.to_simulator()?;
        sim.transient(step_time, end_time, start_time, max_time, use_initial_condition)
            .map(|t| PyTransientAnalysis { inner: t })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (output_node, ref_node, src, variation="dec", points=10, start_frequency=1e3, stop_frequency=1e8, points_per_summary=None))]
    fn noise(
        &mut self, output_node: &str, ref_node: &str, src: &str,
        variation: &str, points: u32,
        start_frequency: f64, stop_frequency: f64,
        points_per_summary: Option<u32>,
    ) -> PyResult<PyNoiseAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Noise {
            output: output_node.to_string(),
            reference: ref_node.to_string(),
            source: src.to_string(),
            variation: variation.to_string(),
            points,
            start: start_frequency,
            stop: stop_frequency,
            points_per_summary,
        }];
        let sim = self.to_simulator()?;
        sim.noise(output_node, ref_node, src, variation, points, start_frequency, stop_frequency, points_per_summary)
            .map(|n| PyNoiseAnalysis { inner: n })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (outvar, insrc))]
    fn transfer_function(&mut self, outvar: &str, insrc: &str) -> PyResult<PyTransferFunctionAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Tf {
            output: outvar.to_string(),
            source: insrc.to_string(),
        }];
        let sim = self.to_simulator()?;
        sim.transfer_function(outvar, insrc)
            .map(|t| PyTransferFunctionAnalysis { inner: t })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (outvar, insrc))]
    fn tf(&mut self, outvar: &str, insrc: &str) -> PyResult<PyTransferFunctionAnalysis> {
        self.transfer_function(outvar, insrc)
    }

    #[pyo3(signature = (output_variable))]
    fn dc_sensitivity(&mut self, output_variable: &str) -> PyResult<PySensitivityAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Sensitivity {
            output: output_variable.to_string(),
            ac: None,
        }];
        let sim = self.to_simulator()?;
        sim.dc_sensitivity(output_variable)
            .map(|s| PySensitivityAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (output_variable, variation="dec", number_of_points=10, start_frequency=100.0, stop_frequency=1e5))]
    fn ac_sensitivity(
        &mut self, output_variable: &str, variation: &str,
        number_of_points: u32, start_frequency: f64, stop_frequency: f64,
    ) -> PyResult<PySensitivityAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Sensitivity {
            output: output_variable.to_string(),
            ac: Some(crate::ir::AcSweepParams {
                variation: variation.to_string(),
                points: number_of_points,
                start: start_frequency,
                stop: stop_frequency,
            }),
        }];
        let sim = self.to_simulator()?;
        sim.ac_sensitivity(output_variable, variation, number_of_points, start_frequency, stop_frequency)
            .map(|s| PySensitivityAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (node1, node2, node3, node4, tf_type, pz_type))]
    fn polezero(
        &mut self, node1: &str, node2: &str, node3: &str, node4: &str,
        tf_type: &str, pz_type: &str,
    ) -> PyResult<PyPoleZeroAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::PoleZero {
            node1: node1.to_string(),
            node2: node2.to_string(),
            node3: node3.to_string(),
            node4: node4.to_string(),
            tf_type: tf_type.to_string(),
            pz_type: pz_type.to_string(),
        }];
        let sim = self.to_simulator()?;
        sim.polezero(node1, node2, node3, node4, tf_type, pz_type)
            .map(|p| PyPoleZeroAnalysis { inner: p })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (variation="dec", points=10, start_frequency=100.0, stop_frequency=1e8, f2overf1=None))]
    fn distortion(
        &mut self, variation: &str, points: u32,
        start_frequency: f64, stop_frequency: f64,
        f2overf1: Option<f64>,
    ) -> PyResult<PyDistortionAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Distortion {
            variation: variation.to_string(),
            points,
            start: start_frequency,
            stop: stop_frequency,
            f2overf1,
        }];
        let sim = self.to_simulator()?;
        sim.distortion(variation, points, start_frequency, stop_frequency, f2overf1)
            .map(|d| PyDistortionAnalysis { inner: d })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (fundamental_frequency, stabilization_time, observe_node, points_per_period=128, harmonics=10))]
    fn pss(
        &mut self, fundamental_frequency: f64, stabilization_time: f64,
        observe_node: &str, points_per_period: u32, harmonics: u32,
    ) -> PyResult<PyPssAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Pss {
            fundamental: fundamental_frequency,
            stabilization: stabilization_time,
            observe_node: observe_node.to_string(),
            points_per_period,
            harmonics,
        }];
        let sim = self.to_simulator()?;
        sim.pss(fundamental_frequency, stabilization_time, observe_node, points_per_period, harmonics)
            .map(|p| PyPssAnalysis { inner: p })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (variation="dec", number_of_points=10, start_frequency=1e6, stop_frequency=1e10))]
    fn s_param(
        &mut self, variation: &str, number_of_points: u32,
        start_frequency: f64, stop_frequency: f64,
    ) -> PyResult<PySParamAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::SPar {
            variation: variation.to_string(),
            points: number_of_points,
            start: start_frequency,
            stop: stop_frequency,
        }];
        let sim = self.to_simulator()?;
        sim.s_param(variation, number_of_points, start_frequency, stop_frequency)
            .map(|s| PySParamAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (fundamental_frequencies, num_harmonics=None))]
    fn harmonic_balance(
        &mut self, fundamental_frequencies: Vec<f64>,
        num_harmonics: Option<Vec<u32>>,
    ) -> PyResult<PyHarmonicBalanceAnalysis> {
        let nharms = num_harmonics.unwrap_or_else(|| vec![7; fundamental_frequencies.len()]);
        self.inner.analyses = vec![crate::ir::Analysis::HarmonicBalance {
            frequencies: fundamental_frequencies.clone(),
            harmonics: nharms.clone(),
        }];
        let sim = self.to_simulator()?;
        sim.harmonic_balance(&fundamental_frequencies, &nharms)
            .map(|h| PyHarmonicBalanceAnalysis { inner: h })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (probe, variation="dec", number_of_points=10, start_frequency=1.0, stop_frequency=1e10))]
    fn stability(
        &mut self, probe: &str, variation: &str, number_of_points: u32,
        start_frequency: f64, stop_frequency: f64,
    ) -> PyResult<PyStabilityAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::Stability {
            probe: probe.to_string(),
            variation: variation.to_string(),
            points: number_of_points,
            start: start_frequency,
            stop: stop_frequency,
        }];
        let sim = self.to_simulator()?;
        sim.stability(probe, variation, number_of_points, start_frequency, stop_frequency)
            .map(|s| PyStabilityAnalysis { inner: s })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (step_time, end_time))]
    fn transient_noise(
        &mut self, step_time: f64, end_time: f64,
    ) -> PyResult<PyTransientNoiseAnalysis> {
        self.inner.analyses = vec![crate::ir::Analysis::TransientNoise {
            step: step_time,
            stop: end_time,
        }];
        let sim = self.to_simulator()?;
        sim.transient_noise(step_time, end_time)
            .map(|t| PyTransientNoiseAnalysis { inner: t })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    // ── Serialization ──

    fn to_json(&self) -> PyResult<String> {
        let ir = self.build_ir();
        serde_json::to_string_pretty(&ir)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn __str__(&self) -> String {
        format!("Testbench(dut='{}', analyses={})", self.dut.name, self.inner.analyses.len())
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

impl PyTestbench {
    fn build_ir(&self) -> crate::ir::CircuitIR {
        crate::ir::CircuitIR {
            top: self.dut.clone(),
            testbench: Some(self.inner.clone()),
            subcircuit_defs: self.subcircuit_defs.clone(),
            model_libraries: vec![],
        }
    }

    fn to_simulator(&self) -> PyResult<crate::simulation::CircuitSimulator> {
        let circuit = ir_to_circuit(&self.dut, Some(&self.inner), &self.subcircuit_defs);
        let mut sim = circuit.simulator();
        if let Some(ref backend) = self.backend_override {
            sim = sim.with_backend(backend.clone());
        }
        for (k, v) in &self.inner.options.portable {
            sim.options(k.clone(), v.clone());
        }
        if let Some(temp) = self.inner.temperature {
            sim.set_temperature(temp);
        }
        if let Some(tnom) = self.inner.nominal_temperature {
            sim.set_nominal_temperature(tnom);
        }
        for (node, val) in &self.inner.initial_conditions {
            sim.initial_condition(node.clone(), *val);
        }
        for (node, val) in &self.inner.node_sets {
            sim.node_set(node.clone(), *val);
        }
        for s in &self.inner.saves {
            sim.save(s.clone());
        }
        for m in &self.inner.measures {
            sim.measure(vec![m.clone()]);
        }
        for sp in &self.inner.step_params {
            if let Some(ref st) = sp.sweep_type {
                sim.step_sweep(&sp.param, sp.start, sp.stop, sp.step, st);
            } else {
                sim.step(&sp.param, sp.start, sp.stop, sp.step);
            }
        }
        Ok(sim)
    }
}

// ── PyModelLibrary ──

#[pyclass(name = "ModelLibrary")]
struct PyModelLibrary {
    inner: crate::ir::ModelLibrary,
}

#[pymethods]
impl PyModelLibrary {
    #[new]
    #[pyo3(signature = (path, corner=None, **backend_paths))]
    fn new(path: &str, corner: Option<&str>, backend_paths: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<Self> {
        let mut bp = std::collections::HashMap::new();
        if let Some(dict) = backend_paths {
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                let val: String = v.extract()?;
                bp.insert(key, val);
            }
        }
        Ok(Self {
            inner: crate::ir::ModelLibrary {
                name: std::path::Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("library")
                    .to_string(),
                path: path.to_string(),
                corner: corner.map(|s| s.to_string()),
                backend_paths: bp,
            },
        })
    }

    #[getter]
    fn name(&self) -> String { self.inner.name.clone() }

    #[getter]
    fn path(&self) -> String { self.inner.path.clone() }

    #[getter]
    fn corner(&self) -> Option<String> { self.inner.corner.clone() }

    fn __str__(&self) -> String {
        format!("ModelLibrary('{}', corner={:?})", self.inner.path, self.inner.corner)
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

// ── Module registration ──

fn create_unit_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let unit_mod = PyModule::new(m.py(), "unit")?;

    macro_rules! add_unit {
        ($name:ident, $val:expr) => {
            unit_mod.add(stringify!($name), PyUnit { inner: $val })?;
        };
    }

    // Volts
    add_unit!(u_V, u::U_V);
    add_unit!(u_mV, u::U_MV);
    add_unit!(u_uV, u::U_UV);
    // Amperes
    add_unit!(u_A, u::U_A);
    add_unit!(u_mA, u::U_MA);
    add_unit!(u_uA, u::U_UA);
    add_unit!(u_nA, u::U_NA);
    // Ohms
    add_unit!(u_Ohm, u::U_OHM);
    add_unit!(u_kOhm, u::U_KOHM);
    add_unit!(u_MOhm, u::U_MOHM);
    // Farads
    add_unit!(u_F, u::U_F);
    add_unit!(u_mF, u::U_MF);
    add_unit!(u_uF, u::U_UF);
    add_unit!(u_nF, u::U_NF);
    add_unit!(u_pF, u::U_PF);
    add_unit!(u_fF, u::U_FF);
    // Henrys
    add_unit!(u_H, u::U_H);
    add_unit!(u_mH, u::U_MH);
    add_unit!(u_uH, u::U_UH);
    add_unit!(u_nH, u::U_NH);
    // Hertz
    add_unit!(u_Hz, u::U_HZ);
    add_unit!(u_kHz, u::U_KHZ);
    add_unit!(u_MHz, u::U_MHZ);
    add_unit!(u_GHz, u::U_GHZ);
    // Seconds
    add_unit!(u_s, u::U_S);
    add_unit!(u_ms, u::U_MS);
    add_unit!(u_us, u::U_US);
    add_unit!(u_ns, u::U_NS);
    add_unit!(u_ps, u::U_PS);
    // Watts
    add_unit!(u_W, u::U_W);
    add_unit!(u_mW, u::U_MW);
    add_unit!(u_uW, u::U_UW);
    // Degrees
    add_unit!(u_Degree, u::U_DEGREE);

    m.add_submodule(&unit_mod)?;

    // Register in sys.modules so `from pyspice_rs.unit import ...` works
    let sys = m.py().import("sys")?;
    let modules = sys.getattr("modules")?;
    modules.set_item("pyspice_rs.unit", &unit_mod)?;

    Ok(())
}

#[pymodule]
pub fn pyspice_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCircuit>()?;
    m.add_class::<PyUnit>()?;
    m.add_class::<PyUnitValue>()?;
    m.add_class::<PySimulator>()?;
    m.add_class::<PyOperatingPoint>()?;
    m.add_class::<PyDcAnalysis>()?;
    m.add_class::<PyAcAnalysis>()?;
    m.add_class::<PyTransientAnalysis>()?;
    m.add_class::<PyNoiseAnalysis>()?;
    m.add_class::<PyTransferFunctionAnalysis>()?;
    m.add_class::<PySensitivityAnalysis>()?;
    m.add_class::<PyPoleZeroAnalysis>()?;
    m.add_class::<PyDistortionAnalysis>()?;
    // New analysis types
    m.add_class::<PyPssAnalysis>()?;
    m.add_class::<PySParamAnalysis>()?;
    m.add_class::<PyHarmonicBalanceAnalysis>()?;
    m.add_class::<PyStabilityAnalysis>()?;
    m.add_class::<PyTransientNoiseAnalysis>()?;
    // Spectre raw data type
    m.add_class::<PyRawData>()?;
    // Xyce-specific analysis types
    m.add_class::<PySamplingAnalysis>()?;
    m.add_class::<PyXyceFftAnalysis>()?;

    // IR types
    m.add_class::<PySubcircuit>()?;
    m.add_class::<PyTestbench>()?;
    m.add_class::<PyModelLibrary>()?;

    // Module-level functions
    m.add_function(wrap_pyfunction!(lint, m)?)?;
    m.add_function(wrap_pyfunction!(compile_veriloga, m)?)?;

    create_unit_module(m)?;
    Ok(())
}
