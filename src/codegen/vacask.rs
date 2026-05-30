use crate::ir::*;
use super::{CodeGen, CodeGenError};
use super::spectre::SpectreCodeGen;

pub struct VacaskCodeGen;

impl CodeGen for VacaskCodeGen {
    fn backend_name(&self) -> &str {
        "vacask"
    }

    fn emit_netlist(&self, ir: &CircuitIR) -> Result<String, CodeGenError> {
        let mut lines = Vec::new();

        // Title
        lines.push(format!("// {}", ir.top.name));
        lines.push(String::new());

        lines.push("simulator lang=spectre".into());
        lines.push(String::new());

        // OSDI: vacask uses `load` (not `ahdl_include`)
        for path in &ir.top.osdi_loads {
            lines.push(format!("load \"{}\"", path));
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
                .get("vacask")
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
        let spectre = SpectreCodeGen;
        for m in &ir.top.models {
            lines.push(spectre.emit_model_pub(m));
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
            lines.push(spectre.emit_instance_pub(inst));
        }

        // Testbench
        if let Some(ref tb) = ir.testbench {
            for comp in &tb.stimulus {
                lines.push(self.emit_component(comp)?);
            }

            let opts = self.emit_options(&tb.options)?;
            if !opts.is_empty() {
                lines.push(opts);
            }

            if let Some(temp) = tb.temperature {
                lines.push(format!("mytemp options temp={}", temp));
            }

            for (node, val) in &tb.initial_conditions {
                lines.push(format!("ic {}={}", node, val));
            }

            for (node, val) in &tb.node_sets {
                lines.push(format!("nodeset {}={}", node, val));
            }

            for save in &tb.saves {
                lines.push(format!("save {}", save));
            }

            for line in &tb.extra_lines {
                lines.push(line.clone());
            }

            lines.push(String::new());

            for analysis in &tb.analyses {
                lines.push(self.emit_analysis(analysis)?);
            }
        }

        Ok(lines.join("\n"))
    }

    fn emit_subcircuit(&self, sc: &Subcircuit) -> Result<String, CodeGenError> {
        let spectre = SpectreCodeGen;
        spectre.emit_subcircuit(sc)
    }

    fn emit_component(&self, comp: &Component) -> Result<String, CodeGenError> {
        let spectre = SpectreCodeGen;
        spectre.emit_component(comp)
    }

    fn emit_analysis(&self, analysis: &Analysis) -> Result<String, CodeGenError> {
        match analysis {
            // Vacask uses dcxf for transfer function
            Analysis::Tf { output, source } => {
                Ok(format!("xf1 dcxf probe={} source={}", output, source))
            }
            // Vacask uses acstb for stability
            Analysis::Stability { probe, variation, points, start, stop } => {
                Ok(format!(
                    "stb1 acstb start={} stop={} {}={} probe={}",
                    crate::circuit::format_spice_number(*start),
                    crate::circuit::format_spice_number(*stop),
                    variation, points, probe,
                ))
            }
            // Vacask trannoise
            Analysis::TransientNoise { step, stop } => {
                Ok(format!(
                    "tn1 trannoise step={} stop={}",
                    crate::circuit::format_spice_number(*step),
                    crate::circuit::format_spice_number(*stop),
                ))
            }
            // Everything else delegates to Spectre codegen
            _ => {
                let spectre = SpectreCodeGen;
                spectre.emit_analysis(analysis)
            }
        }
    }

    fn emit_options(&self, opts: &SimOptions) -> Result<String, CodeGenError> {
        let mut parts = Vec::new();

        for (key, val) in &opts.portable {
            parts.push(format!("{}={}", key, val));
        }

        if let Some(specific) = opts.backend_specific.get("vacask") {
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
