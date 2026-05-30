use crate::circuit::format_spice_number;
use crate::ir::*;
use super::{CodeGen, CodeGenError};

pub struct SpectreCodeGen;

impl SpectreCodeGen {
    fn emit_value(&self, v: &IrValue) -> String {
        match v {
            IrValue::Numeric { value } => format_spice_number(*value),
            IrValue::Expression { expr } => expr.clone(),
            IrValue::Raw { text } => text.clone(),
        }
    }

    fn emit_waveform_params(&self, wf: &IrWaveform) -> String {
        match wf {
            IrWaveform::Sin { offset, amplitude, frequency, delay, damping, phase } => {
                let mut s = format!("type=sine sinedc={} ampl={} freq={}", offset, amplitude, frequency);
                if *delay != 0.0 {
                    s.push_str(&format!(" sinedelay={}", delay));
                }
                if *damping != 0.0 {
                    s.push_str(&format!(" sinedamp={}", damping));
                }
                if *phase != 0.0 {
                    s.push_str(&format!(" sinephase={}", phase));
                }
                s
            }
            IrWaveform::Pulse { initial, pulsed, delay, rise_time, fall_time, pulse_width, period } => {
                format!(
                    "type=pulse val0={} val1={} delay={} rise={} fall={} width={} period={}",
                    initial, pulsed, delay, rise_time, fall_time, pulse_width, period,
                )
            }
            IrWaveform::Pwl { values } => {
                let mut s = String::from("type=pwl wave=[");
                for (i, (t, v)) in values.iter().enumerate() {
                    if i > 0 {
                        s.push(' ');
                    }
                    s.push_str(&format!("{} {}", t, v));
                }
                s.push(']');
                s
            }
            IrWaveform::Exp { initial, pulsed, rise_delay, rise_tau, fall_delay, fall_tau } => {
                format!(
                    "type=exp val0={} val1={} td1={} tau1={} td2={} tau2={}",
                    initial, pulsed, rise_delay, rise_tau, fall_delay, fall_tau,
                )
            }
            IrWaveform::Sffm { offset, amplitude, carrier_freq, modulation_index, signal_freq } => {
                format!(
                    "type=sffm sffmdc={} ampl={} carrier={} mdi={} signal={}",
                    offset, amplitude, carrier_freq, modulation_index, signal_freq,
                )
            }
            IrWaveform::Am { amplitude, offset, modulating_freq, carrier_freq, delay } => {
                format!(
                    "type=am ampl={} amdc={} modf={} carrierf={} delay={}",
                    amplitude, offset, modulating_freq, carrier_freq, delay,
                )
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
        let mut s = format!("model {} {}", m.name, m.kind);
        if !m.parameters.is_empty() {
            s.push_str(" (");
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

        // Parameters
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

        Ok(lines.join("\n"))
    }

    fn map_option_name(&self, canonical: &str) -> String {
        match canonical {
            "reltol" => "reltol".into(),
            "abstol" => "iabstol".into(),
            "vntol" => "vabstol".into(),
            "gmin" => "gmin".into(),
            "max_iterations" => "maxiters".into(),
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

        // Title as comment
        lines.push(format!("// {}", ir.top.name));
        lines.push(String::new());

        // Simulator language
        lines.push("simulator lang=spectre".into());
        lines.push(String::new());

        // OSDI / Verilog-A includes
        for path in &ir.top.osdi_loads {
            lines.push(format!("ahdl_include \"{}\"", path));
        }

        // Includes
        for inc in &ir.top.includes {
            lines.push(format!("include \"{}\"", inc));
        }

        // Libs
        for (path, section) in &ir.top.libs {
            lines.push(format!("include \"{}\" section={}", path, section));
        }

        // Model libraries
        for lib in &ir.model_libraries {
            let path = lib.backend_paths
                .get("spectre")
                .unwrap_or(&lib.path);
            if let Some(ref corner) = lib.corner {
                lines.push(format!("include \"{}\" section={}", path, corner));
            } else {
                lines.push(format!("include \"{}\"", path));
            }
        }

        // Parameters
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

        // Models
        for m in &ir.top.models {
            lines.push(self.emit_model(m));
        }

        // Subcircuit definitions
        for sc in &ir.subcircuit_defs {
            lines.push(String::new());
            lines.push(self.emit_subcircuit(sc)?);
        }

        lines.push(String::new());

        // Components
        for comp in &ir.top.components {
            lines.push(self.emit_component(comp)?);
        }

        // Instances
        for inst in &ir.top.instances {
            lines.push(self.emit_instance(inst));
        }

        // Testbench
        if let Some(ref tb) = ir.testbench {
            // Stimulus
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
                lines.push(format!("mytemp options temp={}", temp));
            }

            // Initial conditions
            for (node, val) in &tb.initial_conditions {
                lines.push(format!("ic {}={}", node, val));
            }

            // Node sets
            for (node, val) in &tb.node_sets {
                lines.push(format!("nodeset {}={}", node, val));
            }

            // Saves
            for save in &tb.saves {
                lines.push(format!("save {}", save));
            }

            // Extra lines
            for line in &tb.extra_lines {
                lines.push(line.clone());
            }

            lines.push(String::new());

            // Analyses
            for analysis in &tb.analyses {
                lines.push(self.emit_analysis(analysis)?);
            }
        }

        Ok(lines.join("\n"))
    }

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
            Component::Resistor { name, n1, n2, value, params } => {
                let mut s = format!("r{} ({} {}) resistor r={}", name.to_lowercase(), n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Capacitor { name, n1, n2, value, params } => {
                let mut s = format!("c{} ({} {}) capacitor c={}", name.to_lowercase(), n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Inductor { name, n1, n2, value, params } => {
                let mut s = format!("l{} ({} {}) inductor l={}", name.to_lowercase(), n1, n2, self.emit_value(value));
                s.push_str(&self.emit_params(params));
                s
            }
            Component::MutualInductor { name, inductor1, inductor2, coupling } => {
                format!("k{} mutual_inductor coupling={} ind1=l{} ind2=l{}",
                    name.to_lowercase(), coupling, inductor1.to_lowercase(), inductor2.to_lowercase())
            }
            Component::VoltageSource { name, np, nm, value, waveform } => {
                let mut s = format!("v{} ({} {}) vsource dc={}", name.to_lowercase(), np, nm, self.emit_value(value));
                if let Some(wf) = waveform {
                    s.push_str(&format!(" {}", self.emit_waveform_params(wf)));
                }
                s
            }
            Component::CurrentSource { name, np, nm, value, waveform } => {
                let mut s = format!("i{} ({} {}) isource dc={}", name.to_lowercase(), np, nm, self.emit_value(value));
                if let Some(wf) = waveform {
                    s.push_str(&format!(" {}", self.emit_waveform_params(wf)));
                }
                s
            }
            Component::BehavioralVoltage { name, np, nm, expression } => {
                format!("b{} ({} {}) bsource v={}", name.to_lowercase(), np, nm, expression)
            }
            Component::BehavioralCurrent { name, np, nm, expression } => {
                format!("b{} ({} {}) bsource i={}", name.to_lowercase(), np, nm, expression)
            }
            Component::Vcvs { name, np, nm, ncp, ncm, gain } => {
                format!("e{} ({} {} {} {}) vcvs gain={}", name.to_lowercase(), np, nm, ncp, ncm, gain)
            }
            Component::Vccs { name, np, nm, ncp, ncm, transconductance } => {
                format!("g{} ({} {} {} {}) vccs gm={}", name.to_lowercase(), np, nm, ncp, ncm, transconductance)
            }
            Component::Cccs { name, np, nm, vsense, gain } => {
                format!("f{} ({} {}) cccs probe={} gain={}", name.to_lowercase(), np, nm, vsense, gain)
            }
            Component::Ccvs { name, np, nm, vsense, transresistance } => {
                format!("h{} ({} {}) ccvs probe={} rm={}", name.to_lowercase(), np, nm, vsense, transresistance)
            }
            Component::Diode { name, np, nm, model, params } => {
                let mut s = format!("d{} ({} {}) {}", name.to_lowercase(), np, nm, model);
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Bjt { name, nc, nb, ne, model, params } => {
                let mut s = format!("q{} ({} {} {}) {}", name.to_lowercase(), nc, nb, ne, model);
                s.push_str(&self.emit_params(params));
                s
            }
            Component::Mosfet { name, nd, ng, ns, nb, model, params } => {
                let mut s = format!("m{} ({} {} {} {}) {}", name.to_lowercase(), nd, ng, ns, nb, model);
                s.push_str(&self.emit_params(params));
                s
            }
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
            Component::VSwitch { name, np, nm, ncp, ncm, model } => {
                format!("s{} ({} {} {} {}) {}", name.to_lowercase(), np, nm, ncp, ncm, model)
            }
            Component::ISwitch { name, np, nm, vcontrol, model } => {
                format!("w{} ({} {}) {} vref={}", name.to_lowercase(), np, nm, model, vcontrol)
            }
            Component::TLine { name, inp, inm, outp, outm, z0, td } => {
                format!("t{} ({} {} {} {}) tline z0={} td={}", name.to_lowercase(), inp, inm, outp, outm, z0, td)
            }
            Component::Xspice { name, connections, model } => {
                // XSPICE is ngspice-specific; emit as comment
                let mut s = format!("// XSPICE (unsupported in Spectre): a{}", name.to_lowercase());
                for conn in connections {
                    s.push_str(&format!(" {}", conn));
                }
                s.push_str(&format!(" {}", model));
                s
            }
            Component::RawSpice { line } => {
                // Raw SPICE lines may not be valid Spectre; emit as comment
                format!("// raw: {}", line)
            }
        };
        Ok(s)
    }

    fn emit_analysis(&self, analysis: &Analysis) -> Result<String, CodeGenError> {
        let s = match analysis {
            Analysis::Op => "op1 dc".into(),
            Analysis::Dc { sweeps } => {
                if let Some(sw) = sweeps.first() {
                    format!(
                        "dc1 dc param={} start={} stop={} step={}",
                        sw.source,
                        format_spice_number(sw.start),
                        format_spice_number(sw.stop),
                        format_spice_number(sw.step),
                    )
                } else {
                    "dc1 dc".into()
                }
            }
            Analysis::Ac { variation, points, start, stop } => {
                format!(
                    "ac1 ac start={} stop={} {}={}",
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    variation,
                    points,
                )
            }
            Analysis::Transient { step, stop, start, max_step, .. } => {
                let mut s = format!(
                    "tran1 tran step={} stop={}",
                    format_spice_number(*step),
                    format_spice_number(*stop),
                );
                if let Some(st) = start {
                    s.push_str(&format!(" start={}", format_spice_number(*st)));
                }
                if let Some(ms) = max_step {
                    s.push_str(&format!(" maxstep={}", format_spice_number(*ms)));
                }
                s
            }
            Analysis::Noise { output, reference, source, variation, points, start, stop, .. } => {
                let out_spec = if reference.is_empty() || reference == "0" {
                    format!("V({})", output)
                } else {
                    format!("V({},{})", output, reference)
                };
                format!(
                    "noise1 noise start={} stop={} {}={} oprobe={} iprobe={}",
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    variation,
                    points,
                    out_spec,
                    source,
                )
            }
            Analysis::Tf { output, source } => {
                format!("xf1 xf probe={} source={}", output, source)
            }
            Analysis::Sensitivity { output, ac } => {
                let mut s = format!("sens1 sens probe={}", output);
                if let Some(ac_params) = ac {
                    s.push_str(&format!(
                        " start={} stop={} {}={}",
                        format_spice_number(ac_params.start),
                        format_spice_number(ac_params.stop),
                        ac_params.variation,
                        ac_params.points,
                    ));
                }
                s
            }
            Analysis::Pss { fundamental, stabilization, observe_node, points_per_period, harmonics } => {
                format!(
                    "pss1 pss fund={} tstab={} harms={} ppv={} probe={}",
                    format_spice_number(*fundamental),
                    format_spice_number(*stabilization),
                    harmonics,
                    points_per_period,
                    observe_node,
                )
            }
            Analysis::HarmonicBalance { frequencies, harmonics } => {
                let mut s = String::from("hb1 hb");
                for (i, (freq, harm)) in frequencies.iter().zip(harmonics.iter()).enumerate() {
                    s.push_str(&format!(" tone{}={} nharm{}={}", i + 1, format_spice_number(*freq), i + 1, harm));
                }
                s
            }
            Analysis::SPar { variation, points, start, stop } => {
                format!(
                    "sp1 sp start={} stop={} {}={}",
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    variation,
                    points,
                )
            }
            Analysis::Stability { probe, variation, points, start, stop } => {
                format!(
                    "stb1 stb start={} stop={} {}={} probe={}",
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    variation,
                    points,
                    probe,
                )
            }
            Analysis::TransientNoise { step, stop } => {
                format!(
                    "tn1 trnoise step={} stop={}",
                    format_spice_number(*step),
                    format_spice_number(*stop),
                )
            }
            Analysis::Fourier { fundamental, outputs, .. } => {
                let mut s = format!("dft1 fourier fund={}", format_spice_number(*fundamental));
                for out in outputs {
                    s.push_str(&format!(" signal={}", out));
                }
                s
            }
            Analysis::SpectreSweep { param, start, stop, step, inner, inner_type } => {
                format!(
                    "sweep1 sweep param={} start={} stop={} step={} {{ {} {} }}",
                    param,
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    format_spice_number(*step),
                    inner,
                    inner_type,
                )
            }
            Analysis::SpectreMonteCarlo { iterations, inner, inner_type, seed } => {
                let mut s = format!(
                    "mc1 montecarlo numruns={} {{ {} {} }}",
                    iterations, inner, inner_type,
                );
                if let Some(sd) = seed {
                    s.push_str(&format!(" seed={}", sd));
                }
                s
            }
            Analysis::SpectrePac { pss_fundamental, pss_stabilization, pss_harmonics, variation, points, start, stop, sweep_type } => {
                format!(
                    "pss_pac pss fund={} tstab={} harms={}\npac1 pac start={} stop={} {}={} sweeptype={}",
                    format_spice_number(*pss_fundamental),
                    format_spice_number(*pss_stabilization),
                    pss_harmonics,
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    variation,
                    points,
                    sweep_type,
                )
            }
            Analysis::SpectrePnoise { pss_fundamental, pss_stabilization, pss_harmonics, output, reference, variation, points, start, stop } => {
                let out_spec = if reference.is_empty() || reference == "0" {
                    output.clone()
                } else {
                    format!("{},{}", output, reference)
                };
                format!(
                    "pss_pnoise pss fund={} tstab={} harms={}\npnoise1 pnoise start={} stop={} {}={} oprobe={}",
                    format_spice_number(*pss_fundamental),
                    format_spice_number(*pss_stabilization),
                    pss_harmonics,
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    variation,
                    points,
                    out_spec,
                )
            }
            Analysis::SpectrePxf { pss_fundamental, pss_stabilization, pss_harmonics, output, source, variation, points, start, stop } => {
                format!(
                    "pss_pxf pss fund={} tstab={} harms={}\npxf1 pxf start={} stop={} {}={} oprobe={} iprobe={}",
                    format_spice_number(*pss_fundamental),
                    format_spice_number(*pss_stabilization),
                    pss_harmonics,
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    variation,
                    points,
                    output,
                    source,
                )
            }
            Analysis::SpectrePstb { pss_fundamental, pss_stabilization, pss_harmonics, probe, variation, points, start, stop } => {
                format!(
                    "pss_pstb pss fund={} tstab={} harms={}\npstb1 pstb start={} stop={} {}={} probe={}",
                    format_spice_number(*pss_fundamental),
                    format_spice_number(*pss_stabilization),
                    pss_harmonics,
                    format_spice_number(*start),
                    format_spice_number(*stop),
                    variation,
                    points,
                    probe,
                )
            }
            // Xyce-only analyses not supported in Spectre
            Analysis::PoleZero { .. }
            | Analysis::Distortion { .. }
            | Analysis::XyceSampling { .. }
            | Analysis::XyceEmbeddedSampling { .. }
            | Analysis::XycePce { .. }
            | Analysis::XyceFft { .. } => {
                return Err(CodeGenError::UnsupportedAnalysis {
                    backend: "spectre".into(),
                    analysis: format!("{:?}", analysis).split_whitespace().next().unwrap_or("unknown").into(),
                });
            }
        };
        Ok(s)
    }

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
        assert!(sin_str.contains("freq=1000000"), "sin freq: {}", sin_str);

        let pulse_str = cg.emit_component(&pulse_comp).unwrap();
        assert!(pulse_str.contains("type=pulse"), "pulse: {}", pulse_str);
        assert!(pulse_str.contains("val0=0"), "pulse val0: {}", pulse_str);
        assert!(pulse_str.contains("val1=3.3"), "pulse val1: {}", pulse_str);
    }

    #[test]
    fn test_spectre_all_analyses() {
        let cg = SpectreCodeGen;

        // Op
        assert_eq!(cg.emit_analysis(&Analysis::Op).unwrap(), "op1 dc");

        // DC
        let dc = Analysis::Dc {
            sweeps: vec![DcSweep { source: "Vsrc".into(), start: 0.0, stop: 5.0, step: 0.1 }],
        };
        let dc_str = cg.emit_analysis(&dc).unwrap();
        assert!(dc_str.contains("dc1 dc"), "dc: {}", dc_str);
        assert!(dc_str.contains("param=Vsrc"), "dc param: {}", dc_str);

        // AC
        let ac = Analysis::Ac { variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 };
        let ac_str = cg.emit_analysis(&ac).unwrap();
        assert!(ac_str.contains("ac1 ac"), "ac: {}", ac_str);
        assert!(ac_str.contains("dec=100"), "ac dec: {}", ac_str);

        // Transient
        let tran = Analysis::Transient { step: 1e-9, stop: 1e-6, start: None, max_step: None, uic: false };
        let tran_str = cg.emit_analysis(&tran).unwrap();
        assert!(tran_str.contains("tran1 tran"), "tran: {}", tran_str);
        assert!(tran_str.contains("step=1n"), "tran step: {}", tran_str);

        // PSS (Spectre-specific)
        let pss = Analysis::Pss {
            fundamental: 1e6,
            stabilization: 10e-6,
            observe_node: "out".into(),
            points_per_period: 128,
            harmonics: 10,
        };
        let pss_str = cg.emit_analysis(&pss).unwrap();
        assert!(pss_str.contains("pss1 pss"), "pss: {}", pss_str);
        assert!(pss_str.contains("fund=1meg"), "pss fund: {}", pss_str);
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
        assert!(s.contains("maxiters=200"), "maxiters: {}", s);
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
    fn test_spectre_xspice_commented() {
        let comp = Component::Xspice {
            name: "1".into(),
            connections: vec!["in".into(), "out".into()],
            model: "d_and".into(),
        };

        let cg = SpectreCodeGen;
        let s = cg.emit_component(&comp).unwrap();
        assert!(s.starts_with("//"), "xspice should be comment: {}", s);
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
            }],
        };

        let cg = SpectreCodeGen;
        let netlist = cg.emit_netlist(&ir).unwrap();
        assert!(netlist.contains("include \"/pdk/spectre/sky130.scs\" section=tt"),
            "spectre path: {}", netlist);
        assert!(!netlist.contains("ngspice"), "no ngspice path: {}", netlist);
    }
}
