//! ESPice subprocess backend: write the deck, run `espice deck.sp -r out.raw`,
//! read the binary raw file with the same parser ngspice uses.
//!
//! ESPice compiles Verilog-A through VerA at run time (`.hdl`), so a deck with
//! `veriloga()` models needs no OpenVAF. Its first load of a model builds a
//! shared library, cached under `$ESPICE_CACHE` (else `~/.cache/espice`).

use std::process::Command;
use tempfile::TempDir;

use super::{Backend, BackendCapabilities, BackendError};
use crate::rawfile;
use crate::result::RawData;

pub struct EspiceSubprocess;

pub const ESPICE_CAPS: BackendCapabilities = BackendCapabilities {
    xspice: false,
    // Verilog-A loads, compiled by VerA (`.hdl`). A compiled `.osdi` is refused
    // by `EspiceCodeGen`: ESPice loads no OSDI binaries.
    osdi: true,
    // `.meas` results print to stdout in ngspice's format
    // (tests/test_espice_analyses.rs `measure_works_on_dc_ac_and_tran`).
    measures: true,
    // `.step` exists in ESPice, but every point lands in one raw file as
    // separate plots and `rawfile::parse_raw` keeps one: not wired here.
    step_params: false,
    // ngspice `.control` scripting is skipped with a warning by ESPice.
    control_blocks: false,
    laplace_sources: false,
    verilog_cosim: false,
};

impl Backend for EspiceSubprocess {
    fn name(&self) -> &str {
        "espice"
    }

    fn capabilities(&self) -> BackendCapabilities {
        ESPICE_CAPS
    }

    fn codegen(&self) -> Box<dyn crate::codegen::CodeGen> {
        Box::new(crate::codegen::espice::EspiceCodeGen)
    }

    /// A finished SPICE string from the legacy `Circuit` path. Its OSDI
    /// loads came from OpenVAF, which ESPice cannot use.
    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        if let Some(l) = netlist.lines().find(|l| l.trim_start().to_ascii_lowercase().starts_with("pre_osdi")) {
            return Err(BackendError::SimulationError(format!(
                "ESPice does not load OSDI binaries ({}); build the circuit with \
                 Subcircuit/Testbench.veriloga(\"model.va\") so the source reaches ESPice as .hdl",
                l.trim()
            )));
        }
        run_espice(netlist)
    }

    fn run_netlist(&self, netlist: &str) -> Result<RawData, BackendError> {
        run_espice(netlist)
    }
}

fn run_espice(netlist: &str) -> Result<RawData, BackendError> {
    let dir = TempDir::new()?;
    let deck = dir.path().join("deck.sp");
    let raw_path = dir.path().join("out.raw");
    std::fs::write(&deck, netlist)?;

    let output = Command::new("espice")
        .arg(&deck)
        .arg("--rawfile")
        .arg(&raw_path)
        .arg("--format=binary")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BackendError::SimulationError("espice not found on $PATH".into())
            } else {
                BackendError::Io(e)
            }
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        let errors: Vec<&str> = stderr
            .lines()
            .chain(stdout.lines())
            .filter(|l| l.contains("error") || l.contains("Error") || l.contains("failed"))
            .collect();
        let detail = if errors.is_empty() { stderr.chars().take(2000).collect() } else { errors.join("\n") };
        return Err(BackendError::SimulationError(format!("espice exited with {}: {}", output.status, detail)));
    }
    let bytes = std::fs::read(&raw_path).map_err(|e| {
        BackendError::SimulationError(format!("espice wrote no raw file ({e}); stderr: {}", stderr.chars().take(2000).collect::<String>()))
    })?;
    let mut raw = rawfile::parse_raw(&bytes)?;
    raw.stdout = stdout;
    Ok(raw)
}
