//! ESPice netlists: the ngspice dialect, with Verilog-A loaded as `.hdl`.
//!
//! ESPice reads ngspice decks and compiles Verilog-A itself through VerA, so
//! the only difference from [`Spice3CodeGen`] is the model loads: each `.va`
//! becomes `.hdl "<path>"` and there is no `pre_osdi` control block. ESPice
//! loads no OSDI binaries, so a compiled `.osdi` in the load list is an error
//! here rather than a deck ESPice would refuse later.

use super::spice3::{Spice3CodeGen, Spice3Dialect};
use super::{CodeGen, CodeGenError};
use crate::ir::*;

pub struct EspiceCodeGen;

const NG: Spice3CodeGen = Spice3CodeGen { dialect: Spice3Dialect::Ngspice };

fn hdl_lines(loads: &[String]) -> Result<Vec<String>, CodeGenError> {
    loads
        .iter()
        .map(|l| {
            if crate::veriloga::is_source(l) {
                Ok(format!(".hdl \"{}\"", l))
            } else {
                Err(CodeGenError::Other(format!(
                    "ESPice does not load OSDI binaries ({l}); load the Verilog-A source with veriloga(\"model.va\")"
                )))
            }
        })
        .collect()
}

impl CodeGen for EspiceCodeGen {
    fn backend_name(&self) -> &str {
        "espice"
    }

    fn emit_netlist(&self, ir: &CircuitIR) -> Result<String, CodeGenError> {
        let mut hdl = hdl_lines(&ir.top.osdi_loads)?;
        for sub in &ir.subcircuit_defs {
            for l in hdl_lines(&sub.osdi_loads)? {
                if !hdl.contains(&l) {
                    hdl.push(l);
                }
            }
        }
        let mut stripped = ir.clone();
        stripped.top.osdi_loads.clear();
        for sub in &mut stripped.subcircuit_defs {
            sub.osdi_loads.clear();
        }
        let text = NG.emit_netlist(&stripped)?;
        // After the title line, before any card that instantiates a model.
        let (title, rest) = text.split_once('\n').unwrap_or((&text, ""));
        let mut out = String::with_capacity(text.len() + 64 * hdl.len());
        out.push_str(title);
        out.push('\n');
        for l in &hdl {
            out.push_str(l);
            out.push('\n');
        }
        out.push_str(rest);
        Ok(out)
    }

    fn emit_subcircuit(&self, sc: &Subcircuit) -> Result<String, CodeGenError> {
        NG.emit_subcircuit(sc)
    }

    fn emit_component(&self, comp: &Component) -> Result<String, CodeGenError> {
        NG.emit_component(comp)
    }

    fn emit_analysis(&self, analysis: &Analysis) -> Result<String, CodeGenError> {
        NG.emit_analysis(analysis)
    }

    fn emit_options(&self, opts: &SimOptions) -> Result<String, CodeGenError> {
        NG.emit_options(opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ir(loads: Vec<String>) -> CircuitIR {
        CircuitIR {
            top: Subcircuit {
                name: "t".into(),
                ports: vec![],
                parameters: vec![],
                components: vec![],
                instances: vec![],
                models: vec![],
                raw_spice: vec!["Nx a 0 vres r=2k".into()],
                includes: vec![],
                libs: vec![],
                osdi_loads: loads,
                verilog_blocks: vec![],
            },
            testbench: None,
            subcircuit_defs: vec![],
            model_libraries: vec![],
        }
    }

    #[test]
    fn veriloga_becomes_hdl_without_pre_osdi() {
        let n = EspiceCodeGen.emit_netlist(&ir(vec!["/m/vres.va".into()])).unwrap();
        assert!(n.starts_with("* t\n.hdl \"/m/vres.va\"\n"), "{n}");
        assert!(!n.contains("pre_osdi") && !n.contains(".control"), "{n}");
    }

    #[test]
    fn osdi_binary_is_refused() {
        let e = EspiceCodeGen.emit_netlist(&ir(vec!["/m/vres.osdi".into()])).unwrap_err();
        assert!(e.to_string().contains("does not load OSDI"), "{e}");
    }
}
