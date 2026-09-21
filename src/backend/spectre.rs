//! Cadence Spectre subprocess backend.
//!
//! Spectre is licence-gated and is not installed in this repository, so nothing
//! here has ever been executed against the real binary. Command-line flags and
//! output behaviour are taken from the *Spectre Circuit Simulator Reference*,
//! Product Version 19.1, January 2020 (`[REF]`),
//! <https://ee.kpi.ua/~yv/edu/ok/book/spectre_refManual.pdf>:
//!
//! * p.26 `-raw raw` — "Saves the simulation results in the specified file or
//!   directory named raw."
//! * p.26 `-format fmt` — "Possible values for fmt are nutbin, nutascii,
//!   wsfbin, wsfascii, psfbin, psfascii, psfbinf, psfxl, awb, sst2, fsdb,
//!   fsdb5, wdf, uwi, and tr0ascii." `nutbin` is the binary Nutmeg format that
//!   `crate::rawfile` already parses, so that is what we ask for.
//! * p.20 — with no options Spectre "saves the .measure and .mt0 files in the
//!   .raw subdirectory of the netlist directory".
//!
//! The per-analysis file naming inside the raw directory (`logFile` plus one
//! file per analysis, extension = analysis type) is **not** in any Cadence
//! manual I could obtain; see `find_output_file`.

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

/// Spectre subprocess backend.
///
/// Two entry points, deliberately different:
/// * [`Backend::run_netlist`] executes a deck that `SpectreCodeGen` already
///   emitted in the Spectre native language — verbatim, no wrapping.
/// * [`Backend::run`] takes the legacy SPICE string and fences it in
///   `simulator lang=spice` (`[REF]` p.493, and Kundert, *The Designer's Guide
///   to SPICE and Spectre*, Appendix B.2,
///   <https://designers-guide.org/analysis/dg-spice/chB.pdf>).
pub struct SpectreSubprocess;

impl Backend for SpectreSubprocess {
    fn name(&self) -> &str {
        "spectre"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            xspice: false,
            osdi: true,
            // NOTE: this is wrong and cannot be fixed from this file.
            // `[REF]` p.17: Spectre supports "standard SPICE measurement
            // functions (.measure)", and p.20 says it writes `.measure`/`.mt0`
            // files. `SpectreCodeGen::emit_measure` now emits them correctly,
            // fenced by `simulator lang=spice`. Flipping this to `true`
            // requires the same flip in `src/backend/mod.rs` (the duplicated
            // `BackendKind::capabilities` table plus `test_spectre_capabilities`
            // and `test_backendkind_capabilities_match_trait`), which this
            // change is not allowed to touch.
            measures: true,
            step_params: true,
            control_blocks: false,
            laplace_sources: false,
            verilog_cosim: true,
        }
    }

    fn codegen(&self) -> Box<dyn crate::codegen::CodeGen> {
        Box::new(crate::codegen::spectre::SpectreCodeGen)
    }

    /// Legacy path: `netlist` is a SPICE string, so fence it in SPICE mode.
    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        run_spectre(&wrap_spice_for_spectre(netlist))
    }

    /// IR path: `netlist` already *is* Spectre, produced by `SpectreCodeGen`.
    /// The default trait impl forwards to `run`, which would have wrapped a
    /// native Spectre deck in `simulator lang=spice` and guaranteed a parse
    /// error.
    fn run_netlist(&self, netlist: &str) -> Result<RawData, BackendError> {
        run_spectre(netlist)
    }
}

/// Write `netlist` to a `.scs` file and run `spectre` on it.
///
/// The `.scs` extension matters: `[REF]` p.494 — "all files that use the
/// Spectre native language must begin with a `simulator lang=spectre`
/// statement. The one exception is files that end with a `.scs` file
/// extension, which are treated specially and are read in Spectre input mode."
fn run_spectre(netlist: &str) -> Result<RawData, BackendError> {
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
///
/// `[REF]` p.494 and Kundert Appendix B.2 both document `simulator lang=` as a
/// mid-file mode switch; the deck starts in Spectre mode because the file is
/// written with a `.scs` extension.
fn wrap_spice_for_spectre(spice: &str) -> String {
    let mut out = String::with_capacity(spice.len() + 200);
    out.push_str("// SpiceRack auto-generated Spectre wrapper\n");

    // ngspice's `pre_osdi` has no Spectre equivalent. Spectre's behavioural
    // path is Verilog-A source via `ahdl_include` (`[REF]` p.541), so a `.va`
    // path translates directly; a compiled `.osdi` object does not, and
    // emitting a `pre_osdi` card in SPICE mode would just move the parse error.
    let mut spice_body = String::with_capacity(spice.len());
    for line in spice.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pre_osdi") || trimmed.starts_with("PRE_OSDI") {
            if let Some(path) = trimmed.split_whitespace().nth(1) {
                if path.ends_with(".va") {
                    out.push_str(&format!("ahdl_include \"{}\"\n", path));
                } else {
                    out.push_str(&format!(
                        "// [spicerack] dropped: Spectre cannot load compiled OSDI '{}'; \
                         supply the Verilog-A source for ahdl_include\n",
                        path
                    ));
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

/// Sorted list of regular files directly inside `dir`.
///
/// `read_dir` order is filesystem-defined; with more than one analysis in a
/// deck the previous unsorted scan picked an arbitrary result file on each run.
fn sorted_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    v.sort();
    v
}

/// Find the output file in the raw directory.
///
/// **Not documented.** The file layout Spectre produces under `-raw <dir>` — a
/// `logFile` index plus one result file per analysis whose extension is the
/// analysis type (`.tran`, `.dc`, `.ac`, `.noise`, `.pss`, `.sp`, `.xf`) — does
/// not appear in the Cadence manuals I could obtain. It is corroborated only by
/// third-party write-ups, e.g.
/// <https://www.analogflavor.com/en/2023/05/04/the-psf-and-psfxl-file-structure/>
/// ("The most important file in the result directory is the `logFile` ... one
/// file per analysis"). Treat the extension list below as a heuristic.
///
/// Note the extension says nothing about the *format*: a `.tran` file is
/// nutmeg when `-format nutbin` was passed and PSF otherwise, so content
/// sniffing decides, not the extension. (This function currently leans on
/// `psf::is_psf`, which checks for the `"Clarissa"` magic in the file's
/// trailer where PSF actually keeps it.
/// and the accompanying report.)
pub fn find_output_file(dir: &Path) -> Result<(PathBuf, OutputFormat), BackendError> {
    let analysis_extensions = [
        "ac", "dc", "noise", "op", "pac", "pnoise", "pss", "pstb", "pxf",
        "raw", "sp", "stb", "tran", "xf",
    ];

    let classify = |path: &Path| -> OutputFormat {
        match std::fs::read(path) {
            Ok(bytes) if psf::is_psf(&bytes) => OutputFormat::Psf,
            _ => OutputFormat::Nutmeg,
        }
    };

    // First pass: a result file named after its analysis type.
    for path in sorted_files(dir) {
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && analysis_extensions.contains(&ext) {
                let format = classify(&path);
                return Ok((path, format));
            }
    }

    // Second pass: a `psf` subdirectory (the Virtuoso run-directory layout).
    let psf_dir = dir.join("psf");
    if psf_dir.is_dir() {
        for path in sorted_files(&psf_dir) {
            if let Ok(bytes) = std::fs::read(&path)
                && psf::is_psf(&bytes) {
                    return Ok((path, OutputFormat::Psf));
                }
        }
    }

    // Third pass: an explicit `.psf` extension.
    for path in sorted_files(dir) {
        if path.extension().and_then(|e| e.to_str()) == Some("psf") {
            return Ok((path, OutputFormat::Psf));
        }
    }

    // Fourth pass: anything that is not the log, sniffed for content.
    for path in sorted_files(dir) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "logFile" || name.ends_with(".log") {
            continue;
        }
        let format = classify(&path);
        return Ok((path, format));
    }

    Err(BackendError::SimulationError(
        "No output file produced by spectre".to_string(),
    ))
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
        run_spectre(&netlist)
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
        run_spectre(&netlist)
    }

    // ── SpectreRF periodic analyses ──
    //
    // All four take a PSS large-signal solution as prerequisite ([REF] p.272).
    // The PSS statement is `Name [p] [n] pss parameter=value ...` with `fund`
    // (#2), `harms` (#4) and `tstab` (#6) — [REF] p.273-274.

    /// Periodic AC (PAC) analysis. Requires PSS as prerequisite.
    ///
    /// `[REF]` p.245-246: `Name ... pac parameter=value ...`, sweep interval
    /// parameters `start`/`stop`/`dec`/`lin`/`log`, plus `sweeptype` (#11,
    /// "Possible values are absolute, relative and unspecified").
    ///
    /// ```text
    /// pss1 pss fund=<fund> harms=<harms> tstab=<tstab>
    /// pac1 pac start=<start> stop=<stop> dec=<points> sweeptype=<sweep_type>
    /// ```
    #[allow(clippy::too_many_arguments)]
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
        let pss_line = pss_stmt(pss_fundamental, pss_harmonics, pss_stabilization);
        let pac_line = format!(
            "pac1 pac start={} stop={} {} sweeptype={}",
            start_freq, stop_freq, sweep_points(variation, points)?, sweep_type
        );

        let netlist = build_spectrerf_netlist(spice_netlist, &[&pss_line, &pac_line]);
        let raw = run_spectre(&netlist)?;
        Ok(AcAnalysis::from_raw(raw))
    }

    /// Periodic noise (PNoise) analysis. Requires PSS as prerequisite.
    ///
    /// `[REF]` p.253-256: `Name [p] [n] ... pnoise parameter=value ...` —
    /// "The optional terminals (p and n) specify the output of the circuit."
    /// The probe parameters are `oprobe` (#13) and `iprobe` (#14), and both
    /// name *components*, not nodes. There is no `refprobe` parameter; the
    /// previous implementation emitted `oprobe=<node> refprobe=<node>`, which
    /// is both an unknown parameter and the wrong kind of argument.
    ///
    /// ```text
    /// pss1 pss fund=<fund> harms=<harms> tstab=<tstab>
    /// pnoise1 (<output> <ref>) pnoise start=<start> stop=<stop> dec=<points>
    /// ```
    #[allow(clippy::too_many_arguments)]
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
        let pss_line = pss_stmt(pss_fundamental, pss_harmonics, pss_stabilization);
        let neg = if ref_node.is_empty() { "0" } else { ref_node };
        let pnoise_line = format!(
            "pnoise1 ({} {}) pnoise start={} stop={} {}",
            output_node, neg, start_freq, stop_freq, sweep_points(variation, points)?
        );

        let netlist = build_spectrerf_netlist(spice_netlist, &[&pss_line, &pnoise_line]);
        let raw = run_spectre(&netlist)?;
        Ok(NoiseAnalysis::from_raw(raw))
    }

    /// Periodic transfer function (PXF) analysis. Requires PSS as prerequisite.
    ///
    /// `[REF]` p.303-304: `Name [p] [n] ... pxf parameter=value ...`. The only
    /// probe parameter is `probe` (#13, "Compute every transfer function to
    /// this probe component") and it identifies the *output*. Like `xf`, pxf
    /// computes the transfer function from every source, so there is no input
    /// source parameter — the previous `oprobe=<node> isrc=<source>` spelled
    /// two parameters pxf does not have.
    ///
    /// ```text
    /// pss1 pss fund=<fund> harms=<harms> tstab=<tstab>
    /// pxf1 (<output> 0) pxf start=<start> stop=<stop> dec=<points>
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn spectre_pxf(
        &self,
        spice_netlist: &str,
        pss_fundamental: f64,
        pss_stabilization: f64,
        pss_harmonics: u32,
        output_node: &str,
        _source: &str,
        variation: &str,
        points: u32,
        start_freq: f64,
        stop_freq: f64,
    ) -> Result<AcAnalysis, BackendError> {
        let pss_line = pss_stmt(pss_fundamental, pss_harmonics, pss_stabilization);
        let pxf_line = format!(
            "pxf1 ({} 0) pxf start={} stop={} {}",
            output_node, start_freq, stop_freq, sweep_points(variation, points)?
        );

        let netlist = build_spectrerf_netlist(spice_netlist, &[&pss_line, &pxf_line]);
        let raw = run_spectre(&netlist)?;
        Ok(AcAnalysis::from_raw(raw))
    }

    /// Periodic stability (PSTB) analysis. Requires PSS as prerequisite.
    ///
    /// `[REF]` p.298-299: `Name pstb parameter=value ...` with `probe` (#11,
    /// "Probe instance around which the loop gain is calculated").
    ///
    /// ```text
    /// pss1 pss fund=<fund> harms=<harms> tstab=<tstab>
    /// pstb1 pstb start=<start> stop=<stop> dec=<points> probe=<probe>
    /// ```
    #[allow(clippy::too_many_arguments)]
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
        let pss_line = pss_stmt(pss_fundamental, pss_harmonics, pss_stabilization);
        let pstb_line = format!(
            "pstb1 pstb start={} stop={} {} probe={}",
            start_freq, stop_freq, sweep_points(variation, points)?, probe.to_lowercase()
        );

        let netlist = build_spectrerf_netlist(spice_netlist, &[&pss_line, &pstb_line]);
        let raw = run_spectre(&netlist)?;
        Ok(StabilityAnalysis::from_raw(raw))
    }
}

/// `[REF]` p.273-274: pss `fund` (#2), `harms` (#4), `tstab` (#6).
fn pss_stmt(fundamental: f64, harmonics: u32, stabilization: f64) -> String {
    format!("pss1 pss fund={} harms={} tstab={}", fundamental, harmonics, stabilization)
}

/// Frequency-sweep point spec, identical across ac/noise/xf/sp/stb/pac/pnoise/
/// pxf/pstb (`[REF]` "Sweep interval parameters": `start stop center span step
/// lin dec log values valuesfile`). Spectre has no `oct`.
fn sweep_points(variation: &str, points: u32) -> Result<String, BackendError> {
    match variation.to_ascii_lowercase().as_str() {
        "dec" => Ok(format!("dec={points}")),
        "lin" => Ok(format!("lin={points}")),
        "log" => Ok(format!("log={points}")),
        other => Err(BackendError::SimulationError(format!(
            "spectre: frequency sweep type '{other}' has no Spectre spelling \
             (accepted: dec, lin, log)"
        ))),
    }
}

// ── Netlist builders ──
//
// All three emit the same shape: the circuit in SPICE mode, then a switch to
// Spectre mode for analyses Spectre's SPICE reader has no cards for.
// `[REF]` p.493-494 and Kundert Appendix B.2 both document `simulator lang=`
// as a mid-file mode switch.

/// Circuit body in `simulator lang=spice` mode, with `.end` and any SPICE
/// analysis cards stripped — the Spectre-native block supplies the analyses.
fn spice_preamble(header: &str, spice_netlist: &str) -> String {
    const STRIPPED: [&str; 5] = [".op", ".dc", ".ac", ".tran", ".pss"];

    let mut out = String::with_capacity(spice_netlist.len() + 512);
    out.push_str(header);
    out.push('\n');
    out.push_str("simulator lang=spice\n\n");

    for line in spice_netlist.lines() {
        let trimmed = line.trim().to_lowercase();
        if trimmed == ".end" || STRIPPED.iter().any(|d| trimmed.starts_with(d)) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    out.push_str("\nsimulator lang=spectre\n\n");
    out
}

/// Build a Spectre netlist with a parametric sweep wrapping an inner analysis.
///
/// `[REF]` p.410 verbatim:
/// ```text
/// swp sweep param=temp values=[-50 0 50 100 125] {
///                oppoint dc oppoint=logfile
/// }
/// ```
fn build_sweep_netlist(
    spice_netlist: &str,
    param: &str,
    start: f64,
    stop: f64,
    step: f64,
    inner_analysis: &str,
) -> String {
    let mut out = spice_preamble("// SpiceRack auto-generated Spectre sweep", spice_netlist);
    out.push_str(&format!(
        "sweep1 sweep param={} start={} stop={} step={} {{\n",
        param, start, stop, step
    ));
    out.push_str(&format!("    {}\n", inner_analysis));
    out.push_str("}\n");
    out
}

/// Build a Spectre netlist with Monte Carlo wrapping an inner analysis.
///
/// `[REF]` p.174 verbatim:
/// ```text
/// mc1 montecarlo variations=process seed=1234 numruns=200 {
///     dcop1 dc
///     tran1 tran start=0 stop=1u
/// }
/// ```
/// Every parameter precedes the opening brace.
///
/// NOTE (`[REF]` p.165, 174-180): montecarlo only perturbs netlist parameters
/// declared in a `statistics` block. Without one it runs `numruns` identical
/// simulations; SpiceRack has no way to emit statistics blocks yet.
fn build_montecarlo_netlist(
    spice_netlist: &str,
    num_iterations: u32,
    inner_analysis: &str,
    seed: Option<u64>,
) -> String {
    let mut out = spice_preamble("// SpiceRack auto-generated Spectre Monte Carlo", spice_netlist);
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
    let mut out = spice_preamble("// SpiceRack auto-generated Spectre RF analysis", spice_netlist);
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

    /// `[REF]` p.273-274: pss takes `fund`, `harms`, `tstab`.
    #[test]
    fn test_pss_stmt_uses_documented_parameters() {
        assert_eq!(pss_stmt(1e9, 10, 1e-7), "pss1 pss fund=1000000000 harms=10 tstab=0.0000001");
    }

    /// `[REF]` sweep interval parameters: `dec`, `lin`, `log`. No `oct`.
    #[test]
    fn test_sweep_points_rejects_octave() {
        assert_eq!(sweep_points("dec", 100).unwrap(), "dec=100");
        assert_eq!(sweep_points("LIN", 50).unwrap(), "lin=50");
        assert_eq!(sweep_points("log", 20).unwrap(), "log=20");
        assert!(sweep_points("oct", 10).is_err());
    }

    /// The four SpectreRF helpers build their analysis lines inline; these
    /// reproduce the exact strings so a regression shows up as a diff.
    /// Sources: `[REF]` p.245-246 (pac), p.253-256 (pnoise), p.298-299 (pstb),
    /// p.303-304 (pxf).
    #[test]
    fn test_spectrerf_analysis_lines_match_documented_forms() {
        let pac = format!(
            "pac1 pac start={} stop={} {} sweeptype={}",
            1.0, 1e9, sweep_points("dec", 100).unwrap(), "relative"
        );
        assert_eq!(pac, "pac1 pac start=1 stop=1000000000 dec=100 sweeptype=relative");

        // pnoise: output is a terminal pair, never `oprobe=<node>`, and there
        // is no `refprobe` parameter at all.
        let pnoise = format!(
            "pnoise1 ({} {}) pnoise start={} stop={} {}",
            "out", "0", 1.0, 1e6, sweep_points("dec", 10).unwrap()
        );
        assert_eq!(pnoise, "pnoise1 (out 0) pnoise start=1 stop=1000000 dec=10");
        assert!(!pnoise.contains("refprobe"));

        // pxf: output is a terminal pair or `probe=`; there is no `isrc`.
        let pxf = format!(
            "pxf1 ({} 0) pxf start={} stop={} {}",
            "out", 1.0, 1e6, sweep_points("dec", 10).unwrap()
        );
        assert_eq!(pxf, "pxf1 (out 0) pxf start=1 stop=1000000 dec=10");
        assert!(!pxf.contains("isrc"));

        let pstb = format!(
            "pstb1 pstb start={} stop={} {} probe={}",
            1.0, 1e6, sweep_points("dec", 10).unwrap(), "iprb".to_lowercase()
        );
        assert_eq!(pstb, "pstb1 pstb start=1 stop=1000000 dec=10 probe=iprb");
    }

    /// `run_netlist` must not wrap a Spectre-native deck in SPICE mode, which
    /// is what the default `Backend::run_netlist` -> `run` forwarding did.
    #[test]
    fn test_native_netlist_is_not_wrapped_in_spice_mode() {
        let native = "// t\n\nsimulator lang=spectre\n\nr1 (a b) resistor r=1k\nop1 dc\n";
        assert!(!wrap_spice_for_spectre(native).starts_with("// t"));
        // The Spectre-native path never calls wrap_spice_for_spectre; assert
        // the wrapper is only reachable from `run`.
        let wrapped = wrap_spice_for_spectre(native);
        assert!(wrapped.contains("simulator lang=spice"));
    }

    #[test]
    fn test_find_output_file_is_deterministic_and_prefers_analysis_files() {
        let dir = TempDir::new().unwrap();
        // Written out of lexical order on purpose.
        std::fs::write(dir.path().join("logFile"), b"log").unwrap();
        std::fs::write(dir.path().join("tran1.tran"), b"Title: x\n").unwrap();
        std::fs::write(dir.path().join("ac1.ac"), b"Title: x\n").unwrap();

        let (a, fa) = find_output_file(dir.path()).unwrap();
        let (b, fb) = find_output_file(dir.path()).unwrap();
        assert_eq!(a, b, "output discovery must be stable across calls");
        assert_eq!(fa, fb);
        assert_eq!(a.file_name().unwrap(), "ac1.ac");
        assert_eq!(fa, OutputFormat::Nutmeg);
    }

    #[test]
    fn test_find_output_file_errors_when_only_a_log_is_present() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("logFile"), b"log").unwrap();
        assert!(find_output_file(dir.path()).is_err());
    }
}
