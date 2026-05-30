use std::process::Command;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use std::io::Write;

use crate::result::RawData;
use crate::rawfile;
use super::{Backend, BackendError};

/// LTspice subprocess backend: write .cir, run via Wine or native, read .raw
pub struct LtspiceSubprocess {
    pub executable: PathBuf,
    pub use_wine: bool,
    /// When true, passes `-FastAccess` to LTspice for column-major raw file output
    pub fast_access: bool,
}

impl LtspiceSubprocess {
    /// Inject options to normalize raw file output (disable compression, force f64)
    fn normalize_netlist(netlist: &str) -> String {
        let mut result = String::with_capacity(netlist.len() + 100);

        // Find the position after the title line to insert options
        let mut lines = netlist.lines();
        if let Some(title) = lines.next() {
            result.push_str(title);
            result.push('\n');
        }

        // Inject normalization options right after title
        result.push_str(".options plotwinsize=0\n");
        result.push_str(".options numdgt=15\n");

        for line in lines {
            result.push_str(line);
            result.push('\n');
        }

        result
    }
}

impl Backend for LtspiceSubprocess {
    fn name(&self) -> &str {
        "ltspice"
    }

    fn codegen(&self) -> Box<dyn crate::codegen::CodeGen> {
        Box::new(crate::codegen::spice3::Spice3CodeGen {
            dialect: crate::codegen::spice3::Spice3Dialect::Ltspice,
        })
    }

    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        let normalized = Self::normalize_netlist(netlist);

        let mut cir_file = NamedTempFile::with_suffix(".cir")?;
        cir_file.write_all(normalized.as_bytes())?;
        cir_file.flush()?;

        let cir_path = cir_file.path();
        let raw_path = cir_path.with_extension("raw");

        let output = if self.use_wine {
            let mut cmd = Command::new("wine");
            cmd.arg(&self.executable)
                .arg("-b")
                .arg("-wine");
            if self.fast_access {
                cmd.arg("-FastAccess");
            }
            cmd.arg(cir_path).output()?
        } else {
            let mut cmd = Command::new(&self.executable);
            cmd.arg("-b");
            if self.fast_access {
                cmd.arg("-FastAccess");
            }
            cmd.arg(cir_path).output()?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(BackendError::SimulationError(format!(
                "LTspice exited with status {}\nstdout: {}\nstderr: {}",
                output.status,
                stdout.chars().take(500).collect::<String>(),
                stderr.chars().take(500).collect::<String>(),
            )));
        }

        // Capture stdout
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();

        // LTspice puts .meas results in the .log file -- read before cleanup
        let log_path = cir_path.with_extension("log");
        let log_content = std::fs::read_to_string(&log_path).unwrap_or_default();

        // LTspice puts the raw file next to the input with .raw extension
        let raw_bytes = std::fs::read(&raw_path).map_err(|e| {
            BackendError::SimulationError(format!(
                "Failed to read raw file '{}': {}",
                raw_path.display(), e
            ))
        })?;

        let mut result = rawfile::parse_raw(&raw_bytes)?;
        result.stdout = stdout_str;
        result.log_content = log_content;
        let _ = std::fs::remove_file(&raw_path);
        let _ = std::fs::remove_file(&log_path);

        Ok(result)
    }
}

/// Detect LTspice executable on the system
pub fn detect_ltspice() -> Option<(PathBuf, bool)> {
    // Check PATH first (user may have symlinked it)
    if let Ok(output) = Command::new("which").arg("ltspice").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Some((PathBuf::from(path), false));
        }
    }

    // macOS native
    let macos_path = PathBuf::from("/Applications/LTspice.app/Contents/MacOS/LTspice");
    if macos_path.exists() {
        return Some((macos_path, false));
    }

    // Linux via Wine — check common install locations
    if let Ok(home) = std::env::var("HOME") {
        let wine_paths = [
            format!("{}/.wine/drive_c/users/{}/AppData/Local/Programs/ADI/LTspice/LTspice.exe",
                    home, std::env::var("USER").unwrap_or_default()),
            format!("{}/.wine/drive_c/Program Files/ADI/LTspice/LTspice.exe", home),
            format!("{}/.wine/drive_c/Program Files/LTC/LTspiceXVII/XVIIx64.exe", home),
        ];

        // Only try Wine paths if wine is available
        let has_wine = Command::new("which")
            .arg("wine")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if has_wine {
            for path_str in &wine_paths {
                let path = PathBuf::from(path_str);
                if path.exists() {
                    return Some((path, true));
                }
            }
        }
    }

    // Windows native
    #[cfg(target_os = "windows")]
    {
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            let win_path = PathBuf::from(format!(
                "{}/Programs/ADI/LTspice/LTspice.exe", localappdata
            ));
            if win_path.exists() {
                return Some((win_path, false));
            }
        }
    }

    None
}
