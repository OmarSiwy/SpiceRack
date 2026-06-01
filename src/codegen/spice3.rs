use crate::circuit::format_spice_number;
use crate::ir::*;
use super::{CodeGen, CodeGenError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spice3Dialect {
    Ngspice,
    Xyce,
    Ltspice,
}

pub struct Spice3CodeGen {
    pub dialect: Spice3Dialect,
}

impl Spice3CodeGen {
    fn emit_value(&self, v: &IrValue) -> String {
        match v {
            IrValue::Numeric { value } => format_spice_number(*value),
            IrValue::Expression { expr } => format!("{{{}}}", expr),
            IrValue::Raw { text } => text.clone(),
        }
    }

    fn emit_waveform(&self, wf: &IrWaveform) -> String {
        match wf {
            IrWaveform::Sin { offset, amplitude, frequency, delay, damping, phase } => {
                format!("SIN({} {} {} {} {} {})", offset, amplitude, frequency, delay, damping, phase)
            }
            IrWaveform::Pulse { initial, pulsed, delay, rise_time, fall_time, pulse_width, period } => {
                format!("PULSE({} {} {} {} {} {} {})", initial, pulsed, delay, rise_time, fall_time, pulse_width, period)
            }
            IrWaveform::Pwl { values } => {
                let mut s = String::from("PWL(");
                for (i, (t, v)) in values.iter().enumerate() {
                    if i > 0 {
                        s.push(' ');
                    }
                    s.push_str(&format!("{} {}", t, v));
                }
                s.push(')');
                s
            }
            IrWaveform::Exp { initial, pulsed, rise_delay, rise_tau, fall_delay, fall_tau } => {
                format!("EXP({} {} {} {} {} {})", initial, pulsed, rise_delay, rise_tau, fall_delay, fall_tau)
            }
            IrWaveform::Sffm { offset, amplitude, carrier_freq, modulation_index, signal_freq } => {
                format!("SFFM({} {} {} {} {})", offset, amplitude, carrier_freq, modulation_index, signal_freq)
            }
            IrWaveform::Am { amplitude, offset, modulating_freq, carrier_freq, delay } => {
                format!("AM({} {} {} {} {})", amplitude, offset, modulating_freq, carrier_freq, delay)
            }
        }
    }

    fn emit_params(&self, params: &[(String, String)]) -> String {
        let mut s = String::new();
        for (k, v) in params {
            s.push_str(&format!(" {}={}", k, v));
        }
        s
    }

    fn emit_model(&self, m: &ModelDef) -> String {
        let mut s = format!(".model {} {}", m.name, m.kind);
        if !m.parameters.is_empty() {
            s.push('(');
            for (i, (k, v)) in m.parameters.iter().enumerate() {
                if i > 0 {
                    s.push(' ');
                }
                s.push_str(&format!("{}={}", k, v));
            }
            s.push(')');
        }
        s
    }

    fn emit_instance(&self, inst: &Instance) -> String {
        let mut s = format!("X{}", inst.name);
        for port in &inst.port_mapping {
            s.push_str(&format!(" {}", port));
        }
        s.push_str(&format!(" {}", inst.subcircuit));
        s.push_str(&self.emit_params(&inst.parameters));
        s
    }

    #[allow(dead_code)]
    fn hierarchy_separator(&self) -> &str {
        match self.dialect {
            Spice3Dialect::Xyce => ":",
            _ => ".",
        }
    }

    fn map_option_name(&self, canonical: &str) -> String {
        match self.dialect {
            Spice3Dialect::Ngspice => match canonical {
                "reltol" => "reltol".into(),
                "abstol" => "abstol".into(),
                "vntol" => "vntol".into(),
                "gmin" => "gmin".into(),
                "max_iterations" => "ITL1".into(),
                other => other.into(),
            },
            Spice3Dialect::Xyce => match canonical {
                "reltol" => "RELTOL".into(),
                "abstol" => "ABSTOL".into(),
                "vntol" => "VNTOL".into(),
                "gmin" => "gmin".into(),
                "max_iterations" => "NONLIN-MAXSTEP".into(),
                other => other.into(),
            },
            Spice3Dialect::Ltspice => match canonical {
                "reltol" => "reltol".into(),
                "abstol" => "abstol".into(),
                "vntol" => "vntol".into(),
                "gmin" => "gmin".into(),
                "max_iterations" => "ITL1".into(),
                other => other.into(),
            },
        }
    }

    fn emit_subcircuit_body(&self, sc: &Subcircuit) -> Result<String, CodeGenError> {
        let mut lines = Vec::new();

        // Models
        for m in &sc.models {
            lines.push(self.emit_model(m));
        }

        // Components
        for comp in &sc.components {
            lines.push(self.emit_component(comp)?);
        }

        // Instances
        for inst in &sc.instances {
            lines.push(self.emit_instance(inst));
        }

        // Raw SPICE lines
        for raw in &sc.raw_spice {
            lines.push(raw.clone());
        }

        Ok(lines.join("\n"))
    }

    fn emit_step_param(&self, sp: &StepParam) -> String {
        match self.dialect {
            Spice3Dialect::Ngspice => {
                // ngspice doesn't have native .step; omit for now (would need .control block)
                format!("* .step param {} {} {} {}", sp.param, sp.start, sp.stop, sp.step)
            }
            Spice3Dialect::Xyce | Spice3Dialect::Ltspice => {
                if let Some(ref sweep) = sp.sweep_type {
                    format!(".step {} {} {} {} {}", sweep, sp.param, sp.start, sp.stop, sp.step)
                } else {
                    format!(".step param {} {} {} {}", sp.param, sp.start, sp.stop, sp.step)
                }
            }
        }
    }

    fn emit_measure(&self, meas: &str) -> String {
        if meas.starts_with(".meas") || meas.starts_with(".measure") {
            meas.to_string()
        } else {
            format!(".meas {}", meas)
        }
    }
}

impl CodeGen for Spice3CodeGen {
    fn backend_name(&self) -> &str {
        match self.dialect {
            Spice3Dialect::Ngspice => "ngspice",
            Spice3Dialect::Xyce => "xyce",
            Spice3Dialect::Ltspice => "ltspice",
        }
    }

    fn emit_netlist(&self, ir: &CircuitIR) -> Result<String, CodeGenError> {
        let mut lines = Vec::new();

        // Title must be first line (SPICE treats line 1 as title/comment)
        lines.push(format!("* {}", ir.top.name));

        // OSDI loads (ngspice only — must precede component references)
        // pre_osdi is a control command, not a dot command
        if self.dialect == Spice3Dialect::Ngspice && !ir.top.osdi_loads.is_empty() {
            lines.push(".control".into());
            for path in &ir.top.osdi_loads {
                lines.push(format!("pre_osdi {}", path));
            }
            lines.push(".endc".into());
        }

        // Includes
        for inc in &ir.top.includes {
            lines.push(format!(".include {}", inc));
        }

        // Libs
        for (path, section) in &ir.top.libs {
            lines.push(format!(".lib {} {}", path, section));
        }

        // Model libraries
        for lib in &ir.model_libraries {
            for setup in &lib.setup_includes {
                lines.push(format!(".include {}", setup));
            }
            let path = lib.backend_paths
                .get(self.backend_name())
                .unwrap_or(&lib.path);
            if let Some(ref corner) = lib.corner {
                lines.push(format!(".lib {} {}", path, corner));
            } else {
                lines.push(format!(".include {}", path));
            }
        }

        // Parameters
        for p in &ir.top.parameters {
            if let Some(ref default) = p.default {
                lines.push(format!(".param {}={}", p.name, default));
            }
        }

        // Models
        for m in &ir.top.models {
            lines.push(self.emit_model(m));
        }

        // Subcircuit definitions
        for sc in &ir.subcircuit_defs {
            lines.push(String::new());
            lines.push(self.emit_subcircuit(sc)?);
        }

        // Components
        for comp in &ir.top.components {
            lines.push(self.emit_component(comp)?);
        }

        // Instances
        for inst in &ir.top.instances {
            lines.push(self.emit_instance(inst));
        }

        // Raw SPICE lines
        for raw in &ir.top.raw_spice {
            lines.push(raw.clone());
        }

        // Testbench
        if let Some(ref tb) = ir.testbench {
            // Stimulus components
            for comp in &tb.stimulus {
                lines.push(self.emit_component(comp)?);
            }

            // Options
            let opts = self.emit_options(&tb.options)?;
            if !opts.is_empty() {
                lines.push(opts);
            }

            // Temperature
            if let Some(temp) = tb.temperature {
                lines.push(format!(".temp {}", temp));
            }
            if let Some(tnom) = tb.nominal_temperature {
                lines.push(format!(".options tnom={}", tnom));
            }

            // Initial conditions
            for (node, val) in &tb.initial_conditions {
                lines.push(format!(".ic V({})={}", node, val));
            }

            // Node sets
            for (node, val) in &tb.node_sets {
                lines.push(format!(".nodeset V({})={}", node, val));
            }

            // Saves — Xyce uses .PRINT, ngspice/LTspice use .save
            for save in &tb.saves {
                match self.dialect {
                    Spice3Dialect::Xyce => lines.push(format!(".PRINT DC {}", save)),
                    _ => lines.push(format!(".save {}", save)),
                }
            }

            // Measures
            for meas in &tb.measures {
                lines.push(self.emit_measure(meas));
            }

            // Step params
            for sp in &tb.step_params {
                lines.push(self.emit_step_param(sp));
            }

            // Extra lines
            for line in &tb.extra_lines {
                lines.push(line.clone());
            }

            // Analyses
            for analysis in &tb.analyses {
                lines.push(self.emit_analysis(analysis)?);
            }
        }

        lines.push(".end".into());

        Ok(lines.join("\n"))
    }

    fn emit_subcircuit(&self, sc: &Subcircuit) -> Result<String, CodeGenError> {
        let mut header = format!(".subckt {}", sc.name);
        for port in &sc.ports {
            header.push_str(&format!(" {}", port.name));
        }
        if !sc.parameters.is_empty() {
            header.push_str(" PARAMS:");
            for p in &sc.parameters {
                if let Some(ref default) = p.default {
                    header.push_str(&format!(" {}={}", p.name, default));
                } else {
                    header.push_str(&format!(" {}", p.name));
                }
            }
        }

        let body = self.emit_subcircuit_body(sc)?;

        Ok(format!("{}\n{}\n.ends {}", header, body, sc.name))
    }

    fn emit_component(&self, comp: &Component) -> Result<String, CodeGenError> {
        let s = match comp {
            Component::Resistor { name, n1, n2, value, params } => {
                let mut s = format!("R{} {} {} {}", name, n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Capacitor { name, n1, n2, value, params } => {
                let mut s = format!("C{} {} {} {}", name, n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Inductor { name, n1, n2, value, params } => {
                let mut s = format!("L{} {} {} {}", name, n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            Component::MutualInductor { name, inductor1, inductor2, coupling } => {
                format!("K{} L{} L{} {}", name, inductor1, inductor2, coupling)
            }
            Component::VoltageSource { name, np, nm, value, ac_magnitude, ac_phase, waveform } => {
                let mut s = format!("V{} {} {} {}", name, np, nm, self.emit_value(value));
                if let Some(mag) = ac_magnitude {
                    s.push_str(&format!(" AC {}", mag));
                    if let Some(phase) = ac_phase {
                        s.push_str(&format!(" {}", phase));
                    }
                }
                if let Some(wf) = waveform {
                    s.push_str(&format!(" {}", self.emit_waveform(wf)));
                }
                s
            }
            Component::CurrentSource { name, np, nm, value, ac_magnitude, ac_phase, waveform } => {
                let mut s = format!("I{} {} {} {}", name, np, nm, self.emit_value(value));
                if let Some(mag) = ac_magnitude {
                    s.push_str(&format!(" AC {}", mag));
                    if let Some(phase) = ac_phase {
                        s.push_str(&format!(" {}", phase));
                    }
                }
                if let Some(wf) = waveform {
                    s.push_str(&format!(" {}", self.emit_waveform(wf)));
                }
                s
            }
            Component::BehavioralVoltage { name, np, nm, expression } => {
                format!("B{} {} {} V={}", name, np, nm, expression)
            }
            Component::BehavioralCurrent { name, np, nm, expression } => {
                format!("B{} {} {} I={}", name, np, nm, expression)
            }
            Component::Vcvs { name, np, nm, ncp, ncm, gain } => {
                format!("E{} {} {} {} {} {}", name, np, nm, ncp, ncm, gain)
            }
            Component::Vccs { name, np, nm, ncp, ncm, transconductance } => {
                format!("G{} {} {} {} {} {}", name, np, nm, ncp, ncm, transconductance)
            }
            Component::Cccs { name, np, nm, vsense, gain } => {
                format!("F{} {} {} {} {}", name, np, nm, vsense, gain)
            }
            Component::Ccvs { name, np, nm, vsense, transresistance } => {
                format!("H{} {} {} {} {}", name, np, nm, vsense, transresistance)
            }
            Component::Diode { name, np, nm, model, params } => {
                let mut s = format!("D{} {} {} {}", name, np, nm, model);
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Bjt { name, nc, nb, ne, model, params } => {
                let mut s = format!("Q{} {} {} {} {}", name, nc, nb, ne, model);
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Mosfet { name, nd, ng, ns, nb, model, params } => {
                let mut s = format!("M{} {} {} {} {} {}", name, nd, ng, ns, nb, model);
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Jfet { name, nd, ng, ns, model, params } => {
                let mut s = format!("J{} {} {} {} {}", name, nd, ng, ns, model);
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Mesfet { name, nd, ng, ns, model, params } => {
                let mut s = format!("Z{} {} {} {} {}", name, nd, ng, ns, model);
                s.push_str(&self.emit_params(params));
                s
            }
            Component::VSwitch { name, np, nm, ncp, ncm, model } => {
                format!("S{} {} {} {} {} {}", name, np, nm, ncp, ncm, model)
            }
            Component::ISwitch { name, np, nm, vcontrol, model } => {
                format!("W{} {} {} {} {}", name, np, nm, vcontrol, model)
            }
            Component::TLine { name, inp, inm, outp, outm, z0, td } => {
                format!("T{} {} {} {} {} Z0={} TD={}", name, inp, inm, outp, outm, z0, td)
            }
            Component::Xspice { name, connections, model } => {
                match self.dialect {
                    Spice3Dialect::Ngspice => {
                        let mut s = format!("A{}", name);
                        for conn in connections {
                            s.push_str(&format!(" {}", conn));
                        }
                        s.push_str(&format!(" {}", model));
                        s
                    }
                    _ => {
                        // XSPICE not supported on Xyce/LTspice -- emit as comment
                        let mut s = format!("* XSPICE (unsupported): A{}", name);
                        for conn in connections {
                            s.push_str(&format!(" {}", conn));
                        }
                        s.push_str(&format!(" {}", model));
                        s
                    }
                }
            }
            Component::RawSpice { line } => {
                line.clone()
            }
        };
        Ok(s)
    }

    fn emit_analysis(&self, analysis: &Analysis) -> Result<String, CodeGenError> {
        let s = match analysis {
            Analysis::Op => ".op".into(),
            Analysis::Dc { sweeps } => {
                let mut s = String::from(".dc");
                for sw in sweeps {
                    s.push_str(&format!(
                        " {} {} {} {}",
                        sw.source,
                        format_spice_number(sw.start),
                        format_spice_number(sw.stop),
                        format_spice_number(sw.step),
                    ));
                }
                s
            }
            Analysis::Ac { variation, points, start, stop } => {
                format!(
                    ".ac {} {} {} {}",
                    variation,
                    points,
                    format_spice_number(*start),
                    format_spice_number(*stop),
                )
            }
            Analysis::Transient { step, stop, start, max_step, uic } => {
                let mut s = format!(
                    ".tran {} {}",
                    format_spice_number(*step),
                    format_spice_number(*stop),
                );
                if let Some(st) = start {
                    s.push_str(&format!(" {}", format_spice_number(*st)));
                }
                if let Some(ms) = max_step {
                    if start.is_none() {
                        // Need a placeholder for start when max_step is given
                        s.push_str(" 0");
                    }
                    s.push_str(&format!(" {}", format_spice_number(*ms)));
                }
                if *uic {
                    s.push_str(" UIC");
                }
                s
            }
            Analysis::Noise { output, reference, source, variation, points, start, stop, points_per_summary } => {
                let out_spec = if reference.is_empty() || reference == "0" {
                    format!("V({})", output)
                } else {
                    format!("V({},{})", output, reference)
                };
                let mut s = format!(
                    ".noise {} {} {} {} {} {}",
                    out_spec,
                    source,
                    variation,
                    points,
                    format_spice_number(*start),
                    format_spice_number(*stop),
                );
                if let Some(pps) = points_per_summary {
                    s.push_str(&format!(" {}", pps));
                }
                s
            }
            Analysis::Tf { output, source } => {
                format!(".tf {} {}", output, source)
            }
            Analysis::Sensitivity { output, ac } => {
                let mut s = format!(".sens {}", output);
                if let Some(ac_params) = ac {
                    s.push_str(&format!(
                        " AC {} {} {} {}",
                        ac_params.variation,
                        ac_params.points,
                        format_spice_number(ac_params.start),
                        format_spice_number(ac_params.stop),
                    ));
                }
                s
            }
            Analysis::PoleZero { node1, node2, node3, node4, tf_type, pz_type } => {
                if self.dialect != Spice3Dialect::Ngspice {
                    return Err(CodeGenError::UnsupportedAnalysis {
                        backend: self.backend_name().into(),
                        analysis: "PoleZero".into(),
                    });
                }
                format!(".pz {} {} {} {} {} {}", node1, node2, node3, node4, tf_type, pz_type)
            }
            Analysis::Distortion { variation, points, start, stop, f2overf1 } => {
                if self.dialect != Spice3Dialect::Ngspice {
                    return Err(CodeGenError::UnsupportedAnalysis {
                        backend: self.backend_name().into(),
                        analysis: "Distortion".into(),
                    });
                }
                let mut s = format!(
                    ".disto {} {} {} {}",
                    variation,
                    points,
                    format_spice_number(*start),
                    format_spice_number(*stop),
                );
                if let Some(ratio) = f2overf1 {
                    s.push_str(&format!(" {}", ratio));
                }
                s
            }
            Analysis::Fourier { fundamental, outputs, num_harmonics } => {
                let mut s = format!(".four {}", format_spice_number(*fundamental));
                if let Some(nh) = num_harmonics {
                    s.push_str(&format!(" {}", nh));
                }
                for out in outputs {
                    s.push_str(&format!(" {}", out));
                }
                s
            }
            // Vendor-specific: Xyce analyses
            Analysis::XyceSampling { num_samples, distributions } => {
                match self.dialect {
                    Spice3Dialect::Xyce => {
                        let mut s = format!(".SAMPLING\n+ param = {}", num_samples);
                        for (param, dist) in distributions {
                            s.push_str(&format!("\n+ {}={}", param, dist));
                        }
                        s
                    }
                    _ => return Err(CodeGenError::UnsupportedAnalysis {
                        backend: self.backend_name().into(),
                        analysis: "XyceSampling".into(),
                    }),
                }
            }
            Analysis::XyceEmbeddedSampling { num_samples, distributions } => {
                match self.dialect {
                    Spice3Dialect::Xyce => {
                        let mut s = format!(".EMBEDDEDSAMPLING\n+ param = {}", num_samples);
                        for (param, dist) in distributions {
                            s.push_str(&format!("\n+ {}={}", param, dist));
                        }
                        s
                    }
                    _ => return Err(CodeGenError::UnsupportedAnalysis {
                        backend: self.backend_name().into(),
                        analysis: "XyceEmbeddedSampling".into(),
                    }),
                }
            }
            Analysis::XycePce { num_samples, distributions, order } => {
                match self.dialect {
                    Spice3Dialect::Xyce => {
                        let mut s = format!(".PCE\n+ param = {}\n+ order = {}", num_samples, order);
                        for (param, dist) in distributions {
                            s.push_str(&format!("\n+ {}={}", param, dist));
                        }
                        s
                    }
                    _ => return Err(CodeGenError::UnsupportedAnalysis {
                        backend: self.backend_name().into(),
                        analysis: "XycePce".into(),
                    }),
                }
            }
            Analysis::XyceFft { signal, np, start, stop, window, format: fmt } => {
                match self.dialect {
                    Spice3Dialect::Xyce => {
                        format!(
                            ".FFT {} NP={} START={} STOP={} WINDOW={} FORMAT={}",
                            signal, np,
                            format_spice_number(*start),
                            format_spice_number(*stop),
                            window, fmt,
                        )
                    }
                    _ => return Err(CodeGenError::UnsupportedAnalysis {
                        backend: self.backend_name().into(),
                        analysis: "XyceFft".into(),
                    }),
                }
            }
            // Spectre-only analyses are not emittable in SPICE3
            Analysis::Pss { .. }
            | Analysis::HarmonicBalance { .. }
            | Analysis::SPar { .. }
            | Analysis::Stability { .. }
            | Analysis::TransientNoise { .. }
            | Analysis::SpectreSweep { .. }
            | Analysis::SpectreMonteCarlo { .. }
            | Analysis::SpectrePac { .. }
            | Analysis::SpectrePnoise { .. }
            | Analysis::SpectrePxf { .. }
            | Analysis::SpectrePstb { .. } => {
                return Err(CodeGenError::UnsupportedAnalysis {
                    backend: self.backend_name().into(),
                    analysis: format!("{:?}", analysis).split_whitespace().next().unwrap_or("unknown").into(),
                });
            }
        };
        Ok(s)
    }

    fn emit_options(&self, opts: &SimOptions) -> Result<String, CodeGenError> {
        let mut parts = Vec::new();

        // Portable options
        for (key, val) in &opts.portable {
            let mapped = self.map_option_name(key);
            parts.push(format!("{}={}", mapped, val));
        }

        // Backend-specific options
        let backend = self.backend_name();
        if let Some(specific) = opts.backend_specific.get(backend) {
            for (key, val) in specific {
                parts.push(format!("{}={}", key, val));
            }
        }

        if parts.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!(".options {}", parts.join(" ")))
        }
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
    fn test_ngspice_resistor_divider() {
        let ir = sample_resistor_divider();
        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("* Voltage Divider"), "missing title: {}", netlist);
        assert!(netlist.contains("Vin input 0 10"), "missing Vin: {}", netlist);
        assert!(netlist.contains("R1 input output 10k"), "missing R1: {}", netlist);
        assert!(netlist.contains("R2 output 0 10k"), "missing R2: {}", netlist);
        assert!(netlist.contains(".op"), "missing .op: {}", netlist);
        assert!(netlist.contains(".end"), "missing .end: {}", netlist);
    }

    #[test]
    fn test_xyce_resistor_divider() {
        let ir = sample_resistor_divider();
        let cg = Spice3CodeGen { dialect: Spice3Dialect::Xyce };
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("* Voltage Divider"));
        assert!(netlist.contains("Vin input 0 10"));
        assert!(netlist.contains(".op"));
        assert!(netlist.contains(".end"));
        // Xyce should NOT have .pre_osdi
        assert!(!netlist.contains(".pre_osdi"));
    }

    #[test]
    fn test_mosfet_circuit() {
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
                        params: vec![("W".into(), "2u".into()), ("L".into(), "180n".into())],
                    },
                    Component::Mosfet {
                        name: "2".into(),
                        nd: "out".into(),
                        ng: "in".into(),
                        ns: "0".into(),
                        nb: "0".into(),
                        model: "nmos_3p3".into(),
                        params: vec![("W".into(), "1u".into()), ("L".into(), "180n".into())],
                    },
                    Component::VoltageSource {
                        name: "dd".into(),
                        np: "vdd".into(),
                        nm: "0".into(),
                        value: IrValue::Numeric { value: 3.3 },
                        ac_magnitude: None,
                        ac_phase: None,
                        waveform: None,
                    },
                ],
                instances: vec![],
                models: vec![
                    ModelDef {
                        name: "nmos_3p3".into(),
                        kind: "NMOS".into(),
                        parameters: vec![("VTO".into(), "0.7".into()), ("KP".into(), "110e-6".into())],
                    },
                ],
                raw_spice: vec![],
                includes: vec![],
                libs: vec![],
                osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "CMOS Inverter".into(),
                stimulus: vec![],
                analyses: vec![
                    Analysis::Dc { sweeps: vec![DcSweep { source: "Vdd".into(), start: 0.0, stop: 3.3, step: 0.01 }] },
                ],
                options: SimOptions::default(),
                saves: vec![],
                measures: vec![],
                temperature: Some(27.0),
                nominal_temperature: None,
                initial_conditions: vec![],
                node_sets: vec![],
                step_params: vec![],
                extra_lines: vec![],
            }),
            subcircuit_defs: vec![],
            model_libraries: vec![],
        };

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("M1 out in vdd vdd pmos_3p3 W=2u L=180n"), "missing M1: {}", netlist);
        assert!(netlist.contains("M2 out in 0 0 nmos_3p3 W=1u L=180n"), "missing M2: {}", netlist);
        assert!(netlist.contains(".model nmos_3p3 NMOS(VTO=0.7 KP=110e-6)"), "missing model: {}", netlist);
        assert!(netlist.contains(".temp 27"), "missing temp: {}", netlist);
        assert!(netlist.contains(".dc Vdd 0 3"), "missing dc: {}", netlist);
    }

    #[test]
    fn test_subcircuit_instances() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "Top".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![
                    Component::VoltageSource {
                        name: "dd".into(),
                        np: "vdd".into(),
                        nm: "0".into(),
                        value: IrValue::Numeric { value: 1.8 },
                        ac_magnitude: None,
                        ac_phase: None,
                        waveform: None,
                    },
                ],
                instances: vec![
                    Instance {
                        name: "1".into(),
                        subcircuit: "inverter".into(),
                        port_mapping: vec!["in".into(), "out".into(), "vdd".into(), "0".into()],
                        parameters: vec![("wp".into(), "2u".into())],
                    },
                ],
                models: vec![],
                raw_spice: vec![],
                includes: vec![],
                libs: vec![],
                osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: vec![
                Subcircuit {
                    name: "inverter".into(),
                    ports: vec![
                        Port { name: "in".into(), direction: PortDirection::Input },
                        Port { name: "out".into(), direction: PortDirection::Output },
                        Port { name: "vdd".into(), direction: PortDirection::InOut },
                        Port { name: "gnd".into(), direction: PortDirection::InOut },
                    ],
                    parameters: vec![
                        ParamDef { name: "wp".into(), default: Some("1u".into()) },
                        ParamDef { name: "wn".into(), default: Some("500n".into()) },
                    ],
                    components: vec![
                        Component::Mosfet {
                            name: "p".into(),
                            nd: "out".into(),
                            ng: "in".into(),
                            ns: "vdd".into(),
                            nb: "vdd".into(),
                            model: "pmos".into(),
                            params: vec![("W".into(), "{wp}".into()), ("L".into(), "180n".into())],
                        },
                        Component::Mosfet {
                            name: "n".into(),
                            nd: "out".into(),
                            ng: "in".into(),
                            ns: "gnd".into(),
                            nb: "gnd".into(),
                            model: "nmos".into(),
                            params: vec![("W".into(), "{wn}".into()), ("L".into(), "180n".into())],
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
            ],
            model_libraries: vec![],
        };

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains(".subckt inverter in out vdd gnd PARAMS: wp=1u wn=500n"), "missing subckt header: {}", netlist);
        assert!(netlist.contains("Mp out in vdd vdd pmos W={wp} L=180n"), "missing Mp: {}", netlist);
        assert!(netlist.contains(".ends inverter"), "missing .ends: {}", netlist);
        assert!(netlist.contains("X1 in out vdd 0 inverter wp=2u"), "missing instance: {}", netlist);
    }

    #[test]
    fn test_waveforms_sin_pulse_pwl() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "Waveforms".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![
                    Component::VoltageSource {
                        name: "sin".into(),
                        np: "sin_out".into(),
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
                    },
                    Component::VoltageSource {
                        name: "pulse".into(),
                        np: "pulse_out".into(),
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
                    },
                    Component::VoltageSource {
                        name: "pwl".into(),
                        np: "pwl_out".into(),
                        nm: "0".into(),
                        value: IrValue::Numeric { value: 0.0 },
                        ac_magnitude: None,
                        ac_phase: None,
                        waveform: Some(IrWaveform::Pwl {
                            values: vec![(0.0, 0.0), (1e-6, 3.3), (2e-6, 0.0)],
                        }),
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

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("SIN(1.65 1.65 1000000"), "missing SIN: {}", netlist);
        assert!(netlist.contains("PULSE(0 3.3 0"), "missing PULSE: {}", netlist);
        assert!(netlist.contains("PWL(0 0"), "missing PWL: {}", netlist);
    }

    #[test]
    fn test_all_analysis_types() {
        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };

        // Op
        assert_eq!(cg.emit_analysis(&Analysis::Op).unwrap(), ".op");

        // DC
        let dc = Analysis::Dc {
            sweeps: vec![DcSweep { source: "V1".into(), start: 0.0, stop: 5.0, step: 0.1 }],
        };
        let dc_str = cg.emit_analysis(&dc).unwrap();
        assert!(dc_str.starts_with(".dc V1"), "dc: {}", dc_str);

        // AC
        let ac = Analysis::Ac { variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 };
        let ac_str = cg.emit_analysis(&ac).unwrap();
        assert!(ac_str.starts_with(".ac dec 100"), "ac: {}", ac_str);

        // Transient
        let tran = Analysis::Transient { step: 1e-9, stop: 1e-6, start: None, max_step: None, uic: false };
        let tran_str = cg.emit_analysis(&tran).unwrap();
        assert!(tran_str.starts_with(".tran 1n"), "tran: {}", tran_str);

        // Transient with UIC
        let tran_uic = Analysis::Transient { step: 1e-9, stop: 1e-6, start: None, max_step: None, uic: true };
        let tran_uic_str = cg.emit_analysis(&tran_uic).unwrap();
        assert!(tran_uic_str.contains("UIC"), "tran_uic: {}", tran_uic_str);

        // Noise
        let noise = Analysis::Noise {
            output: "out".into(),
            reference: "0".into(),
            source: "V1".into(),
            variation: "dec".into(),
            points: 10,
            start: 1.0,
            stop: 1e6,
            points_per_summary: None,
        };
        let noise_str = cg.emit_analysis(&noise).unwrap();
        assert!(noise_str.starts_with(".noise V(out) V1 dec"), "noise: {}", noise_str);

        // TF
        let tf = Analysis::Tf { output: "V(out)".into(), source: "Vin".into() };
        assert_eq!(cg.emit_analysis(&tf).unwrap(), ".tf V(out) Vin");

        // Sensitivity
        let sens = Analysis::Sensitivity { output: "V(out)".into(), ac: None };
        assert_eq!(cg.emit_analysis(&sens).unwrap(), ".sens V(out)");

        // PoleZero
        let pz = Analysis::PoleZero {
            node1: "1".into(), node2: "0".into(), node3: "3".into(), node4: "0".into(),
            tf_type: "vol".into(), pz_type: "pz".into(),
        };
        assert_eq!(cg.emit_analysis(&pz).unwrap(), ".pz 1 0 3 0 vol pz");

        // Distortion
        let disto = Analysis::Distortion {
            variation: "dec".into(), points: 10, start: 1e3, stop: 1e6, f2overf1: None,
        };
        let disto_str = cg.emit_analysis(&disto).unwrap();
        assert!(disto_str.starts_with(".disto dec 10"), "disto: {}", disto_str);

        // Fourier
        let four = Analysis::Fourier {
            fundamental: 1e3,
            outputs: vec!["V(out)".into()],
            num_harmonics: Some(10),
        };
        let four_str = cg.emit_analysis(&four).unwrap();
        assert!(four_str.contains(".four 1k 10 V(out)"), "four: {}", four_str);
    }

    #[test]
    fn test_options_ngspice_vs_xyce() {
        let opts = SimOptions {
            portable: vec![
                ("reltol".into(), "1e-3".into()),
                ("max_iterations".into(), "200".into()),
            ],
            backend_specific: HashMap::new(),
        };

        // Ngspice
        let cg_ng = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let ng_str = cg_ng.emit_options(&opts).unwrap();
        assert!(ng_str.contains("reltol=1e-3"), "ng reltol: {}", ng_str);
        assert!(ng_str.contains("ITL1=200"), "ng ITL1: {}", ng_str);

        // Xyce
        let cg_xy = Spice3CodeGen { dialect: Spice3Dialect::Xyce };
        let xy_str = cg_xy.emit_options(&opts).unwrap();
        assert!(xy_str.contains("RELTOL=1e-3"), "xy RELTOL: {}", xy_str);
        assert!(xy_str.contains("NONLIN-MAXSTEP=200"), "xy NONLIN-MAXSTEP: {}", xy_str);
    }

    #[test]
    fn test_options_backend_specific() {
        let mut backend_specific = HashMap::new();
        backend_specific.insert("ngspice".into(), vec![("SEED".into(), "42".into())]);

        let opts = SimOptions {
            portable: vec![("reltol".into(), "1e-4".into())],
            backend_specific,
        };

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let s = cg.emit_options(&opts).unwrap();
        assert!(s.contains("reltol=1e-4"), "opts: {}", s);
        assert!(s.contains("SEED=42"), "opts: {}", s);
    }

    #[test]
    fn test_osdi_ngspice_only() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "OSDI Test".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![],
                instances: vec![],
                models: vec![],
                raw_spice: vec![],
                includes: vec![],
                libs: vec![],
                osdi_loads: vec!["/path/to/model.osdi".into()],
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: vec![],
            model_libraries: vec![],
        };

        // ngspice wraps OSDI loads in .control block
        let cg_ng = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let ng_str = cg_ng.emit_netlist(&ir).unwrap();
        assert!(ng_str.contains(".control\npre_osdi /path/to/model.osdi\n.endc"),
            "ng osdi control block: {}", ng_str);
        assert!(!ng_str.contains(".pre_osdi"), "no dot-command form: {}", ng_str);

        // Xyce does not
        let cg_xy = Spice3CodeGen { dialect: Spice3Dialect::Xyce };
        let xy_str = cg_xy.emit_netlist(&ir).unwrap();
        assert!(!xy_str.contains("pre_osdi"), "xy should not have osdi: {}", xy_str);
    }

    #[test]
    fn test_xspice_ngspice_vs_others() {
        let comp = Component::Xspice {
            name: "1".into(),
            connections: vec!["[in1 in2]".into(), "out".into()],
            model: "d_and".into(),
        };

        let cg_ng = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let ng_str = cg_ng.emit_component(&comp).unwrap();
        assert!(ng_str.starts_with("A1 "), "ng xspice: {}", ng_str);
        assert!(ng_str.contains("d_and"), "ng xspice model: {}", ng_str);

        let cg_xy = Spice3CodeGen { dialect: Spice3Dialect::Xyce };
        let xy_str = cg_xy.emit_component(&comp).unwrap();
        assert!(xy_str.starts_with("* XSPICE"), "xy xspice should be comment: {}", xy_str);
    }

    #[test]
    fn test_step_params_xyce() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "Step Test".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![
                    Component::Resistor {
                        name: "1".into(),
                        n1: "a".into(),
                        n2: "0".into(),
                        value: IrValue::Expression { expr: "rval".into() },
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
                dut: "Step Test".into(),
                stimulus: vec![],
                analyses: vec![Analysis::Op],
                options: SimOptions::default(),
                saves: vec![],
                measures: vec![],
                temperature: None,
                nominal_temperature: None,
                initial_conditions: vec![],
                node_sets: vec![],
                step_params: vec![StepParam {
                    param: "rval".into(),
                    start: 1000.0,
                    stop: 10000.0,
                    step: 1000.0,
                    sweep_type: None,
                }],
                extra_lines: vec![],
            }),
            subcircuit_defs: vec![],
            model_libraries: vec![],
        };

        // Xyce emits native .step
        let cg = Spice3CodeGen { dialect: Spice3Dialect::Xyce };
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains(".step param rval 1000 10000 1000"), "step: {}", netlist);
    }

    #[test]
    fn test_spice3_testbench_measure_and_nominal_temperature() {
        let mut ir = sample_resistor_divider();
        let tb = ir.testbench.as_mut().unwrap();
        tb.nominal_temperature = Some(27.0);
        tb.measures.push("tran vmax MAX V(output)".into());

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains(".options tnom=27"), "tnom: {}", netlist);
        assert!(netlist.contains(".meas tran vmax MAX V(output)"), "measure: {}", netlist);
    }

    #[test]
    fn test_roundtrip_from_circuit() {
        use crate::circuit::{Circuit, Param};

        let mut c = Circuit::new("Roundtrip");
        c.r("1", "in", "out", 1000.0);
        c.v("dd", "vdd", "0", 3.3);
        c.model("nmod", "NMOS", vec![Param::new("VTO", "0.7")]);

        let ir = CircuitIR::from_circuit(&c);
        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let emitted = cg.emit_netlist(&ir).unwrap();

        // Compare key lines against Circuit::to_string
        let original = c.to_string();
        assert!(emitted.contains("R1 in out 1k"), "emitted R1: {}", emitted);
        assert!(original.contains("R1 in out 1k"), "original R1: {}", original);
        assert!(emitted.contains(".model nmod NMOS(VTO=0.7)"), "emitted model: {}", emitted);
        assert!(original.contains(".model nmod NMOS(VTO=0.7)"), "original model: {}", original);
    }

    // ── Issue 03: ModelLibrary per-backend path + corner ──

    #[test]
    fn test_model_library_backend_path_ngspice() {
        let mut backend_paths = HashMap::new();
        backend_paths.insert("ngspice".into(), "/pdk/ngspice/sky130.lib".into());
        backend_paths.insert("spectre".into(), "/pdk/spectre/sky130.scs".into());

        let ir = CircuitIR {
            top: Subcircuit {
                name: "pdk_test".into(),
                ports: vec![], parameters: vec![], components: vec![],
                instances: vec![], models: vec![], raw_spice: vec![],
                includes: vec![], libs: vec![], osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "pdk_test".into(), stimulus: vec![],
                analyses: vec![Analysis::Op],
                options: SimOptions::default(), saves: vec![],
                measures: vec![], temperature: None, nominal_temperature: None,
                initial_conditions: vec![], node_sets: vec![],
                step_params: vec![], extra_lines: vec![],
            }),
            subcircuit_defs: vec![],
            model_libraries: vec![ModelLibrary {
                name: "sky130".into(),
                path: "/pdk/default/sky130.lib".into(),
                corner: Some("tt".into()),
                backend_paths,
                setup_includes: vec![],
            }],
        };

        // Ngspice selects its path
        let ng = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let ng_netlist = ng.emit_netlist(&ir).unwrap();
        assert!(ng_netlist.contains(".lib /pdk/ngspice/sky130.lib tt"),
            "ngspice path: {}", ng_netlist);
        assert!(!ng_netlist.contains("spectre"), "no spectre path in ngspice: {}", ng_netlist);

        // Xyce falls back to default path (no xyce key)
        let xy = Spice3CodeGen { dialect: Spice3Dialect::Xyce };
        let xy_netlist = xy.emit_netlist(&ir).unwrap();
        assert!(xy_netlist.contains(".lib /pdk/default/sky130.lib tt"),
            "xyce fallback: {}", xy_netlist);
    }

    #[test]
    fn test_model_library_corner_switch() {
        let ir_tt = CircuitIR {
            top: Subcircuit {
                name: "corner".into(),
                ports: vec![], parameters: vec![], components: vec![],
                instances: vec![], models: vec![], raw_spice: vec![],
                includes: vec![], libs: vec![], osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: None, subcircuit_defs: vec![],
            model_libraries: vec![ModelLibrary {
                name: "sky130".into(),
                path: "/pdk/sky130.lib".into(),
                corner: Some("tt".into()),
                backend_paths: HashMap::new(),
                setup_includes: vec![],
            }],
        };

        let mut ir_ss = ir_tt.clone();
        ir_ss.model_libraries[0].corner = Some("ss".into());

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };

        let tt_netlist = cg.emit_netlist(&ir_tt).unwrap();
        let ss_netlist = cg.emit_netlist(&ir_ss).unwrap();

        assert!(tt_netlist.contains(".lib /pdk/sky130.lib tt"), "tt: {}", tt_netlist);
        assert!(ss_netlist.contains(".lib /pdk/sky130.lib ss"), "ss: {}", ss_netlist);
        assert!(!tt_netlist.contains("ss"), "tt has no ss");
    }

    #[test]
    fn test_model_library_no_corner_uses_include() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "nocorner".into(),
                ports: vec![], parameters: vec![], components: vec![],
                instances: vec![], models: vec![], raw_spice: vec![],
                includes: vec![], libs: vec![], osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: None, subcircuit_defs: vec![],
            model_libraries: vec![ModelLibrary {
                name: "custom".into(),
                path: "/models/custom.lib".into(),
                corner: None,
                backend_paths: HashMap::new(),
                setup_includes: vec![],
            }],
        };

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains(".include /models/custom.lib"), "include: {}", netlist);
        assert!(!netlist.contains(".lib /models"), "no .lib without corner: {}", netlist);
    }

    // ── Issue 05: Dialect correctness ──

    #[test]
    fn test_xyce_uses_print_not_save() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "xyce_print".into(),
                ports: vec![], parameters: vec![],
                components: vec![Component::Resistor {
                    name: "1".into(), n1: "a".into(), n2: "0".into(),
                    value: IrValue::Numeric { value: 1000.0 }, params: vec![],
                }],
                instances: vec![], models: vec![], raw_spice: vec![],
                includes: vec![], libs: vec![], osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "xyce_print".into(), stimulus: vec![],
                analyses: vec![Analysis::Op],
                options: SimOptions::default(),
                saves: vec!["V(a)".into()],
                measures: vec![], temperature: None, nominal_temperature: None,
                initial_conditions: vec![], node_sets: vec![],
                step_params: vec![], extra_lines: vec![],
            }),
            subcircuit_defs: vec![], model_libraries: vec![],
        };

        let xy = Spice3CodeGen { dialect: Spice3Dialect::Xyce };
        let netlist = xy.emit_netlist(&ir).unwrap();
        assert!(netlist.contains(".PRINT"), "xyce needs .PRINT: {}", netlist);
        assert!(!netlist.contains(".save"), "xyce must not use .save: {}", netlist);
    }

    #[test]
    fn test_ngspice_uses_save() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "ng_save".into(),
                ports: vec![], parameters: vec![],
                components: vec![], instances: vec![], models: vec![],
                raw_spice: vec![], includes: vec![], libs: vec![],
                osdi_loads: vec![], verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "ng_save".into(), stimulus: vec![],
                analyses: vec![Analysis::Op],
                options: SimOptions::default(),
                saves: vec!["V(a)".into()],
                measures: vec![], temperature: None, nominal_temperature: None,
                initial_conditions: vec![], node_sets: vec![],
                step_params: vec![], extra_lines: vec![],
            }),
            subcircuit_defs: vec![], model_libraries: vec![],
        };

        let ng = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = ng.emit_netlist(&ir).unwrap();
        assert!(netlist.contains(".save V(a)"), "ngspice uses .save: {}", netlist);
    }

    #[test]
    fn test_pz_disto_error_on_xyce() {
        let cg_xy = Spice3CodeGen { dialect: Spice3Dialect::Xyce };
        let pz = Analysis::PoleZero {
            node1: "1".into(), node2: "0".into(),
            node3: "3".into(), node4: "0".into(),
            tf_type: "vol".into(), pz_type: "pz".into(),
        };
        assert!(cg_xy.emit_analysis(&pz).is_err(), "pz should error on xyce");

        let disto = Analysis::Distortion {
            variation: "dec".into(), points: 10,
            start: 1e3, stop: 1e6, f2overf1: None,
        };
        assert!(cg_xy.emit_analysis(&disto).is_err(), "disto should error on xyce");
    }

    #[test]
    fn test_pz_disto_ok_on_ngspice() {
        let cg_ng = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let pz = Analysis::PoleZero {
            node1: "1".into(), node2: "0".into(),
            node3: "3".into(), node4: "0".into(),
            tf_type: "vol".into(), pz_type: "pz".into(),
        };
        assert!(cg_ng.emit_analysis(&pz).is_ok(), "pz should work on ngspice");
    }

    #[test]
    fn test_voltage_source_ac_magnitude_and_phase() {
        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };

        // AC magnitude only
        let v_ac = Component::VoltageSource {
            name: "in".into(), np: "inp".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 },
            ac_magnitude: Some(1.0), ac_phase: None,
            waveform: None,
        };
        let s = cg.emit_component(&v_ac).unwrap();
        assert_eq!(s, "Vin inp 0 0 AC 1", "ac mag only: {}", s);

        // AC magnitude + phase
        let v_ac_phase = Component::VoltageSource {
            name: "in".into(), np: "inp".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 },
            ac_magnitude: Some(1.0), ac_phase: Some(45.0),
            waveform: None,
        };
        let s = cg.emit_component(&v_ac_phase).unwrap();
        assert_eq!(s, "Vin inp 0 0 AC 1 45", "ac mag+phase: {}", s);

        // No AC — regression check
        let v_dc = Component::VoltageSource {
            name: "dd".into(), np: "vdd".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 3.3 },
            ac_magnitude: None, ac_phase: None,
            waveform: None,
        };
        let s = cg.emit_component(&v_dc).unwrap();
        assert_eq!(s, "Vdd vdd 0 3.3", "dc only: {}", s);
        assert!(!s.contains("AC"), "no AC in dc-only source");
    }

    #[test]
    fn test_current_source_ac_magnitude() {
        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let i_ac = Component::CurrentSource {
            name: "ac".into(), np: "a".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 },
            ac_magnitude: Some(1e-3), ac_phase: Some(90.0),
            waveform: None,
        };
        let s = cg.emit_component(&i_ac).unwrap();
        assert!(s.contains("AC 0.001 90"), "current AC: {}", s);
    }

    #[test]
    fn test_pdk_netlist_lib_before_subckt_and_components() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "cs_amp_tb".into(),
                ports: vec![], parameters: vec![],
                components: vec![],
                instances: vec![Instance {
                    name: "dut".into(),
                    subcircuit: "cs_amp".into(),
                    port_mapping: vec!["vdd".into(), "vin".into(), "vout".into()],
                    parameters: vec![],
                }],
                models: vec![], raw_spice: vec![],
                includes: vec![], libs: vec![], osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "cs_amp_tb".into(), stimulus: vec![
                    Component::VoltageSource {
                        name: "dd".into(), np: "vdd".into(), nm: "0".into(),
                        value: IrValue::Numeric { value: 1.8 },
                        ac_magnitude: None, ac_phase: None,
                        waveform: None,
                    },
                ],
                analyses: vec![Analysis::Op],
                options: SimOptions::default(), saves: vec![],
                measures: vec![], temperature: None, nominal_temperature: None,
                initial_conditions: vec![], node_sets: vec![],
                step_params: vec![], extra_lines: vec![],
            }),
            subcircuit_defs: vec![Subcircuit {
                name: "cs_amp".into(),
                ports: vec![
                    Port { name: "vdd".into(), direction: PortDirection::InOut },
                    Port { name: "vin".into(), direction: PortDirection::Input },
                    Port { name: "vout".into(), direction: PortDirection::Output },
                ],
                parameters: vec![], models: vec![],
                components: vec![
                    Component::Resistor {
                        name: "drain".into(), n1: "vdd".into(), n2: "vout".into(),
                        value: IrValue::Numeric { value: 5e3 }, params: vec![],
                    },
                    Component::Mosfet {
                        name: "n1".into(), nd: "vout".into(), ng: "vin".into(),
                        ns: "0".into(), nb: "0".into(),
                        model: "sky130_fd_pr__nfet_01v8".into(),
                        params: vec![],
                    },
                ],
                instances: vec![], raw_spice: vec![],
                includes: vec![], libs: vec![], osdi_loads: vec![],
                verilog_blocks: vec![],
            }],
            model_libraries: vec![ModelLibrary {
                name: "sky130".into(),
                path: "/pdk/sky130.lib.spice".into(),
                corner: Some("tt".into()),
                backend_paths: HashMap::new(),
                setup_includes: vec![],
            }],
        };

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = cg.emit_netlist(&ir).unwrap();

        // Title must be line 1
        let lines: Vec<&str> = netlist.lines().collect();
        assert!(lines[0].starts_with("*"), "first line is title: {}", lines[0]);

        // .lib must come before .subckt
        let lib_pos = netlist.find(".lib").expect("must have .lib");
        let subckt_pos = netlist.find(".subckt").expect("must have .subckt");
        assert!(lib_pos < subckt_pos,
            ".lib ({}) must come before .subckt ({}):\n{}", lib_pos, subckt_pos, netlist);

        // .lib must come before components
        let mosfet_pos = netlist.find("Mn1").expect("must have Mn1");
        assert!(lib_pos < mosfet_pos,
            ".lib must come before MOSFET:\n{}", netlist);

        // Verify the .lib line itself
        assert!(netlist.contains(".lib /pdk/sky130.lib.spice tt"),
            "lib line: {}", netlist);
    }

    #[test]
    fn test_setup_includes_emitted_before_lib() {
        let ir = CircuitIR {
            top: Subcircuit {
                name: "gf180_test".into(),
                ports: vec![], parameters: vec![], components: vec![],
                instances: vec![], models: vec![], raw_spice: vec![],
                includes: vec![], libs: vec![], osdi_loads: vec![],
                verilog_blocks: vec![],
            },
            testbench: Some(Testbench {
                dut: "gf180_test".into(), stimulus: vec![],
                analyses: vec![Analysis::Op],
                options: SimOptions::default(), saves: vec![],
                measures: vec![], temperature: None, nominal_temperature: None,
                initial_conditions: vec![], node_sets: vec![],
                step_params: vec![], extra_lines: vec![],
            }),
            subcircuit_defs: vec![],
            model_libraries: vec![ModelLibrary {
                name: "gf180".into(),
                path: "/pdk/gf180/sm141064.ngspice".into(),
                corner: Some("typical".into()),
                backend_paths: HashMap::new(),
                setup_includes: vec!["/pdk/gf180/design.ngspice".into()],
            }],
        };

        let cg = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };
        let netlist = cg.emit_netlist(&ir).unwrap();

        // setup_includes emitted as .include
        assert!(netlist.contains(".include /pdk/gf180/design.ngspice"),
            "setup include present: {}", netlist);

        // setup_includes comes BEFORE the .lib
        let setup_pos = netlist.find(".include /pdk/gf180/design.ngspice").unwrap();
        let lib_pos = netlist.find(".lib /pdk/gf180/sm141064.ngspice typical").unwrap();
        assert!(setup_pos < lib_pos,
            "setup ({}) before lib ({}): {}", setup_pos, lib_pos, netlist);
    }
}
