pub mod ngspice;
pub mod xyce;
pub mod ltspice;
pub mod vacask;
pub mod spectre;
pub mod detect;

use crate::result::RawData;
use crate::rawfile;
use crate::codegen::CodeGen;
use crate::codegen::spice3::{Spice3CodeGen, Spice3Dialect};

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("No simulator backend found. Install ngspice: sudo apt install ngspice")]
    NoBackend,

    #[error("Analysis '{analysis}' not supported by {backend}.\n\n\
        This analysis requires {required}. Either:\n  \
        1. Install {required}: {install_cmd}\n  \
        2. Alternative: {alternative}")]
    UnsupportedAnalysis {
        analysis: String,
        backend: String,
        required: String,
        install_cmd: String,
        alternative: String,
    },

    #[error("Simulation failed: {0}")]
    SimulationError(String),

    #[error("Raw file parse error: {0}")]
    RawParseError(#[from] rawfile::RawFileError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Backend trait — each simulator implements this
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    fn run(&self, netlist: &str) -> Result<RawData, BackendError>;
    fn capabilities(&self) -> BackendCapabilities;

    /// The `CodeGen` that turns backend-neutral IR into this backend's netlist
    /// dialect. This is the only path from IR to text (ADR-0001). The default
    /// emits the SPICE3/ngspice dialect; backends override to name their own.
    fn codegen(&self) -> Box<dyn CodeGen> {
        Box::new(Spice3CodeGen { dialect: Spice3Dialect::Ngspice })
    }

    /// Execute an already-emitted netlist with no further string translation.
    /// Backends still carrying legacy string post-processing in `run` (vacask
    /// `spice_to_vacask`, spectre `wrap_spice_for_spectre`) keep the default,
    /// which routes through `run`; once a backend's `codegen` emits its native
    /// dialect it overrides this to execute the text verbatim.
    fn run_netlist(&self, netlist: &str) -> Result<RawData, BackendError> {
        self.run(netlist)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackendKind {
    NgspiceSubprocess,
    NgspiceShared,
    XyceSerial,
    XyceParallel,
    Ltspice {
        executable: std::path::PathBuf,
        use_wine: bool,
    },
    Vacask,
    VacaskShared,
    Spectre,
}

impl BackendKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ngspice-subprocess" | "ngspice" => Some(Self::NgspiceSubprocess),
            "ngspice-shared" => Some(Self::NgspiceShared),
            "xyce-serial" | "xyce" => Some(Self::XyceSerial),
            "xyce-parallel" => Some(Self::XyceParallel),
            "ltspice" => Some(Self::Ltspice {
                executable: std::path::PathBuf::from("ltspice"),
                use_wine: false,
            }),
            "vacask" => Some(Self::Vacask),
            "vacask-shared" => Some(Self::VacaskShared),
            "spectre" => Some(Self::Spectre),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::NgspiceSubprocess => "ngspice",
            Self::NgspiceShared => "ngspice-shared",
            Self::XyceSerial => "xyce",
            Self::XyceParallel => "xyce-parallel",
            Self::Ltspice { .. } => "ltspice",
            Self::Vacask => "vacask",
            Self::VacaskShared => "vacask-shared",
            Self::Spectre => "spectre",
        }
    }

    pub fn capabilities(&self) -> BackendCapabilities {
        match self {
            Self::NgspiceSubprocess | Self::NgspiceShared => ngspice::NGSPICE_CAPS,
            Self::XyceSerial | Self::XyceParallel => BackendCapabilities {
                xspice: false,
                osdi: false,
                measures: true,
                step_params: true,
                control_blocks: false,
                laplace_sources: false,
                verilog_cosim: false,
            },
            Self::Ltspice { .. } => BackendCapabilities {
                xspice: false,
                osdi: false,
                measures: true,
                step_params: true,
                control_blocks: false,
                laplace_sources: true,
                verilog_cosim: false,
            },
            Self::Vacask | Self::VacaskShared => vacask::VACASK_CAPS,
            Self::Spectre => BackendCapabilities {
                xspice: false,
                osdi: true,
                measures: false,
                step_params: true,
                control_blocks: false,
                laplace_sources: false,
                verilog_cosim: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BackendCapabilities {
    pub xspice: bool,
    pub osdi: bool,
    pub measures: bool,
    pub step_params: bool,
    pub control_blocks: bool,
    pub laplace_sources: bool,
    pub verilog_cosim: bool,
}

impl BackendCapabilities {
    pub fn supports_features(&self, features: &CircuitFeatures) -> bool {
        if features.has_xspice && !self.xspice { return false; }
        if features.has_osdi && !self.osdi { return false; }
        if features.has_measures && !self.measures { return false; }
        if features.has_step_params && !self.step_params { return false; }
        if features.has_control_blocks && !self.control_blocks { return false; }
        if features.has_laplace_sources && !self.laplace_sources { return false; }
        if features.has_verilog_cosim && !self.verilog_cosim { return false; }
        true
    }
}

/// Circuit feature flags that affect backend selection.
///
/// Built from both the `Circuit` (element types, raw lines, OSDI loads)
/// and the `CircuitSimulator` config (measures, step params). Each flag
/// acts as a hard constraint — backends that don't support the feature
/// are filtered out during auto-selection.
#[derive(Debug, Default, Clone)]
pub struct CircuitFeatures {
    /// Circuit uses XSPICE A-elements (only ngspice supports these)
    pub has_xspice: bool,
    /// Circuit loads OSDI/Verilog-A compiled models
    pub has_osdi: bool,
    /// Simulator uses .meas directives (ngspice, xyce, ltspice)
    pub has_measures: bool,
    /// Simulator uses .step param sweeps (xyce, ltspice, spectre)
    pub has_step_params: bool,
    /// Circuit contains .control blocks in raw lines (ngspice only)
    pub has_control_blocks: bool,
    /// Circuit uses Laplace-domain B-sources (ngspice, ltspice)
    pub has_laplace_sources: bool,
    /// Circuit uses Verilog co-simulation (ngspice d_cosim, spectre xrun)
    pub has_verilog_cosim: bool,
    /// Total element count — used to prefer xyce-parallel for large circuits
    pub element_count: usize,
}

impl From<&crate::ir::FeatureFlags> for CircuitFeatures {
    fn from(f: &crate::ir::FeatureFlags) -> Self {
        CircuitFeatures {
            has_xspice: f.has_xspice,
            has_osdi: f.has_osdi,
            has_measures: f.has_measures,
            has_step_params: f.has_step_params,
            has_control_blocks: f.has_control_blocks,
            has_laplace_sources: f.has_laplace_sources,
            has_verilog_cosim: f.has_verilog_cosim,
            element_count: f.element_count,
        }
    }
}

/// Analysis routing preferences — which backends support which analyses
fn analysis_backend_preference(analysis_type: &str) -> &'static [&'static str] {
    match analysis_type {
        // NGSpice-only
        "pz" | "disto" => &["ngspice"],
        // TF: ngspice and ltspice have native .tf, vacask has dcxf, spectre has xf
        "tf" => &["ngspice", "ltspice", "vacask", "spectre"],
        // Harmonic Balance: xyce, vacask, spectre
        "hb" => &["xyce", "vacask", "spectre"],
        // S-parameters
        "sp" | "s_param" => &["xyce", "vacask", "spectre", "ngspice"],
        // Stability (loop gain)
        "stb" | "stability" => &["vacask", "spectre"],
        // PSS
        "pss" => &["spectre", "ngspice"],
        // Transient noise
        "trannoise" => &["vacask"],
        // Xyce-only statistical
        "sampling" | "pce" | "embedded_sampling" => &["xyce"],
        // Xyce-only: transient/adjoint sensitivity
        "sens_tran" | "sens_adjoint" => &["xyce"],
        // AC sensitivity
        "sens_ac" => &["ngspice", "xyce", "spectre"],
        // Spectre-only periodic analyses
        "pac" | "pnoise" | "pxf" | "pstb" | "psp" | "pdisto" |
        "hbac" | "hbnoise" | "hbsp" => &["spectre"],
        // Universal analyses — all backends
        _ => &["ngspice", "xyce", "ltspice", "vacask", "spectre"],
    }
}

/// Auto-detect and select best backend for given analysis type and circuit features
pub fn detect_and_select(
    analysis_type: &str,
    override_backend: Option<&str>,
) -> Result<Box<dyn Backend>, BackendError> {
    detect_and_select_with_features(analysis_type, override_backend, &CircuitFeatures::default())
}

/// Auto-detect and select best backend, considering circuit features
/// (XSPICE, OSDI) in addition to analysis type.
pub fn detect_and_select_with_features(
    analysis_type: &str,
    override_backend: Option<&str>,
    features: &CircuitFeatures,
) -> Result<Box<dyn Backend>, BackendError> {
    if let Some(name) = override_backend {
        return create_backend_by_name(name, analysis_type);
    }

    let available = detect::detect_backends();
    if available.is_empty() {
        return Err(BackendError::NoBackend);
    }

    let preferences = analysis_backend_preference(analysis_type);

    // Try each preferred backend in order, filtering by circuit feature support
    for &pref in preferences {
        for kind in available.iter() {
            let name = kind.display_name();
            if !name.starts_with(pref) {
                continue;
            }
            if !kind.capabilities().supports_features(features) {
                continue;
            }
            return create_backend_from_kind(kind);
        }
    }

    // Fallback: if no preferred backend found, give a helpful error
    if !preferences.is_empty() && preferences[0] != "ngspice" {
        let required = preferences[0];
        let alt = match analysis_type {
            "pz" => ".ac() + post-process for poles/zeros",
            "tf" => ".dc() with small-signal sweep",
            "disto" => ".transient() + FFT post-processing",
            "hb" => "long .tran() + discard startup + FFT",
            "stb" | "stability" => ".ac() + Middlebrook method",
            "pss" => "long .tran() + discard startup",
            "trannoise" => ".tran() with trnoise sources",
            "sampling" | "pce" | "embedded_sampling" => ".control Monte Carlo loops (ngspice)",
            _ => "N/A",
        };
        return Err(BackendError::UnsupportedAnalysis {
            analysis: analysis_type.to_string(),
            backend: available.iter().map(|b| b.display_name()).collect::<Vec<_>>().join(", "),
            required: required.to_string(),
            install_cmd: format!("See docs/backends/{}.md", required),
            alternative: alt.to_string(),
        });
    }

    // Absolute fallback: use first available
    create_backend_from_kind(&available[0])
}

fn create_backend_from_kind(kind: &BackendKind) -> Result<Box<dyn Backend>, BackendError> {
    match kind {
        BackendKind::NgspiceSubprocess => Ok(Box::new(ngspice::NgspiceSubprocess)),
        BackendKind::NgspiceShared => {
            Ok(Box::new(ngspice::NgspiceShared::new()?))
        }
        BackendKind::XyceSerial => Ok(Box::new(xyce::XyceSubprocess { parallel: false })),
        BackendKind::XyceParallel => Ok(Box::new(xyce::XyceSubprocess { parallel: true })),
        BackendKind::Ltspice { executable, use_wine } => Ok(Box::new(ltspice::LtspiceSubprocess {
            executable: executable.clone(),
            use_wine: *use_wine,
            fast_access: false,
        })),
        BackendKind::Vacask => Ok(Box::new(vacask::VacaskSubprocess)),
        BackendKind::VacaskShared => {
            Ok(Box::new(vacask::VacaskLibrary::new()?))
        }
        BackendKind::Spectre => Ok(Box::new(spectre::SpectreSubprocess)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_supports_features_xspice() {
        let caps_with = BackendCapabilities {
            xspice: true,
            osdi: false,
            measures: false,
            step_params: false,
            control_blocks: false,
            laplace_sources: false,
            verilog_cosim: false,
        };
        let caps_without = BackendCapabilities {
            xspice: false,
            ..caps_with
        };
        let features = CircuitFeatures { has_xspice: true, ..Default::default() };
        assert!(caps_with.supports_features(&features));
        assert!(!caps_without.supports_features(&features));
    }

    #[test]
    fn test_capabilities_supports_features_all_false_passes() {
        let caps = BackendCapabilities {
            xspice: false,
            osdi: false,
            measures: false,
            step_params: false,
            control_blocks: false,
            laplace_sources: false,
            verilog_cosim: false,
        };
        let features = CircuitFeatures::default();
        assert!(caps.supports_features(&features));
    }

    #[test]
    fn test_capabilities_supports_features_combined() {
        let caps = BackendCapabilities {
            xspice: true,
            osdi: false,
            measures: true,
            step_params: false,
            control_blocks: false,
            laplace_sources: false,
            verilog_cosim: false,
        };
        let features_both = CircuitFeatures {
            has_xspice: true,
            has_measures: true,
            ..Default::default()
        };
        let features_missing = CircuitFeatures {
            has_xspice: true,
            has_osdi: true,
            ..Default::default()
        };
        assert!(caps.supports_features(&features_both));
        assert!(!caps.supports_features(&features_missing));
    }

    #[test]
    fn test_ngspice_capabilities() {
        let b = ngspice::NgspiceSubprocess;
        let c = b.capabilities();
        assert!(c.xspice);
        assert!(c.osdi);
        assert!(c.measures);
        assert!(!c.step_params);
        assert!(c.control_blocks);
        assert!(c.laplace_sources);
        assert!(c.verilog_cosim);
    }

    #[test]
    fn test_xyce_capabilities() {
        let b = xyce::XyceSubprocess { parallel: false };
        let c = b.capabilities();
        assert!(!c.xspice);
        assert!(!c.osdi);
        assert!(c.measures);
        assert!(c.step_params);
        assert!(!c.control_blocks);
        assert!(!c.laplace_sources);
        assert!(!c.verilog_cosim);
    }

    #[test]
    fn test_ltspice_capabilities() {
        let b = ltspice::LtspiceSubprocess {
            executable: std::path::PathBuf::from("ltspice"),
            use_wine: false,
            fast_access: false,
        };
        let c = b.capabilities();
        assert!(!c.xspice);
        assert!(!c.osdi);
        assert!(c.measures);
        assert!(c.step_params);
        assert!(!c.control_blocks);
        assert!(c.laplace_sources);
        assert!(!c.verilog_cosim);
    }

    #[test]
    fn test_vacask_capabilities() {
        let b = vacask::VacaskSubprocess;
        let c = b.capabilities();
        assert!(!c.xspice);
        assert!(c.osdi);
        assert!(!c.measures);
        assert!(!c.step_params);
        assert!(!c.control_blocks);
        assert!(!c.laplace_sources);
        assert!(!c.verilog_cosim);
    }

    #[test]
    fn test_spectre_capabilities() {
        let b = spectre::SpectreSubprocess;
        let c = b.capabilities();
        assert!(!c.xspice);
        assert!(c.osdi);
        assert!(!c.measures);
        assert!(c.step_params);
        assert!(!c.control_blocks);
        assert!(!c.laplace_sources);
        assert!(c.verilog_cosim);
    }

    #[test]
    fn test_backendkind_capabilities_match_trait() {
        let cases: Vec<(BackendKind, Box<dyn Backend>)> = vec![
            (BackendKind::NgspiceSubprocess, Box::new(ngspice::NgspiceSubprocess)),
            (BackendKind::XyceSerial, Box::new(xyce::XyceSubprocess { parallel: false })),
            (BackendKind::XyceParallel, Box::new(xyce::XyceSubprocess { parallel: true })),
            (BackendKind::Ltspice {
                executable: std::path::PathBuf::from("ltspice"),
                use_wine: false,
            }, Box::new(ltspice::LtspiceSubprocess {
                executable: std::path::PathBuf::from("ltspice"),
                use_wine: false,
                fast_access: false,
            })),
            (BackendKind::Vacask, Box::new(vacask::VacaskSubprocess)),
            (BackendKind::Spectre, Box::new(spectre::SpectreSubprocess)),
        ];

        for (kind, backend) in &cases {
            let kc = kind.capabilities();
            let bc = backend.capabilities();
            assert_eq!(kc.xspice, bc.xspice, "{}", kind.display_name());
            assert_eq!(kc.osdi, bc.osdi, "{}", kind.display_name());
            assert_eq!(kc.measures, bc.measures, "{}", kind.display_name());
            assert_eq!(kc.step_params, bc.step_params, "{}", kind.display_name());
            assert_eq!(kc.control_blocks, bc.control_blocks, "{}", kind.display_name());
            assert_eq!(kc.laplace_sources, bc.laplace_sources, "{}", kind.display_name());
            assert_eq!(kc.verilog_cosim, bc.verilog_cosim, "{}", kind.display_name());
        }
    }

    #[test]
    fn test_kind_supports_features_xspice_filters_non_ngspice() {
        let f = CircuitFeatures { has_xspice: true, ..Default::default() };
        assert!(BackendKind::NgspiceSubprocess.capabilities().supports_features(&f));
        assert!(!BackendKind::XyceSerial.capabilities().supports_features(&f));
        assert!(!BackendKind::Vacask.capabilities().supports_features(&f));
        assert!(!BackendKind::Spectre.capabilities().supports_features(&f));
    }

    #[test]
    fn test_kind_supports_features_osdi_filters_xyce_ltspice() {
        let f = CircuitFeatures { has_osdi: true, ..Default::default() };
        assert!(BackendKind::NgspiceSubprocess.capabilities().supports_features(&f));
        assert!(!BackendKind::XyceSerial.capabilities().supports_features(&f));
        assert!(BackendKind::Vacask.capabilities().supports_features(&f));
        assert!(BackendKind::Spectre.capabilities().supports_features(&f));
    }

    #[test]
    fn test_kind_supports_features_step_and_measures() {
        let f = CircuitFeatures {
            has_step_params: true,
            has_measures: true,
            ..Default::default()
        };
        assert!(!BackendKind::NgspiceSubprocess.capabilities().supports_features(&f));
        assert!(BackendKind::XyceSerial.capabilities().supports_features(&f));
        assert!(!BackendKind::Vacask.capabilities().supports_features(&f));
        assert!(!BackendKind::Spectre.capabilities().supports_features(&f));
    }

    #[test]
    fn test_analysis_backend_preference_pz_ngspice_only() {
        let prefs = analysis_backend_preference("pz");
        assert_eq!(prefs, &["ngspice"]);
    }

    #[test]
    fn test_analysis_backend_preference_disto_ngspice_only() {
        let prefs = analysis_backend_preference("disto");
        assert_eq!(prefs, &["ngspice"]);
    }

    #[test]
    fn test_analysis_backend_preference_hb() {
        let prefs = analysis_backend_preference("hb");
        assert_eq!(prefs, &["xyce", "vacask", "spectre"]);
    }

    #[test]
    fn test_analysis_backend_preference_stability() {
        let prefs = analysis_backend_preference("stb");
        assert_eq!(prefs, &["vacask", "spectre"]);
    }

    #[test]
    fn test_analysis_backend_preference_trannoise_vacask_only() {
        let prefs = analysis_backend_preference("trannoise");
        assert_eq!(prefs, &["vacask"]);
    }

    #[test]
    fn test_analysis_backend_preference_universal_includes_all() {
        let prefs = analysis_backend_preference("tran");
        assert!(prefs.contains(&"ngspice"));
        assert!(prefs.contains(&"xyce"));
        assert!(prefs.contains(&"ltspice"));
        assert!(prefs.contains(&"vacask"));
        assert!(prefs.contains(&"spectre"));
    }

    #[test]
    fn test_analysis_backend_preference_spectre_only_analyses() {
        for analysis in &["pac", "pnoise", "pxf", "pstb", "psp", "pdisto", "hbac", "hbnoise", "hbsp"] {
            let prefs = analysis_backend_preference(analysis);
            assert_eq!(prefs, &["spectre"], "Expected spectre-only for {}", analysis);
        }
    }

    #[test]
    fn test_analysis_backend_preference_xyce_only_analyses() {
        for analysis in &["sampling", "pce", "embedded_sampling", "sens_tran", "sens_adjoint"] {
            let prefs = analysis_backend_preference(analysis);
            assert_eq!(prefs, &["xyce"], "Expected xyce-only for {}", analysis);
        }
    }

    #[test]
    fn test_circuit_features_default_all_false() {
        let f = CircuitFeatures::default();
        assert!(!f.has_xspice);
        assert!(!f.has_osdi);
        assert!(!f.has_measures);
        assert!(!f.has_step_params);
        assert!(!f.has_control_blocks);
        assert!(!f.has_laplace_sources);
        assert!(!f.has_verilog_cosim);
        assert_eq!(f.element_count, 0);
    }

}

fn create_backend_by_name(name: &str, analysis_type: &str) -> Result<Box<dyn Backend>, BackendError> {
    // Check for analysis compatibility with the requested backend
    let incompatible = match name {
        "xyce" | "xyce-serial" | "xyce-parallel" => {
            matches!(analysis_type, "pz" | "disto")
        }
        "ltspice" => {
            matches!(analysis_type, "pz" | "disto" | "sens" | "sens_ac" | "hb" | "pss" | "stb")
        }
        "vacask" => {
            matches!(analysis_type, "pz" | "disto" | "sens" | "sens_ac" | "dc")
        }
        _ => false,
    };

    if incompatible {
        return Err(BackendError::UnsupportedAnalysis {
            analysis: analysis_type.to_string(),
            backend: name.to_string(),
            required: analysis_backend_preference(analysis_type).first().unwrap_or(&"ngspice").to_string(),
            install_cmd: "See docs/backends/ for installation instructions".to_string(),
            alternative: "See docs/backends/analysis-map.md for emulation strategies".to_string(),
        });
    }

    match name {
        "ngspice-subprocess" | "ngspice" => Ok(Box::new(ngspice::NgspiceSubprocess)),
        "ngspice-shared" => Ok(Box::new(ngspice::NgspiceShared::new()?)),
        "xyce-serial" | "xyce" => Ok(Box::new(xyce::XyceSubprocess { parallel: false })),
        "xyce-parallel" => Ok(Box::new(xyce::XyceSubprocess { parallel: true })),
        "ltspice" => {
            if let Some((exe, wine)) = ltspice::detect_ltspice() {
                Ok(Box::new(ltspice::LtspiceSubprocess { executable: exe, use_wine: wine, fast_access: false }))
            } else {
                Err(BackendError::SimulationError("LTspice not found on this system".to_string()))
            }
        }
        "vacask" => Ok(Box::new(vacask::VacaskSubprocess)),
        "vacask-shared" => Ok(Box::new(vacask::VacaskLibrary::new()?)),
        "spectre" => Ok(Box::new(spectre::SpectreSubprocess)),
        _ => Err(BackendError::SimulationError(format!("Unknown backend: {}", name))),
    }
}
