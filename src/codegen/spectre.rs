//! Cadence Spectre netlist generation (Spectre native language, `simulator lang=spectre`).
//!
//! # Provenance of every construct in this file
//!
//! Spectre is licence-gated and is **not** installed here, so nothing below was
//! ever executed. Every statement shape is taken from Cadence's own manuals;
//! the citation lives next to the code that emits it. The two primary sources:
//!
//! * `[REF]` — *Spectre Circuit Simulator Reference*, Product Version 19.1,
//!   January 2020. <https://ee.kpi.ua/~yv/edu/ok/book/spectre_refManual.pdf>
//!   (analysis statements, `options`, `save`, `ic`, `nodeset`, `include`,
//!   `subckt`, `parameters`, `global`).
//! * `[CMP]` — *Spectre Circuit Simulator Reference*, Product Version 5.0,
//!   September 2003. <http://eece.cu.edu.eg/~fhussien/Spectre_tutorial.pdf>
//!   (the component chapters: `resistor`, `capacitor`, `inductor`,
//!   `mutual_inductor`, `vsource`, `isource`, `vcvs`, `vccs`, `cccs`, `ccvs`,
//!   `tline`, `diode`, `relay`, `fourier`, `bsource`). 19.1 split these into a
//!   separate manual; the statement forms are unchanged.
//! * `[KUN]` — Kundert, *The Designer's Guide to SPICE and Spectre*,
//!   Appendix B "Spectre Netlist Language".
//!   <https://designers-guide.org/analysis/dg-spice/chB.pdf> (scale factors,
//!   case sensitivity, `simulator lang=` rules).
//!
//! # Deliberate refusals
//!
//! Where SPICE can express something Spectre has no documented spelling for,
//! this file returns `CodeGenError` instead of guessing. Currently: XSPICE
//! A-devices, raw SPICE lines, `bsource` expressions (SPICE `V(x)`/`I(Vx)`
//! syntax is not Spectre's lowercase `v(x)`/`i("x:0")` — see `[CMP]` bsource),
//! current-controlled switches, `AM` sources, and transient noise (Spectre
//! spells it `tran ... noisefmax=` and the IR carries no bandwidth).

use crate::circuit::format_spice_number;
use crate::ir::*;
use super::{CodeGen, CodeGenError};

pub struct SpectreCodeGen;

/// Format a number for the Spectre native language.
///
/// `format_spice_number` emits SPICE scale factors. Per `[KUN]` Table B.1 vs
/// B.2 the sets differ: Spectre uses SI factors (`T G M K k m u n p f a`) while
/// SPICE uses `t g meg k m u n p f`. `f p n u m k` mean the same thing in both,
/// so only the three large SPICE-only factors need rewriting. Cadence's own
/// examples use `=1k`, `=100k`, `=1.2K`, `=900M`, `=10M`, `=1G`, `=2.45G`
/// (`[REF]`/`[CMP]`), never `meg`/`g`/`t`.
fn spectre_num(v: f64) -> String {
    let s = format_spice_number(v);
    if let Some(head) = s.strip_suffix("meg") {
        format!("{head}M")
    } else if let Some(head) = s.strip_suffix('g') {
        format!("{head}G")
    } else if let Some(head) = s.strip_suffix('t') {
        format!("{head}T")
    } else {
        s
    }
}

/// Reference to another instance in the deck (probe, swept device, ...).
///
/// `[KUN]` B.2: "The language becomes case-sensitive". Every instance this file
/// emits is lowercased, so references to them must be lowercased identically.
/// The IR carries SPICE names (`Vin`, `R1`), which are case-insensitive there.
fn inst_ref(name: &str) -> String {
    name.to_lowercase()
}

/// Strip a SPICE `V(x)` / `I(x)` wrapper down to the bare name.
///
/// Spectre names nodes bare — `[REF]` p.502 `nodeset 7=0 out=1`, p.524
/// `sens (q1:betadc 2 Out) ...`. `V(out)` is not a Spectre node reference.
fn bare_node(s: &str) -> &str {
    let t = s.trim();
    let rest = t
        .strip_prefix("V(")
        .or_else(|| t.strip_prefix("v("))
        .or_else(|| t.strip_prefix("I("))
        .or_else(|| t.strip_prefix("i("));
    match rest.and_then(|r| r.strip_suffix(')')) {
        Some(inner) => inner.trim(),
        None => t,
    }
}

impl SpectreCodeGen {
    fn unsupported_component(&self, what: &str) -> CodeGenError {
        CodeGenError::UnsupportedComponent {
            backend: "spectre".into(),
            component: what.into(),
        }
    }

    fn unsupported_analysis(&self, what: &str) -> CodeGenError {
        CodeGenError::UnsupportedAnalysis {
            backend: "spectre".into(),
            analysis: what.into(),
        }
    }

    fn emit_value(&self, v: &IrValue) -> String {
        match v {
            IrValue::Numeric { value } => spectre_num(*value),
            IrValue::Expression { expr } => expr.clone(),
            IrValue::Raw { text } => text.clone(),
        }
    }

    /// Frequency-sweep point spec shared by `ac`, `noise`, `xf`, `sp`, `stb`,
    /// `pac`, `pnoise`, `pxf`, `pstb`.
    ///
    /// `[REF]` p.43/44 (ac), p.182 (noise), p.387 (sp), p.393 (stb), p.245
    /// (pac), p.255 (pnoise), p.299 (pstb), p.303 (pxf), p.439 (xf) all list the
    /// identical "Sweep interval parameters" block:
    /// `start stop center span step lin dec log values valuesfile`.
    /// There is no `oct` — SPICE's octave sweep has no Spectre spelling.
    fn sweep_points(&self, variation: &str, points: u32) -> Result<String, CodeGenError> {
        match variation.to_ascii_lowercase().as_str() {
            "dec" => Ok(format!("dec={points}")),
            "lin" => Ok(format!("lin={points}")),
            "log" => Ok(format!("log={points}")),
            other => Err(self.unsupported_analysis(&format!(
                "frequency sweep type '{other}' (Spectre accepts only dec/lin/log, see spectre -h ac)"
            ))),
        }
    }

    /// `[CMP]` p.683-685 (`vsource`) / p.380-382 (`isource`): the waveform is
    /// selected with `type=`, whose documented values are exactly
    /// `dc, pulse, pwl, sine, exp`.
    fn emit_waveform_params(&self, wf: &IrWaveform) -> Result<String, CodeGenError> {
        let s = match wf {
            // `[CMP]` p.684: sinedc, ampl, freq, sinephase, damp — and the
            // delay is the *general* waveform parameter `delay` (p.683 #4),
            // not a sine-specific one.
            IrWaveform::Sin { offset, amplitude, frequency, delay, damping, phase } => {
                let mut s = format!(
                    "type=sine sinedc={} ampl={} freq={}",
                    spectre_num(*offset), spectre_num(*amplitude), spectre_num(*frequency),
                );
                if *delay != 0.0 {
                    s.push_str(&format!(" delay={}", spectre_num(*delay)));
                }
                if *damping != 0.0 {
                    s.push_str(&format!(" damp={}", spectre_num(*damping)));
                }
                if *phase != 0.0 {
                    s.push_str(&format!(" sinephase={}", spectre_num(*phase)));
                }
                s
            }
            // `[CMP]` p.683: verbatim sample statement
            // `vpulse1 (1 0) vsource type=pulse val0=0 val1=5 period=100n
            //  rise=10n fall=10n width=40n`
            IrWaveform::Pulse { initial, pulsed, delay, rise_time, fall_time, pulse_width, period } => {
                format!(
                    "type=pulse val0={} val1={} delay={} rise={} fall={} width={} period={}",
                    spectre_num(*initial), spectre_num(*pulsed), spectre_num(*delay),
                    spectre_num(*rise_time), spectre_num(*fall_time),
                    spectre_num(*pulse_width), spectre_num(*period),
                )
            }
            // `[CMP]` p.683: `vpwl1 (1 0) vsource type=pwl
            //  wave=[1n 0 1.1n 2 1.5n 0.5 2n 3 5n 5]`
            IrWaveform::Pwl { values } => {
                let mut s = String::from("type=pwl wave=[");
                for (i, (t, v)) in values.iter().enumerate() {
                    if i > 0 {
                        s.push(' ');
                    }
                    s.push_str(&format!("{} {}", spectre_num(*t), spectre_num(*v)));
                }
                s.push(']');
                s
            }
            // `[CMP]` p.685 "Exponential waveform parameters": td1 tau1 td2 tau2,
            // with val0/val1 shared with the pulse waveform (p.683 #5/#6).
            IrWaveform::Exp { initial, pulsed, rise_delay, rise_tau, fall_delay, fall_tau } => {
                format!(
                    "type=exp val0={} val1={} td1={} tau1={} td2={} tau2={}",
                    spectre_num(*initial), spectre_num(*pulsed),
                    spectre_num(*rise_delay), spectre_num(*rise_tau),
                    spectre_num(*fall_delay), spectre_num(*fall_tau),
                )
            }
            // SPICE SFFM is a sine carrier with sinusoidal frequency modulation.
            // `[CMP]` p.684-685 gives Spectre the identically-named knobs
            // fmmodindex ("FM index of modulation") and fmmodfreq ("FM
            // modulation frequency") on `type=sine`. The parameter names are
            // documented; the SPICE->Spectre mapping of the *values* is
            // inferred from the standard definition of an FM index and is not
            // stated in any Cadence document.
            IrWaveform::Sffm { offset, amplitude, carrier_freq, modulation_index, signal_freq } => {
                format!(
                    "type=sine sinedc={} ampl={} freq={} fmmodindex={} fmmodfreq={}",
                    spectre_num(*offset), spectre_num(*amplitude),
                    spectre_num(*carrier_freq), spectre_num(*modulation_index),
                    spectre_num(*signal_freq),
                )
            }
            // Spectre has ammodindex/ammodfreq/ammodphase (`[CMP]` p.685) but
            // publishes no waveform equation, and SPICE's own AM() definition
            // varies between simulators. Refusing beats guessing the mapping.
            IrWaveform::Am { .. } => {
                return Err(self.unsupported_component(
                    "AM source (Spectre documents ammodindex/ammodfreq but no waveform equation; \
                     the SPICE AM() -> Spectre mapping cannot be established)",
                ));
            }
        };
        Ok(s)
    }

    fn emit_params(&self, params: &[(String, String)]) -> String {
        let mut s = String::new();
        for (k, v) in params {
            s.push_str(&format!(" {}={}", k, v));
        }
        s
    }

    /// `[REF]` p.406-410 `sweep`, verbatim example:
    /// ```text
    /// swp sweep param=temp values=[-50 0 50 100 125] {
    ///                oppoint dc oppoint=logfile
    /// }
    /// ```
    fn emit_sweep_header(&self, idx: usize, sp: &StepParam) -> Result<String, CodeGenError> {
        let suffix: String = sp.param.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        let sweep = match sp.sweep_type.as_deref() {
            Some("dec") | Some("DEC") => format!("dec={}", spectre_num(sp.step)),
            Some("lin") | Some("LIN") | None => format!("step={}", spectre_num(sp.step)),
            Some("log") | Some("LOG") => format!("log={}", spectre_num(sp.step)),
            Some(other) => {
                return Err(self.unsupported_analysis(&format!(
                    "parameter sweep type '{other}' (Spectre sweep accepts step/lin/dec/log)"
                )));
            }
        };
        Ok(format!(
            "sweep{}_{} sweep param={} start={} stop={} {} {{",
            idx + 1,
            suffix,
            sp.param,
            spectre_num(sp.start),
            spectre_num(sp.stop),
            sweep,
        ))
    }

    fn emit_analysis_block(&self, analyses: &[Analysis], step_params: &[StepParam]) -> Result<Vec<String>, CodeGenError> {
        let mut block = Vec::new();
        for analysis in analyses {
            block.push(self.emit_analysis(analysis)?);
        }

        for (idx, sp) in step_params.iter().enumerate().rev() {
            let mut wrapped = Vec::new();
            wrapped.push(self.emit_sweep_header(idx, sp)?);
            for line in block {
                wrapped.push(format!("  {}", line.replace('\n', "\n  ")));
            }
            wrapped.push("}".into());
            block = wrapped;
        }

        Ok(block)
    }

    /// Spectre supports SPICE `.measure` directly — `[REF]` p.17: "In addition
    /// to supporting standard SPICE measurement functions (.measure), it offers
    /// a measurement description language (MDL)", and p.20 notes Spectre "saves
    /// the .measure and .mt0 files in the .raw subdirectory".
    ///
    /// `.measure` is SPICE syntax, so it has to be fenced by language switches
    /// (`[REF]` p.493 include / `[KUN]` B.2 `simulator lang=`). The previous
    /// implementation stripped the leading `.meas` and emitted the bare tail,
    /// which Spectre would read as an instance statement named `tran`.
    fn emit_measure(&self, meas: &str) -> String {
        let body = meas.trim();
        let body = if body.starts_with('.') {
            body.to_string()
        } else {
            format!(".measure {body}")
        };
        format!("simulator lang=spice\n{body}\nsimulator lang=spectre")
    }

    pub fn emit_model_pub(&self, m: &ModelDef) -> String {
        self.emit_model(m)
    }

    pub fn emit_instance_pub(&self, inst: &Instance) -> String {
        self.emit_instance(inst)
    }

    /// `[CMP]` p.628 `model resmod resistor rsh=150 l=2u w=2u etch=0.05u ...`,
    /// p.645 `model lmodel tline f=10M z0=50 alphac=8501 fc=10M dcr=88`,
    /// p.625 `model my_relay relay vt1=2.5 vt2=5 ropen=100M rclosed=0.1`,
    /// `[REF]` p.498 `model nch bsim3v3 type=n mobmod=1 capmod=2 version=3.1`.
    /// No Cadence example parenthesises model parameters; the previous version
    /// of this file did.
    fn emit_model(&self, m: &ModelDef) -> String {
        let mut s = format!("model {} {}", m.name, m.kind);
        for (k, v) in &m.parameters {
            s.push_str(&format!(" {}={}", k, v));
        }
        s
    }

    /// `[REF]` p.535: `Coax1 pin nin out gnd coax zin=75 zout=150 len=35m`
    /// (instance name, node list, subcircuit master, parameters). Parentheses
    /// around the node list are accepted and used throughout `[CMP]`.
    fn emit_instance(&self, inst: &Instance) -> String {
        let mut s = format!("x{} (", inst.name.to_lowercase());
        for (i, port) in inst.port_mapping.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            s.push_str(port);
        }
        s.push_str(&format!(") {}", inst.subcircuit));
        s.push_str(&self.emit_params(&inst.parameters));
        s
    }

    fn emit_subcircuit_body(&self, sc: &Subcircuit) -> Result<String, CodeGenError> {
        let mut lines = Vec::new();

        // `[REF]` p.506: `parameters <param=value> [param=value]...`, and p.535
        // shows it as the first line of a subckt body.
        if !sc.parameters.is_empty() {
            let mut param_line = String::from("parameters");
            for p in &sc.parameters {
                if let Some(ref default) = p.default {
                    param_line.push_str(&format!(" {}={}", p.name, default));
                } else {
                    param_line.push_str(&format!(" {}", p.name));
                }
            }
            lines.push(param_line);
        }

        for m in &sc.models {
            lines.push(self.emit_model(m));
        }

        for comp in &sc.components {
            lines.push(self.emit_component(comp)?);
        }

        for inst in &sc.instances {
            lines.push(self.emit_instance(inst));
        }

        Ok(lines.join("\n"))
    }

    /// Map portable option names onto `options` statement parameters.
    ///
    /// `[REF]` p.187-236 "Immediate Set Options (options)": `reltol` (#1),
    /// `vabstol` (#3), `iabstol` (#4), `temp` (#6), `tnom` (#7), `gmin` (#128),
    /// `dcmaxiters` (#56, "Maximum number of Newton iterations in DC
    /// simulation"). The parameter index on p.237-240 has no plain `maxiters` —
    /// that name belongs to the `dc` analysis statement (p.66 #29), not to
    /// `options`, so the previous `max_iterations -> maxiters` mapping produced
    /// an unknown option.
    fn map_option_name(&self, canonical: &str) -> String {
        match canonical {
            "abstol" => "iabstol".into(),
            "vntol" => "vabstol".into(),
            "max_iterations" => "dcmaxiters".into(),
            other => other.into(),
        }
    }
}

impl CodeGen for SpectreCodeGen {
    fn backend_name(&self) -> &str {
        "spectre"
    }

    fn emit_netlist(&self, ir: &CircuitIR) -> Result<String, CodeGenError> {
        let mut lines = Vec::new();

        // `[KUN]` B.2: `//` is a Spectre comment; "all Spectre netlists must
        // begin with a lang=spectre statement".
        lines.push(format!("// {}", ir.top.name));
        lines.push(String::new());
        lines.push("simulator lang=spectre".into());
        lines.push(String::new());

        // `[CMP]` p.888 / `[REF]` p.541: `ahdl_include "VerilogAfile.va"`.
        for path in &ir.top.osdi_loads {
            lines.push(format!("ahdl_include \"{}\"", path));
        }

        // `[REF]` p.494: `include "filename"`.
        for inc in &ir.top.includes {
            lines.push(format!("include \"{}\"", inc));
        }

        // `[REF]` p.493: `include "file" section=sectionName` — the documented
        // way to pick a PDK corner out of a `library`/`section` file (p.498).
        for (path, section) in &ir.top.libs {
            lines.push(format!("include \"{}\" section={}", path, section));
        }

        for lib in &ir.model_libraries {
            for setup in &lib.setup_includes {
                lines.push(format!("include \"{}\"", setup));
            }
            let path = lib.backend_paths
                .get("spectre")
                .unwrap_or(&lib.path);
            if let Some(ref corner) = lib.corner {
                lines.push(format!("include \"{}\" section={}", path, corner));
            } else {
                lines.push(format!("include \"{}\"", path));
            }
        }

        // `[REF]` p.506: `parameters p1=1 p2=2`.
        if !ir.top.parameters.is_empty() {
            let mut param_line = String::from("parameters");
            for p in &ir.top.parameters {
                if let Some(ref default) = p.default {
                    param_line.push_str(&format!(" {}={}", p.name, default));
                } else {
                    param_line.push_str(&format!(" {}", p.name));
                }
            }
            lines.push(param_line);
        }

        for m in &ir.top.models {
            lines.push(self.emit_model(m));
        }

        for sc in &ir.subcircuit_defs {
            lines.push(String::new());
            lines.push(self.emit_subcircuit(sc)?);
        }

        lines.push(String::new());

        for comp in &ir.top.components {
            lines.push(self.emit_component(comp)?);
        }

        for inst in &ir.top.instances {
            lines.push(self.emit_instance(inst));
        }

        if let Some(ref tb) = ir.testbench {
            for comp in &tb.stimulus {
                lines.push(self.emit_component(comp)?);
            }

            let opts = self.emit_options(&tb.options)?;
            if !opts.is_empty() {
                lines.push(opts);
            }

            // `[REF]` p.236 verbatim: `myopt options temp=27`; temp is options
            // parameter #6 and tnom #7 (p.187-188).
            if let Some(temp) = tb.temperature {
                lines.push(format!("mytemp options temp={}", spectre_num(temp)));
            }
            if let Some(tnom) = tb.nominal_temperature {
                lines.push(format!("mytnom options tnom={}", spectre_num(tnom)));
            }

            // `[REF]` p.490: `ic 7=0 out=1 OpAmp1.comp=5 L1:1=1.0u`.
            for (node, val) in &tb.initial_conditions {
                lines.push(format!("ic {}={}", bare_node(node), spectre_num(*val)));
            }

            // `[REF]` p.502: `nodeset 7=0 out=1 OpAmp1.comp=5 L1:1=1.0u`.
            for (node, val) in &tb.node_sets {
                lines.push(format!("nodeset {}={}", bare_node(node), spectre_num(*val)));
            }

            // `[REF]` p.517-518: `save 7 out OpAmp1.comp M1:currents ...` —
            // bare signal names, not `V(...)`.
            for save in &tb.saves {
                lines.push(format!("save {}", bare_node(save)));
            }

            for meas in &tb.measures {
                lines.push(self.emit_measure(meas));
            }

            for line in &tb.extra_lines {
                lines.push(line.clone());
            }

            lines.push(String::new());

            lines.extend(self.emit_analysis_block(&tb.analyses, &tb.step_params)?);
        }

        Ok(lines.join("\n"))
    }

    /// `[REF]` p.533-535: `[inline] subckt <Name> (<node1> ... <nodeN>)` ...
    /// `ends <Name>`, verbatim example `subckt coax (i1 o1 i2 o2)` / `ends coax`.
    fn emit_subcircuit(&self, sc: &Subcircuit) -> Result<String, CodeGenError> {
        let mut header = format!("subckt {} (", sc.name);
        for (i, port) in sc.ports.iter().enumerate() {
            if i > 0 {
                header.push(' ');
            }
            header.push_str(&port.name);
        }
        header.push(')');

        let body = self.emit_subcircuit_body(sc)?;

        Ok(format!("{}\n{}\nends {}", header, body, sc.name))
    }

    fn emit_component(&self, comp: &Component) -> Result<String, CodeGenError> {
        let s = match comp {
            // `[CMP]` p.628 `r1 (1 2) resistor r=1.2K m=2`
            Component::Resistor { name, n1, n2, value, params } => {
                let mut s = format!("r{} ({} {}) resistor r={}", name.to_lowercase(), n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            // `[CMP]` p.283 `c2 (1 0) capacitor c=2.5u tc1=1e-8`
            Component::Capacitor { name, n1, n2, value, params } => {
                let mut s = format!("c{} ({} {}) capacitor c={}", name.to_lowercase(), n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            // `[CMP]` p.372 `l33 (0 net29) inductor l=10e-9 r=1 m=1`
            Component::Inductor { name, n1, n2, value, params } => {
                let mut s = format!("l{} ({} {}) inductor l={}", name.to_lowercase(), n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            // `[CMP]` p.577 `ml1 mutual_inductor coupling=1 ind1=l1 ind2=l2`
            Component::MutualInductor { name, inductor1, inductor2, coupling } => {
                format!("k{} mutual_inductor coupling={} ind1=l{} ind2=l{}",
                    name.to_lowercase(), coupling, inductor1.to_lowercase(), inductor2.to_lowercase())
            }
            // `[CMP]` p.683 `Name p n vsource parameter=value ...`; `dc` is
            // instance parameter #1, `mag`/`phase` are the small-signal
            // parameters #39/#40 (p.685).
            Component::VoltageSource { name, np, nm, value, ac_magnitude, ac_phase, waveform } => {
                let mut s = format!("v{} ({} {}) vsource dc={}", name.to_lowercase(), np, nm, self.emit_value(value));
                if let Some(mag) = ac_magnitude {
                    s.push_str(&format!(" mag={}", spectre_num(*mag)));
                    if let Some(phase) = ac_phase {
                        s.push_str(&format!(" phase={}", spectre_num(*phase)));
                    }
                }
                if let Some(wf) = waveform {
                    s.push_str(&format!(" {}", self.emit_waveform_params(wf)?));
                }
                s
            }
            // `[CMP]` p.380 `i1 (in 0) isource dc=0 type=pulse delay=10n ...`
            Component::CurrentSource { name, np, nm, value, ac_magnitude, ac_phase, waveform } => {
                let mut s = format!("i{} ({} {}) isource dc={}", name.to_lowercase(), np, nm, self.emit_value(value));
                if let Some(mag) = ac_magnitude {
                    s.push_str(&format!(" mag={}", spectre_num(*mag)));
                    if let Some(phase) = ac_phase {
                        s.push_str(&format!(" phase={}", spectre_num(*phase)));
                    }
                }
                if let Some(wf) = waveform {
                    s.push_str(&format!(" {}", self.emit_waveform_params(wf)?));
                }
                s
            }
            // Spectre's bsource does exist (`[CMP]` p.856:
            // `name (node1 node2) bsource v=generic_expr`) but its expression
            // grammar is not SPICE's: node voltages are `v(a,b)` (lowercase —
            // the language is case sensitive, `[KUN]` B.2), branch currents are
            // `i("inst_id:index")` not `I(Vx)`, and time is `$time`. The IR
            // holds an opaque SPICE expression string we cannot faithfully
            // rewrite, so refuse rather than emit something that looks right.
            Component::BehavioralVoltage { .. } | Component::BehavioralCurrent { .. } => {
                return Err(self.unsupported_component(
                    "behavioural source (Spectre bsource uses v(a,b) / i(\"inst:idx\") / $time, \
                     not SPICE V()/I()/time; the expression cannot be translated safely)",
                ));
            }
            // `[CMP]` p.680 `e1 (out1 0 pos neg) vcvs gain=10`
            Component::Vcvs { name, np, nm, ncp, ncm, gain } => {
                format!("e{} ({} {} {} {}) vcvs gain={}", name.to_lowercase(), np, nm, ncp, ncm, gain)
            }
            // `[CMP]` p.678 `Name sink src ps ns ... vccs`, parameter `gm`
            Component::Vccs { name, np, nm, ncp, ncm, transconductance } => {
                format!("g{} ({} {} {} {}) vccs gm={}", name.to_lowercase(), np, nm, ncp, ncm, transconductance)
            }
            // `[CMP]` p.286 `vcs (pos gnd) cccs gain=2.5 probe=v1`
            Component::Cccs { name, np, nm, vsense, gain } => {
                format!("f{} ({} {}) cccs probe={} gain={}", name.to_lowercase(), np, nm, inst_ref(vsense), gain)
            }
            // `[CMP]` p.288 `vvs (pos gnd) ccvs rm=1 probe=v1`
            Component::Ccvs { name, np, nm, vsense, transresistance } => {
                format!("h{} ({} {}) ccvs probe={} rm={}", name.to_lowercase(), np, nm, inst_ref(vsense), transresistance)
            }
            // `[CMP]` p.302 `Name a c ModelName parameter=value ...`,
            // `d0 (dp dn) pdiode l=3e-4 w=2.5e-4 area=1`
            Component::Diode { name, np, nm, model, params } => {
                let mut s = format!("d{} ({} {}) {}", name.to_lowercase(), np, nm, model);
                s.push_str(&self.emit_params(params));
                s
            }
            // `[CMP]` bjt models: `Name c b e [s] ModelName parameter=value ...`
            Component::Bjt { name, nc, nb, ne, model, params } => {
                let mut s = format!("q{} ({} {} {}) {}", name.to_lowercase(), nc, nb, ne, model);
                s.push_str(&self.emit_params(params));
                s
            }
            // `[CMP]` mos models: `Name d g s b ModelName parameter=value ...`
            Component::Mosfet { name, nd, ng, ns, nb, model, params } => {
                let mut s = format!("m{} ({} {} {} {}) {}", name.to_lowercase(), nd, ng, ns, nb, model);
                s.push_str(&self.emit_params(params));
                s
            }
            // `[CMP]` jfet: `Name d g s ModelName parameter=value ...`
            Component::Jfet { name, nd, ng, ns, model, params } => {
                let mut s = format!("j{} ({} {} {}) {}", name.to_lowercase(), nd, ng, ns, model);
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Mesfet { name, nd, ng, ns, model, params } => {
                let mut s = format!("z{} ({} {} {}) {}", name.to_lowercase(), nd, ng, ns, model);
                s.push_str(&self.emit_params(params));
                s
            }
            // Spectre's voltage-controlled switch is `relay`, `[CMP]` p.625:
            // `Name 1 2 ps ns ModelName parameter=value ...` /
            // `rel1 (1 2 ps ns) my_relay ropen=1G rclosed=2` — the same node
            // order as SPICE `S`. NOTE: the referenced model must be declared
            // `model <name> relay ...`; a SPICE `.model <name> sw` carried
            // through the IR names a primitive Spectre does not have.
            Component::VSwitch { name, np, nm, ncp, ncm, model } => {
                format!("s{} ({} {} {} {}) {}", name.to_lowercase(), np, nm, ncp, ncm, model)
            }
            // `relay` is voltage controlled only; `switch` (`[CMP]` p.643) is a
            // multi-throw switch whose position only changes between analyses.
            // Neither is a current-controlled switch.
            Component::ISwitch { .. } => {
                return Err(self.unsupported_component(
                    "current-controlled switch (Spectre has relay (voltage controlled) and \
                     switch (position set between analyses); neither is SPICE's W element)",
                ));
            }
            // `[CMP]` p.644-645 `t1 (1 0 2 0) tline z0=100`, instance params
            // `z0` (#1) and `td` (#2).
            Component::TLine { name, inp, inm, outp, outm, z0, td } => {
                format!("t{} ({} {} {} {}) tline z0={} td={}", name.to_lowercase(), inp, inm, outp, outm, z0, td)
            }
            Component::Xspice { .. } => {
                return Err(self.unsupported_component(
                    "XSPICE A-device (ngspice-specific; Spectre's behavioural path is Verilog-A \
                     via ahdl_include)",
                ));
            }
            Component::RawSpice { line } => {
                return Err(self.unsupported_component(&format!(
                    "raw SPICE line '{}' (Spectre reads SPICE only inside a simulator lang=spice \
                     region, which cannot be placed automatically)",
                    line.trim()
                )));
            }
        };
        Ok(s)
    }

    fn emit_analysis(&self, analysis: &Analysis) -> Result<String, CodeGenError> {
        let s = match analysis {
            // `[REF]` p.20143 verbatim: `dc1 dc`. A `dc` with no sweep
            // parameter is the operating point.
            Analysis::Op => "op1 dc".into(),

            // `[REF]` p.64: "sweep the circuit temperature by giving the
            // parameter name as param=temp without a dev, mod or sub
            // parameter", "sweep a top-level netlist parameter by giving the
            // parameter name without a dev, mod or sub parameter", and #12
            // `dev` = "Device instance whose parameter value is to be swept".
            // SPICE `.dc Vin 0 5 0.1` sweeps the *dc value of a source*, which
            // in Spectre is `dev=vin param=dc` — not `param=Vin`.
            Analysis::Dc { sweeps } => {
                if let Some(sw) = sweeps.first() {
                    let selector = dc_sweep_selector(&sw.source);
                    format!(
                        "dc1 dc {} start={} stop={} step={}",
                        selector,
                        spectre_num(sw.start),
                        spectre_num(sw.stop),
                        spectre_num(sw.step),
                    )
                } else {
                    "dc1 dc".into()
                }
            }

            // `[REF]` p.43 `Name ac parameter=value ...` with start/stop/dec.
            Analysis::Ac { variation, points, start, stop } => {
                format!(
                    "ac1 ac start={} stop={} {}",
                    spectre_num(*start),
                    spectre_num(*stop),
                    self.sweep_points(variation, *points)?,
                )
            }

            // `[REF]` p.416-417 `Name tran parameter=value ...`: `stop` (#1),
            // `start` (#3), `maxstep` (#8), `step` (#9, "Minimum time step used
            // by the simulator solely to maintain the aesthetics of the
            // computed waveforms") — the closest analogue of SPICE's tstep.
            // Verbatim example `[REF]` p.433: `tran1 tran stop=0.5u ...`.
            Analysis::Transient { step, stop, start, max_step, .. } => {
                let mut s = format!(
                    "tran1 tran step={} stop={}",
                    spectre_num(*step),
                    spectre_num(*stop),
                );
                if let Some(st) = start {
                    s.push_str(&format!(" start={}", spectre_num(*st)));
                }
                if let Some(ms) = max_step {
                    s.push_str(&format!(" maxstep={}", spectre_num(*ms)));
                }
                s
            }

            // `[REF]` p.181-182: `Name [p] [n] noise parameter=value ...`,
            // "The optional terminals (p and n) specify the output of the
            // circuit". #16 `oprobe` and #17 `iprobe` name *components*, not
            // nodes — so the output goes in the terminal list and only the
            // input source becomes `iprobe`.
            Analysis::Noise { output, reference, source, variation, points, start, stop, .. } => {
                let neg = if reference.is_empty() { "0" } else { bare_node(reference) };
                format!(
                    "noise1 ({} {}) noise start={} stop={} {} iprobe={}",
                    bare_node(output),
                    neg,
                    spectre_num(*start),
                    spectre_num(*stop),
                    self.sweep_points(variation, *points)?,
                    inst_ref(source),
                )
            }

            // `[REF]` p.438-439: `Name [p] [n] xf parameter=value ...`,
            // "you can simply specify a voltage to be the output by giving a
            // pair of nodes on the xf analysis statement". xf computes the
            // transfer function from *every* independent source to that output,
            // so there is no `source=` parameter (the previous version emitted
            // one). `freq` is parameter #15.
            //
            // INFERRED: SPICE `.tf` is a DC transfer function and Spectre's xf
            // is a small-signal/AC one; `freq=0` is this file's choice for the
            // DC case and is not stated in any Cadence document.
            Analysis::Tf { output, source: _ } => {
                format!("xf1 ({} 0) xf freq=0", bare_node(output))
            }

            // `[REF]` p.524: `sens (output_variables_list) to
            // (design_parameters_list) for (analyses_list)`, verbatim example
            // `sens (1 n2 7) for (analAC)`. `sens` is a control statement that
            // refers to a named analysis, so the analysis is emitted too.
            Analysis::Sensitivity { output, ac } => {
                match ac {
                    Some(p) => format!(
                        "sensac1 ac start={} stop={} {}\nsens ({}) for (sensac1)",
                        spectre_num(p.start),
                        spectre_num(p.stop),
                        self.sweep_points(&p.variation, p.points)?,
                        bare_node(output),
                    ),
                    None => format!("sensdc1 dc\nsens ({}) for (sensdc1)", bare_node(output)),
                }
            }

            // `[REF]` p.272-277: `Name [p] [n] pss parameter=value ...` with
            // `fund` (#2), `harms` (#4), `tstab` (#6). There is no `ppv` and no
            // `probe` parameter (the previous version emitted both); for an
            // autonomous circuit the observed node goes in the terminal list.
            // `points_per_period` has no documented pss parameter and is
            // dropped.
            Analysis::Pss { fundamental, stabilization, observe_node, harmonics, .. } => {
                let terms = if observe_node.is_empty() {
                    String::new()
                } else {
                    format!("({} 0) ", bare_node(observe_node))
                };
                format!(
                    "pss1 {}pss fund={} tstab={} harms={}",
                    terms,
                    spectre_num(*fundamental),
                    spectre_num(*stabilization),
                    harmonics,
                )
            }

            // `[REF]` p.90-91: `Name [p] [n] hb parameter=value ...` with
            // `fundfreqs=[...]` (#2, "Array of fundamental frequencies") and
            // `maxharms=[...]` (#3). There are no `toneN`/`nharmN` parameters.
            Analysis::HarmonicBalance { frequencies, harmonics } => {
                let freqs: Vec<String> = frequencies.iter().map(|f| spectre_num(*f)).collect();
                let harms: Vec<String> = harmonics.iter().map(|h| h.to_string()).collect();
                format!("hb1 hb fundfreqs=[{}] maxharms=[{}]", freqs.join(" "), harms.join(" "))
            }

            // `[REF]` p.387-389: `Name sp parameter=value ...`. NOTE the deck
            // must also contain `port` instances — "There must be at least one
            // active port statement in the circuit" — which the IR cannot
            // express, so this statement alone is necessary but not sufficient.
            Analysis::SPar { variation, points, start, stop } => {
                format!(
                    "sp1 sp start={} stop={} {}",
                    spectre_num(*start),
                    spectre_num(*stop),
                    self.sweep_points(variation, *points)?,
                )
            }

            // `[REF]` p.393-394: `Name stb parameter=value ...`, `probe` =
            // "Probe instance around which the loop gain is calculated".
            Analysis::Stability { probe, variation, points, start, stop } => {
                format!(
                    "stb1 stb start={} stop={} {} probe={}",
                    spectre_num(*start),
                    spectre_num(*stop),
                    self.sweep_points(variation, *points)?,
                    inst_ref(probe),
                )
            }

            // Spectre has no `trnoise` analysis. Transient noise is a `tran`
            // with `noisefmax`/`noiseseed` (`[REF]` p.433 verbatim:
            // `tran1 tran stop=0.5u noisefmax=10G noiseseed=1`) and the IR
            // carries no noise bandwidth, so there is nothing to emit.
            Analysis::TransientNoise { .. } => {
                return Err(self.unsupported_analysis(
                    "transient noise (Spectre spells it `tran ... noisefmax=<Hz>`; \
                     the IR carries no noise bandwidth)",
                ));
            }

            // `[CMP]` p.323-325: `fourier` is a *component*, not an analysis —
            // `Name [p] [n] [pr] [nr] fourier parameter=value ...`, verbatim
            // `four1 (1 0) fourmod harms=50` with `fund` as instance parameter
            // #1. It is active during transient analysis.
            Analysis::Fourier { fundamental, outputs, num_harmonics } => {
                let harms = num_harmonics.map(|h| format!(" harms={h}")).unwrap_or_default();
                outputs
                    .iter()
                    .enumerate()
                    .map(|(i, out)| format!(
                        "four{} ({} 0) fourier fund={}{}",
                        i + 1, bare_node(out), spectre_num(*fundamental), harms,
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            }

            // `[REF]` p.406-410 `sweep`. `inner` is the child analysis name and
            // `inner_type` its type, which together form the child statement.
            Analysis::SpectreSweep { param, start, stop, step, inner, inner_type } => {
                format!(
                    "sweep1 sweep param={} start={} stop={} step={} {{\n  {} {}\n}}",
                    param,
                    spectre_num(*start),
                    spectre_num(*stop),
                    spectre_num(*step),
                    inner,
                    inner_type,
                )
            }

            // `[REF]` p.174 verbatim:
            // `mc1 montecarlo variations=process seed=1234 numruns=200 { ... }`
            // — every parameter precedes the brace. The previous version put
            // `seed=` *after* the closing brace.
            //
            // NOTE: montecarlo only varies parameters declared in a
            // `statistics` block (`[REF]` p.174-180), which the IR cannot
            // express; without one this runs `numruns` identical simulations.
            Analysis::SpectreMonteCarlo { iterations, inner, inner_type, seed } => {
                let seed_str = seed.map(|s| format!(" seed={s}")).unwrap_or_default();
                format!(
                    "mc1 montecarlo numruns={}{} {{\n  {} {}\n}}",
                    iterations, seed_str, inner, inner_type,
                )
            }

            // `[REF]` p.245-246 `pac`: sweep interval params plus `sweeptype`
            // (#11, values absolute/relative/unspecified). PSS is its
            // prerequisite (`[REF]` p.272).
            Analysis::SpectrePac { pss_fundamental, pss_stabilization, pss_harmonics, variation, points, start, stop, sweep_type } => {
                format!(
                    "pss1 pss fund={} tstab={} harms={}\npac1 pac start={} stop={} {} sweeptype={}",
                    spectre_num(*pss_fundamental),
                    spectre_num(*pss_stabilization),
                    pss_harmonics,
                    spectre_num(*start),
                    spectre_num(*stop),
                    self.sweep_points(variation, *points)?,
                    sweep_type,
                )
            }

            // `[REF]` p.253-256 `pnoise`: `Name [p] [n] ... pnoise ...`, probe
            // parameters are `oprobe` (#13) and `iprobe` (#14) — both component
            // names. There is no `refprobe` (the previous version emitted one);
            // the reference node belongs in the terminal list.
            Analysis::SpectrePnoise { pss_fundamental, pss_stabilization, pss_harmonics, output, reference, variation, points, start, stop } => {
                let neg = if reference.is_empty() { "0" } else { bare_node(reference) };
                format!(
                    "pss1 pss fund={} tstab={} harms={}\npnoise1 ({} {}) pnoise start={} stop={} {}",
                    spectre_num(*pss_fundamental),
                    spectre_num(*pss_stabilization),
                    pss_harmonics,
                    bare_node(output),
                    neg,
                    spectre_num(*start),
                    spectre_num(*stop),
                    self.sweep_points(variation, *points)?,
                )
            }

            // `[REF]` p.303-304 `pxf`: `Name [p] [n] ... pxf ...`, the only
            // probe parameter is `probe` (#13, the *output*). Like `xf` it
            // computes transfer functions from every source, so there is no
            // input-source parameter (the previous version emitted `isrc=`).
            Analysis::SpectrePxf { pss_fundamental, pss_stabilization, pss_harmonics, output, source: _, variation, points, start, stop } => {
                format!(
                    "pss1 pss fund={} tstab={} harms={}\npxf1 ({} 0) pxf start={} stop={} {}",
                    spectre_num(*pss_fundamental),
                    spectre_num(*pss_stabilization),
                    pss_harmonics,
                    bare_node(output),
                    spectre_num(*start),
                    spectre_num(*stop),
                    self.sweep_points(variation, *points)?,
                )
            }

            // `[REF]` p.298-299 `pstb`: `probe` (#11) = "Probe instance around
            // which the loop gain is calculated".
            Analysis::SpectrePstb { pss_fundamental, pss_stabilization, pss_harmonics, probe, variation, points, start, stop } => {
                format!(
                    "pss1 pss fund={} tstab={} harms={}\npstb1 pstb start={} stop={} {} probe={}",
                    spectre_num(*pss_fundamental),
                    spectre_num(*pss_stabilization),
                    pss_harmonics,
                    spectre_num(*start),
                    spectre_num(*stop),
                    self.sweep_points(variation, *points)?,
                    inst_ref(probe),
                )
            }

            // Spectre does have `pz` (`[REF]` p.312) but its parameter set does
            // not match SPICE's `.pz` node quadruple, and everything else left
            // here is another simulator's dialect.
            other => {
                return Err(self.unsupported_analysis(other.kind_str()));
            }
        };
        Ok(s)
    }

    /// `[REF]` p.236 verbatim: `o1 options scale=1.2 subckt=chip1`,
    /// `myopt options temp=27`.
    fn emit_options(&self, opts: &SimOptions) -> Result<String, CodeGenError> {
        let mut parts = Vec::new();

        for (key, val) in &opts.portable {
            let mapped = self.map_option_name(key);
            parts.push(format!("{}={}", mapped, val));
        }

        if let Some(specific) = opts.backend_specific.get("spectre") {
            for (key, val) in specific {
                parts.push(format!("{}={}", key, val));
            }
        }

        if parts.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!("myopts options {}", parts.join(" ")))
        }
    }
}

/// Pick the Spectre sweep-variable spelling for a SPICE `.dc` first argument.
///
/// `[REF]` p.64 lists the three forms: `param=temp` for temperature, a bare
/// `param=<name>` for a top-level netlist parameter, and `dev=<instance>`
/// combined with `param=<instance parameter>` for a device. SPICE sweeps the
/// `dc` value of an independent source, whose Spectre instance parameter is
/// `dc` (`[CMP]` p.683 vsource #1, p.380 isource #1).
fn dc_sweep_selector(source: &str) -> String {
    let lower = source.to_lowercase();
    if lower == "temp" {
        return "param=temp".into();
    }
    // SPICE requires the swept element of a `.dc` card to be an independent
    // source; V/I prefixes are the only legal spellings for one.
    match lower.as_bytes().first() {
        Some(b'v') | Some(b'i') => format!("dev={lower} param=dc"),
        _ => format!("param={source}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_resistor_divider() -> CircuitIR {
        CircuitIR {
            top: Subcircuit {
                name: "Voltage Divider".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![
                    Component::VoltageSource {
                        name: "in".into(),
                        np: "input".into(),
                        nm: "0".into(),
                        value: IrValue::Numeric { value: 10.0 },
                        ac_magnitude: None,
                        ac_phase: None,
                        waveform: None,
                    },
                    Component::Resistor {
                        name: "1".into(),
                        n1: "input".into(),
                        n2: "output".into(),
                        value: IrValue::Numeric { value: 10000.0 },
                        params: vec![],
                    },
                    Component::Resistor {
                        name: "2".into(),
                        n1: "output".into(),
                        n2: "0".into(),
                        value: IrValue::Numeric { value: 10000.0 },
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
            },
            testbench: Some(Testbench {
                dut: "Voltage Divider".into(),
                stimulus: vec![],
                analyses: vec![Analysis::Op],
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
        }
    }

    #[test]
    fn test_spectre_resistor_divider() {
        let ir = sample_resistor_divider();
        let cg = SpectreCodeGen;
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("// Voltage Divider"), "missing title: {}", netlist);
        assert!(netlist.contains("simulator lang=spectre"), "missing lang: {}", netlist);
        assert!(netlist.contains("vin (input 0) vsource dc=10"), "missing vin: {}", netlist);
        assert!(netlist.contains("r1 (input output) resistor r=10k"), "missing r1: {}", netlist);
        assert!(netlist.contains("r2 (output 0) resistor r=10k"), "missing r2: {}", netlist);
        assert!(netlist.contains("op1 dc"), "missing op: {}", netlist);
        assert!(!netlist.contains(".end"), "should not have .end: {}", netlist);
    }

    #[test]
    fn test_spectre_mosfet() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "CMOS Inverter".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![
                    Component::Mosfet {
                        name: "1".into(),
                        nd: "out".into(),
                        ng: "in".into(),
                        ns: "vdd".into(),
                        nb: "vdd".into(),
                        model: "pmos_3p3".into(),
                        params: vec![("w".into(), "2u".into()), ("l".into(), "180n".into())],
                    },
                ],
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

        let cg = SpectreCodeGen;
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("m1 (out in vdd vdd) pmos_3p3 w=2u l=180n"), "missing m1: {}", netlist);
    }

    #[test]
    fn test_spectre_waveforms() {
        let sin_comp = Component::VoltageSource {
            name: "sin".into(),
            np: "out".into(),
            nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 },
            ac_magnitude: None,
            ac_phase: None,
            waveform: Some(IrWaveform::Sin {
                offset: 1.65,
                amplitude: 1.65,
                frequency: 1e6,
                delay: 0.0,
                damping: 0.0,
                phase: 0.0,
            }),
        };

        let pulse_comp = Component::VoltageSource {
            name: "pulse".into(),
            np: "out".into(),
            nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 },
            ac_magnitude: None,
            ac_phase: None,
            waveform: Some(IrWaveform::Pulse {
                initial: 0.0,
                pulsed: 3.3,
                delay: 0.0,
                rise_time: 1e-9,
                fall_time: 1e-9,
                pulse_width: 5e-7,
                period: 1e-6,
            }),
        };

        let cg = SpectreCodeGen;

        let sin_str = cg.emit_component(&sin_comp).unwrap();
        assert!(sin_str.contains("type=sine"), "sin: {}", sin_str);
        assert!(sin_str.contains("ampl=1.65"), "sin ampl: {}", sin_str);
        assert!(sin_str.contains("freq=1M"), "sin freq: {}", sin_str);

        let pulse_str = cg.emit_component(&pulse_comp).unwrap();
        assert!(pulse_str.contains("type=pulse"), "pulse: {}", pulse_str);
        assert!(pulse_str.contains("val0=0"), "pulse val0: {}", pulse_str);
        assert!(pulse_str.contains("val1=3.3"), "pulse val1: {}", pulse_str);
    }

    #[test]
    fn test_spectre_all_analyses() {
        let cg = SpectreCodeGen;

        assert_eq!(cg.emit_analysis(&Analysis::Op).unwrap(), "op1 dc");

        let dc = Analysis::Dc {
            sweeps: vec![DcSweep { source: "Vsrc".into(), start: 0.0, stop: 5.0, step: 0.1 }],
        };
        let dc_str = cg.emit_analysis(&dc).unwrap();
        assert!(dc_str.contains("dc1 dc"), "dc: {}", dc_str);
        assert!(dc_str.contains("dev=vsrc param=dc"), "dc selector: {}", dc_str);

        let ac = Analysis::Ac { variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 };
        let ac_str = cg.emit_analysis(&ac).unwrap();
        assert!(ac_str.contains("ac1 ac"), "ac: {}", ac_str);
        assert!(ac_str.contains("dec=100"), "ac dec: {}", ac_str);

        let tran = Analysis::Transient { step: 1e-9, stop: 1e-6, start: None, max_step: None, uic: false };
        let tran_str = cg.emit_analysis(&tran).unwrap();
        assert!(tran_str.contains("tran1 tran"), "tran: {}", tran_str);
        assert!(tran_str.contains("step=1n"), "tran step: {}", tran_str);

        let pss = Analysis::Pss {
            fundamental: 1e6,
            stabilization: 10e-6,
            observe_node: "out".into(),
            points_per_period: 128,
            harmonics: 10,
        };
        let pss_str = cg.emit_analysis(&pss).unwrap();
        assert!(pss_str.contains("pss1 (out 0) pss"), "pss: {}", pss_str);
        assert!(pss_str.contains("fund=1M"), "pss fund: {}", pss_str);
        assert!(!pss_str.contains("ppv="), "pss must not invent ppv: {}", pss_str);
    }

    #[test]
    fn test_spectre_options() {
        let opts = SimOptions {
            portable: vec![
                ("reltol".into(), "1e-3".into()),
                ("max_iterations".into(), "200".into()),
            ],
            backend_specific: HashMap::new(),
        };

        let cg = SpectreCodeGen;
        let s = cg.emit_options(&opts).unwrap();
        assert!(s.contains("myopts options"), "opts header: {}", s);
        assert!(s.contains("reltol=1e-3"), "reltol: {}", s);
        assert!(s.contains("dcmaxiters=200"), "dcmaxiters: {}", s);
    }

    #[test]
    fn test_spectre_testbench_measures_temperature_and_steps() {
        let mut ir = sample_resistor_divider();
        let tb = ir.testbench.as_mut().unwrap();
        tb.temperature = Some(85.0);
        tb.nominal_temperature = Some(27.0);
        tb.measures.push(".meas tran vmax max V(output)".into());
        tb.step_params.push(StepParam {
            param: "rload".into(),
            start: 1e3,
            stop: 10e3,
            step: 1e3,
            sweep_type: None,
        });
        tb.analyses = vec![
            Analysis::Transient {
                step: 1e-9,
                stop: 1e-6,
                start: None,
                max_step: None,
                uic: false,
            },
            Analysis::Op,
        ];

        let cg = SpectreCodeGen;
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("mytemp options temp=85"), "temp: {}", netlist);
        assert!(netlist.contains("mytnom options tnom=27"), "tnom: {}", netlist);
        assert!(
            netlist.contains("simulator lang=spice\n.meas tran vmax max V(output)\nsimulator lang=spectre"),
            "measure must be fenced by language switches: {}", netlist,
        );
        assert!(netlist.contains("sweep1_rload sweep param=rload start=1k stop=10k step=1k {"), "sweep: {}", netlist);
        assert!(netlist.contains("  tran1 tran"), "wrapped tran: {}", netlist);
        assert!(netlist.contains("  op1 dc"), "wrapped op: {}", netlist);
    }

    #[test]
    fn test_spectre_subcircuit() {
        let sc = Subcircuit {
            name: "mybuf".into(),
            ports: vec![
                Port { name: "in".into(), direction: PortDirection::Input },
                Port { name: "out".into(), direction: PortDirection::Output },
            ],
            parameters: vec![
                ParamDef { name: "wp".into(), default: Some("1u".into()) },
            ],
            components: vec![
                Component::Mosfet {
                    name: "p".into(),
                    nd: "out".into(),
                    ng: "in".into(),
                    ns: "vdd".into(),
                    nb: "vdd".into(),
                    model: "pmos".into(),
                    params: vec![("w".into(), "wp".into())],
                },
            ],
            instances: vec![],
            models: vec![],
            raw_spice: vec![],
            includes: vec![],
            libs: vec![],
            osdi_loads: vec![],
            verilog_blocks: vec![],
        };

        let cg = SpectreCodeGen;
        let s = cg.emit_subcircuit(&sc).unwrap();
        assert!(s.contains("subckt mybuf (in out)"), "subckt header: {}", s);
        assert!(s.contains("parameters wp=1u"), "params: {}", s);
        assert!(s.contains("mp (out in vdd vdd) pmos w=wp"), "mosfet: {}", s);
        assert!(s.contains("ends mybuf"), "ends: {}", s);
    }

    #[test]
    fn test_spectre_osdi() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "VA Test".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![],
                instances: vec![],
                models: vec![],
                raw_spice: vec![],
                includes: vec![],
                libs: vec![],
                osdi_loads: vec!["/path/to/model.va".into()],
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: vec![],
            model_libraries: vec![],
        };

        let cg = SpectreCodeGen;
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("ahdl_include \"/path/to/model.va\""), "osdi: {}", netlist);
    }

    #[test]
    fn test_spectre_xspice_rejected() {
        let comp = Component::Xspice {
            name: "1".into(),
            connections: vec!["in".into(), "out".into()],
            model: "d_and".into(),
        };

        let cg = SpectreCodeGen;
        assert!(matches!(
            cg.emit_component(&comp),
            Err(CodeGenError::UnsupportedComponent { .. })
        ));
    }

    #[test]
    fn test_spectre_model_library_uses_spectre_path() {
        let mut backend_paths = HashMap::new();
        backend_paths.insert("ngspice".into(), "/pdk/ngspice/sky130.lib".into());
        backend_paths.insert("spectre".into(), "/pdk/spectre/sky130.scs".into());

        let ir = CircuitIR {
            top: Subcircuit {
                name: "pdk_spectre".into(),
                ports: vec![], parameters: vec![], components: vec![],
                instances: vec![], models: vec![], raw_spice: vec![],
                includes: vec![], libs: vec![], osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: None, subcircuit_defs: vec![],
            model_libraries: vec![ModelLibrary {
                name: "sky130".into(),
                path: "/pdk/default/sky130.lib".into(),
                corner: Some("tt".into()),
                backend_paths,
                setup_includes: vec![],
            }],
        };

        let cg = SpectreCodeGen;
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("include \"/pdk/spectre/sky130.scs\" section=tt"),
            "spectre path: {}", netlist);
        assert!(!netlist.contains("ngspice"), "no ngspice path: {}", netlist);
    }

    #[test]
    fn test_spectre_voltage_source_ac() {
        let cg = SpectreCodeGen;

        let v = Component::VoltageSource {
            name: "in".into(), np: "inp".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 },
            ac_magnitude: Some(1.0), ac_phase: Some(45.0),
            waveform: None,
        };
        let s = cg.emit_component(&v).unwrap();
        assert!(s.contains("mag=1"), "spectre ac mag: {}", s);
        assert!(s.contains("phase=45"), "spectre ac phase: {}", s);

        let v_no_ac = Component::VoltageSource {
            name: "dd".into(), np: "vdd".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 3.3 },
            ac_magnitude: None, ac_phase: None,
            waveform: None,
        };
        let s = cg.emit_component(&v_no_ac).unwrap();
        assert!(!s.contains("mag="), "no mag without ac: {}", s);
    }

    #[test]
    fn test_spectre_scale_factors_are_si() {
        // `[KUN]` Table B.1 vs B.2: SPICE's meg/g/t are not Spectre factors.
        assert_eq!(spectre_num(1e6), "1M");
        assert_eq!(spectre_num(2.4e9), "2.4G");
        assert_eq!(spectre_num(1e12), "1T");
        assert_eq!(spectre_num(1e3), "1k");
        assert_eq!(spectre_num(1e-9), "1n");
        assert_eq!(spectre_num(1e-15), "1f");
    }
}
