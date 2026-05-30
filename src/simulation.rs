use crate::circuit::Circuit;
use crate::result::*;
use crate::backend::{Backend, BackendError, detect_and_select_with_features, CircuitFeatures, detect};

/// Parameter sweep definition for `.step` directives
#[derive(Debug, Clone)]
pub struct StepParam {
    pub param: String,
    pub start: f64,
    pub stop: f64,
    pub step: f64,
    /// Sweep type: "lin", "oct", "dec", or None (linear default)
    pub sweep_type: Option<String>,
}

/// Simulator configuration and analysis dispatch
pub struct CircuitSimulator {
    circuit: Circuit,
    backend_override: Option<String>,
    options: Vec<(String, String)>,
    initial_conditions: Vec<(String, f64)>,
    node_sets: Vec<(String, f64)>,
    saves: Vec<String>,
    save_currents: bool,
    temperature: Option<f64>,
    nominal_temperature: Option<f64>,
    measures: Vec<String>,
    step_params: Vec<StepParam>,
    /// Network parameter (.net) directive for LTspice S-parameter analysis
    _net_directive: Option<NetDirective>,
    /// Extra lines to inject before the analysis statement (e.g., .OPTIONS HBINT)
    extra_lines: Vec<String>,
    /// Backend-neutral IR for this simulation. When present, the netlist is
    /// emitted by the selected backend's `CodeGen` (the production path);
    /// when absent (legacy Rust/cabi callers) the string `build_netlist` path
    /// is used. See ADR-0001 / issue 01.
    ir: Option<crate::ir::CircuitIR>,
}

/// LTspice .NET directive for network parameter analysis
#[derive(Debug, Clone)]
pub struct NetDirective {
    pub output_current: String,
    pub input_source: String,
    pub z_in: f64,
    pub z_out: f64,
}

impl CircuitSimulator {
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            backend_override: None,
            options: Vec::new(),
            initial_conditions: Vec::new(),
            node_sets: Vec::new(),
            saves: Vec::new(),
            save_currents: false,
            temperature: None,
            nominal_temperature: None,
            measures: Vec::new(),
            step_params: Vec::new(),
            _net_directive: None,
            extra_lines: Vec::new(),
            ir: None,
        }
    }

    /// Attach the backend-neutral IR for this simulation. Once set, every run
    /// emits its netlist through the selected backend's `CodeGen` instead of
    /// `Circuit::Display`.
    pub fn set_ir(&mut self, ir: crate::ir::CircuitIR) {
        self.ir = Some(ir);
    }

    /// The netlist the given backend will actually run, sourced from its
    /// `CodeGen`. Requires an attached IR; this is the IR→text path that
    /// replaces string translation.
    pub fn netlist_to_run(&self, backend: &dyn Backend) -> Result<String, BackendError> {
        match &self.ir {
            Some(ir) => backend
                .codegen()
                .emit_netlist(ir)
                .map_err(|e| BackendError::SimulationError(e.to_string())),
            None => Err(BackendError::SimulationError(
                "no IR attached to simulator".into(),
            )),
        }
    }

    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend_override = Some(backend.into());
        self
    }

    pub fn circuit(&self) -> &Circuit {
        &self.circuit
    }

    pub fn options(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.options.push((key.into(), value.into()));
    }

    pub fn initial_condition(&mut self, node: impl Into<String>, value: f64) {
        self.initial_conditions.push((node.into(), value));
    }

    pub fn node_set(&mut self, node: impl Into<String>, value: f64) {
        self.node_sets.push((node.into(), value));
    }

    pub fn save(&mut self, what: impl Into<String>) {
        self.saves.push(what.into());
    }

    pub fn set_save_currents(&mut self, v: bool) {
        self.save_currents = v;
    }

    pub fn set_temperature(&mut self, temp: f64) {
        self.temperature = Some(temp);
    }

    pub fn set_nominal_temperature(&mut self, temp: f64) {
        self.nominal_temperature = Some(temp);
    }

    pub fn measure(&mut self, args: Vec<impl Into<String>>) {
        let parts: Vec<String> = args.into_iter().map(|a| a.into()).collect();
        self.measures.push(parts.join(" "));
    }

    /// Add a linear `.step param` sweep (LTspice/Xyce compatible)
    pub fn step(&mut self, param: &str, start: f64, stop: f64, step: f64) {
        self.step_params.push(StepParam {
            param: param.to_string(),
            start,
            stop,
            step,
            sweep_type: None,
        });
    }

    /// Add a `.step param` sweep with explicit sweep type ("lin", "oct", "dec")
    pub fn step_sweep(&mut self, param: &str, start: f64, stop: f64, step: f64, sweep_type: &str) {
        self.step_params.push(StepParam {
            param: param.to_string(),
            start,
            stop,
            step,
            sweep_type: Some(sweep_type.to_string()),
        });
    }

    /// LTspice .NET network parameter analysis.
    ///
    /// Generates a `.net` directive and an `.ac` sweep, returning S-parameter results.
    /// LTspice syntax: `.net I(R1) Vin Rout=50 Rin=50`
    pub fn network_params(
        &self,
        output_current: &str,
        input_source: &str,
        z_in: f64,
        z_out: f64,
        variation: &str,
        points: u32,
        start_freq: f64,
        stop_freq: f64,
    ) -> Result<SParamAnalysis, BackendError> {
        let net_stmt = format!(
            ".net {} {} Rout={} Rin={}",
            output_current, input_source, z_out, z_in
        );
        let ac_stmt = format!(
            ".ac {} {} {} {}",
            variation, points, start_freq, stop_freq
        );
        let combined = format!("{}\n{}", net_stmt, ac_stmt);
        let raw = self.run(&combined, "ac")?;
        Ok(SParamAnalysis::from_raw(raw))
    }

    // ── Analysis methods ──

    pub fn operating_point(&self) -> Result<OperatingPoint, BackendError> {
        let raw = self.run(".op", "op")?;
        Ok(OperatingPoint::from_raw(raw))
    }

    /// DC sweep. `sweeps` is vec of (source_name, start, stop, step) for nested sweeps.
    pub fn dc_multi(&self, sweeps: &[(&str, f64, f64, f64)]) -> Result<DcAnalysis, BackendError> {
        let mut parts = vec![".dc".to_string()];
        for (var, start, stop, step) in sweeps {
            parts.push(format!("{} {} {} {}", var, start, stop, step));
        }
        let analysis = parts.join(" ");
        let raw = self.run(&analysis, "dc")?;
        Ok(DcAnalysis::from_raw(raw))
    }

    pub fn dc(&self, sweep_var: &str, start: f64, stop: f64, step: f64) -> Result<DcAnalysis, BackendError> {
        self.dc_multi(&[(sweep_var, start, stop, step)])
    }

    pub fn ac(
        &self,
        variation: &str,
        number_of_points: u32,
        start_frequency: f64,
        stop_frequency: f64,
    ) -> Result<AcAnalysis, BackendError> {
        let analysis = format!(
            ".ac {} {} {} {}",
            variation, number_of_points, start_frequency, stop_frequency
        );
        let raw = self.run(&analysis, "ac")?;
        Ok(AcAnalysis::from_raw(raw))
    }

    pub fn transient(
        &self,
        step_time: f64,
        end_time: f64,
        start_time: Option<f64>,
        max_time: Option<f64>,
        use_initial_condition: bool,
    ) -> Result<TransientAnalysis, BackendError> {
        let mut analysis = format!(".tran {} {}", step_time, end_time);
        if let Some(st) = start_time {
            analysis.push_str(&format!(" {}", st));
            if let Some(mt) = max_time {
                analysis.push_str(&format!(" {}", mt));
            }
        } else if let Some(mt) = max_time {
            analysis.push_str(&format!(" 0 {}", mt));
        }
        if use_initial_condition {
            analysis.push_str(" uic");
        }
        let raw = self.run(&analysis, "tran")?;
        Ok(TransientAnalysis::from_raw(raw))
    }

    pub fn noise(
        &self,
        output_node: &str,
        ref_node: &str,
        src: &str,
        variation: &str,
        points: u32,
        start_frequency: f64,
        stop_frequency: f64,
        points_per_summary: Option<u32>,
    ) -> Result<NoiseAnalysis, BackendError> {
        let mut analysis = format!(
            ".noise V({},{}) {} {} {} {} {}",
            output_node, ref_node, src, variation, points, start_frequency, stop_frequency
        );
        if let Some(pps) = points_per_summary {
            analysis.push_str(&format!(" {}", pps));
        }
        let raw = self.run(&analysis, "noise")?;
        Ok(NoiseAnalysis::from_raw(raw))
    }

    pub fn transfer_function(
        &self,
        outvar: &str,
        insrc: &str,
    ) -> Result<TransferFunctionAnalysis, BackendError> {
        let analysis = format!(".tf {} {}", outvar, insrc);
        let raw = self.run(&analysis, "tf")?;
        Ok(TransferFunctionAnalysis::from_raw(raw))
    }

    /// Alias for transfer_function
    pub fn tf(
        &self,
        outvar: &str,
        insrc: &str,
    ) -> Result<TransferFunctionAnalysis, BackendError> {
        self.transfer_function(outvar, insrc)
    }

    pub fn dc_sensitivity(&self, output_variable: &str) -> Result<SensitivityAnalysis, BackendError> {
        let analysis = format!(".sens {}", output_variable);
        let raw = self.run(&analysis, "sens")?;
        Ok(SensitivityAnalysis::from_raw(raw))
    }

    pub fn ac_sensitivity(
        &self,
        output_variable: &str,
        variation: &str,
        number_of_points: u32,
        start_frequency: f64,
        stop_frequency: f64,
    ) -> Result<SensitivityAnalysis, BackendError> {
        let analysis = format!(
            ".sens {} ac {} {} {} {}",
            output_variable, variation, number_of_points, start_frequency, stop_frequency
        );
        let raw = self.run(&analysis, "sens_ac")?;
        Ok(SensitivityAnalysis::from_raw(raw))
    }

    pub fn polezero(
        &self,
        node1: &str,
        node2: &str,
        node3: &str,
        node4: &str,
        tf_type: &str,
        pz_type: &str,
    ) -> Result<PoleZeroAnalysis, BackendError> {
        let analysis = format!(
            ".pz {} {} {} {} {} {}",
            node1, node2, node3, node4, tf_type, pz_type
        );
        let raw = self.run(&analysis, "pz")?;
        Ok(PoleZeroAnalysis::from_raw(raw))
    }

    pub fn distortion(
        &self,
        variation: &str,
        points: u32,
        start_frequency: f64,
        stop_frequency: f64,
        f2overf1: Option<f64>,
    ) -> Result<DistortionAnalysis, BackendError> {
        let mut analysis = format!(
            ".disto {} {} {} {}",
            variation, points, start_frequency, stop_frequency
        );
        if let Some(ratio) = f2overf1 {
            analysis.push_str(&format!(" {}", ratio));
        }
        let raw = self.run(&analysis, "disto")?;
        Ok(DistortionAnalysis::from_raw(raw))
    }

    // ── New analyses ──

    /// Periodic Steady State analysis (ngspice experimental, spectre)
    pub fn pss(
        &self,
        fundamental_frequency: f64,
        stabilization_time: f64,
        observe_node: &str,
        points_per_period: u32,
        harmonics: u32,
    ) -> Result<PssAnalysis, BackendError> {
        let analysis = format!(
            ".pss {} {} {} {} {}",
            fundamental_frequency, stabilization_time, observe_node,
            points_per_period, harmonics
        );
        let raw = self.run(&analysis, "pss")?;
        Ok(PssAnalysis::from_raw(raw))
    }

    /// S-parameter analysis
    pub fn s_param(
        &self,
        variation: &str,
        number_of_points: u32,
        start_frequency: f64,
        stop_frequency: f64,
    ) -> Result<SParamAnalysis, BackendError> {
        // Different backends use different syntax:
        // ngspice: .sp dec N fstart fstop
        // xyce: .AC + .LIN sparcalc=1
        let analysis = format!(
            ".sp {} {} {} {}",
            variation, number_of_points, start_frequency, stop_frequency
        );
        let raw = self.run(&analysis, "sp")?;
        Ok(SParamAnalysis::from_raw(raw))
    }

    /// Harmonic Balance analysis (Xyce, Vacask, Spectre).
    ///
    /// Xyce uses a single `numfreq` value for all tones, so we take the max of
    /// the `num_harmonics` slice and emit `.OPTIONS HBINT numfreq=N` before `.HB`.
    pub fn harmonic_balance(
        &self,
        fundamental_frequencies: &[f64],
        num_harmonics: &[u32],
    ) -> Result<HarmonicBalanceAnalysis, BackendError> {
        let freq_str: Vec<String> = fundamental_frequencies.iter()
            .map(|f| format!("{}", f))
            .collect();

        // Xyce uses one numfreq for all tones — take the max
        let max_harmonics = num_harmonics.iter().copied().max().unwrap_or(7);
        let options_line = format!(".OPTIONS HBINT numfreq={}", max_harmonics);
        let hb_line = format!(".HB {}", freq_str.join(" "));
        let analysis = format!("{}\n{}", options_line, hb_line);

        let raw = self.run(&analysis, "hb")?;
        Ok(HarmonicBalanceAnalysis::from_raw(raw))
    }

    /// Stability / loop gain analysis (Vacask acstb, Spectre stb)
    pub fn stability(
        &self,
        probe: &str,
        variation: &str,
        number_of_points: u32,
        start_frequency: f64,
        stop_frequency: f64,
    ) -> Result<StabilityAnalysis, BackendError> {
        // Backend-dependent — vacask uses `acstb`, spectre uses `stb`
        let analysis = format!(
            ".acstb {} {} {} {} probe={}",
            variation, number_of_points, start_frequency, stop_frequency, probe
        );
        let raw = self.run(&analysis, "stb")?;
        Ok(StabilityAnalysis::from_raw(raw))
    }

    /// Transient noise analysis (Vacask only)
    pub fn transient_noise(
        &self,
        step_time: f64,
        end_time: f64,
    ) -> Result<TransientNoiseAnalysis, BackendError> {
        let analysis = format!(".trannoise {} {}", step_time, end_time);
        let raw = self.run(&analysis, "trannoise")?;
        Ok(TransientNoiseAnalysis::from_raw(raw))
    }

    /// Fourier analysis — post-processes transient data
    /// Returns parsed stdout (batch mode only for ngspice)
    pub fn fourier(
        &self,
        fundamental_frequency: f64,
        output_variables: &[&str],
        num_harmonics: Option<u32>,
    ) -> Result<TransientAnalysis, BackendError> {
        // Fourier requires a transient run — we add .four after .tran
        // The .four results come from stdout, but the .tran data goes to raw
        let mut four_stmt = format!(".four {}", fundamental_frequency);
        if let Some(n) = num_harmonics {
            four_stmt.push_str(&format!(" {}", n));
        }
        for var in output_variables {
            four_stmt.push_str(&format!(" {}", var));
        }

        // We need a .tran that covers enough periods
        let period = 1.0 / fundamental_frequency;
        let num_periods = 10.0;
        let tran_stop = period * num_periods;
        let tran_step = period / 100.0;
        let tran_stmt = format!(".tran {} {}", tran_step, tran_stop);

        // Build composite netlist with both .tran and .four
        let combined = format!("{}\n{}", tran_stmt, four_stmt);
        let raw = self.run(&combined, "tran")?;
        Ok(TransientAnalysis::from_raw(raw))
    }

    // ── Spectre-specific analyses ──

    /// Spectre parametric sweep. Wraps an inner analysis inside a Spectre
    /// `sweep` block.
    ///
    /// The `inner_analysis` is in Spectre-native syntax, e.g.,
    /// `"ac1 ac start=1 stop=1G dec=100"`.
    pub fn spectre_sweep(
        &self,
        param: &str,
        start: f64,
        stop: f64,
        step: f64,
        inner_analysis: &str,
        inner_type: &str,
    ) -> Result<RawData, BackendError> {
        let netlist = self.build_netlist("");
        let backend = crate::backend::spectre::SpectreSubprocess;
        backend.spectre_sweep(&netlist, param, start, stop, step, inner_analysis, inner_type)
    }

    /// Spectre Monte Carlo analysis. Wraps an inner analysis inside a Spectre
    /// `montecarlo` block.
    pub fn spectre_montecarlo(
        &self,
        num_iterations: u32,
        inner_analysis: &str,
        inner_type: &str,
        seed: Option<u64>,
    ) -> Result<RawData, BackendError> {
        let netlist = self.build_netlist("");
        let backend = crate::backend::spectre::SpectreSubprocess;
        backend.spectre_montecarlo(&netlist, num_iterations, inner_analysis, inner_type, seed)
    }

    /// SpectreRF Periodic AC (PAC) analysis. Automatically includes PSS prerequisite.
    pub fn spectre_pac(
        &self,
        pss_fundamental: f64,
        pss_stabilization: f64,
        pss_harmonics: u32,
        variation: &str,
        points: u32,
        start_freq: f64,
        stop_freq: f64,
        sweep_type: &str,
    ) -> Result<AcAnalysis, BackendError> {
        let netlist = self.build_netlist("");
        let backend = crate::backend::spectre::SpectreSubprocess;
        backend.spectre_pac(
            &netlist, pss_fundamental, pss_stabilization, pss_harmonics,
            variation, points, start_freq, stop_freq, sweep_type,
        )
    }

    /// SpectreRF Periodic Noise (PNoise) analysis. Automatically includes PSS prerequisite.
    pub fn spectre_pnoise(
        &self,
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
        let netlist = self.build_netlist("");
        let backend = crate::backend::spectre::SpectreSubprocess;
        backend.spectre_pnoise(
            &netlist, pss_fundamental, pss_stabilization, pss_harmonics,
            output_node, ref_node, variation, points, start_freq, stop_freq,
        )
    }

    /// SpectreRF Periodic Transfer Function (PXF) analysis. Automatically includes PSS prerequisite.
    pub fn spectre_pxf(
        &self,
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
        let netlist = self.build_netlist("");
        let backend = crate::backend::spectre::SpectreSubprocess;
        backend.spectre_pxf(
            &netlist, pss_fundamental, pss_stabilization, pss_harmonics,
            output_node, source, variation, points, start_freq, stop_freq,
        )
    }

    /// SpectreRF Periodic Stability (PSTB) analysis. Automatically includes PSS prerequisite.
    pub fn spectre_pstb(
        &self,
        pss_fundamental: f64,
        pss_stabilization: f64,
        pss_harmonics: u32,
        probe: &str,
        variation: &str,
        points: u32,
        start_freq: f64,
        stop_freq: f64,
    ) -> Result<StabilityAnalysis, BackendError> {
        let netlist = self.build_netlist("");
        let backend = crate::backend::spectre::SpectreSubprocess;
        backend.spectre_pstb(
            &netlist, pss_fundamental, pss_stabilization, pss_harmonics,
            probe, variation, points, start_freq, stop_freq,
        )
    }

    // ── Xyce-specific analyses ──

    /// Xyce .SAMPLING Monte Carlo uncertainty quantification.
    ///
    /// `param_distributions` is a slice of `(param_name, distribution_spec)` pairs.
    /// Distribution specs use Xyce syntax: `"normal(mean,stddev)"`, `"uniform(low,high)"`.
    ///
    /// Generates:
    /// ```spice
    /// .SAMPLING
    /// .options SAMPLES num_samples=100 projection_type=MC
    /// .options SAMPLES param=R1:R dist=normal(1000,50)
    /// ```
    pub fn xyce_sampling(
        &self,
        num_samples: u32,
        param_distributions: &[(&str, &str)],
    ) -> Result<SamplingAnalysis, BackendError> {
        let analysis = build_xyce_sampling_stmt(
            ".SAMPLING", num_samples, param_distributions, None,
        );
        let raw = self.run(&analysis, "sampling")?;
        Ok(SamplingAnalysis::from_raw(raw))
    }

    /// Xyce .EMBEDDEDSAMPLING — embeds Monte Carlo into time/freq analysis.
    ///
    /// Same parameters as `xyce_sampling` but uses `.EMBEDDEDSAMPLING` directive.
    pub fn xyce_embedded_sampling(
        &self,
        num_samples: u32,
        param_distributions: &[(&str, &str)],
    ) -> Result<SamplingAnalysis, BackendError> {
        let analysis = build_xyce_sampling_stmt(
            ".EMBEDDEDSAMPLING", num_samples, param_distributions, None,
        );
        let raw = self.run(&analysis, "embedded_sampling")?;
        Ok(SamplingAnalysis::from_raw(raw))
    }

    /// Xyce .PCE Polynomial Chaos Expansion uncertainty quantification.
    ///
    /// `expansion_order` controls the PCE polynomial order (typically 2-5).
    pub fn xyce_pce(
        &self,
        num_samples: u32,
        param_distributions: &[(&str, &str)],
        expansion_order: u32,
    ) -> Result<SamplingAnalysis, BackendError> {
        let analysis = build_xyce_sampling_stmt(
            ".PCE", num_samples, param_distributions, Some(expansion_order),
        );
        let raw = self.run(&analysis, "pce")?;
        Ok(SamplingAnalysis::from_raw(raw))
    }

    /// Xyce .FFT with spectral metrics (ENOB, SFDR, SNR, THD).
    ///
    /// Requires a transient analysis to have been set up (adds .tran internally).
    /// The FFT is computed by Xyce and metrics are derived from the magnitude data.
    ///
    /// Generates:
    /// ```spice
    /// .tran <step> <stop>
    /// .FFT V(out) NP=1024 START=0 STOP=1m WINDOW=HANN FORMAT=UNORM
    /// ```
    pub fn xyce_fft(
        &self,
        signal: &str,
        options: &XyceFftOptions,
    ) -> Result<XyceFftAnalysis, BackendError> {
        let fft_stmt = format!(
            ".FFT {} NP={} START={} STOP={} WINDOW={} FORMAT={}",
            signal, options.np, options.start, options.stop, options.window, options.format
        );

        // FFT requires a transient run — compute step from NP and time window
        let time_span = options.stop - options.start;
        let tran_step = if options.np > 0 {
            time_span / (options.np as f64)
        } else {
            time_span / 1024.0
        };
        let tran_stmt = format!(".tran {} {}", tran_step, options.stop);
        let combined = format!("{}\n{}", tran_stmt, fft_stmt);

        let raw = self.run(&combined, "tran")?;
        Ok(XyceFftAnalysis::from_raw(raw))
    }

    /// List all available backends on this system
    pub fn available_backends() -> Vec<String> {
        detect::detect_backends()
            .iter()
            .map(|b| b.display_name().to_string())
            .collect()
    }

    // ── Internal ──

    /// Expose netlist building for testing
    pub fn build_netlist_for_test(&self, analysis_stmt: &str) -> String {
        self.build_netlist(analysis_stmt)
    }

    fn build_netlist(&self, analysis_stmt: &str) -> String {
        let mut netlist = self.circuit.to_string();

        // Remove trailing .end
        if netlist.ends_with(".end") {
            netlist.truncate(netlist.len() - 4);
        }

        // Options
        if !self.options.is_empty() {
            let opts: Vec<String> = self.options.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
            netlist.push_str(&format!(".options {}\n", opts.join(" ")));
        }

        // Temperature
        if let Some(temp) = self.temperature {
            netlist.push_str(&format!(".temp {}\n", temp));
        }
        if let Some(tnom) = self.nominal_temperature {
            netlist.push_str(&format!(".options tnom={}\n", tnom));
        }

        // Initial conditions
        if !self.initial_conditions.is_empty() {
            let ics: Vec<String> = self.initial_conditions
                .iter()
                .map(|(n, v)| format!("V({})={}", n, v))
                .collect();
            netlist.push_str(&format!(".ic {}\n", ics.join(" ")));
        }

        // Node sets
        for (node, val) in &self.node_sets {
            netlist.push_str(&format!(".nodeset V({})={}\n", node, val));
        }

        // Saves
        if self.save_currents {
            netlist.push_str(".save all @all[i]\n");
        } else if !self.saves.is_empty() {
            netlist.push_str(&format!(".save {}\n", self.saves.join(" ")));
        }

        // Measures
        for m in &self.measures {
            netlist.push_str(&format!(".meas {}\n", m));
        }

        // Step parameter sweeps
        for sp in &self.step_params {
            if let Some(ref sweep_type) = sp.sweep_type {
                netlist.push_str(&format!(
                    ".step {} param {} {} {} {}\n",
                    sweep_type, sp.param, sp.start, sp.stop, sp.step
                ));
            } else {
                netlist.push_str(&format!(
                    ".step param {} {} {} {}\n",
                    sp.param, sp.start, sp.stop, sp.step
                ));
            }
        }

        // Extra lines (e.g., .OPTIONS HBINT for HB analysis)
        for line in &self.extra_lines {
            netlist.push_str(&format!("{}\n", line));
        }

        // Analysis statement
        netlist.push_str(&format!("{}\n", analysis_stmt));

        netlist.push_str(".end\n");
        netlist
    }

    fn run(&self, analysis_stmt: &str, analysis_type: &str) -> Result<RawData, BackendError> {
        let features: CircuitFeatures = match &self.ir {
            Some(ir) => (&ir.compute_features()).into(),
            None => CircuitFeatures {
                has_xspice: self.circuit.has_xspice(),
                has_osdi: self.circuit.has_osdi(),
                has_measures: !self.measures.is_empty(),
                has_step_params: !self.step_params.is_empty(),
                has_control_blocks: self.circuit.has_control_blocks(),
                has_laplace_sources: self.circuit.has_laplace_sources(),
                has_verilog_cosim: self.circuit.has_verilog_cosim(),
                element_count: self.circuit.elements().len(),
            },
        };
        let backend = detect_and_select_with_features(
            analysis_type, self.backend_override.as_deref(), &features
        )?;
        let backend_name = backend.name().to_string();
        // IR present → emit the netlist via the backend's CodeGen (the only
        // IR→text path). Analyses whose `emit_analysis` arm is not yet
        // implemented (Pss/HB/SPar/Stability/TransientNoise/Spectre*, issue 05)
        // fall back to the legacy single-dialect string so they don't regress.
        // Absent IR (legacy Rust/cabi callers) → legacy string build.
        let netlist = match self.ir {
            Some(_) => self
                .netlist_to_run(backend.as_ref())
                .unwrap_or_else(|_| self.build_netlist(analysis_stmt)),
            None => self.build_netlist(analysis_stmt),
        };
        let mut raw = backend.run_netlist(&netlist)?;

        // Tag the raw data with which backend produced it (for name normalization)
        raw.backend_hint = backend_name.clone();

        // Parse .meas results from stdout (or log for LTspice)
        let text_to_parse = if backend_name == "ltspice" && !raw.log_content.is_empty() {
            raw.log_content.clone()
        } else {
            raw.stdout.clone()
        };

        if !text_to_parse.is_empty() {
            raw.measures = crate::measure_parse::parse_measures(&text_to_parse, &backend_name);
        }

        Ok(raw)
    }
}

/// Build a Xyce sampling/PCE analysis statement block.
fn build_xyce_sampling_stmt(
    directive: &str,
    num_samples: u32,
    param_distributions: &[(&str, &str)],
    expansion_order: Option<u32>,
) -> String {
    let mut lines = Vec::new();
    lines.push(directive.to_string());

    if let Some(order) = expansion_order {
        // PCE mode
        lines.push(format!(
            ".options SAMPLES num_samples={} expansion_order={}",
            num_samples, order
        ));
    } else {
        // MC sampling mode
        lines.push(format!(
            ".options SAMPLES num_samples={} projection_type=MC",
            num_samples
        ));
    }

    for (param, dist) in param_distributions {
        lines.push(format!(".options SAMPLES param={} dist={}", param, dist));
    }

    lines.join("\n")
}

impl Circuit {
    pub fn simulator(&self) -> CircuitSimulator {
        CircuitSimulator::new(self.clone())
    }

    pub fn simulator_with_backend(&self, backend: &str) -> CircuitSimulator {
        CircuitSimulator::new(self.clone()).with_backend(backend)
    }
}
