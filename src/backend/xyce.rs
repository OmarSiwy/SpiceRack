use std::process::Command;
use tempfile::NamedTempFile;
use std::io::Write;

use crate::result::RawData;
use crate::rawfile;
use super::{Backend, BackendCapabilities, BackendError};

/// Xyce subprocess backend
pub struct XyceSubprocess {
    pub parallel: bool,
}

impl Backend for XyceSubprocess {
    fn name(&self) -> &str {
        if self.parallel {
            "xyce-parallel"
        } else {
            "xyce-serial"
        }
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            xspice: false,
            osdi: false,
            measures: true,
            step_params: true,
            control_blocks: false,
            laplace_sources: false,
            verilog_cosim: false,
        }
    }

    fn codegen(&self) -> Box<dyn crate::codegen::CodeGen> {
        Box::new(crate::codegen::spice3::Spice3CodeGen {
            dialect: crate::codegen::spice3::Spice3Dialect::Xyce,
        })
    }

    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        let mut cir_file = NamedTempFile::with_suffix(".cir")?;
        cir_file.write_all(netlist.as_bytes())?;
        cir_file.flush()?;

        let cir_path = cir_file.path();
        let raw_path = cir_path.with_extension("raw");

        let program = if self.parallel { "Xyce" } else { "Xyce" };

        let mut cmd = Command::new(program);
        cmd.arg("-r").arg(&raw_path).arg(cir_path);

        if self.parallel {
            // Xyce parallel mode uses MPI
            let mut mpi_cmd = Command::new("mpirun");
            mpi_cmd
                .arg("-np")
                .arg("4")
                .arg(program)
                .arg("-r")
                .arg(&raw_path)
                .arg(cir_path);
            cmd = mpi_cmd;
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(BackendError::SimulationError(format!(
                "Xyce exited with status {}\nstdout: {}\nstderr: {}",
                output.status,
                stdout.chars().take(500).collect::<String>(),
                stderr.chars().take(500).collect::<String>(),
            )));
        }

        // Capture stdout for .meas parsing
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();

        let raw_bytes = std::fs::read(&raw_path).map_err(|e| {
            BackendError::SimulationError(format!(
                "Failed to read raw file '{}': {}",
                raw_path.display(),
                e
            ))
        })?;

        let mut result = rawfile::parse_raw(&raw_bytes)?;
        result.stdout = stdout_str;
        let _ = std::fs::remove_file(&raw_path);

        Ok(result)
    }
}
