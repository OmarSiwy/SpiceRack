use std::collections::HashMap;
use num_complex::Complex64;

/// Raw simulation data parsed from .raw file
#[derive(Debug, Clone)]
pub struct RawData {
    pub title: String,
    pub plot_name: String,
    pub flags: String,
    pub variables: Vec<VarInfo>,
    /// Real data vectors (each inner vec = one variable's values across sweep)
    pub real_data: Vec<Vec<f64>>,
    /// Complex data vectors (for AC analysis)
    pub complex_data: Vec<Vec<Complex64>>,
    pub is_complex: bool,
    /// Captured stdout from simulator process (contains .meas results, etc.)
    pub stdout: String,
    /// Captured log file content (LTspice puts .meas results in .log)
    pub log_content: String,
    /// Parsed .meas results extracted from stdout/log
    pub measures: Vec<MeasureResult>,
    /// Which backend produced this data (for name normalization).
    /// Empty string defaults to "ngspice" behavior.
    pub backend_hint: String,
}

#[derive(Debug, Clone)]
pub struct VarInfo {
    pub index: usize,
    pub name: String,
    pub var_type: String, // "voltage", "current", "time", "frequency"
}

impl RawData {
    pub fn empty() -> Self {
        Self {
            title: String::new(),
            plot_name: String::new(),
            flags: String::new(),
            variables: Vec::new(),
            real_data: Vec::new(),
            complex_data: Vec::new(),
            is_complex: false,
            stdout: String::new(),
            log_content: String::new(),
            measures: Vec::new(),
            backend_hint: String::new(),
        }
    }
}

/// A named waveform vector with optional unit info
#[derive(Debug, Clone)]
pub struct WaveForm {
    pub name: String,
    /// Real part. For a complex plot this is `Re`, not the magnitude.
    pub data: Vec<f64>,
    /// Present only for complex plots (AC, noise). Kept so magnitude and
    /// phase are reachable: `data` alone cannot reconstruct them.
    pub complex: Option<Vec<Complex64>>,
}

impl WaveForm {
    pub fn real(name: String, data: Vec<f64>) -> Self {
        Self { name, data, complex: None }
    }

    pub fn complex(name: String, data: &[Complex64]) -> Self {
        Self {
            name,
            data: data.iter().map(|c| c.re).collect(),
            complex: Some(data.to_vec()),
        }
    }

    /// `sqrt(re^2 + im^2)` per point; `None` for a real-valued plot.
    pub fn magnitude(&self) -> Option<Vec<f64>> {
        self.complex
            .as_ref()
            .map(|c| c.iter().map(|v| v.norm()).collect())
    }

    /// Magnitude in dB (`20*log10`); `None` for a real-valued plot.
    pub fn magnitude_db(&self) -> Option<Vec<f64>> {
        self.complex
            .as_ref()
            .map(|c| c.iter().map(|v| 20.0 * v.norm().log10()).collect())
    }

    /// Phase in degrees. ngspice's own `vp()` reports radians; this does not.
    pub fn phase_deg(&self) -> Option<Vec<f64>> {
        self.complex
            .as_ref()
            .map(|c| c.iter().map(|v| v.arg().to_degrees()).collect())
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Base analysis result: maps node/branch names -> waveforms
#[derive(Debug, Clone)]
pub struct AnalysisBase {
    pub nodes: HashMap<String, WaveForm>,
    pub branches: HashMap<String, WaveForm>,
    pub measures: Vec<MeasureResult>,
}

impl AnalysisBase {
    pub fn from_raw(raw: &RawData) -> Self {
        let backend = if raw.backend_hint.is_empty() {
            "ngspice"
        } else {
            &raw.backend_hint
        };
        Self::from_raw_with_backend(raw, backend)
    }

    pub fn from_raw_with_backend(raw: &RawData, backend: &str) -> Self {
        use crate::normalize;

        let mut nodes = HashMap::new();
        let mut branches = HashMap::new();

        for var in &raw.variables {
            let wf = if raw.is_complex {
                WaveForm::complex(var.name.clone(), &raw.complex_data[var.index])
            } else {
                WaveForm::real(var.name.clone(), raw.real_data[var.index].clone())
            };

            let normalized = normalize::normalize_var_name(&var.name, backend);

            // Classify: currents go to branches, everything else to nodes
            if is_current(&var.name, &var.var_type, backend) {
                branches.insert(normalized, wf);
            } else {
                nodes.insert(normalized, wf);
            }
        }

        Self { nodes, branches, measures: raw.measures.clone() }
    }

    pub fn get(&self, name: &str) -> Option<&WaveForm> {
        let lower = name.to_lowercase();
        self.nodes.get(&lower)
            .or_else(|| self.branches.get(&lower))
            .or_else(|| self.nodes.get(name))
            .or_else(|| self.branches.get(name))
    }

    /// Get parsed .meas results
    pub fn measures(&self) -> &[MeasureResult] {
        &self.measures
    }

    /// Get a specific .meas result by name (case-insensitive)
    pub fn measure(&self, name: &str) -> Option<f64> {
        let lower = name.to_lowercase();
        self.measures.iter()
            .find(|m| m.name.to_lowercase() == lower)
            .map(|m| m.value)
    }
}

/// Classify whether a variable represents a current measurement.
fn is_current(name: &str, var_type: &str, backend: &str) -> bool {
    if var_type == "current" {
        return true;
    }
    crate::normalize::is_current_name(name, backend)
}

// ── Specific analysis result types ──

#[derive(Debug, Clone)]
pub struct OperatingPoint {
    pub base: AnalysisBase,
}

impl OperatingPoint {
    pub fn from_raw(raw: RawData) -> Self {
        Self { base: AnalysisBase::from_raw(&raw) }
    }

    pub fn get(&self, name: &str) -> Option<f64> {
        self.base.get(name).and_then(|wf| wf.data.first().copied())
    }
}

#[derive(Debug, Clone)]
pub struct DcAnalysis {
    pub base: AnalysisBase,
    pub sweep: Vec<f64>,
}

impl DcAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let sweep = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            sweep,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AcAnalysis {
    pub base: AnalysisBase,
    pub frequency: Vec<f64>,
}

impl AcAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let frequency = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else if !raw.complex_data.is_empty() {
            raw.complex_data[0].iter().map(|c| c.re).collect()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            frequency,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransientAnalysis {
    pub base: AnalysisBase,
    pub time: Vec<f64>,
}

impl TransientAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let time = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            time,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoiseAnalysis {
    pub base: AnalysisBase,
}

impl NoiseAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        Self { base: AnalysisBase::from_raw(&raw) }
    }
}

#[derive(Debug, Clone)]
pub struct TransferFunctionAnalysis {
    pub base: AnalysisBase,
}

impl TransferFunctionAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        Self { base: AnalysisBase::from_raw(&raw) }
    }
}

#[derive(Debug, Clone)]
pub struct SensitivityAnalysis {
    pub base: AnalysisBase,
}

impl SensitivityAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        Self { base: AnalysisBase::from_raw(&raw) }
    }
}

#[derive(Debug, Clone)]
pub struct PoleZeroAnalysis {
    pub base: AnalysisBase,
}

impl PoleZeroAnalysis {
    /// Poles and zeros arrive as ordinary named vectors in the raw file and are
    /// reached through `base` (e.g. `pz["pole(1)"]`).
    pub fn from_raw(raw: RawData) -> Self {
        Self {
            base: AnalysisBase::from_raw(&raw),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DistortionAnalysis {
    pub base: AnalysisBase,
    pub frequency: Vec<f64>,
}

impl DistortionAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let frequency = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            frequency,
        }
    }
}

// ── New analysis result types ──

#[derive(Debug, Clone)]
pub struct PssAnalysis {
    pub base: AnalysisBase,
    pub time: Vec<f64>,
}

impl PssAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let time = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            time,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SParamAnalysis {
    pub base: AnalysisBase,
    pub frequency: Vec<f64>,
}

impl SParamAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let frequency = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else if !raw.complex_data.is_empty() {
            raw.complex_data[0].iter().map(|c| c.re).collect()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            frequency,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HarmonicBalanceAnalysis {
    pub base: AnalysisBase,
    pub frequency: Vec<f64>,
}

impl HarmonicBalanceAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let frequency = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else if !raw.complex_data.is_empty() {
            raw.complex_data[0].iter().map(|c| c.re).collect()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            frequency,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StabilityAnalysis {
    pub base: AnalysisBase,
    pub frequency: Vec<f64>,
}

impl StabilityAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let frequency = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else if !raw.complex_data.is_empty() {
            raw.complex_data[0].iter().map(|c| c.re).collect()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            frequency,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeasureResult {
    pub name: String,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct TransientNoiseAnalysis {
    pub base: AnalysisBase,
    pub time: Vec<f64>,
}

impl TransientNoiseAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let time = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else {
            Vec::new()
        };
        Self {
            base: AnalysisBase::from_raw(&raw),
            time,
        }
    }
}

// ── Xyce-specific analysis result types ──

/// Result from Xyce .SAMPLING / .EMBEDDEDSAMPLING / .PCE analysis
#[derive(Debug, Clone)]
pub struct SamplingAnalysis {
    pub base: AnalysisBase,
}

impl SamplingAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        Self {
            base: AnalysisBase::from_raw(&raw),
        }
    }
}

/// Options for Xyce .FFT analysis
#[derive(Debug, Clone)]
pub struct XyceFftOptions {
    /// Number of points (should be power of 2)
    pub np: u32,
    /// Start time for FFT window
    pub start: f64,
    /// Stop time for FFT window
    pub stop: f64,
    /// Window function: "HANN", "RECT", "BARTLETT", "BLACKMAN", "HAMMING", etc.
    pub window: String,
    /// Output format: "UNORM", "NORM", "MAG"
    pub format: String,
}

impl Default for XyceFftOptions {
    fn default() -> Self {
        Self {
            np: 1024,
            start: 0.0,
            stop: 1e-3,
            window: "HANN".to_string(),
            format: "UNORM".to_string(),
        }
    }
}

/// Result from Xyce .FFT analysis
#[derive(Debug, Clone)]
pub struct XyceFftAnalysis {
    pub base: AnalysisBase,
    pub frequency: Vec<f64>,
    pub magnitude: Vec<f64>,
    pub phase: Vec<f64>,
    /// Effective Number of Bits
    pub enob: f64,
    /// Spurious-Free Dynamic Range in dB
    pub sfdr_db: f64,
    /// Signal-to-Noise Ratio in dB
    pub snr_db: f64,
    /// Total Harmonic Distortion in dB
    pub thd_db: f64,
}

impl XyceFftAnalysis {
    pub fn from_raw(raw: RawData) -> Self {
        let base = AnalysisBase::from_raw(&raw);

        // Extract frequency, magnitude, and phase from raw data
        let frequency = if !raw.real_data.is_empty() {
            raw.real_data[0].clone()
        } else {
            Vec::new()
        };

        // Magnitude is typically the second variable, phase the third
        let magnitude = if raw.real_data.len() > 1 {
            raw.real_data[1].clone()
        } else {
            Vec::new()
        };

        let phase = if raw.real_data.len() > 2 {
            raw.real_data[2].clone()
        } else {
            Vec::new()
        };

        // Compute spectral metrics from magnitude data
        let (enob, sfdr_db, snr_db, thd_db) = compute_fft_metrics(&magnitude);

        Self {
            base,
            frequency,
            magnitude,
            phase,
            enob,
            sfdr_db,
            snr_db,
            thd_db,
        }
    }
}

/// Compute ENOB, SFDR, SNR, and THD from FFT magnitude bins.
///
/// Assumes bin 0 is DC, bin with max magnitude is the fundamental,
/// and harmonics are at integer multiples of the fundamental bin index.
pub fn compute_fft_metrics(magnitude: &[f64]) -> (f64, f64, f64, f64) {
    if magnitude.len() < 4 {
        return (0.0, 0.0, 0.0, 0.0);
    }

    // Find the fundamental: largest magnitude bin (skip DC at index 0)
    let (fund_idx, fund_mag) = magnitude.iter().enumerate()
        .skip(1)
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((1, &0.0));

    if *fund_mag <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let fund_power = fund_mag * fund_mag;

    // Identify harmonic bins (2nd, 3rd, ... up to Nyquist)
    let mut harmonic_power = 0.0;
    let mut harmonic_idx = 2 * fund_idx;
    while harmonic_idx < magnitude.len() {
        harmonic_power += magnitude[harmonic_idx] * magnitude[harmonic_idx];
        harmonic_idx += fund_idx;
    }

    // Noise power: everything except DC, fundamental, and harmonics
    let mut noise_power = 0.0;
    let harmonic_bins: std::collections::HashSet<usize> = {
        let mut set = std::collections::HashSet::new();
        set.insert(0); // DC
        set.insert(fund_idx); // fundamental
        let mut h = 2 * fund_idx;
        while h < magnitude.len() {
            set.insert(h);
            h += fund_idx;
        }
        set
    };

    for (i, &m) in magnitude.iter().enumerate() {
        if !harmonic_bins.contains(&i) {
            noise_power += m * m;
        }
    }

    // THD = sqrt(sum of harmonic powers) / fundamental magnitude
    let thd = if fund_power > 0.0 {
        (harmonic_power / fund_power).sqrt()
    } else {
        0.0
    };
    let thd_db = if thd > 0.0 { 20.0 * thd.log10() } else { f64::NEG_INFINITY };

    // SNR = fundamental power / noise power (in dB)
    let snr_db = if noise_power > 0.0 {
        10.0 * (fund_power / noise_power).log10()
    } else {
        f64::INFINITY
    };

    // SFDR = fundamental magnitude / next-largest spur (in dB)
    let max_spur = magnitude.iter().enumerate()
        .skip(1)
        .filter(|(i, _)| *i != fund_idx)
        .map(|(_, &m)| m)
        .fold(0.0_f64, |a, b| a.max(b));
    let sfdr_db = if max_spur > 0.0 {
        20.0 * (fund_mag / max_spur).log10()
    } else {
        f64::INFINITY
    };

    // ENOB = (SNR - 1.76) / 6.02
    let sinad_db = if (noise_power + harmonic_power) > 0.0 {
        10.0 * (fund_power / (noise_power + harmonic_power)).log10()
    } else {
        f64::INFINITY
    };
    let enob = (sinad_db - 1.76) / 6.02;

    (enob, sfdr_db, snr_db, thd_db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waveform_complex_magnitude_and_phase() {
        // 3-4-5 triangle: |3+4i| = 5, arg = 53.13 deg
        let wf = WaveForm::complex("h".into(), &[Complex64::new(3.0, 4.0)]);
        assert!((wf.data[0] - 3.0).abs() < 1e-12, "data must stay the real part");
        assert!((wf.magnitude().unwrap()[0] - 5.0).abs() < 1e-12);
        assert!((wf.phase_deg().unwrap()[0] - 53.13010235).abs() < 1e-6);
        assert!((wf.magnitude_db().unwrap()[0] - 20.0 * 5.0f64.log10()).abs() < 1e-12);
    }

    #[test]
    fn test_waveform_real_has_no_complex_part() {
        let wf = WaveForm::real("v".into(), vec![1.0, 2.0]);
        assert!(wf.magnitude().is_none());
        assert!(wf.phase_deg().is_none());
    }
}
