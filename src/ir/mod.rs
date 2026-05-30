use std::collections::HashMap;
use serde::{Serialize, Deserialize};

// ── Core IR types ──

/// Backend-neutral intermediate representation for a complete simulation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CircuitIR {
    pub top: Subcircuit,
    pub testbench: Option<Testbench>,
    pub subcircuit_defs: Vec<Subcircuit>,
    pub model_libraries: Vec<ModelLibrary>,
}

/// Named, parameterized, composable circuit fragment -- the unit of reuse
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subcircuit {
    pub name: String,
    pub ports: Vec<Port>,
    pub parameters: Vec<ParamDef>,
    pub components: Vec<Component>,
    pub instances: Vec<Instance>,
    pub models: Vec<ModelDef>,
    pub raw_spice: Vec<String>,
    pub includes: Vec<String>,
    pub libs: Vec<(String, String)>,
    pub osdi_loads: Vec<String>,
    #[serde(default)]
    pub verilog_blocks: Vec<VerilogBlock>,
}

/// Verilog co-simulation or synthesis block stored in the IR
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerilogBlock {
    pub source: String,
    pub mode: VerilogMode,
    pub instance_name: String,
    pub connections: HashMap<String, VerilogConnection>,
    pub pdk: Option<String>,
    pub liberty: Option<String>,
    pub spice_models: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VerilogMode {
    Simulate,
    Synthesize,
}

/// A Verilog port connection: single net or a bus (vector of nets)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VerilogConnection {
    Single(String),
    Bus(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub direction: PortDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PortDirection {
    InOut,
    Input,
    Output,
}

/// Parameter definition with optional default
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamDef {
    pub name: String,
    pub default: Option<String>,
}

/// Reference to a Subcircuit definition + port mapping + parameter overrides
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub name: String,
    pub subcircuit: String,
    pub port_mapping: Vec<String>,
    pub parameters: Vec<(String, String)>,
}

/// Model definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDef {
    pub name: String,
    pub kind: String,
    pub parameters: Vec<(String, String)>,
}

// ── Component types ──

/// All SPICE component types as structured data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Component {
    Resistor { name: String, n1: String, n2: String, value: IrValue, params: Vec<(String, String)> },
    Capacitor { name: String, n1: String, n2: String, value: IrValue, params: Vec<(String, String)> },
    Inductor { name: String, n1: String, n2: String, value: IrValue, params: Vec<(String, String)> },
    MutualInductor { name: String, inductor1: String, inductor2: String, coupling: f64 },
    VoltageSource { name: String, np: String, nm: String, value: IrValue, waveform: Option<IrWaveform> },
    CurrentSource { name: String, np: String, nm: String, value: IrValue, waveform: Option<IrWaveform> },
    BehavioralVoltage { name: String, np: String, nm: String, expression: String },
    BehavioralCurrent { name: String, np: String, nm: String, expression: String },
    Vcvs { name: String, np: String, nm: String, ncp: String, ncm: String, gain: f64 },
    Vccs { name: String, np: String, nm: String, ncp: String, ncm: String, transconductance: f64 },
    Cccs { name: String, np: String, nm: String, vsense: String, gain: f64 },
    Ccvs { name: String, np: String, nm: String, vsense: String, transresistance: f64 },
    Diode { name: String, np: String, nm: String, model: String, params: Vec<(String, String)> },
    Bjt { name: String, nc: String, nb: String, ne: String, model: String, params: Vec<(String, String)> },
    Mosfet { name: String, nd: String, ng: String, ns: String, nb: String, model: String, params: Vec<(String, String)> },
    Jfet { name: String, nd: String, ng: String, ns: String, model: String, params: Vec<(String, String)> },
    Mesfet { name: String, nd: String, ng: String, ns: String, model: String, params: Vec<(String, String)> },
    VSwitch { name: String, np: String, nm: String, ncp: String, ncm: String, model: String },
    ISwitch { name: String, np: String, nm: String, vcontrol: String, model: String },
    TLine { name: String, inp: String, inm: String, outp: String, outm: String, z0: f64, td: f64 },
    Xspice { name: String, connections: Vec<String>, model: String },
    RawSpice { line: String },
}

// ── Value and waveform types ──

/// Component value in the IR -- numeric, expression, or raw
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IrValue {
    Numeric { value: f64 },
    Expression { expr: String },
    Raw { text: String },
}

/// Waveform types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IrWaveform {
    Sin { offset: f64, amplitude: f64, frequency: f64, delay: f64, damping: f64, phase: f64 },
    Pulse { initial: f64, pulsed: f64, delay: f64, rise_time: f64, fall_time: f64, pulse_width: f64, period: f64 },
    Pwl { values: Vec<(f64, f64)> },
    Exp { initial: f64, pulsed: f64, rise_delay: f64, rise_tau: f64, fall_delay: f64, fall_tau: f64 },
    Sffm { offset: f64, amplitude: f64, carrier_freq: f64, modulation_index: f64, signal_freq: f64 },
    Am { amplitude: f64, offset: f64, modulating_freq: f64, carrier_freq: f64, delay: f64 },
}

// ── Testbench ──

/// Testbench -- wraps a DUT with stimulus + analysis
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Testbench {
    pub dut: String,
    pub stimulus: Vec<Component>,
    pub analyses: Vec<Analysis>,
    pub options: SimOptions,
    pub saves: Vec<String>,
    pub measures: Vec<String>,
    pub temperature: Option<f64>,
    pub nominal_temperature: Option<f64>,
    pub initial_conditions: Vec<(String, f64)>,
    pub node_sets: Vec<(String, f64)>,
    pub step_params: Vec<StepParam>,
    pub extra_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepParam {
    pub param: String,
    pub start: f64,
    pub stop: f64,
    pub step: f64,
    pub sweep_type: Option<String>,
}

// ── Analysis types ──

/// Analysis types -- all supported analyses as a tagged union
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Analysis {
    Op,
    Dc { sweeps: Vec<DcSweep> },
    Ac { variation: String, points: u32, start: f64, stop: f64 },
    Transient { step: f64, stop: f64, start: Option<f64>, max_step: Option<f64>, uic: bool },
    Noise { output: String, reference: String, source: String, variation: String, points: u32, start: f64, stop: f64, points_per_summary: Option<u32> },
    Tf { output: String, source: String },
    Sensitivity { output: String, ac: Option<AcSweepParams> },
    PoleZero { node1: String, node2: String, node3: String, node4: String, tf_type: String, pz_type: String },
    Distortion { variation: String, points: u32, start: f64, stop: f64, f2overf1: Option<f64> },
    Pss { fundamental: f64, stabilization: f64, observe_node: String, points_per_period: u32, harmonics: u32 },
    HarmonicBalance { frequencies: Vec<f64>, harmonics: Vec<u32> },
    SPar { variation: String, points: u32, start: f64, stop: f64 },
    Stability { probe: String, variation: String, points: u32, start: f64, stop: f64 },
    TransientNoise { step: f64, stop: f64 },
    Fourier { fundamental: f64, outputs: Vec<String>, num_harmonics: Option<u32> },
    // Vendor-specific
    XyceSampling { num_samples: u32, distributions: Vec<(String, String)> },
    XyceEmbeddedSampling { num_samples: u32, distributions: Vec<(String, String)> },
    XycePce { num_samples: u32, distributions: Vec<(String, String)>, order: u32 },
    XyceFft { signal: String, np: u32, start: f64, stop: f64, window: String, format: String },
    SpectreSweep { param: String, start: f64, stop: f64, step: f64, inner: String, inner_type: String },
    SpectreMonteCarlo { iterations: u32, inner: String, inner_type: String, seed: Option<u64> },
    SpectrePac { pss_fundamental: f64, pss_stabilization: f64, pss_harmonics: u32, variation: String, points: u32, start: f64, stop: f64, sweep_type: String },
    SpectrePnoise { pss_fundamental: f64, pss_stabilization: f64, pss_harmonics: u32, output: String, reference: String, variation: String, points: u32, start: f64, stop: f64 },
    SpectrePxf { pss_fundamental: f64, pss_stabilization: f64, pss_harmonics: u32, output: String, source: String, variation: String, points: u32, start: f64, stop: f64 },
    SpectrePstb { pss_fundamental: f64, pss_stabilization: f64, pss_harmonics: u32, probe: String, variation: String, points: u32, start: f64, stop: f64 },
}

impl Analysis {
    pub fn kind_str(&self) -> &str {
        match self {
            Analysis::Op => "op",
            Analysis::Dc { .. } => "dc",
            Analysis::Ac { .. } => "ac",
            Analysis::Transient { .. } => "tran",
            Analysis::Noise { .. } => "noise",
            Analysis::Tf { .. } => "tf",
            Analysis::Sensitivity { ac: Some(_), .. } => "sens_ac",
            Analysis::Sensitivity { .. } => "sens",
            Analysis::PoleZero { .. } => "pz",
            Analysis::Distortion { .. } => "disto",
            Analysis::Pss { .. } => "pss",
            Analysis::HarmonicBalance { .. } => "hb",
            Analysis::SPar { .. } => "sp",
            Analysis::Stability { .. } => "stb",
            Analysis::TransientNoise { .. } => "trannoise",
            Analysis::Fourier { .. } => "tran",
            Analysis::XyceSampling { .. } => "sampling",
            Analysis::XyceEmbeddedSampling { .. } => "embedded_sampling",
            Analysis::XycePce { .. } => "pce",
            Analysis::XyceFft { .. } => "tran",
            Analysis::SpectreSweep { .. } => "tran",
            Analysis::SpectreMonteCarlo { .. } => "tran",
            Analysis::SpectrePac { .. } => "pac",
            Analysis::SpectrePnoise { .. } => "pnoise",
            Analysis::SpectrePxf { .. } => "pxf",
            Analysis::SpectrePstb { .. } => "pstb",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DcSweep {
    pub source: String,
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcSweepParams {
    pub variation: String,
    pub points: u32,
    pub start: f64,
    pub stop: f64,
}

// ── Simulation options ──

/// Simulation options -- portable + per-backend
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SimOptions {
    pub portable: Vec<(String, String)>,
    pub backend_specific: HashMap<String, Vec<(String, String)>>,
}

/// Opaque PDK/model library reference
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelLibrary {
    pub name: String,
    pub path: String,
    pub corner: Option<String>,
    pub backend_paths: HashMap<String, String>,
}

// ── Feature flags ──

/// Feature flags computed from IR
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub has_xspice: bool,
    pub has_osdi: bool,
    pub has_measures: bool,
    pub has_step_params: bool,
    pub has_control_blocks: bool,
    pub has_laplace_sources: bool,
    pub has_verilog_cosim: bool,
    pub element_count: usize,
}

// ── Compatibility checking ──

/// Issue found by check_backend
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub severity: IssueSeverity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Error,
    Warning,
}

// ── CircuitIR methods ──

impl CircuitIR {
    /// Compute feature flags by walking the IR
    pub fn compute_features(&self) -> FeatureFlags {
        let mut flags = FeatureFlags::default();

        self.scan_subcircuit(&self.top, &mut flags);
        for sub in &self.subcircuit_defs {
            self.scan_subcircuit(sub, &mut flags);
        }

        // Check testbench for measures, step params
        if let Some(ref tb) = self.testbench {
            if !tb.measures.is_empty() {
                flags.has_measures = true;
            }
            if !tb.step_params.is_empty() {
                flags.has_step_params = true;
            }
        }

        flags
    }

    fn scan_subcircuit(&self, sub: &Subcircuit, flags: &mut FeatureFlags) {
        if !sub.osdi_loads.is_empty() {
            flags.has_osdi = true;
        }

        if sub.verilog_blocks.iter().any(|vb| matches!(vb.mode, VerilogMode::Simulate)) {
            flags.has_verilog_cosim = true;
        }

        for line in &sub.raw_spice {
            let trimmed = line.trim().to_lowercase();
            if trimmed == ".control" || trimmed.starts_with(".control ") {
                flags.has_control_blocks = true;
            }
        }

        for comp in &sub.components {
            flags.element_count += 1;
            match comp {
                Component::Xspice { .. } => {
                    flags.has_xspice = true;
                }
                Component::BehavioralVoltage { expression, .. }
                | Component::BehavioralCurrent { expression, .. } => {
                    if expression.contains("laplace") {
                        flags.has_laplace_sources = true;
                    }
                }
                _ => {}
            }
        }

        flags.element_count += sub.instances.len();
    }

    /// Check backend compatibility -- pure function, no I/O.
    ///
    /// Returns a list of issues (errors and warnings) for the given backend name.
    pub fn check_backend(&self, backend: &str) -> Vec<Issue> {
        let features = self.compute_features();
        let mut issues = Vec::new();

        if features.has_xspice && !supports_xspice(backend) {
            issues.push(Issue {
                severity: IssueSeverity::Error,
                message: format!(
                    "Backend '{}' does not support XSPICE A-elements (only ngspice does)",
                    backend
                ),
            });
        }

        if features.has_osdi && !supports_osdi(backend) {
            issues.push(Issue {
                severity: IssueSeverity::Error,
                message: format!(
                    "Backend '{}' does not support OSDI/Verilog-A model loading",
                    backend
                ),
            });
        }

        if features.has_measures && !supports_measures(backend) {
            issues.push(Issue {
                severity: IssueSeverity::Error,
                message: format!(
                    "Backend '{}' does not support .meas directives",
                    backend
                ),
            });
        }

        if features.has_step_params && !supports_step(backend) {
            issues.push(Issue {
                severity: IssueSeverity::Error,
                message: format!(
                    "Backend '{}' does not support .step param sweeps",
                    backend
                ),
            });
        }

        if features.has_control_blocks && !supports_control_blocks(backend) {
            issues.push(Issue {
                severity: IssueSeverity::Error,
                message: format!(
                    "Backend '{}' does not support .control blocks (ngspice only)",
                    backend
                ),
            });
        }

        if features.has_laplace_sources && !supports_laplace(backend) {
            issues.push(Issue {
                severity: IssueSeverity::Warning,
                message: format!(
                    "Backend '{}' does not support Laplace-domain B-sources",
                    backend
                ),
            });
        }

        if features.element_count > 10_000 {
            if backend == "ngspice" || backend == "ngspice-subprocess" {
                issues.push(Issue {
                    severity: IssueSeverity::Warning,
                    message: format!(
                        "Large circuit ({} elements) may be slow on '{}'; consider xyce-parallel",
                        features.element_count, backend
                    ),
                });
            }
        }

        issues
    }

    /// Convert from the existing Circuit struct to IR
    pub fn from_circuit(circuit: &crate::circuit::Circuit) -> Self {
        let top = convert_circuit_to_subcircuit(circuit);
        let subcircuit_defs = circuit
            .subcircuits()
            .iter()
            .map(convert_subcircuit_def)
            .collect();

        CircuitIR {
            top,
            testbench: None,
            subcircuit_defs,
            model_libraries: Vec::new(),
        }
    }
}

// ── Backend capability tables (mirrors src/backend/mod.rs) ──

fn supports_xspice(backend: &str) -> bool {
    matches!(backend, "ngspice" | "ngspice-shared" | "ngspice-subprocess")
}

fn supports_osdi(backend: &str) -> bool {
    matches!(
        backend,
        "ngspice" | "ngspice-shared" | "ngspice-subprocess"
            | "vacask" | "vacask-shared"
            | "spectre"
    )
}

fn supports_measures(backend: &str) -> bool {
    matches!(
        backend,
        "ngspice" | "ngspice-shared" | "ngspice-subprocess"
            | "xyce" | "xyce-serial" | "xyce-parallel"
            | "ltspice"
    )
}

fn supports_step(backend: &str) -> bool {
    matches!(
        backend,
        "xyce" | "xyce-serial" | "xyce-parallel"
            | "ltspice"
            | "spectre"
    )
}

fn supports_control_blocks(backend: &str) -> bool {
    matches!(backend, "ngspice" | "ngspice-shared" | "ngspice-subprocess")
}

fn supports_laplace(backend: &str) -> bool {
    matches!(
        backend,
        "ngspice" | "ngspice-shared" | "ngspice-subprocess"
            | "ltspice"
    )
}

// ── Conversion helpers: Circuit -> IR ──

fn convert_circuit_to_subcircuit(circuit: &crate::circuit::Circuit) -> Subcircuit {
    let mut components = Vec::new();
    let mut instances = Vec::new();

    for elem in circuit.elements() {
        match elem {
            crate::circuit::Element::X(xi) => {
                instances.push(Instance {
                    name: xi.name.clone(),
                    subcircuit: xi.subcircuit_name.clone(),
                    port_mapping: xi.nodes.iter().map(|n| n.spice_name().to_string()).collect(),
                    parameters: xi.params.iter().map(|p| (p.name.clone(), p.value.clone())).collect(),
                });
            }
            other => {
                components.push(convert_element(other));
            }
        }
    }

    let models = circuit
        .models()
        .iter()
        .map(|m| ModelDef {
            name: m.name.clone(),
            kind: m.kind.clone(),
            parameters: m.params.iter().map(|p| (p.name.clone(), p.value.clone())).collect(),
        })
        .collect();

    let parameters = circuit
        .parameters()
        .iter()
        .map(|p| ParamDef {
            name: p.name.clone(),
            default: Some(p.value.clone()),
        })
        .collect();

    Subcircuit {
        name: circuit.title.clone(),
        ports: Vec::new(),
        parameters,
        components,
        instances,
        models,
        raw_spice: circuit.raw_lines().to_vec(),
        includes: circuit.includes().to_vec(),
        libs: circuit.libs().to_vec(),
        osdi_loads: circuit.osdi_loads().to_vec(),
        verilog_blocks: vec![],
    }
}

fn convert_subcircuit_def(sub: &crate::circuit::SubCircuitDef) -> Subcircuit {
    let mut components = Vec::new();
    let mut instances = Vec::new();

    for elem in &sub.elements {
        match elem {
            crate::circuit::Element::X(xi) => {
                instances.push(Instance {
                    name: xi.name.clone(),
                    subcircuit: xi.subcircuit_name.clone(),
                    port_mapping: xi.nodes.iter().map(|n| n.spice_name().to_string()).collect(),
                    parameters: xi.params.iter().map(|p| (p.name.clone(), p.value.clone())).collect(),
                });
            }
            other => {
                components.push(convert_element(other));
            }
        }
    }

    let models = sub
        .models
        .iter()
        .map(|m| ModelDef {
            name: m.name.clone(),
            kind: m.kind.clone(),
            parameters: m.params.iter().map(|p| (p.name.clone(), p.value.clone())).collect(),
        })
        .collect();

    let ports = sub
        .pins
        .iter()
        .map(|pin| Port {
            name: pin.clone(),
            direction: PortDirection::InOut,
        })
        .collect();

    let parameters = sub
        .params
        .iter()
        .map(|p| ParamDef {
            name: p.name.clone(),
            default: Some(p.value.clone()),
        })
        .collect();

    Subcircuit {
        name: sub.name.clone(),
        ports,
        parameters,
        components,
        instances,
        models,
        raw_spice: Vec::new(),
        includes: Vec::new(),
        libs: Vec::new(),
        osdi_loads: Vec::new(),
        verilog_blocks: vec![],
    }
}

fn convert_element(elem: &crate::circuit::Element) -> Component {
    use crate::circuit::Element;

    match elem {
        Element::R(r) => Component::Resistor {
            name: r.name.clone(),
            n1: r.n1.spice_name().to_string(),
            n2: r.n2.spice_name().to_string(),
            value: convert_value(&r.value),
            params: convert_params(&r.params),
        },
        Element::C(c) => Component::Capacitor {
            name: c.name.clone(),
            n1: c.n1.spice_name().to_string(),
            n2: c.n2.spice_name().to_string(),
            value: convert_value(&c.value),
            params: convert_params(&c.params),
        },
        Element::L(l) => Component::Inductor {
            name: l.name.clone(),
            n1: l.n1.spice_name().to_string(),
            n2: l.n2.spice_name().to_string(),
            value: convert_value(&l.value),
            params: convert_params(&l.params),
        },
        Element::K(k) => Component::MutualInductor {
            name: k.name.clone(),
            inductor1: k.inductor1.clone(),
            inductor2: k.inductor2.clone(),
            coupling: k.coupling,
        },
        Element::V(v) => Component::VoltageSource {
            name: v.name.clone(),
            np: v.np.spice_name().to_string(),
            nm: v.nm.spice_name().to_string(),
            value: convert_value(&v.value),
            waveform: v.waveform.as_ref().map(convert_waveform),
        },
        Element::I(i) => Component::CurrentSource {
            name: i.name.clone(),
            np: i.np.spice_name().to_string(),
            nm: i.nm.spice_name().to_string(),
            value: convert_value(&i.value),
            waveform: i.waveform.as_ref().map(convert_waveform),
        },
        Element::BV(bv) => Component::BehavioralVoltage {
            name: bv.name.clone(),
            np: bv.np.spice_name().to_string(),
            nm: bv.nm.spice_name().to_string(),
            expression: bv.expression.clone(),
        },
        Element::BI(bi) => Component::BehavioralCurrent {
            name: bi.name.clone(),
            np: bi.np.spice_name().to_string(),
            nm: bi.nm.spice_name().to_string(),
            expression: bi.expression.clone(),
        },
        Element::E(e) => Component::Vcvs {
            name: e.name.clone(),
            np: e.np.spice_name().to_string(),
            nm: e.nm.spice_name().to_string(),
            ncp: e.ncp.spice_name().to_string(),
            ncm: e.ncm.spice_name().to_string(),
            gain: e.gain,
        },
        Element::G(g) => Component::Vccs {
            name: g.name.clone(),
            np: g.np.spice_name().to_string(),
            nm: g.nm.spice_name().to_string(),
            ncp: g.ncp.spice_name().to_string(),
            ncm: g.ncm.spice_name().to_string(),
            transconductance: g.transconductance,
        },
        Element::F(f) => Component::Cccs {
            name: f.name.clone(),
            np: f.np.spice_name().to_string(),
            nm: f.nm.spice_name().to_string(),
            vsense: f.vsense.clone(),
            gain: f.gain,
        },
        Element::H(h) => Component::Ccvs {
            name: h.name.clone(),
            np: h.np.spice_name().to_string(),
            nm: h.nm.spice_name().to_string(),
            vsense: h.vsense.clone(),
            transresistance: h.transresistance,
        },
        Element::D(d) => Component::Diode {
            name: d.name.clone(),
            np: d.np.spice_name().to_string(),
            nm: d.nm.spice_name().to_string(),
            model: d.model.clone(),
            params: convert_params(&d.params),
        },
        Element::Q(q) => Component::Bjt {
            name: q.name.clone(),
            nc: q.nc.spice_name().to_string(),
            nb: q.nb.spice_name().to_string(),
            ne: q.ne.spice_name().to_string(),
            model: q.model.clone(),
            params: convert_params(&q.params),
        },
        Element::M(m) => Component::Mosfet {
            name: m.name.clone(),
            nd: m.nd.spice_name().to_string(),
            ng: m.ng.spice_name().to_string(),
            ns: m.ns.spice_name().to_string(),
            nb: m.nb.spice_name().to_string(),
            model: m.model.clone(),
            params: convert_params(&m.params),
        },
        Element::J(j) => Component::Jfet {
            name: j.name.clone(),
            nd: j.nd.spice_name().to_string(),
            ng: j.ng.spice_name().to_string(),
            ns: j.ns.spice_name().to_string(),
            model: j.model.clone(),
            params: convert_params(&j.params),
        },
        Element::Z(z) => Component::Mesfet {
            name: z.name.clone(),
            nd: z.nd.spice_name().to_string(),
            ng: z.ng.spice_name().to_string(),
            ns: z.ns.spice_name().to_string(),
            model: z.model.clone(),
            params: convert_params(&z.params),
        },
        Element::S(s) => Component::VSwitch {
            name: s.name.clone(),
            np: s.np.spice_name().to_string(),
            nm: s.nm.spice_name().to_string(),
            ncp: s.ncp.spice_name().to_string(),
            ncm: s.ncm.spice_name().to_string(),
            model: s.model.clone(),
        },
        Element::W(w) => Component::ISwitch {
            name: w.name.clone(),
            np: w.np.spice_name().to_string(),
            nm: w.nm.spice_name().to_string(),
            vcontrol: w.vcontrol.clone(),
            model: w.model.clone(),
        },
        Element::T(t) => Component::TLine {
            name: t.name.clone(),
            inp: t.inp.spice_name().to_string(),
            inm: t.inm.spice_name().to_string(),
            outp: t.outp.spice_name().to_string(),
            outm: t.outm.spice_name().to_string(),
            z0: t.z0,
            td: t.td,
        },
        Element::X(_) => {
            // Subcircuit instances are handled separately as Instance, not Component.
            // This branch should not be reached in normal usage since the caller
            // filters X-elements into instances.
            unreachable!("SubcircuitInstance should be converted to Instance, not Component")
        }
        Element::A(a) => Component::Xspice {
            name: a.name.clone(),
            connections: a.connections.clone(),
            model: a.model.clone(),
        },
        Element::RawSpice(s) => Component::RawSpice {
            line: s.clone(),
        },
    }
}

fn convert_value(cv: &crate::circuit::ComponentValue) -> IrValue {
    use crate::circuit::ComponentValue;

    match cv {
        ComponentValue::Numeric(v) => IrValue::Numeric { value: *v },
        ComponentValue::Unit(uv) => IrValue::Numeric { value: uv.as_f64() },
        ComponentValue::Expression(e) => IrValue::Expression { expr: e.clone() },
        ComponentValue::Raw(r) => IrValue::Raw { text: r.clone() },
    }
}

fn convert_waveform(wf: &crate::circuit::Waveform) -> IrWaveform {
    use crate::circuit::Waveform;

    match wf {
        Waveform::Sin(w) => IrWaveform::Sin {
            offset: w.offset,
            amplitude: w.amplitude,
            frequency: w.frequency,
            delay: w.delay,
            damping: w.damping,
            phase: w.phase,
        },
        Waveform::Pulse(w) => IrWaveform::Pulse {
            initial: w.initial,
            pulsed: w.pulsed,
            delay: w.delay,
            rise_time: w.rise_time,
            fall_time: w.fall_time,
            pulse_width: w.pulse_width,
            period: w.period,
        },
        Waveform::Pwl(w) => IrWaveform::Pwl {
            values: w.values.clone(),
        },
        Waveform::Exp(w) => IrWaveform::Exp {
            initial: w.initial,
            pulsed: w.pulsed,
            rise_delay: w.rise_delay,
            rise_tau: w.rise_tau,
            fall_delay: w.fall_delay,
            fall_tau: w.fall_tau,
        },
        Waveform::Sffm(w) => IrWaveform::Sffm {
            offset: w.offset,
            amplitude: w.amplitude,
            carrier_freq: w.carrier_freq,
            modulation_index: w.modulation_index,
            signal_freq: w.signal_freq,
        },
        Waveform::Am(w) => IrWaveform::Am {
            amplitude: w.amplitude,
            offset: w.offset,
            modulating_freq: w.modulating_freq,
            carrier_freq: w.carrier_freq,
            delay: w.delay,
        },
    }
}

fn convert_params(params: &[crate::circuit::Param]) -> Vec<(String, String)> {
    params.iter().map(|p| (p.name.clone(), p.value.clone())).collect()
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::{Circuit, Param, SubCircuitDef, Node, Mosfet, Element};

    #[test]
    fn test_ir_construction() {
        let sub = Subcircuit {
            name: "top".into(),
            ports: Vec::new(),
            parameters: vec![ParamDef { name: "vdd".into(), default: Some("3.3".into()) }],
            components: vec![
                Component::Resistor {
                    name: "1".into(),
                    n1: "in".into(),
                    n2: "out".into(),
                    value: IrValue::Numeric { value: 1000.0 },
                    params: Vec::new(),
                },
                Component::Capacitor {
                    name: "1".into(),
                    n1: "out".into(),
                    n2: "0".into(),
                    value: IrValue::Numeric { value: 10e-12 },
                    params: Vec::new(),
                },
                Component::VoltageSource {
                    name: "dd".into(),
                    np: "vdd".into(),
                    nm: "0".into(),
                    value: IrValue::Numeric { value: 3.3 },
                    waveform: Some(IrWaveform::Sin {
                        offset: 0.0,
                        amplitude: 1.0,
                        frequency: 1e6,
                        delay: 0.0,
                        damping: 0.0,
                        phase: 0.0,
                    }),
                },
            ],
            instances: vec![Instance {
                name: "1".into(),
                subcircuit: "MyBuf".into(),
                port_mapping: vec!["in".into(), "out".into()],
                parameters: Vec::new(),
            }],
            models: vec![ModelDef {
                name: "nmos_3p3".into(),
                kind: "NMOS".into(),
                parameters: vec![("VTO".into(), "0.7".into())],
            }],
            raw_spice: Vec::new(),
            includes: Vec::new(),
            libs: Vec::new(),
            osdi_loads: Vec::new(),
            verilog_blocks: vec![],
        };

        let ir = CircuitIR {
            top: sub.clone(),
            testbench: None,
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        assert_eq!(ir.top.name, "top");
        assert_eq!(ir.top.components.len(), 3);
        assert_eq!(ir.top.instances.len(), 1);
        assert_eq!(ir.top.models.len(), 1);
        assert_eq!(ir.top.parameters.len(), 1);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "roundtrip_test".into(),
                ports: vec![Port { name: "in".into(), direction: PortDirection::Input }],
                parameters: Vec::new(),
                components: vec![
                    Component::Resistor {
                        name: "1".into(),
                        n1: "in".into(),
                        n2: "out".into(),
                        value: IrValue::Numeric { value: 1000.0 },
                        params: Vec::new(),
                    },
                    Component::Mosfet {
                        name: "1".into(),
                        nd: "drain".into(),
                        ng: "gate".into(),
                        ns: "source".into(),
                        nb: "bulk".into(),
                        model: "nmos".into(),
                        params: vec![("W".into(), "1u".into()), ("L".into(), "180n".into())],
                    },
                    Component::BehavioralVoltage {
                        name: "1".into(),
                        np: "out".into(),
                        nm: "0".into(),
                        expression: "V(in)*2".into(),
                    },
                    Component::Xspice {
                        name: "1".into(),
                        connections: vec!["in".into(), "out".into()],
                        model: "d_and".into(),
                    },
                ],
                instances: Vec::new(),
                models: Vec::new(),
                raw_spice: Vec::new(),
                includes: Vec::new(),
                libs: Vec::new(),
                osdi_loads: Vec::new(),
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "roundtrip_test".into(),
                stimulus: Vec::new(),
                analyses: vec![
                    Analysis::Op,
                    Analysis::Transient { step: 1e-9, stop: 1e-6, start: None, max_step: None, uic: false },
                    Analysis::Ac { variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 },
                ],
                options: SimOptions::default(),
                saves: vec!["V(out)".into()],
                measures: Vec::new(),
                temperature: Some(27.0),
                nominal_temperature: None,
                initial_conditions: Vec::new(),
                node_sets: Vec::new(),
                step_params: Vec::new(),
                extra_lines: Vec::new(),
            }),
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        let json = serde_json::to_string_pretty(&ir).expect("serialize");
        let deserialized: CircuitIR = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(ir, deserialized);
    }

    #[test]
    fn test_compute_features_xspice() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "xspice_test".into(),
                ports: Vec::new(),
                parameters: Vec::new(),
                components: vec![
                    Component::Xspice {
                        name: "1".into(),
                        connections: vec!["in".into(), "out".into()],
                        model: "d_and".into(),
                    },
                    Component::Resistor {
                        name: "1".into(),
                        n1: "out".into(),
                        n2: "0".into(),
                        value: IrValue::Numeric { value: 1000.0 },
                        params: Vec::new(),
                    },
                ],
                instances: Vec::new(),
                models: Vec::new(),
                raw_spice: Vec::new(),
                includes: Vec::new(),
                libs: Vec::new(),
                osdi_loads: Vec::new(),
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        let features = ir.compute_features();
        assert!(features.has_xspice);
        assert!(!features.has_osdi);
        assert!(!features.has_measures);
        assert!(!features.has_laplace_sources);
        assert_eq!(features.element_count, 2);
    }

    #[test]
    fn test_compute_features_osdi_and_laplace() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "osdi_test".into(),
                ports: Vec::new(),
                parameters: Vec::new(),
                components: vec![
                    Component::BehavioralVoltage {
                        name: "1".into(),
                        np: "out".into(),
                        nm: "0".into(),
                        expression: "laplace(V(in), {1/(1+s*1e-6)})".into(),
                    },
                ],
                instances: Vec::new(),
                models: Vec::new(),
                raw_spice: Vec::new(),
                includes: Vec::new(),
                libs: Vec::new(),
                osdi_loads: vec!["model.osdi".into()],
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        let features = ir.compute_features();
        assert!(!features.has_xspice);
        assert!(features.has_osdi);
        assert!(features.has_laplace_sources);
        assert_eq!(features.element_count, 1);
    }

    #[test]
    fn test_compute_features_control_blocks() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "ctrl_test".into(),
                ports: Vec::new(),
                parameters: Vec::new(),
                components: Vec::new(),
                instances: Vec::new(),
                models: Vec::new(),
                raw_spice: vec![".control".into(), "run".into(), ".endc".into()],
                includes: Vec::new(),
                libs: Vec::new(),
                osdi_loads: Vec::new(),
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        let features = ir.compute_features();
        assert!(features.has_control_blocks);
    }

    #[test]
    fn test_compute_features_measures_and_step() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "meas_test".into(),
                ports: Vec::new(),
                parameters: Vec::new(),
                components: Vec::new(),
                instances: Vec::new(),
                models: Vec::new(),
                raw_spice: Vec::new(),
                includes: Vec::new(),
                libs: Vec::new(),
                osdi_loads: Vec::new(),
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "meas_test".into(),
                stimulus: Vec::new(),
                analyses: Vec::new(),
                options: SimOptions::default(),
                saves: Vec::new(),
                measures: vec!["tran v_max MAX V(out)".into()],
                temperature: None,
                nominal_temperature: None,
                initial_conditions: Vec::new(),
                node_sets: Vec::new(),
                step_params: vec![StepParam {
                    param: "R1".into(),
                    start: 100.0,
                    stop: 10000.0,
                    step: 100.0,
                    sweep_type: None,
                }],
                extra_lines: Vec::new(),
            }),
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        let features = ir.compute_features();
        assert!(features.has_measures);
        assert!(features.has_step_params);
    }

    #[test]
    fn test_check_backend_xspice_against_xyce() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "xspice_check".into(),
                ports: Vec::new(),
                parameters: Vec::new(),
                components: vec![Component::Xspice {
                    name: "1".into(),
                    connections: vec!["in".into(), "out".into()],
                    model: "d_and".into(),
                }],
                instances: Vec::new(),
                models: Vec::new(),
                raw_spice: Vec::new(),
                includes: Vec::new(),
                libs: Vec::new(),
                osdi_loads: Vec::new(),
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        let issues = ir.check_backend("xyce");
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.severity == IssueSeverity::Error));
        assert!(issues.iter().any(|i| i.message.contains("XSPICE")));

        // ngspice should be fine
        let ngspice_issues = ir.check_backend("ngspice");
        assert!(ngspice_issues.is_empty());
    }

    #[test]
    fn test_check_backend_osdi_against_ltspice() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "osdi_check".into(),
                ports: Vec::new(),
                parameters: Vec::new(),
                components: Vec::new(),
                instances: Vec::new(),
                models: Vec::new(),
                raw_spice: Vec::new(),
                includes: Vec::new(),
                libs: Vec::new(),
                osdi_loads: vec!["model.osdi".into()],
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        let issues = ir.check_backend("ltspice");
        assert!(issues.iter().any(|i| {
            i.severity == IssueSeverity::Error && i.message.contains("OSDI")
        }));

        // Spectre supports OSDI
        assert!(ir.check_backend("spectre").is_empty());
    }

    #[test]
    fn test_check_backend_no_issues_for_clean_circuit() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "clean".into(),
                ports: Vec::new(),
                parameters: Vec::new(),
                components: vec![Component::Resistor {
                    name: "1".into(),
                    n1: "in".into(),
                    n2: "out".into(),
                    value: IrValue::Numeric { value: 1000.0 },
                    params: Vec::new(),
                }],
                instances: Vec::new(),
                models: Vec::new(),
                raw_spice: Vec::new(),
                includes: Vec::new(),
                libs: Vec::new(),
                osdi_loads: Vec::new(),
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: Vec::new(),
            model_libraries: Vec::new(),
        };

        for backend in &["ngspice", "xyce", "ltspice", "vacask", "spectre"] {
            assert!(ir.check_backend(backend).is_empty(), "Expected no issues for {}", backend);
        }
    }

    #[test]
    fn test_from_circuit_basic() {
        let mut c = Circuit::new("test_conversion");
        c.r("1", "in", "out", 1000.0);
        c.c("1", "out", Node::Ground, 10e-12);
        c.v("dd", "vdd", Node::Ground, 3.3);
        c.model("nmos_3p3", "NMOS", vec![
            Param::new("LEVEL", "1"),
            Param::new("VTO", "0.7"),
        ]);
        c.parameter("vdd_val", "3.3");
        c.include("/path/to/model.lib");
        c.lib("/path/to/pdk.lib", "tt");
        c.raw_spice(".global vdd gnd");

        let ir = CircuitIR::from_circuit(&c);

        assert_eq!(ir.top.name, "test_conversion");
        assert_eq!(ir.top.components.len(), 3);
        assert_eq!(ir.top.models.len(), 1);
        assert_eq!(ir.top.parameters.len(), 1);
        assert_eq!(ir.top.includes.len(), 1);
        assert_eq!(ir.top.libs.len(), 1);
        assert_eq!(ir.top.raw_spice.len(), 1);

        // Check resistor conversion
        match &ir.top.components[0] {
            Component::Resistor { name, n1, n2, value, .. } => {
                assert_eq!(name, "1");
                assert_eq!(n1, "in");
                assert_eq!(n2, "out");
                assert_eq!(*value, IrValue::Numeric { value: 1000.0 });
            }
            other => panic!("Expected Resistor, got {:?}", other),
        }

        // Check capacitor - node Ground becomes "0"
        match &ir.top.components[1] {
            Component::Capacitor { n2, value, .. } => {
                assert_eq!(n2, "0");
                assert_eq!(*value, IrValue::Numeric { value: 10e-12 });
            }
            other => panic!("Expected Capacitor, got {:?}", other),
        }

        // Check model
        assert_eq!(ir.top.models[0].name, "nmos_3p3");
        assert_eq!(ir.top.models[0].kind, "NMOS");
        assert_eq!(ir.top.models[0].parameters.len(), 2);
    }

    #[test]
    fn test_from_circuit_with_subcircuit() {
        let mut c = Circuit::new("subckt_test");
        c.subcircuit(SubCircuitDef {
            name: "MyBuf".into(),
            pins: vec!["in".into(), "out".into(), "vdd".into(), "gnd".into()],
            elements: vec![Element::M(Mosfet {
                name: "1".into(),
                nd: Node::named("out"),
                ng: Node::named("in"),
                ns: Node::named("vdd"),
                nb: Node::named("vdd"),
                model: "pmos".into(),
                params: vec![],
            })],
            models: vec![],
            params: vec![Param::new("wp", "1u")],
        });
        c.x("1", "MyBuf", vec!["in", "out", "vdd", "gnd"]);

        let ir = CircuitIR::from_circuit(&c);

        // SubcircuitInstance becomes an Instance on the top-level subcircuit
        assert_eq!(ir.top.instances.len(), 1);
        assert_eq!(ir.top.instances[0].subcircuit, "MyBuf");
        assert_eq!(ir.top.instances[0].port_mapping, vec!["in", "out", "vdd", "0"]);

        // SubCircuitDef becomes a subcircuit_defs entry
        assert_eq!(ir.subcircuit_defs.len(), 1);
        assert_eq!(ir.subcircuit_defs[0].name, "MyBuf");
        assert_eq!(ir.subcircuit_defs[0].ports.len(), 4);
        assert_eq!(ir.subcircuit_defs[0].components.len(), 1);
        assert_eq!(ir.subcircuit_defs[0].parameters.len(), 1);
        assert_eq!(ir.subcircuit_defs[0].parameters[0].name, "wp");
        assert_eq!(ir.subcircuit_defs[0].parameters[0].default, Some("1u".into()));

        // The MOSFET in the subcircuit definition
        match &ir.subcircuit_defs[0].components[0] {
            Component::Mosfet { name, nd, ng, ns, nb, model, .. } => {
                assert_eq!(name, "1");
                assert_eq!(nd, "out");
                assert_eq!(ng, "in");
                assert_eq!(ns, "vdd");
                assert_eq!(nb, "vdd");
                assert_eq!(model, "pmos");
            }
            other => panic!("Expected Mosfet, got {:?}", other),
        }
    }

    // ── JSON serialization tests ──

    #[test]
    fn test_full_ir_json_roundtrip() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "opamp_tb".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![
                    Component::Resistor {
                        name: "f".into(),
                        n1: "out".into(),
                        n2: "inv".into(),
                        value: IrValue::Numeric { value: 100e3 },
                        params: vec![],
                    },
                    Component::Mosfet {
                        name: "1".into(),
                        nd: "out".into(),
                        ng: "inv".into(),
                        ns: "vss".into(),
                        nb: "vss".into(),
                        model: "nmos_3p3".into(),
                        params: vec![("W".into(), "10u".into()), ("L".into(), "1u".into())],
                    },
                    Component::VoltageSource {
                        name: "dd".into(),
                        np: "vdd".into(),
                        nm: "0".into(),
                        value: IrValue::Numeric { value: 3.3 },
                        waveform: None,
                    },
                    Component::VoltageSource {
                        name: "in".into(),
                        np: "inp".into(),
                        nm: "0".into(),
                        value: IrValue::Numeric { value: 0.0 },
                        waveform: Some(IrWaveform::Sin {
                            offset: 1.65,
                            amplitude: 0.1,
                            frequency: 1e3,
                            delay: 0.0,
                            damping: 0.0,
                            phase: 0.0,
                        }),
                    },
                    Component::Xspice {
                        name: "buf1".into(),
                        connections: vec!["[din]".into(), "[dout]".into()],
                        model: "buf_model".into(),
                    },
                ],
                instances: vec![Instance {
                    name: "dut".into(),
                    subcircuit: "opamp".into(),
                    port_mapping: vec!["inp".into(), "inv".into(), "out".into(), "vdd".into(), "vss".into()],
                    parameters: vec![("gain".into(), "100".into())],
                }],
                models: vec![ModelDef {
                    name: "nmos_3p3".into(),
                    kind: "NMOS".into(),
                    parameters: vec![("VTO".into(), "0.7".into()), ("KP".into(), "110u".into())],
                }],
                raw_spice: vec![".global vdd vss".into()],
                includes: vec!["/pdk/sky130.lib".into()],
                libs: vec![("/pdk/sky130.lib".into(), "tt".into())],
                osdi_loads: vec!["/models/bsim4.osdi".into()],
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "opamp_tb".into(),
                stimulus: vec![],
                analyses: vec![
                    Analysis::Op,
                    Analysis::Ac {
                        variation: "dec".into(),
                        points: 100,
                        start: 1.0,
                        stop: 1e9,
                    },
                    Analysis::Transient {
                        step: 1e-9,
                        stop: 1e-3,
                        start: None,
                        max_step: Some(1e-10),
                        uic: false,
                    },
                ],
                options: SimOptions {
                    portable: vec![("reltol".into(), "1e-4".into())],
                    backend_specific: {
                        let mut m = HashMap::new();
                        m.insert("spectre".into(), vec![("errpreset".into(), "moderate".into())]);
                        m
                    },
                },
                saves: vec!["all".into()],
                measures: vec!["tran avg_vout AVG V(out) FROM=100n TO=900n".into()],
                temperature: Some(27.0),
                nominal_temperature: None,
                initial_conditions: vec![("out".into(), 1.65)],
                node_sets: vec![],
                step_params: vec![StepParam {
                    param: "Rf".into(),
                    start: 10e3,
                    stop: 100e3,
                    step: 10e3,
                    sweep_type: None,
                }],
                extra_lines: vec![],
            }),
            subcircuit_defs: vec![Subcircuit {
                name: "opamp".into(),
                ports: vec![
                    Port { name: "inp".into(), direction: PortDirection::Input },
                    Port { name: "inv".into(), direction: PortDirection::Input },
                    Port { name: "out".into(), direction: PortDirection::Output },
                    Port { name: "vdd".into(), direction: PortDirection::InOut },
                    Port { name: "vss".into(), direction: PortDirection::InOut },
                ],
                parameters: vec![ParamDef { name: "gain".into(), default: Some("100".into()) }],
                components: vec![
                    Component::Resistor {
                        name: "1".into(),
                        n1: "inp".into(),
                        n2: "out".into(),
                        value: IrValue::Numeric { value: 1e6 },
                        params: vec![],
                    },
                ],
                instances: vec![],
                models: vec![],
                raw_spice: vec![],
                includes: vec![],
                libs: vec![],
                osdi_loads: vec![],
                verilog_blocks: vec![],
            }],
            model_libraries: vec![ModelLibrary {
                name: "sky130".into(),
                path: "/pdk/sky130.lib".into(),
                corner: Some("tt".into()),
                backend_paths: {
                    let mut m = HashMap::new();
                    m.insert("ngspice".into(), "/pdk/ngspice/sky130.lib".into());
                    m.insert("spectre".into(), "/pdk/spectre/sky130.scs".into());
                    m
                },
            }],
        };

        // Serialize
        let json = serde_json::to_string_pretty(&ir).unwrap();

        // Check key fields exist in JSON
        assert!(json.contains("\"type\": \"Resistor\""));
        assert!(json.contains("\"type\": \"Mosfet\""));
        assert!(json.contains("\"type\": \"Op\""));
        assert!(json.contains("\"type\": \"Ac\""));
        assert!(json.contains("\"type\": \"Sin\""));
        assert!(json.contains("\"type\": \"Xspice\""));
        assert!(json.contains("\"reltol\""));
        assert!(json.contains("\"sky130\""));

        // Deserialize
        let ir2: CircuitIR = serde_json::from_str(&json).unwrap();

        // Verify equality
        assert_eq!(ir, ir2);

        // Re-serialize, deserialize again, verify structural equality
        // (we don't compare JSON strings directly because HashMap key order is non-deterministic)
        let json2 = serde_json::to_string_pretty(&ir2).unwrap();
        let ir3: CircuitIR = serde_json::from_str(&json2).unwrap();
        assert_eq!(ir2, ir3);
    }

    #[test]
    fn test_component_json_tags() {
        // Verify each component type produces the right JSON tag
        let r = Component::Resistor {
            name: "1".into(),
            n1: "a".into(),
            n2: "b".into(),
            value: IrValue::Numeric { value: 1000.0 },
            params: vec![],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"type\":\"Resistor\""));

        let c = Component::Capacitor {
            name: "1".into(),
            n1: "a".into(),
            n2: "b".into(),
            value: IrValue::Numeric { value: 1e-12 },
            params: vec![],
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"type\":\"Capacitor\""));

        let l = Component::Inductor {
            name: "1".into(),
            n1: "a".into(),
            n2: "b".into(),
            value: IrValue::Numeric { value: 1e-6 },
            params: vec![],
        };
        let json = serde_json::to_string(&l).unwrap();
        assert!(json.contains("\"type\":\"Inductor\""));

        let k = Component::MutualInductor {
            name: "1".into(),
            inductor1: "L1".into(),
            inductor2: "L2".into(),
            coupling: 0.99,
        };
        let json = serde_json::to_string(&k).unwrap();
        assert!(json.contains("\"type\":\"MutualInductor\""));

        let bv = Component::BehavioralVoltage {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            expression: "V(in)*2".into(),
        };
        let json = serde_json::to_string(&bv).unwrap();
        assert!(json.contains("\"type\":\"BehavioralVoltage\""));

        let bi = Component::BehavioralCurrent {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            expression: "I(Vsense)*10".into(),
        };
        let json = serde_json::to_string(&bi).unwrap();
        assert!(json.contains("\"type\":\"BehavioralCurrent\""));

        let vcvs = Component::Vcvs {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            ncp: "c".into(),
            ncm: "d".into(),
            gain: 10.0,
        };
        let json = serde_json::to_string(&vcvs).unwrap();
        assert!(json.contains("\"type\":\"Vcvs\""));

        let vccs = Component::Vccs {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            ncp: "c".into(),
            ncm: "d".into(),
            transconductance: 1e-3,
        };
        let json = serde_json::to_string(&vccs).unwrap();
        assert!(json.contains("\"type\":\"Vccs\""));

        let cccs = Component::Cccs {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            vsense: "Vs".into(),
            gain: 100.0,
        };
        let json = serde_json::to_string(&cccs).unwrap();
        assert!(json.contains("\"type\":\"Cccs\""));

        let ccvs = Component::Ccvs {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            vsense: "Vs".into(),
            transresistance: 1e3,
        };
        let json = serde_json::to_string(&ccvs).unwrap();
        assert!(json.contains("\"type\":\"Ccvs\""));

        let d = Component::Diode {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            model: "D1N4148".into(),
            params: vec![],
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"type\":\"Diode\""));

        let q = Component::Bjt {
            name: "1".into(),
            nc: "c".into(),
            nb: "b".into(),
            ne: "e".into(),
            model: "2N2222".into(),
            params: vec![],
        };
        let json = serde_json::to_string(&q).unwrap();
        assert!(json.contains("\"type\":\"Bjt\""));

        let m = Component::Mosfet {
            name: "1".into(),
            nd: "d".into(),
            ng: "g".into(),
            ns: "s".into(),
            nb: "b".into(),
            model: "nmos".into(),
            params: vec![],
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"type\":\"Mosfet\""));

        let j = Component::Jfet {
            name: "1".into(),
            nd: "d".into(),
            ng: "g".into(),
            ns: "s".into(),
            model: "J201".into(),
            params: vec![],
        };
        let json = serde_json::to_string(&j).unwrap();
        assert!(json.contains("\"type\":\"Jfet\""));

        let z = Component::Mesfet {
            name: "1".into(),
            nd: "d".into(),
            ng: "g".into(),
            ns: "s".into(),
            model: "GAASfet".into(),
            params: vec![],
        };
        let json = serde_json::to_string(&z).unwrap();
        assert!(json.contains("\"type\":\"Mesfet\""));

        let vs = Component::VSwitch {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            ncp: "c".into(),
            ncm: "d".into(),
            model: "SW1".into(),
        };
        let json = serde_json::to_string(&vs).unwrap();
        assert!(json.contains("\"type\":\"VSwitch\""));

        let is = Component::ISwitch {
            name: "1".into(),
            np: "a".into(),
            nm: "b".into(),
            vcontrol: "Vsense".into(),
            model: "CSW1".into(),
        };
        let json = serde_json::to_string(&is).unwrap();
        assert!(json.contains("\"type\":\"ISwitch\""));

        let tl = Component::TLine {
            name: "1".into(),
            inp: "in_p".into(),
            inm: "in_m".into(),
            outp: "out_p".into(),
            outm: "out_m".into(),
            z0: 50.0,
            td: 1e-9,
        };
        let json = serde_json::to_string(&tl).unwrap();
        assert!(json.contains("\"type\":\"TLine\""));

        let x = Component::Xspice {
            name: "1".into(),
            connections: vec!["[d]".into()],
            model: "buf".into(),
        };
        let json = serde_json::to_string(&x).unwrap();
        assert!(json.contains("\"type\":\"Xspice\""));

        let raw = Component::RawSpice {
            line: ".probe V(out)".into(),
        };
        let json = serde_json::to_string(&raw).unwrap();
        assert!(json.contains("\"type\":\"RawSpice\""));
    }

    #[test]
    fn test_analysis_json_tags() {
        let a = Analysis::Op;
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"Op\""));

        let a = Analysis::Transient { step: 1e-9, stop: 1e-6, start: None, max_step: None, uic: false };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"Transient\""));

        let a = Analysis::Ac { variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"Ac\""));

        let a = Analysis::Dc { sweeps: vec![DcSweep { source: "V1".into(), start: 0.0, stop: 5.0, step: 0.1 }] };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"Dc\""));

        let a = Analysis::Noise {
            output: "V(out)".into(), reference: "0".into(), source: "V1".into(),
            variation: "dec".into(), points: 10, start: 1.0, stop: 1e6,
            points_per_summary: Some(1),
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"Noise\""));

        let a = Analysis::Tf { output: "V(out)".into(), source: "Vin".into() };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"Tf\""));

        let a = Analysis::Sensitivity { output: "V(out)".into(), ac: None };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"Sensitivity\""));

        let a = Analysis::Fourier { fundamental: 1e3, outputs: vec!["V(out)".into()], num_harmonics: Some(10) };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"Fourier\""));
    }

    #[test]
    fn test_irvalue_json_tags() {
        let v = IrValue::Numeric { value: 42.0 };
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"type\":\"Numeric\""));

        let v = IrValue::Expression { expr: "vdd/2".into() };
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"type\":\"Expression\""));

        let v = IrValue::Raw { text: "1k".into() };
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"type\":\"Raw\""));
    }

    #[test]
    fn test_waveform_json_tags() {
        let w = IrWaveform::Sin { offset: 0.0, amplitude: 1.0, frequency: 1e3, delay: 0.0, damping: 0.0, phase: 0.0 };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"type\":\"Sin\""));

        let w = IrWaveform::Pulse { initial: 0.0, pulsed: 3.3, delay: 0.0, rise_time: 1e-9, fall_time: 1e-9, pulse_width: 5e-9, period: 10e-9 };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"type\":\"Pulse\""));

        let w = IrWaveform::Pwl { values: vec![(0.0, 0.0), (1e-6, 1.0), (2e-6, 0.0)] };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"type\":\"Pwl\""));

        let w = IrWaveform::Exp { initial: 0.0, pulsed: 1.0, rise_delay: 0.0, rise_tau: 1e-6, fall_delay: 5e-6, fall_tau: 1e-6 };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"type\":\"Exp\""));

        let w = IrWaveform::Sffm { offset: 0.0, amplitude: 1.0, carrier_freq: 1e6, modulation_index: 5.0, signal_freq: 1e3 };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"type\":\"Sffm\""));

        let w = IrWaveform::Am { amplitude: 1.0, offset: 0.0, modulating_freq: 1e3, carrier_freq: 1e6, delay: 0.0 };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"type\":\"Am\""));
    }

    #[test]
    fn test_invalid_json_fails_gracefully() {
        let bad_json = r#"{"not": "a circuit"}"#;
        let result: Result<CircuitIR, _> = serde_json::from_str(bad_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_minimal_ir_roundtrip() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "empty".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![],
                instances: vec![],
                models: vec![],
                raw_spice: vec![],
                includes: vec![],
                libs: vec![],
                osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: vec![],
            model_libraries: vec![],
        };
        let json = serde_json::to_string(&ir).unwrap();
        let ir2: CircuitIR = serde_json::from_str(&json).unwrap();
        assert_eq!(ir, ir2);
    }

    #[test]
    fn test_load_example_voltage_divider() {
        let json = include_str!("../../schema/examples/voltage_divider.circuit.json");
        let ir: CircuitIR = serde_json::from_str(json).unwrap();
        assert_eq!(ir.top.name, "Voltage Divider");
        assert_eq!(ir.top.components.len(), 3);
        assert!(ir.testbench.is_some());
        let tb = ir.testbench.as_ref().unwrap();
        assert_eq!(tb.analyses.len(), 1);
        match &tb.analyses[0] {
            Analysis::Op => {}
            other => panic!("Expected Op analysis, got {:?}", other),
        }
    }

    #[test]
    fn test_tuple_serialization_format() {
        // Verify that (String, String) tuples serialize as JSON arrays, not objects
        let sub = Subcircuit {
            name: "tuple_test".into(),
            ports: vec![],
            parameters: vec![],
            components: vec![],
            instances: vec![],
            models: vec![],
            raw_spice: vec![],
            includes: vec![],
            libs: vec![("/pdk/sky130.lib".into(), "tt".into())],
            osdi_loads: vec![],
            verilog_blocks: vec![],
        };
        let json = serde_json::to_string_pretty(&sub).unwrap();
        // libs should be [["/pdk/sky130.lib", "tt"]]
        assert!(json.contains("\"/pdk/sky130.lib\""));
        assert!(json.contains("\"tt\""));

        // Roundtrip check
        let sub2: Subcircuit = serde_json::from_str(&json).unwrap();
        assert_eq!(sub, sub2);
    }

    #[test]
    fn test_all_component_variants_roundtrip() {
        // Build an IR containing every Component variant to ensure full roundtrip coverage
        let all_components = vec![
            Component::Resistor { name: "1".into(), n1: "a".into(), n2: "b".into(), value: IrValue::Numeric { value: 1e3 }, params: vec![("tc1".into(), "0.001".into())] },
            Component::Capacitor { name: "1".into(), n1: "a".into(), n2: "b".into(), value: IrValue::Expression { expr: "cap_val".into() }, params: vec![] },
            Component::Inductor { name: "1".into(), n1: "a".into(), n2: "b".into(), value: IrValue::Raw { text: "10u".into() }, params: vec![] },
            Component::MutualInductor { name: "1".into(), inductor1: "L1".into(), inductor2: "L2".into(), coupling: 0.95 },
            Component::VoltageSource { name: "1".into(), np: "a".into(), nm: "b".into(), value: IrValue::Numeric { value: 5.0 }, waveform: None },
            Component::CurrentSource { name: "1".into(), np: "a".into(), nm: "b".into(), value: IrValue::Numeric { value: 1e-3 }, waveform: Some(IrWaveform::Pulse { initial: 0.0, pulsed: 1e-3, delay: 0.0, rise_time: 1e-9, fall_time: 1e-9, pulse_width: 5e-6, period: 10e-6 }) },
            Component::BehavioralVoltage { name: "1".into(), np: "a".into(), nm: "b".into(), expression: "V(x)*2+1".into() },
            Component::BehavioralCurrent { name: "1".into(), np: "a".into(), nm: "b".into(), expression: "I(V1)*10".into() },
            Component::Vcvs { name: "1".into(), np: "a".into(), nm: "b".into(), ncp: "c".into(), ncm: "d".into(), gain: 100.0 },
            Component::Vccs { name: "1".into(), np: "a".into(), nm: "b".into(), ncp: "c".into(), ncm: "d".into(), transconductance: 0.01 },
            Component::Cccs { name: "1".into(), np: "a".into(), nm: "b".into(), vsense: "Vs".into(), gain: 50.0 },
            Component::Ccvs { name: "1".into(), np: "a".into(), nm: "b".into(), vsense: "Vs".into(), transresistance: 500.0 },
            Component::Diode { name: "1".into(), np: "a".into(), nm: "b".into(), model: "D1".into(), params: vec![] },
            Component::Bjt { name: "1".into(), nc: "c".into(), nb: "b".into(), ne: "e".into(), model: "Q1".into(), params: vec![("area".into(), "2".into())] },
            Component::Mosfet { name: "1".into(), nd: "d".into(), ng: "g".into(), ns: "s".into(), nb: "b".into(), model: "M1".into(), params: vec![("W".into(), "1u".into()), ("L".into(), "180n".into())] },
            Component::Jfet { name: "1".into(), nd: "d".into(), ng: "g".into(), ns: "s".into(), model: "J1".into(), params: vec![] },
            Component::Mesfet { name: "1".into(), nd: "d".into(), ng: "g".into(), ns: "s".into(), model: "Z1".into(), params: vec![] },
            Component::VSwitch { name: "1".into(), np: "a".into(), nm: "b".into(), ncp: "c".into(), ncm: "d".into(), model: "SW".into() },
            Component::ISwitch { name: "1".into(), np: "a".into(), nm: "b".into(), vcontrol: "Vs".into(), model: "CSW".into() },
            Component::TLine { name: "1".into(), inp: "i1".into(), inm: "i2".into(), outp: "o1".into(), outm: "o2".into(), z0: 50.0, td: 1e-9 },
            Component::Xspice { name: "1".into(), connections: vec!["[din]".into(), "[dout]".into()], model: "d_buf".into() },
            Component::RawSpice { line: ".probe V(out)".into() },
        ];

        let ir = CircuitIR {
            top: Subcircuit {
                name: "all_components".into(),
                ports: vec![],
                parameters: vec![],
                components: all_components,
                instances: vec![],
                models: vec![],
                raw_spice: vec![],
                includes: vec![],
                libs: vec![],
                osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: vec![],
            model_libraries: vec![],
        };

        let json = serde_json::to_string_pretty(&ir).unwrap();
        let ir2: CircuitIR = serde_json::from_str(&json).unwrap();
        assert_eq!(ir, ir2);
    }

    #[test]
    fn test_all_analysis_variants_roundtrip() {
        let analyses = vec![
            Analysis::Op,
            Analysis::Dc { sweeps: vec![DcSweep { source: "V1".into(), start: 0.0, stop: 5.0, step: 0.01 }] },
            Analysis::Ac { variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 },
            Analysis::Transient { step: 1e-9, stop: 1e-3, start: Some(0.0), max_step: Some(1e-10), uic: true },
            Analysis::Noise { output: "V(out)".into(), reference: "0".into(), source: "V1".into(), variation: "dec".into(), points: 10, start: 1.0, stop: 1e6, points_per_summary: None },
            Analysis::Tf { output: "V(out)".into(), source: "Vin".into() },
            Analysis::Sensitivity { output: "V(out)".into(), ac: Some(AcSweepParams { variation: "dec".into(), points: 10, start: 1.0, stop: 1e6 }) },
            Analysis::PoleZero { node1: "in".into(), node2: "0".into(), node3: "out".into(), node4: "0".into(), tf_type: "vol".into(), pz_type: "pz".into() },
            Analysis::Distortion { variation: "dec".into(), points: 10, start: 1.0, stop: 1e6, f2overf1: Some(0.9) },
            Analysis::Pss { fundamental: 1e6, stabilization: 100e-6, observe_node: "out".into(), points_per_period: 100, harmonics: 10 },
            Analysis::HarmonicBalance { frequencies: vec![1e6, 2e6], harmonics: vec![5, 3] },
            Analysis::SPar { variation: "lin".into(), points: 201, start: 1e6, stop: 10e9 },
            Analysis::Stability { probe: "iprobe".into(), variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 },
            Analysis::TransientNoise { step: 1e-9, stop: 1e-6 },
            Analysis::Fourier { fundamental: 1e3, outputs: vec!["V(out)".into()], num_harmonics: Some(10) },
            Analysis::XyceSampling { num_samples: 100, distributions: vec![("R1".into(), "gaussian 1k 100".into())] },
            Analysis::XyceEmbeddedSampling { num_samples: 50, distributions: vec![] },
            Analysis::XycePce { num_samples: 100, distributions: vec![], order: 3 },
            Analysis::XyceFft { signal: "V(out)".into(), np: 1024, start: 0.0, stop: 1e-3, window: "hanning".into(), format: "mag".into() },
            Analysis::SpectreSweep { param: "R1".into(), start: 1e3, stop: 10e3, step: 1e3, inner: "dc1".into(), inner_type: "dc".into() },
            Analysis::SpectreMonteCarlo { iterations: 100, inner: "tran1".into(), inner_type: "tran".into(), seed: Some(42) },
            Analysis::SpectrePac { pss_fundamental: 1e6, pss_stabilization: 100e-6, pss_harmonics: 10, variation: "dec".into(), points: 100, start: 1.0, stop: 1e9, sweep_type: "relative".into() },
            Analysis::SpectrePnoise { pss_fundamental: 1e6, pss_stabilization: 100e-6, pss_harmonics: 10, output: "out".into(), reference: "0".into(), variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 },
            Analysis::SpectrePxf { pss_fundamental: 1e6, pss_stabilization: 100e-6, pss_harmonics: 10, output: "out".into(), source: "V1".into(), variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 },
            Analysis::SpectrePstb { pss_fundamental: 1e6, pss_stabilization: 100e-6, pss_harmonics: 10, probe: "iprobe".into(), variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 },
        ];

        let ir = CircuitIR {
            top: Subcircuit {
                name: "analysis_test".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![],
                instances: vec![],
                models: vec![],
                raw_spice: vec![],
                includes: vec![],
                libs: vec![],
                osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "analysis_test".into(),
                stimulus: vec![],
                analyses,
                options: SimOptions::default(),
                saves: vec![],
                measures: vec![],
                temperature: None,
                nominal_temperature: None,
                initial_conditions: vec![],
                node_sets: vec![],
                step_params: vec![],
                extra_lines: vec![],
            }),
            subcircuit_defs: vec![],
            model_libraries: vec![],
        };

        let json = serde_json::to_string_pretty(&ir).unwrap();
        let ir2: CircuitIR = serde_json::from_str(&json).unwrap();
        assert_eq!(ir, ir2);
    }

    #[test]
    fn test_from_circuit_xspice() {
        let mut c = Circuit::new("xspice_test");
        c.a("1", vec!["in".into(), "out".into()], "d_and");
        c.osdi("my_model.osdi");

        let ir = CircuitIR::from_circuit(&c);

        let features = ir.compute_features();
        assert!(features.has_xspice);
        assert!(features.has_osdi);
        assert_eq!(features.element_count, 1);

        match &ir.top.components[0] {
            Component::Xspice { name, connections, model } => {
                assert_eq!(name, "1");
                assert_eq!(connections, &vec!["in".to_string(), "out".to_string()]);
                assert_eq!(model, "d_and");
            }
            other => panic!("Expected Xspice, got {:?}", other),
        }
    }

    #[test]
    fn test_from_circuit_waveforms() {
        let mut c = Circuit::new("waveform_test");
        c.sinusoidal_voltage_source("1", "in", Node::Ground, 0.0, 0.0, 1.0, 1e6);
        c.pulse_voltage_source("2", "clk", Node::Ground, 0.0, 3.3, 5e-9, 10e-9, 0.1e-9, 0.1e-9);

        let ir = CircuitIR::from_circuit(&c);
        assert_eq!(ir.top.components.len(), 2);

        match &ir.top.components[0] {
            Component::VoltageSource { waveform: Some(IrWaveform::Sin { frequency, amplitude, .. }), .. } => {
                assert_eq!(*frequency, 1e6);
                assert_eq!(*amplitude, 1.0);
            }
            other => panic!("Expected VoltageSource with Sin, got {:?}", other),
        }

        match &ir.top.components[1] {
            Component::VoltageSource { waveform: Some(IrWaveform::Pulse { initial, pulsed, period, .. }), .. } => {
                assert_eq!(*initial, 0.0);
                assert_eq!(*pulsed, 3.3);
                assert_eq!(*period, 10e-9);
            }
            other => panic!("Expected VoltageSource with Pulse, got {:?}", other),
        }
    }

    #[test]
    fn test_from_circuit_controlled_sources() {
        let mut c = Circuit::new("ctrl_test");
        c.e("1", "out_p", "out_m", "in_p", "in_m", 10.0);
        c.g("1", "out_p", "out_m", "in_p", "in_m", 1e-3);
        c.f("1", "out_p", "out_m", "Vsense", 100.0);
        c.h("1", "out_p", "out_m", "Vsense", 1e3);

        let ir = CircuitIR::from_circuit(&c);
        assert_eq!(ir.top.components.len(), 4);

        match &ir.top.components[0] {
            Component::Vcvs { gain, .. } => assert_eq!(*gain, 10.0),
            other => panic!("Expected Vcvs, got {:?}", other),
        }
        match &ir.top.components[1] {
            Component::Vccs { transconductance, .. } => assert_eq!(*transconductance, 1e-3),
            other => panic!("Expected Vccs, got {:?}", other),
        }
        match &ir.top.components[2] {
            Component::Cccs { gain, .. } => assert_eq!(*gain, 100.0),
            other => panic!("Expected Cccs, got {:?}", other),
        }
        match &ir.top.components[3] {
            Component::Ccvs { transresistance, .. } => assert_eq!(*transresistance, 1e3),
            other => panic!("Expected Ccvs, got {:?}", other),
        }
    }

    // ── Issue 02: Analysis::kind_str() ──

    #[test]
    fn test_analysis_kind_str() {
        assert_eq!(Analysis::Op.kind_str(), "op");
        assert_eq!(
            Analysis::Dc { sweeps: vec![] }.kind_str(),
            "dc"
        );
        assert_eq!(
            Analysis::Ac { variation: "dec".into(), points: 10, start: 1.0, stop: 1e6 }.kind_str(),
            "ac"
        );
        assert_eq!(
            Analysis::Transient { step: 1e-9, stop: 1e-6, start: None, max_step: None, uic: false }.kind_str(),
            "tran"
        );
        assert_eq!(
            Analysis::Noise { output: "out".into(), reference: "0".into(), source: "V1".into(), variation: "dec".into(), points: 10, start: 1.0, stop: 1e6, points_per_summary: None }.kind_str(),
            "noise"
        );
        assert_eq!(
            Analysis::Pss { fundamental: 1e6, stabilization: 10e-6, observe_node: "out".into(), points_per_period: 100, harmonics: 10 }.kind_str(),
            "pss"
        );
        assert_eq!(
            Analysis::HarmonicBalance { frequencies: vec![1e6], harmonics: vec![5] }.kind_str(),
            "hb"
        );
        assert_eq!(
            Analysis::Stability { probe: "p".into(), variation: "dec".into(), points: 10, start: 1.0, stop: 1e6 }.kind_str(),
            "stb"
        );
        assert_eq!(
            Analysis::TransientNoise { step: 1e-9, stop: 1e-6 }.kind_str(),
            "trannoise"
        );
        assert_eq!(
            Analysis::XyceSampling { num_samples: 100, distributions: vec![] }.kind_str(),
            "sampling"
        );
        assert_eq!(
            Analysis::SpectrePac { pss_fundamental: 1e6, pss_stabilization: 10e-6, pss_harmonics: 10, variation: "dec".into(), points: 10, start: 1.0, stop: 1e6, sweep_type: "relative".into() }.kind_str(),
            "pac"
        );
    }

    // ── Issue 02: FeatureFlags → CircuitFeatures ──

    #[test]
    fn test_feature_flags_to_circuit_features() {
        let flags = FeatureFlags {
            has_xspice: true,
            has_osdi: false,
            has_measures: true,
            has_step_params: false,
            has_control_blocks: false,
            has_laplace_sources: true,
            has_verilog_cosim: false,
            element_count: 42,
        };
        let cf: crate::backend::CircuitFeatures = (&flags).into();
        assert!(cf.has_xspice);
        assert!(!cf.has_osdi);
        assert!(cf.has_measures);
        assert!(!cf.has_step_params);
        assert!(cf.has_laplace_sources);
        assert_eq!(cf.element_count, 42);
    }
}
