pub mod spice3;
pub mod spectre;
pub mod vacask;

use crate::ir::*;

#[derive(Debug, thiserror::Error)]
pub enum CodeGenError {
    #[error("Unsupported component for backend {backend}: {component}")]
    UnsupportedComponent { backend: String, component: String },
    #[error("Unsupported analysis for backend {backend}: {analysis}")]
    UnsupportedAnalysis { backend: String, analysis: String },
    #[error("Code generation error: {0}")]
    Other(String),
}

pub trait CodeGen {
    fn backend_name(&self) -> &str;
    fn emit_netlist(&self, ir: &CircuitIR) -> Result<String, CodeGenError>;
    fn emit_subcircuit(&self, sc: &Subcircuit) -> Result<String, CodeGenError>;
    fn emit_component(&self, comp: &Component) -> Result<String, CodeGenError>;
    fn emit_analysis(&self, analysis: &Analysis) -> Result<String, CodeGenError>;
    fn emit_options(&self, opts: &SimOptions) -> Result<String, CodeGenError>;
}
