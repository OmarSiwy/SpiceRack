use std::fmt;

use crate::unit::UnitValue;

/// A node in the circuit (net name or ground)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Node {
    Ground,
    Named(String),
}

impl Node {
    pub fn named(s: impl Into<String>) -> Self {
        Self::Named(s.into())
    }

    pub fn spice_name(&self) -> &str {
        match self {
            Self::Ground => "0",
            Self::Named(n) => n,
        }
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.spice_name())
    }
}

impl From<&str> for Node {
    fn from(s: &str) -> Self {
        if s == "0" || s.eq_ignore_ascii_case("gnd") {
            Self::Ground
        } else {
            Self::Named(s.to_string())
        }
    }
}

impl From<Node> for String {
    fn from(n: Node) -> Self {
        n.spice_name().to_string()
    }
}

/// Value for a component: numeric, expression, or raw SPICE string
#[derive(Debug, Clone)]
pub enum ComponentValue {
    Numeric(f64),
    Unit(UnitValue),
    Expression(String),
    Raw(String),
}

impl ComponentValue {
    pub fn to_spice(&self) -> String {
        match self {
            Self::Numeric(v) => format_spice_number(*v),
            Self::Unit(uv) => uv.str_spice(),
            Self::Expression(e) => format!("{{{}}}", e),
            Self::Raw(r) => r.clone(),
        }
    }
}

/// Format f64 in SPICE notation (use SI prefix when clean)
pub fn format_spice_number(v: f64) -> String {
    use crate::unit::SiPrefix;
    let prefix = SiPrefix::best_for(v);
    let mantissa = v / prefix.multiplier();
    let suffix = prefix.spice_suffix();
    // Round to avoid floating point artifacts (10e-6/1e-6 = 10.000000000000002)
    let rounded = mantissa.round();
    if (mantissa - rounded).abs() < 1e-9 && rounded.abs() < 1e15 {
        format!("{}{}", rounded as i64, suffix)
    } else {
        format!("{}{}", mantissa, suffix)
    }
}

impl From<f64> for ComponentValue {
    fn from(v: f64) -> Self {
        Self::Numeric(v)
    }
}

impl From<UnitValue> for ComponentValue {
    fn from(uv: UnitValue) -> Self {
        Self::Unit(uv)
    }
}

/// Named parameter on an element (e.g. model params, W/L for MOSFETs)
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub value: String,
}

impl Param {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name, self.value)
    }
}

// ── Element types ──

/// All SPICE element types supported
#[derive(Debug, Clone)]
pub enum Element {
    R(Resistor),
    C(Capacitor),
    L(Inductor),
    K(MutualInductor),
    V(VoltageSource),
    I(CurrentSource),
    BV(BehavioralVoltage),
    BI(BehavioralCurrent),
    E(Vcvs),
    G(Vccs),
    F(Cccs),
    H(Ccvs),
    D(Diode),
    Q(Bjt),
    M(Mosfet),
    J(Jfet),
    Z(Mesfet),
    S(VSwitch),
    W(ISwitch),
    T(TLine),
    X(SubcircuitInstance),
    /// XSPICE code model instance (A-element)
    A(XspiceInstance),
    /// Raw SPICE line pass-through
    RawSpice(String),
}

impl Element {
    pub fn name(&self) -> &str {
        match self {
            Self::R(e) => &e.name,
            Self::C(e) => &e.name,
            Self::L(e) => &e.name,
            Self::K(e) => &e.name,
            Self::V(e) => &e.name,
            Self::I(e) => &e.name,
            Self::BV(e) => &e.name,
            Self::BI(e) => &e.name,
            Self::E(e) => &e.name,
            Self::G(e) => &e.name,
            Self::F(e) => &e.name,
            Self::H(e) => &e.name,
            Self::D(e) => &e.name,
            Self::Q(e) => &e.name,
            Self::M(e) => &e.name,
            Self::J(e) => &e.name,
            Self::Z(e) => &e.name,
            Self::S(e) => &e.name,
            Self::W(e) => &e.name,
            Self::T(e) => &e.name,
            Self::X(e) => &e.name,
            Self::A(e) => &e.name,
            Self::RawSpice(_) => "",
        }
    }

    /// Full SPICE element name with prefix (e.g. "R1", "M1")
    pub fn spice_name(&self) -> String {
        match self {
            Self::R(e) => format!("R{}", e.name),
            Self::C(e) => format!("C{}", e.name),
            Self::L(e) => format!("L{}", e.name),
            Self::K(e) => format!("K{}", e.name),
            Self::V(e) => format!("V{}", e.name),
            Self::I(e) => format!("I{}", e.name),
            Self::BV(e) => format!("B{}", e.name),
            Self::BI(e) => format!("B{}", e.name),
            Self::E(e) => format!("E{}", e.name),
            Self::G(e) => format!("G{}", e.name),
            Self::F(e) => format!("F{}", e.name),
            Self::H(e) => format!("H{}", e.name),
            Self::D(e) => format!("D{}", e.name),
            Self::Q(e) => format!("Q{}", e.name),
            Self::M(e) => format!("M{}", e.name),
            Self::J(e) => format!("J{}", e.name),
            Self::Z(e) => format!("Z{}", e.name),
            Self::S(e) => format!("S{}", e.name),
            Self::W(e) => format!("W{}", e.name),
            Self::T(e) => format!("T{}", e.name),
            Self::X(e) => format!("X{}", e.name),
            Self::A(e) => format!("A{}", e.name),
            Self::RawSpice(_) => String::new(),
        }
    }
}

impl fmt::Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::R(e) => write!(f, "{}", e),
            Self::C(e) => write!(f, "{}", e),
            Self::L(e) => write!(f, "{}", e),
            Self::K(e) => write!(f, "{}", e),
            Self::V(e) => write!(f, "{}", e),
            Self::I(e) => write!(f, "{}", e),
            Self::BV(e) => write!(f, "{}", e),
            Self::BI(e) => write!(f, "{}", e),
            Self::E(e) => write!(f, "{}", e),
            Self::G(e) => write!(f, "{}", e),
            Self::F(e) => write!(f, "{}", e),
            Self::H(e) => write!(f, "{}", e),
            Self::D(e) => write!(f, "{}", e),
            Self::Q(e) => write!(f, "{}", e),
            Self::M(e) => write!(f, "{}", e),
            Self::J(e) => write!(f, "{}", e),
            Self::Z(e) => write!(f, "{}", e),
            Self::S(e) => write!(f, "{}", e),
            Self::W(e) => write!(f, "{}", e),
            Self::T(e) => write!(f, "{}", e),
            Self::X(e) => write!(f, "{}", e),
            Self::A(e) => write!(f, "{}", e),
            Self::RawSpice(s) => write!(f, "{}", s),
        }
    }
}

// ── Individual element structs ──

#[derive(Debug, Clone)]
pub struct Resistor {
    pub name: String,
    pub n1: Node,
    pub n2: Node,
    pub value: ComponentValue,
    pub params: Vec<Param>,
}

impl fmt::Display for Resistor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "R{} {} {} {}", self.name, self.n1, self.n2, self.value.to_spice())?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Capacitor {
    pub name: String,
    pub n1: Node,
    pub n2: Node,
    pub value: ComponentValue,
    pub params: Vec<Param>,
}

impl fmt::Display for Capacitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{} {} {} {}", self.name, self.n1, self.n2, self.value.to_spice())?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Inductor {
    pub name: String,
    pub n1: Node,
    pub n2: Node,
    pub value: ComponentValue,
    pub params: Vec<Param>,
}

impl fmt::Display for Inductor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "L{} {} {} {}", self.name, self.n1, self.n2, self.value.to_spice())?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MutualInductor {
    pub name: String,
    pub inductor1: String,
    pub inductor2: String,
    pub coupling: f64,
}

impl fmt::Display for MutualInductor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "K{} L{} L{} {}", self.name, self.inductor1, self.inductor2, self.coupling)
    }
}

#[derive(Debug, Clone)]
pub struct VoltageSource {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub value: ComponentValue,
    pub waveform: Option<Waveform>,
}

impl fmt::Display for VoltageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "V{} {} {} {}", self.name, self.np, self.nm, self.value.to_spice())?;
        if let Some(ref wf) = self.waveform {
            write!(f, " {}", wf)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CurrentSource {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub value: ComponentValue,
    pub waveform: Option<Waveform>,
}

impl fmt::Display for CurrentSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "I{} {} {} {}", self.name, self.np, self.nm, self.value.to_spice())?;
        if let Some(ref wf) = self.waveform {
            write!(f, " {}", wf)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BehavioralVoltage {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub expression: String,
}

impl fmt::Display for BehavioralVoltage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B{} {} {} V={}", self.name, self.np, self.nm, self.expression)
    }
}

#[derive(Debug, Clone)]
pub struct BehavioralCurrent {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub expression: String,
}

impl fmt::Display for BehavioralCurrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B{} {} {} I={}", self.name, self.np, self.nm, self.expression)
    }
}

// VCVS: Exx np nm ncp ncm gain
#[derive(Debug, Clone)]
pub struct Vcvs {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub ncp: Node,
    pub ncm: Node,
    pub gain: f64,
}

impl fmt::Display for Vcvs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{} {} {} {} {} {}", self.name, self.np, self.nm, self.ncp, self.ncm, self.gain)
    }
}

// VCCS: Gxx np nm ncp ncm transconductance
#[derive(Debug, Clone)]
pub struct Vccs {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub ncp: Node,
    pub ncm: Node,
    pub transconductance: f64,
}

impl fmt::Display for Vccs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "G{} {} {} {} {} {}",
            self.name, self.np, self.nm, self.ncp, self.ncm, self.transconductance
        )
    }
}

// CCCS: Fxx np nm Vsense gain
#[derive(Debug, Clone)]
pub struct Cccs {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub vsense: String,
    pub gain: f64,
}

impl fmt::Display for Cccs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "F{} {} {} {} {}", self.name, self.np, self.nm, self.vsense, self.gain)
    }
}

// CCVS: Hxx np nm Vsense transresistance
#[derive(Debug, Clone)]
pub struct Ccvs {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub vsense: String,
    pub transresistance: f64,
}

impl fmt::Display for Ccvs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "H{} {} {} {} {}",
            self.name, self.np, self.nm, self.vsense, self.transresistance
        )
    }
}

#[derive(Debug, Clone)]
pub struct Diode {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub model: String,
    pub params: Vec<Param>,
}

impl fmt::Display for Diode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "D{} {} {} {}", self.name, self.np, self.nm, self.model)?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Bjt {
    pub name: String,
    pub nc: Node,
    pub nb: Node,
    pub ne: Node,
    pub model: String,
    pub params: Vec<Param>,
}

impl fmt::Display for Bjt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Q{} {} {} {} {}", self.name, self.nc, self.nb, self.ne, self.model)?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Mosfet {
    pub name: String,
    pub nd: Node,
    pub ng: Node,
    pub ns: Node,
    pub nb: Node,
    pub model: String,
    pub params: Vec<Param>,
}

impl fmt::Display for Mosfet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "M{} {} {} {} {} {}",
            self.name, self.nd, self.ng, self.ns, self.nb, self.model
        )?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Jfet {
    pub name: String,
    pub nd: Node,
    pub ng: Node,
    pub ns: Node,
    pub model: String,
    pub params: Vec<Param>,
}

impl fmt::Display for Jfet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "J{} {} {} {} {}", self.name, self.nd, self.ng, self.ns, self.model)?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Mesfet {
    pub name: String,
    pub nd: Node,
    pub ng: Node,
    pub ns: Node,
    pub model: String,
    pub params: Vec<Param>,
}

impl fmt::Display for Mesfet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Z{} {} {} {} {}", self.name, self.nd, self.ng, self.ns, self.model)?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

// Voltage-controlled switch: Sxx np nm ncp ncm model
#[derive(Debug, Clone)]
pub struct VSwitch {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub ncp: Node,
    pub ncm: Node,
    pub model: String,
}

impl fmt::Display for VSwitch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "S{} {} {} {} {} {}",
            self.name, self.np, self.nm, self.ncp, self.ncm, self.model
        )
    }
}

// Current-controlled switch: Wxx np nm Vcontrol model
#[derive(Debug, Clone)]
pub struct ISwitch {
    pub name: String,
    pub np: Node,
    pub nm: Node,
    pub vcontrol: String,
    pub model: String,
}

impl fmt::Display for ISwitch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "W{} {} {} {} {}", self.name, self.np, self.nm, self.vcontrol, self.model)
    }
}

// Lossless transmission line: Txx inp inm outp outm Z0=val TD=val
#[derive(Debug, Clone)]
pub struct TLine {
    pub name: String,
    pub inp: Node,
    pub inm: Node,
    pub outp: Node,
    pub outm: Node,
    pub z0: f64,
    pub td: f64,
}

impl fmt::Display for TLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "T{} {} {} {} {} Z0={} TD={}",
            self.name, self.inp, self.inm, self.outp, self.outm, self.z0, self.td
        )
    }
}

// Subcircuit instance: Xxx n1 n2 ... subcircuit_name [params]
#[derive(Debug, Clone)]
pub struct SubcircuitInstance {
    pub name: String,
    pub subcircuit_name: String,
    pub nodes: Vec<Node>,
    pub params: Vec<Param>,
}

impl fmt::Display for SubcircuitInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "X{}", self.name)?;
        for n in &self.nodes {
            write!(f, " {}", n)?;
        }
        write!(f, " {}", self.subcircuit_name)?;
        for p in &self.params {
            write!(f, " {}", p)?;
        }
        Ok(())
    }
}

// ── XSPICE instance (A-element) ──

/// XSPICE code model instance.
///
/// Syntax: `A<name> <connections> <model>`
///
/// Connections are port groups — each is either a digital bus `[n1 n2 ...]`,
/// a single analog node, or a special port like `%v(node)`, `%vd(n1,n2)`,
/// `$d_hi`, `$d_lo`.
#[derive(Debug, Clone)]
pub struct XspiceInstance {
    pub name: String,
    pub connections: Vec<String>,
    pub model: String,
}

impl fmt::Display for XspiceInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "A{}", self.name)?;
        for conn in &self.connections {
            write!(f, " {}", conn)?;
        }
        write!(f, " {}", self.model)
    }
}

// ── Waveform types (high-level sources) ──

#[derive(Debug, Clone)]
pub enum Waveform {
    Sin(SinWaveform),
    Pulse(PulseWaveform),
    Pwl(PwlWaveform),
    Exp(ExpWaveform),
    Sffm(SffmWaveform),
    Am(AmWaveform),
}

impl fmt::Display for Waveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sin(w) => write!(f, "{}", w),
            Self::Pulse(w) => write!(f, "{}", w),
            Self::Pwl(w) => write!(f, "{}", w),
            Self::Exp(w) => write!(f, "{}", w),
            Self::Sffm(w) => write!(f, "{}", w),
            Self::Am(w) => write!(f, "{}", w),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SinWaveform {
    pub offset: f64,
    pub amplitude: f64,
    pub frequency: f64,
    pub delay: f64,
    pub damping: f64,
    pub phase: f64,
}

impl fmt::Display for SinWaveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SIN({} {} {} {} {} {})",
            self.offset, self.amplitude, self.frequency, self.delay, self.damping, self.phase
        )
    }
}

#[derive(Debug, Clone)]
pub struct PulseWaveform {
    pub initial: f64,
    pub pulsed: f64,
    pub delay: f64,
    pub rise_time: f64,
    pub fall_time: f64,
    pub pulse_width: f64,
    pub period: f64,
}

impl fmt::Display for PulseWaveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PULSE({} {} {} {} {} {} {})",
            self.initial,
            self.pulsed,
            self.delay,
            self.rise_time,
            self.fall_time,
            self.pulse_width,
            self.period
        )
    }
}

#[derive(Debug, Clone)]
pub struct PwlWaveform {
    pub values: Vec<(f64, f64)>,
}

impl fmt::Display for PwlWaveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PWL(")?;
        for (i, (t, v)) in self.values.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }
            write!(f, "{} {}", t, v)?;
        }
        write!(f, ")")
    }
}

#[derive(Debug, Clone)]
pub struct ExpWaveform {
    pub initial: f64,
    pub pulsed: f64,
    pub rise_delay: f64,
    pub rise_tau: f64,
    pub fall_delay: f64,
    pub fall_tau: f64,
}

impl fmt::Display for ExpWaveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EXP({} {} {} {} {} {})",
            self.initial, self.pulsed, self.rise_delay, self.rise_tau, self.fall_delay, self.fall_tau
        )
    }
}

#[derive(Debug, Clone)]
pub struct SffmWaveform {
    pub offset: f64,
    pub amplitude: f64,
    pub carrier_freq: f64,
    pub modulation_index: f64,
    pub signal_freq: f64,
}

impl fmt::Display for SffmWaveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SFFM({} {} {} {} {})",
            self.offset, self.amplitude, self.carrier_freq, self.modulation_index, self.signal_freq
        )
    }
}

#[derive(Debug, Clone)]
pub struct AmWaveform {
    pub amplitude: f64,
    pub offset: f64,
    pub modulating_freq: f64,
    pub carrier_freq: f64,
    pub delay: f64,
}

impl fmt::Display for AmWaveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AM({} {} {} {} {})",
            self.amplitude, self.offset, self.modulating_freq, self.carrier_freq, self.delay
        )
    }
}

// ── Model definition ──

#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub kind: String,
    pub params: Vec<Param>,
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ".model {} {}", self.name, self.kind)?;
        if !self.params.is_empty() {
            write!(f, "(")?;
            for (i, p) in self.params.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{}", p)?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

// ── SubCircuit definition ──

#[derive(Debug, Clone)]
pub struct SubCircuitDef {
    pub name: String,
    pub pins: Vec<String>,
    pub elements: Vec<Element>,
    pub models: Vec<Model>,
    pub params: Vec<Param>,
}

impl fmt::Display for SubCircuitDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ".subckt {}", self.name)?;
        for pin in &self.pins {
            write!(f, " {}", pin)?;
        }
        if !self.params.is_empty() {
            write!(f, " PARAMS:")?;
            for p in &self.params {
                write!(f, " {}", p)?;
            }
        }
        writeln!(f)?;
        for model in &self.models {
            writeln!(f, "{}", model)?;
        }
        for elem in &self.elements {
            writeln!(f, "{}", elem)?;
        }
        write!(f, ".ends {}", self.name)
    }
}

// ── Circuit ──

#[derive(Debug, Clone)]
pub struct Circuit {
    pub title: String,
    elements: Vec<Element>,
    models: Vec<Model>,
    subcircuits: Vec<SubCircuitDef>,
    includes: Vec<String>,
    libs: Vec<(String, String)>,
    parameters: Vec<Param>,
    options: Vec<(String, String)>,
    temperature: Option<f64>,
    raw_lines: Vec<String>,
    /// OSDI model paths loaded via `pre_osdi` (ngspice), `load` (vacask),
    /// or `ahdl_include` (spectre).
    osdi_loads: Vec<String>,
}

impl Circuit {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            elements: Vec::new(),
            models: Vec::new(),
            subcircuits: Vec::new(),
            includes: Vec::new(),
            libs: Vec::new(),
            parameters: Vec::new(),
            options: Vec::new(),
            temperature: None,
            raw_lines: Vec::new(),
            osdi_loads: Vec::new(),
        }
    }

    /// Ground node reference
    pub fn gnd(&self) -> Node {
        Node::Ground
    }

    // ── Component addition methods ──

    pub fn r(
        &mut self,
        name: impl Into<String>,
        n1: impl Into<Node>,
        n2: impl Into<Node>,
        value: impl Into<ComponentValue>,
    ) -> &Element {
        self.elements.push(Element::R(Resistor {
            name: name.into(),
            n1: n1.into(),
            n2: n2.into(),
            value: value.into(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    pub fn r_with_params(
        &mut self,
        name: impl Into<String>,
        n1: impl Into<Node>,
        n2: impl Into<Node>,
        value: impl Into<ComponentValue>,
        params: Vec<Param>,
    ) -> &Element {
        self.elements.push(Element::R(Resistor {
            name: name.into(),
            n1: n1.into(),
            n2: n2.into(),
            value: value.into(),
            params,
        }));
        self.elements.last().unwrap()
    }

    pub fn r_raw(
        &mut self,
        name: impl Into<String>,
        n1: impl Into<Node>,
        n2: impl Into<Node>,
        raw_spice: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::R(Resistor {
            name: name.into(),
            n1: n1.into(),
            n2: n2.into(),
            value: ComponentValue::Raw(raw_spice.into()),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    pub fn c(
        &mut self,
        name: impl Into<String>,
        n1: impl Into<Node>,
        n2: impl Into<Node>,
        value: impl Into<ComponentValue>,
    ) -> &Element {
        self.elements.push(Element::C(Capacitor {
            name: name.into(),
            n1: n1.into(),
            n2: n2.into(),
            value: value.into(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    pub fn l(
        &mut self,
        name: impl Into<String>,
        n1: impl Into<Node>,
        n2: impl Into<Node>,
        value: impl Into<ComponentValue>,
    ) -> &Element {
        self.elements.push(Element::L(Inductor {
            name: name.into(),
            n1: n1.into(),
            n2: n2.into(),
            value: value.into(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    pub fn k(
        &mut self,
        name: impl Into<String>,
        inductor1: impl Into<String>,
        inductor2: impl Into<String>,
        coupling: f64,
    ) -> &Element {
        self.elements.push(Element::K(MutualInductor {
            name: name.into(),
            inductor1: inductor1.into(),
            inductor2: inductor2.into(),
            coupling,
        }));
        self.elements.last().unwrap()
    }

    pub fn v(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        value: impl Into<ComponentValue>,
    ) -> &Element {
        self.elements.push(Element::V(VoltageSource {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            value: value.into(),
            waveform: None,
        }));
        self.elements.last().unwrap()
    }

    pub fn v_with_waveform(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        value: impl Into<ComponentValue>,
        waveform: Waveform,
    ) -> &Element {
        self.elements.push(Element::V(VoltageSource {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            value: value.into(),
            waveform: Some(waveform),
        }));
        self.elements.last().unwrap()
    }

    pub fn i(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        value: impl Into<ComponentValue>,
    ) -> &Element {
        self.elements.push(Element::I(CurrentSource {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            value: value.into(),
            waveform: None,
        }));
        self.elements.last().unwrap()
    }

    pub fn bv(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        expression: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::BV(BehavioralVoltage {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            expression: expression.into(),
        }));
        self.elements.last().unwrap()
    }

    pub fn bi(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        expression: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::BI(BehavioralCurrent {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            expression: expression.into(),
        }));
        self.elements.last().unwrap()
    }

    pub fn e(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        ncp: impl Into<Node>,
        ncm: impl Into<Node>,
        voltage_gain: f64,
    ) -> &Element {
        self.elements.push(Element::E(Vcvs {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            ncp: ncp.into(),
            ncm: ncm.into(),
            gain: voltage_gain,
        }));
        self.elements.last().unwrap()
    }

    pub fn g(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        ncp: impl Into<Node>,
        ncm: impl Into<Node>,
        transconductance: f64,
    ) -> &Element {
        self.elements.push(Element::G(Vccs {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            ncp: ncp.into(),
            ncm: ncm.into(),
            transconductance,
        }));
        self.elements.last().unwrap()
    }

    pub fn f(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        vsense: impl Into<String>,
        current_gain: f64,
    ) -> &Element {
        self.elements.push(Element::F(Cccs {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            vsense: vsense.into(),
            gain: current_gain,
        }));
        self.elements.last().unwrap()
    }

    pub fn h(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        vsense: impl Into<String>,
        transresistance: f64,
    ) -> &Element {
        self.elements.push(Element::H(Ccvs {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            vsense: vsense.into(),
            transresistance,
        }));
        self.elements.last().unwrap()
    }

    pub fn d(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        model: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::D(Diode {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            model: model.into(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    pub fn q(
        &mut self,
        name: impl Into<String>,
        nc: impl Into<Node>,
        nb: impl Into<Node>,
        ne: impl Into<Node>,
        model: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::Q(Bjt {
            name: name.into(),
            nc: nc.into(),
            nb: nb.into(),
            ne: ne.into(),
            model: model.into(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    /// Alias for q()
    pub fn bjt(
        &mut self,
        name: impl Into<String>,
        nc: impl Into<Node>,
        nb: impl Into<Node>,
        ne: impl Into<Node>,
        model: impl Into<String>,
    ) -> &Element {
        self.q(name, nc, nb, ne, model)
    }

    pub fn m(
        &mut self,
        name: impl Into<String>,
        nd: impl Into<Node>,
        ng: impl Into<Node>,
        ns: impl Into<Node>,
        nb: impl Into<Node>,
        model: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::M(Mosfet {
            name: name.into(),
            nd: nd.into(),
            ng: ng.into(),
            ns: ns.into(),
            nb: nb.into(),
            model: model.into(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    pub fn m_with_params(
        &mut self,
        name: impl Into<String>,
        nd: impl Into<Node>,
        ng: impl Into<Node>,
        ns: impl Into<Node>,
        nb: impl Into<Node>,
        model: impl Into<String>,
        params: Vec<Param>,
    ) -> &Element {
        self.elements.push(Element::M(Mosfet {
            name: name.into(),
            nd: nd.into(),
            ng: ng.into(),
            ns: ns.into(),
            nb: nb.into(),
            model: model.into(),
            params,
        }));
        self.elements.last().unwrap()
    }

    /// Alias for m()
    pub fn mosfet(
        &mut self,
        name: impl Into<String>,
        nd: impl Into<Node>,
        ng: impl Into<Node>,
        ns: impl Into<Node>,
        nb: impl Into<Node>,
        model: impl Into<String>,
    ) -> &Element {
        self.m(name, nd, ng, ns, nb, model)
    }

    pub fn j(
        &mut self,
        name: impl Into<String>,
        nd: impl Into<Node>,
        ng: impl Into<Node>,
        ns: impl Into<Node>,
        model: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::J(Jfet {
            name: name.into(),
            nd: nd.into(),
            ng: ng.into(),
            ns: ns.into(),
            model: model.into(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    pub fn z(
        &mut self,
        name: impl Into<String>,
        nd: impl Into<Node>,
        ng: impl Into<Node>,
        ns: impl Into<Node>,
        model: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::Z(Mesfet {
            name: name.into(),
            nd: nd.into(),
            ng: ng.into(),
            ns: ns.into(),
            model: model.into(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    pub fn s(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        ncp: impl Into<Node>,
        ncm: impl Into<Node>,
        model: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::S(VSwitch {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            ncp: ncp.into(),
            ncm: ncm.into(),
            model: model.into(),
        }));
        self.elements.last().unwrap()
    }

    pub fn w(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        vcontrol: impl Into<String>,
        model: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::W(ISwitch {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            vcontrol: vcontrol.into(),
            model: model.into(),
        }));
        self.elements.last().unwrap()
    }

    pub fn t(
        &mut self,
        name: impl Into<String>,
        inp: impl Into<Node>,
        inm: impl Into<Node>,
        outp: impl Into<Node>,
        outm: impl Into<Node>,
        z0: f64,
        td: f64,
    ) -> &Element {
        self.elements.push(Element::T(TLine {
            name: name.into(),
            inp: inp.into(),
            inm: inm.into(),
            outp: outp.into(),
            outm: outm.into(),
            z0,
            td,
        }));
        self.elements.last().unwrap()
    }

    pub fn x(
        &mut self,
        name: impl Into<String>,
        subcircuit_name: impl Into<String>,
        nodes: Vec<impl Into<Node>>,
    ) -> &Element {
        self.elements.push(Element::X(SubcircuitInstance {
            name: name.into(),
            subcircuit_name: subcircuit_name.into(),
            nodes: nodes.into_iter().map(|n| n.into()).collect(),
            params: Vec::new(),
        }));
        self.elements.last().unwrap()
    }

    /// XSPICE code model instance (A-element).
    ///
    /// `connections` are port groups emitted space-separated. Use `[node]` for
    /// digital ports, plain names for analog, `$d_hi`/`$d_lo` for constants.
    pub fn a(
        &mut self,
        name: impl Into<String>,
        connections: Vec<String>,
        model: impl Into<String>,
    ) -> &Element {
        self.elements.push(Element::A(XspiceInstance {
            name: name.into(),
            connections,
            model: model.into(),
        }));
        self.elements.last().unwrap()
    }

    // ── High-level waveform source helpers ──

    pub fn sinusoidal_voltage_source(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        dc_offset: f64,
        offset: f64,
        amplitude: f64,
        frequency: f64,
    ) -> &Element {
        self.elements.push(Element::V(VoltageSource {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            value: ComponentValue::Numeric(dc_offset),
            waveform: Some(Waveform::Sin(SinWaveform {
                offset,
                amplitude,
                frequency,
                delay: 0.0,
                damping: 0.0,
                phase: 0.0,
            })),
        }));
        self.elements.last().unwrap()
    }

    pub fn pulse_voltage_source(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        initial_value: f64,
        pulsed_value: f64,
        pulse_width: f64,
        period: f64,
        rise_time: f64,
        fall_time: f64,
    ) -> &Element {
        self.elements.push(Element::V(VoltageSource {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            value: ComponentValue::Numeric(0.0),
            waveform: Some(Waveform::Pulse(PulseWaveform {
                initial: initial_value,
                pulsed: pulsed_value,
                delay: 0.0,
                rise_time,
                fall_time,
                pulse_width,
                period,
            })),
        }));
        self.elements.last().unwrap()
    }

    pub fn pwl_voltage_source(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        values: Vec<(f64, f64)>,
    ) -> &Element {
        self.elements.push(Element::V(VoltageSource {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            value: ComponentValue::Numeric(0.0),
            waveform: Some(Waveform::Pwl(PwlWaveform { values })),
        }));
        self.elements.last().unwrap()
    }

    pub fn sinusoidal_current_source(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        dc_offset: f64,
        offset: f64,
        amplitude: f64,
        frequency: f64,
    ) -> &Element {
        self.elements.push(Element::I(CurrentSource {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            value: ComponentValue::Numeric(dc_offset),
            waveform: Some(Waveform::Sin(SinWaveform {
                offset,
                amplitude,
                frequency,
                delay: 0.0,
                damping: 0.0,
                phase: 0.0,
            })),
        }));
        self.elements.last().unwrap()
    }

    pub fn pulse_current_source(
        &mut self,
        name: impl Into<String>,
        np: impl Into<Node>,
        nm: impl Into<Node>,
        initial_value: f64,
        pulsed_value: f64,
        pulse_width: f64,
        period: f64,
        rise_time: f64,
        fall_time: f64,
    ) -> &Element {
        self.elements.push(Element::I(CurrentSource {
            name: name.into(),
            np: np.into(),
            nm: nm.into(),
            value: ComponentValue::Numeric(0.0),
            waveform: Some(Waveform::Pulse(PulseWaveform {
                initial: initial_value,
                pulsed: pulsed_value,
                delay: 0.0,
                rise_time,
                fall_time,
                pulse_width,
                period,
            })),
        }));
        self.elements.last().unwrap()
    }

    // ── Circuit-level directives ──

    pub fn model(
        &mut self,
        name: impl Into<String>,
        kind: impl Into<String>,
        params: Vec<Param>,
    ) {
        self.models.push(Model {
            name: name.into(),
            kind: kind.into(),
            params,
        });
    }

    pub fn subcircuit(&mut self, subckt: SubCircuitDef) {
        self.subcircuits.push(subckt);
    }

    pub fn include(&mut self, path: impl Into<String>) {
        self.includes.push(path.into());
    }

    pub fn lib(&mut self, path: impl Into<String>, section: impl Into<String>) {
        self.libs.push((path.into(), section.into()));
    }

    pub fn parameter(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.parameters.push(Param::new(name, value));
    }

    pub fn options(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.options.push((key.into(), value.into()));
    }

    pub fn temp(&mut self, temperature: f64) {
        self.temperature = Some(temperature);
    }

    pub fn raw_spice(&mut self, line: impl Into<String>) {
        self.raw_lines.push(line.into());
    }

    /// Load a compiled Verilog-A model (OSDI binary).
    ///
    /// Emitted as `pre_osdi` (ngspice), `load` (vacask), or
    /// `ahdl_include` (spectre). The path should point to a `.osdi`
    /// file compiled by OpenVAF, or a `.va` source for Spectre.
    pub fn osdi(&mut self, path: impl Into<String>) {
        self.osdi_loads.push(path.into());
    }

    pub fn osdi_loads(&self) -> &[String] {
        &self.osdi_loads
    }

    // ── Element lookup ──

    pub fn element(&self, name: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.name() == name)
    }

    pub fn element_by_spice_name(&self, spice_name: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.spice_name() == spice_name)
    }

    pub fn element_mut(&mut self, name: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.name() == name)
    }

    pub fn set_source_value(&mut self, name: &str, value: f64) -> Result<(), String> {
        match self.element_mut(name) {
            Some(Element::V(vs)) => { vs.value = ComponentValue::Numeric(value); Ok(()) }
            Some(Element::I(is)) => { is.value = ComponentValue::Numeric(value); Ok(()) }
            Some(_) => Err(format!("{} is not a voltage or current source", name)),
            None => Err(format!("element '{}' not found", name)),
        }
    }

    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    pub fn subcircuits(&self) -> &[SubCircuitDef] {
        &self.subcircuits
    }

    pub fn includes(&self) -> &[String] {
        &self.includes
    }

    pub fn libs(&self) -> &[(String, String)] {
        &self.libs
    }

    pub fn parameters(&self) -> &[Param] {
        &self.parameters
    }

    pub fn raw_lines(&self) -> &[String] {
        &self.raw_lines
    }

    /// Returns true if the circuit uses XSPICE A-elements.
    pub fn has_xspice(&self) -> bool {
        self.elements.iter().any(|e| matches!(e, Element::A(_)))
    }

    /// Returns true if the circuit loads OSDI (Verilog-A compiled) models.
    pub fn has_osdi(&self) -> bool {
        !self.osdi_loads.is_empty()
    }

    /// Returns true if the circuit contains `.control` blocks in raw lines.
    pub fn has_control_blocks(&self) -> bool {
        self.raw_lines.iter().any(|l| {
            let trimmed = l.trim().to_lowercase();
            trimmed == ".control" || trimmed.starts_with(".control ")
        })
    }

    /// Returns true if the circuit uses Verilog co-simulation (d_cosim models).
    pub fn has_verilog_cosim(&self) -> bool {
        self.models.iter().any(|m| m.kind.contains("d_cosim"))
            || self.elements.iter().any(|e| {
                if let Element::A(a) = e {
                    a.model.contains("cosim_")
                } else {
                    false
                }
            })
    }

    /// Returns true if any B-source uses `Laplace(` in its expression.
    pub fn has_laplace_sources(&self) -> bool {
        self.elements.iter().any(|e| match e {
            Element::BV(bv) => bv.expression.contains("Laplace("),
            Element::BI(bi) => bi.expression.contains("Laplace("),
            _ => false,
        })
    }
}

// ── SPICE netlist emission ──

impl fmt::Display for Circuit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // OSDI loads must appear before .title in ngspice
        for path in &self.osdi_loads {
            writeln!(f, "pre_osdi {}", path)?;
        }

        // Title line
        writeln!(f, ".title {}", self.title)?;

        // Includes
        for inc in &self.includes {
            writeln!(f, ".include \"{}\"", inc)?;
        }

        // Libraries
        for (path, section) in &self.libs {
            writeln!(f, ".lib \"{}\" {}", path, section)?;
        }

        // Options
        if !self.options.is_empty() {
            let opts: Vec<String> = self.options.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
            writeln!(f, ".options {}", opts.join(" "))?;
        }

        // Temperature
        if let Some(temp) = self.temperature {
            writeln!(f, ".temp {}", temp)?;
        }

        // Parameters
        for p in &self.parameters {
            writeln!(f, ".param {}", p)?;
        }

        // Subcircuit definitions
        for subckt in &self.subcircuits {
            writeln!(f)?;
            writeln!(f, "{}", subckt)?;
        }

        // Models
        for model in &self.models {
            writeln!(f, "{}", model)?;
        }

        // Elements
        for elem in &self.elements {
            writeln!(f, "{}", elem)?;
        }

        // Raw SPICE lines
        for line in &self.raw_lines {
            writeln!(f, "{}", line)?;
        }

        write!(f, ".end")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_circuit() {
        let mut c = Circuit::new("test");
        c.r("1", "in", "out", 1000.0);
        c.c("1", "out", Node::Ground, 10e-12);
        c.v("dd", "vdd", Node::Ground, 3.3);

        let netlist = c.to_string();
        assert!(netlist.contains(".title test"));
        assert!(netlist.contains("R1 in out 1k"));
        assert!(netlist.contains("C1 out 0 10p"));
        assert!(netlist.contains("V"));
        assert!(netlist.contains(".end"));
    }

    #[test]
    fn test_mosfet() {
        let mut c = Circuit::new("mos_test");
        c.m("1", "drain", "gate", "source", "bulk", "nmos_3p3");
        c.model(
            "nmos_3p3",
            "NMOS",
            vec![
                Param::new("LEVEL", "1"),
                Param::new("VTO", "0.7"),
                Param::new("KP", "110e-6"),
            ],
        );

        let netlist = c.to_string();
        assert!(netlist.contains("M1 drain gate source bulk nmos_3p3"));
        assert!(netlist.contains(".model nmos_3p3 NMOS(LEVEL=1 VTO=0.7 KP=110e-6)"));
    }

    #[test]
    fn test_controlled_sources() {
        let mut c = Circuit::new("ctrl_test");
        c.e("1", "out_p", "out_m", "in_p", "in_m", 10.0);
        c.g("1", "out_p", "out_m", "in_p", "in_m", 1e-3);
        c.f("1", "out_p", "out_m", "Vsense", 100.0);
        c.h("1", "out_p", "out_m", "Vsense", 1e3);

        let netlist = c.to_string();
        assert!(netlist.contains("E1 out_p out_m in_p in_m 10"));
        assert!(netlist.contains("G1 out_p out_m in_p in_m 0.001"));
        assert!(netlist.contains("F1 out_p out_m Vsense 100"));
        assert!(netlist.contains("H1 out_p out_m Vsense 1000"));
    }

    #[test]
    fn test_subcircuit() {
        let mut c = Circuit::new("sub_test");
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
            params: vec![],
        });
        c.x("1", "MyBuf", vec!["in", "out", "vdd", "gnd"]);

        let netlist = c.to_string();
        assert!(netlist.contains(".subckt MyBuf in out vdd gnd"));
        assert!(netlist.contains("M1 out in vdd vdd pmos"));
        assert!(netlist.contains(".ends MyBuf"));
        assert!(netlist.contains("X1 in out vdd 0 MyBuf"));
    }
}
