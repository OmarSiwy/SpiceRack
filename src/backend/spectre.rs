use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use crate::result::*;
use crate::rawfile;
use crate::psf;
use super::{Backend, BackendCapabilities, BackendError};

/// Detected output format for Spectre results.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    Nutmeg,
    Psf,
}

/// Spectre subprocess backend: wrap SPICE netlist in `simulator lang=spice`,
/// run spectre with nutmeg output format, parse the raw file.
pub struct SpectreSubprocess;

impl Backend for SpectreSubprocess {
    fn name(&self) -> &str {
        "spectre"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            xspice: false,
            osdi: true,
            measures: false,
            step_params: true,
            control_blocks: false,
            laplace_sources: false,
            verilog_cosim: true,
        }
    }

    fn codegen(&self) -> Box<dyn crate::codegen::CodeGen> {
        Box::new(crate::codegen::spectre::SpectreCodeGen)
    }

    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        let tmp_dir = TempDir::new()?;
        let scs_path = tmp_dir.path().join("circuit.scs");
        let raw_dir = tmp_dir.path().join("raw");
        std::fs::create_dir_all(&raw_dir)?;

        // Wrap SPICE netlist in Spectre's SPICE-compatibility mode
        let spectre_netlist = wrap_spice_for_spectre(netlist);
        std::fs::write(&scs_path, spectre_netlist.as_bytes())?;

        let output = Command::new("spectre")
            .arg("-format")
            .arg("nutbin") // ngspice-compatible binary Nutmeg format
            .arg("-raw")
            .arg(&raw_dir)
            .arg(&scs_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(BackendError::SimulationError(format!(
                "spectre exited with status {}\nstdout: {}\nstderr: {}",
                output.status,
                stdout.chars().take(500).collect::<String>(),
                stderr.chars().take(500).collect::<String>(),
            )));
        }

        // Capture stdout
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();

        // Find output file (try nutmeg first, then PSF)
        let (out_path, format) = find_output_file(&raw_dir)?;
        let raw_bytes = std::fs::read(&out_path).map_err(|e| {
            BackendError::SimulationError(format!(
                "Failed to read output file '{}': {}",
                out_path.display(), e
            ))
        })?;

        let mut result = match format {
            OutputFormat::Nutmeg => rawfile::parse_raw(&raw_bytes)?,
            OutputFormat::Psf => psf::parse_psf(&raw_bytes).map_err(|e| {
                BackendError::SimulationError(format!("PSF parse error: {}", e))
            })?,
        };
        result.stdout = stdout_str;
        Ok(result)
    }
}

/// Run a Spectre-native netlist (not wrapped in `simulator lang=spice`).
/// Used for sweep, montecarlo, and SpectreRF analyses.
fn run_spectre_native(netlist: &str) -> Result<RawData, BackendError> {
    let tmp_dir = TempDir::new()?;
    let scs_path = tmp_dir.path().join("circuit.scs");
    let raw_dir = tmp_dir.path().join("raw");
    std::fs::create_dir_all(&raw_dir)?;

    std::fs::write(&scs_path, netlist.as_bytes())?;

    let output = Command::new("spectre")
        .arg("-format")
        .arg("nutbin")
        .arg("-raw")
        .arg(&raw_dir)
        .arg(&scs_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(BackendError::SimulationError(format!(
            "spectre exited with status {}\nstdout: {}\nstderr: {}",
            output.status,
            stdout.chars().take(500).collect::<String>(),
            stderr.chars().take(500).collect::<String>(),
        )));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();

    let (out_path, format) = find_output_file(&raw_dir)?;
    let raw_bytes = std::fs::read(&out_path).map_err(|e| {
        BackendError::SimulationError(format!(
            "Failed to read output file '{}': {}",
            out_path.display(), e
        ))
    })?;

    let mut result = match format {
        OutputFormat::Nutmeg => rawfile::parse_raw(&raw_bytes)?,
        OutputFormat::Psf => psf::parse_psf(&raw_bytes).map_err(|e| {
            BackendError::SimulationError(format!("PSF parse error: {}", e))
        })?,
    };
    result.stdout = stdout_str;
    Ok(result)
}

/// Wrap a SPICE netlist so Spectre can read it using `simulator lang=spice`.
fn wrap_spice_for_spectre(spice: &str) -> String {
    let mut out = String::with_capacity(spice.len() + 200);
    out.push_str("// PySpice auto-generated Spectre wrapper\n");

    // Extract pre_osdi lines and convert to Spectre's ahdl_include.
    // Spectre can load Verilog-A source directly (.va) or OSDI (.osdi).
    let mut spice_body = String::with_capacity(spice.len());
    for line in spice.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pre_osdi") || trimmed.starts_with("PRE_OSDI") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let path = parts[1];
                // If it's a .va source, use ahdl_include; if .osdi, use load
                if path.ends_with(".va") {
                    out.push_str(&format!("ahdl_include \"{}\"\n", path));
                } else {
                    // Spectre doesn't natively load OSDI, but some versions support it
                    out.push_str(&format!("// OSDI model (compile to .va for native Spectre): {}\n", path));
                    out.push_str(&format!("simulator lang=spice\npre_osdi {}\nsimulator lang=spectre\n", path));
                }
            }
        } else {
            spice_body.push_str(line);
            spice_body.push('\n');
        }
    }

    out.push_str("simulator lang=spice\n\n");
    out.push_str(&spice_body);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Find the output file in the raw directory.
/// Tries nutmeg format first (known extensions), then falls back to PSF.
pub fn find_output_file(dir: &Path) -> Result<(PathBuf, OutputFormat), BackendError> {
    let nutmeg_extensions = ["dc", "ac", "tran", "noise", "op", "raw"];

    // First pass: look for nutmeg files by extension
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if nutmeg_extensions.contains(&ext) {
                    return Ok((path, OutputFormat::Nutmeg));
                }
            }
        }
    }

    // Second pass: look for PSF directory
    let psf_dir = dir.join("psf");
    if psf_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&psf_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(bytes) = std::fs::read(&path) {
                        if psf::is_psf(&bytes) {
                            return Ok((path, OutputFormat::Psf));
                        }
                    }
                }
            }
        }
    }

    // Third pass: files with .psf extension
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "psf" {
                    return Ok((path, OutputFormat::Psf));
                }
            }
        }
    }

    // Fourth pass: try any non-log file and detect format from content
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "logFile" || name.ends_with(".log") || !path.is_file() {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                if psf::is_psf(&bytes) {
                    return Ok((path, OutputFormat::Psf));
                }
                // Assume nutmeg if not PSF
                return Ok((path, OutputFormat::Nutmeg));
            }
        }
    }

    Err(BackendError::SimulationError(
        "No output file produced by spectre".to_string(),
    ))
}

/// Legacy function kept for backward compatibility. Prefer `find_output_file`.
pub fn find_nutmeg_file(dir: &Path) -> Result<PathBuf, BackendError> {
    find_output_file(dir).map(|(path, _)| path)
}

// ── Spectre sweep and Monte Carlo wrappers ──

impl SpectreSubprocess {
    /// Spectre parametric sweep. Wraps an inner analysis inside a Spectre
    /// `sweep` block. The inner analysis is specified in Spectre-native syntax.
    ///
    /// # Arguments
    /// * `spice_netlist` - Original SPICE netlist (without analysis statement)
    /// * `param` - Parameter name to sweep (e.g., "R1" or a .param name)
    /// * `start` - Sweep start value
    /// * `stop` - Sweep stop value
    /// * `step` - Sweep step size
    /// * `inner_analysis` - Inner analysis statement in Spectre syntax (e.g., "ac1 ac start=1 stop=1G dec=100")
    /// * `inner_type` - Analysis type for backend routing (e.g., "ac", "tran")
    pub fn spectre_sweep(
        &self,
        spice_netlist: &str,
        param: &str,
        start: f64,
        stop: f64,
        step: f64,
        inner_analysis: &str,
        _inner_type: &str,
    ) -> Result<RawData, BackendError> {
        let netlist = build_sweep_netlist(
            spice_netlist, param, start, stop, step, inner_analysis,
        );
        run_spectre_native(&netlist)
    }

    /// Spectre Monte Carlo analysis. Wraps an inner analysis inside a Spectre
    /// `montecarlo` block.
    ///
    /// # Arguments
    /// * `spice_netlist` - Original SPICE netlist (without analysis statement)
    /// * `num_iterations` - Number of MC iterations
    /// * `inner_analysis` - Inner analysis in Spectre syntax
    /// * `inner_type` - Analysis type for backend routing
    /// * `seed` - Optional random seed for reproducibility
    pub fn spectre_montecarlo(
        &self,
        spice_netlist: &str,
        num_iterations: u32,
        inner_analysis: &str,
        _inner_type: &str,
        seed: Option<u64>,
    ) -> Result<RawData, BackendError> {
        let netlist = build_montecarlo_netlist(
            spice_netlist, num_iterations, inner_analysis, seed,
        );
        run_spectre_native(&netlist)
    }

    // ── SpectreRF periodic analyses ──

    /// Periodic AC (PAC) analysis. Requires PSS as prerequisite.
    ///
    /// Generates a PSS+PAC analysis pair:
    /// ```text
    /// pss1 pss fund=<fund> harms=<harms> tstab=<tstab>
    /// pac1 pac start=<start> stop=<stop> dec=<points> sweeptype=<sweep_type>
    /// ```
    pub fn spectre_pac(
        &self,
        spice_netlist: &str,
        pss_fundamental: f64,
        pss_stabilization: f64,
        pss_harmonics: u32,
        variation: &str,
        points: u32,
        start_freq: f64,
        stop_freq: f64,
        sweep_type: &str,
    ) -> Result<AcAnalysis, BackendError> {
        let pss_line = format!(
            "pss1 pss fund={} harms={} tstab={}",
            pss_fundamental, pss_harmonics, pss_stabilization
        );
        let pac_line = format!(
            "pac1 pac start={} stop={} {}={} sweeptype={}",
            start_freq, stop_freq, variation, points, sweep_type
        );

        let netlist = build_spectrerf_netlist(spice_netlist, &[&pss_line, &pac_line]);
        let raw = run_spectre_native(&netlist)?;
        Ok(AcAnalysis::from_raw(raw))
    }

    /// Periodic noise (PNoise) analysis. Requires PSS as prerequisite.
    ///
    /// Generates a PSS+PNoise analysis pair:
    /// ```text
    /// pss1 pss fund=<fund> harms=<harms> tstab=<tstab>
    /// pnoise1 pnoise start=<start> stop=<stop> dec=<points> oprobe=<output> refprobe=<ref>
    /// ```
    pub fn spectre_pnoise(
        &self,
        spice_netlist: &str,
        pss_fundamental: f64,
        pss_stabilization: f64,
        pss_harmonics: u32,
        output_node: &str,
        ref_node: &str,
        variation: &str,
        points: u32,
        start_freq: f64,
        stop_freq: f64,
    ) -> Result<NoiseAnalysis, BackendError> {
        let pss_line = format!(
            "pss1 pss fund={} harms={} tstab={}",
            pss_fundamental, pss_harmonics, pss_stabilization
        );
        let pnoise_line = format!(
            "pnoise1 pnoise start={} stop={} {}={} oprobe={} refprobe={}",
            start_freq, stop_freq, variation, points, output_node, ref_node
        );

        let netlist = build_spectrerf_netlist(spice_netlist, &[&pss_line, &pnoise_line]);
        let raw = run_spectre_native(&netlist)?;
        Ok(NoiseAnalysis::from_raw(raw))
    }

    /// Periodic transfer function (PXF) analysis. Requires PSS as prerequisite.
    ///
    /// Generates a PSS+PXF analysis pair:
    /// ```text
    /// pss1 pss fund=<fund> harms=<harms> tstab=<tstab>
    /// pxf1 pxf start=<start> stop=<stop> dec=<points> oprobe=<output> isrc=<source>
    /// ```
    pub fn spectre_pxf(
        &self,
        spice_netlist: &str,
        pss_fundamental: f64,
        pss_stabilization: f64,
        pss_harmonics: u32,
        output_node: &str,
        source: &str,
        variation: &str,
        points: u32,
        start_freq: f64,
        stop_freq: f64,
    ) -> Result<AcAnalysis, BackendError> {
        let pss_line = format!(
            "pss1 pss fund={} harms={} tstab={}",
            pss_fundamental, pss_harmonics, pss_stabilization
        );
        let pxf_line = format!(
            "pxf1 pxf start={} stop={} {}={} oprobe={} isrc={}",
            start_freq, stop_freq, variation, points, output_node, source
        );

        let netlist = build_spectrerf_netlist(spice_netlist, &[&pss_line, &pxf_line]);
        let raw = run_spectre_native(&netlist)?;
        Ok(AcAnalysis::from_raw(raw))
    }

    /// Periodic stability (PSTB) analysis. Requires PSS as prerequisite.
    ///
    /// Generates a PSS+PSTB analysis pair:
    /// ```text
    /// pss1 pss fund=<fund> harms=<harms> tstab=<tstab>
    /// pstb1 pstb start=<start> stop=<stop> dec=<points> probe=<probe>
    /// ```
    pub fn spectre_pstb(
        &self,
        spice_netlist: &str,
        pss_fundamental: f64,
        pss_stabilization: f64,
        pss_harmonics: u32,
        probe: &str,
        variation: &str,
        points: u32,
        start_freq: f64,
        stop_freq: f64,
    ) -> Result<StabilityAnalysis, BackendError> {
        let pss_line = format!(
            "pss1 pss fund={} harms={} tstab={}",
            pss_fundamental, pss_harmonics, pss_stabilization
        );
        let pstb_line = format!(
            "pstb1 pstb start={} stop={} {}={} probe={}",
            start_freq, stop_freq, variation, points, probe
        );

        let netlist = build_spectrerf_netlist(spice_netlist, &[&pss_line, &pstb_line]);
        let raw = run_spectre_native(&netlist)?;
        Ok(StabilityAnalysis::from_raw(raw))
    }
}

// ── Netlist builders ──

/// Build a Spectre netlist with a parametric sweep wrapping an inner analysis.
fn build_sweep_netlist(
    spice_netlist: &str,
    param: &str,
    start: f64,
    stop: f64,
    step: f64,
    inner_analysis: &str,
) -> String {
    let mut out = String::with_capacity(spice_netlist.len() + 256);
    out.push_str("// PySpice auto-generated Spectre sweep\n");
    out.push_str("simulator lang=spice\n\n");

    // Emit the SPICE netlist but strip .end and analysis statements
    for line in spice_netlist.lines() {
        let trimmed = line.trim().to_lowercase();
        if trimmed == ".end" {
            continue;
        }
        if trimmed.starts_with(".op")
            || trimmed.starts_with(".dc")
            || trimmed.starts_with(".ac")
            || trimmed.starts_with(".tran")
        {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    // Switch to Spectre language for the sweep block
    out.push_str("\nsimulator lang=spectre\n\n");
    out.push_str(&format!(
        "sweep1 sweep param={} start={} stop={} step={} {{\n",
        param, start, stop, step
    ));
    out.push_str(&format!("    {}\n", inner_analysis));
    out.push_str("}\n");
    out
}

/// Build a Spectre netlist with Monte Carlo wrapping an inner analysis.
fn build_montecarlo_netlist(
    spice_netlist: &str,
    num_iterations: u32,
    inner_analysis: &str,
    seed: Option<u64>,
) -> String {
    let mut out = String::with_capacity(spice_netlist.len() + 256);
    out.push_str("// PySpice auto-generated Spectre Monte Carlo\n");
    out.push_str("simulator lang=spice\n\n");

    for line in spice_netlist.lines() {
        let trimmed = line.trim().to_lowercase();
        if trimmed == ".end" {
            continue;
        }
        if trimmed.starts_with(".op")
            || trimmed.starts_with(".dc")
            || trimmed.starts_with(".ac")
            || trimmed.starts_with(".tran")
        {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    out.push_str("\nsimulator lang=spectre\n\n");
    let seed_str = seed.map(|s| format!(" seed={}", s)).unwrap_or_default();
    out.push_str(&format!(
        "mc1 montecarlo numruns={}{} {{\n",
        num_iterations, seed_str
    ));
    out.push_str(&format!("    {}\n", inner_analysis));
    out.push_str("}\n");
    out
}

/// Build a Spectre netlist with SpectreRF analysis lines appended in Spectre-native syntax.
fn build_spectrerf_netlist(spice_netlist: &str, analysis_lines: &[&str]) -> String {
    let mut out = String::with_capacity(spice_netlist.len() + 512);
    out.push_str("// PySpice auto-generated Spectre RF analysis\n");
    out.push_str("simulator lang=spice\n\n");

    for line in spice_netlist.lines() {
        let trimmed = line.trim().to_lowercase();
        if trimmed == ".end" {
            continue;
        }
        // Strip existing analysis statements
        if trimmed.starts_with(".op")
            || trimmed.starts_with(".dc")
            || trimmed.starts_with(".ac")
            || trimmed.starts_with(".tran")
            || trimmed.starts_with(".pss")
        {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    // Switch to Spectre native for the analyses
    out.push_str("\nsimulator lang=spectre\n\n");
    for line in analysis_lines {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_spice_for_spectre() {
        let spice = ".title test\nR1 a b 1k\n.end\n";
        let wrapped = wrap_spice_for_spectre(spice);
        assert!(wrapped.contains("simulator lang=spice"));
        assert!(wrapped.contains("R1 a b 1k"));
    }

    #[test]
    fn test_output_format_enum() {
        assert_ne!(OutputFormat::Nutmeg, OutputFormat::Psf);
    }

    #[test]
    fn test_build_sweep_netlist() {
        let spice = ".title sweep_test\nR1 a b 1k\n.ac dec 10 1 1G\n.end\n";
        let result = build_sweep_netlist(
            spice, "R1", 1e3, 10e3, 1e3,
            "ac1 ac start=1 stop=1G dec=100",
        );
        assert!(result.contains("simulator lang=spectre"));
        assert!(result.contains("sweep1 sweep param=R1 start=1000 stop=10000 step=1000"));
        assert!(result.contains("ac1 ac start=1 stop=1G dec=100"));
        // Original .ac should be stripped
        assert!(!result.contains(".ac dec 10 1 1G"));
        // Original .end should be stripped
        assert!(!result.contains(".end"));
    }

    #[test]
    fn test_build_montecarlo_netlist_with_seed() {
        let spice = ".title mc_test\nR1 a b 1k\n.end\n";
        let result = build_montecarlo_netlist(
            spice, 100,
            "ac1 ac start=1 stop=1G dec=100",
            Some(12345),
        );
        assert!(result.contains("mc1 montecarlo numruns=100 seed=12345"));
        assert!(result.contains("ac1 ac start=1 stop=1G dec=100"));
    }

    #[test]
    fn test_build_montecarlo_netlist_without_seed() {
        let spice = ".title mc_test\nR1 a b 1k\n.end\n";
        let result = build_montecarlo_netlist(
            spice, 50,
            "tran1 tran stop=1u",
            None,
        );
        assert!(result.contains("mc1 montecarlo numruns=50 {"));
        assert!(!result.contains("seed="));
    }

    #[test]
    fn test_build_spectrerf_pss_pac() {
        let spice = ".title rf_test\nR1 a b 1k\n.end\n";
        let pss = "pss1 pss fund=1000000000 harms=10 tstab=0.0000001";
        let pac = "pac1 pac start=1 stop=1000000000 dec=100 sweeptype=relative";
        let result = build_spectrerf_netlist(spice, &[pss, pac]);
        assert!(result.contains("simulator lang=spectre"));
        assert!(result.contains("pss1 pss fund=1000000000 harms=10 tstab=0.0000001"));
        assert!(result.contains("pac1 pac start=1 stop=1000000000 dec=100 sweeptype=relative"));
    }

    #[test]
    fn test_build_spectrerf_strips_existing_analyses() {
        let spice = ".title rf_test\nR1 a b 1k\n.ac dec 10 1 1G\n.tran 1u 10m\n.end\n";
        let pss = "pss1 pss fund=1e9 harms=10 tstab=100n";
        let result = build_spectrerf_netlist(spice, &[pss]);
        assert!(!result.contains(".ac dec"));
        assert!(!result.contains(".tran 1u"));
        assert!(result.contains("pss1 pss"));
    }
}
