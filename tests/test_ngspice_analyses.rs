//! End-to-end coverage of every analysis the library exposes on ngspice.
//!
//! Each test runs a real deck through `NgspiceSubprocess` and asserts a
//! closed-form number, not merely that the process exited 0. Verified against
//! ngspice 44.2.
//!
//! Skipped automatically when ngspice is not on PATH.

use spicerack::backend::Backend;
use spicerack::backend::ngspice::NgspiceSubprocess;
use spicerack::measure_parse::parse_measures;
use spicerack::result::{MeasureResult, RawData};

fn ngspice_available() -> bool {
    std::process::Command::new("ngspice")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a deck, or return `None` when ngspice is not installed.
fn run(deck: &str) -> Option<RawData> {
    if !ngspice_available() {
        eprintln!("ngspice not on PATH — skipping");
        return None;
    }
    Some(
        NgspiceSubprocess
            .run(deck)
            .unwrap_or_else(|e| panic!("deck failed: {e}\n{deck}")),
    )
}

/// `Backend::run` only captures stdout; `Simulator::run` is what normally
/// turns it into measures. These tests go straight to the backend, so they
/// parse it themselves.
fn measures(raw: &RawData) -> Vec<MeasureResult> {
    parse_measures(&raw.stdout, "ngspice")
}

/// Index of a vector by bare node name — ngspice reports node voltages as
/// `v(out)` and case-folds inconsistently.
fn index_of(raw: &RawData, name: &str) -> usize {
    let wrapped = format!("v({name})");
    raw.variables
        .iter()
        .position(|v| v.name.eq_ignore_ascii_case(name) || v.name.eq_ignore_ascii_case(&wrapped))
        .unwrap_or_else(|| {
            panic!(
                "no vector '{name}' in {:?}",
                raw.variables.iter().map(|v| &v.name).collect::<Vec<_>>()
            )
        })
}

fn scalar(raw: &RawData, name: &str) -> f64 {
    raw.real_data[index_of(raw, name)][0]
}

fn column<'a>(raw: &'a RawData, name: &str) -> &'a [f64] {
    &raw.real_data[index_of(raw, name)]
}

fn close(actual: f64, expected: f64, rel: f64) -> bool {
    (actual - expected).abs() <= rel * expected.abs().max(1e-30)
}

// ── .op ──

#[test]
fn op_returns_the_exact_divider_ratio() {
    // 10 V across two equal 10k resistors -> 5 V.
    let Some(raw) = run("* op\nV1 in 0 10\nR1 in out 10k\nR2 out 0 10k\n.op\n.end\n") else {
        return;
    };
    assert_eq!(raw.plot_name, "Operating Point");
    assert!(close(scalar(&raw, "out"), 5.0, 1e-9), "{}", scalar(&raw, "out"));
}

// ── .dc ──

#[test]
fn dc_sweep_tracks_the_divider_across_every_point() {
    let Some(raw) = run("* dc\nV1 in 0 0\nR1 in out 10k\nR2 out 0 10k\n.dc V1 0 10 1\n.end\n")
    else {
        return;
    };
    assert_eq!(raw.plot_name, "DC transfer characteristic");
    let sweep = column(&raw, "v-sweep");
    let out = column(&raw, "out");
    assert_eq!(sweep.len(), 11, "0..10 step 1");
    for (v_in, v_out) in sweep.iter().zip(out) {
        assert!(close(*v_out, v_in / 2.0, 1e-9), "{v_in} -> {v_out}");
    }
}

/// ngspice has no `.step`, but `.dc temp` is a real temperature sweep.
/// This is the migration path the codegen's `.step` error points at.
#[test]
fn dc_temp_is_a_working_temperature_sweep() {
    let Some(raw) = run(
        "* dc temp\nV1 in 0 1\nR1 in out 1k\nR2 out 0 1k TC1=0.01\n.dc temp 0 100 25\n.end\n",
    ) else {
        return;
    };
    assert_eq!(column(&raw, "out").len(), 5, "0,25,50,75,100");
}

// ── .ac ──

#[test]
fn ac_hits_minus_3db_exactly_at_the_rc_corner() {
    // R = 1k, C = 159.1549431 nF -> f_c = 1/(2*pi*R*C) = 1000.000 Hz.
    let Some(raw) = run(
        "* ac\nV1 in 0 0 AC 1\nR1 in out 1k\nC1 out 0 159.1549431n\n\
         .ac lin 1 1000 1000\n.end\n",
    ) else {
        return;
    };
    assert!(raw.is_complex, "AC results must be complex");
    let h = raw.complex_data[index_of(&raw, "out")][0];
    let db = 20.0 * h.norm().log10();
    assert!((db + 3.0103).abs() < 1e-3, "corner gain {db} dB, want -3.0103");
    // A single-pole RC is at exactly -45 deg at its corner.
    let phase = h.arg().to_degrees();
    assert!((phase + 45.0).abs() < 1e-3, "corner phase {phase} deg");
}

// ── .tran ──

#[test]
fn tran_charges_an_rc_to_one_minus_one_over_e_after_one_tau() {
    // tau = 1k * 1u = 1 ms; V(tau) = 1 - exp(-1) = 0.6321.
    let Some(raw) = run(
        "* tran\nV1 in 0 PULSE(0 1 0 1n 1n 1 2)\nR1 in out 1k\nC1 out 0 1u\n\
         .tran 1u 1m\n.end\n",
    ) else {
        return;
    };
    assert_eq!(raw.plot_name, "Transient Analysis");
    let out = column(&raw, "out");
    let last = *out.last().unwrap();
    let expected = 1.0 - (-1.0f64).exp();
    assert!(close(last, expected, 2e-3), "V(1 tau) = {last}, want {expected}");
}

// ── .noise ──

#[test]
fn noise_returns_the_spectra_plot_and_matches_sqrt_4ktr() {
    // A 10k resistor into an open (1 fF) node: the whole 4kTR shows up at `out`.
    // sqrt(4 * k * 300.15 K * 10k) = 1.2874809e-8 V/sqrt(Hz).
    let Some(raw) = run(
        "* noise\nV1 in 0 0 AC 1\nR1 in out 10k\nC1 out 0 1f\n.save all\n\
         .noise V(out) V1 lin 2 1 2\n.end\n",
    ) else {
        return;
    };
    // The first plot in the raw file must be the spectra, not "Integrated
    // Noise" — `rawfile::parse_raw` only reads the first one.
    assert_eq!(raw.plot_name, "Noise Spectral Density Curves");
    let nsd = column(&raw, "onoise_spectrum");
    let expected = (4.0 * 1.380649e-23 * 300.15 * 10e3f64).sqrt();
    for v in nsd {
        assert!(close(*v, expected, 1e-4), "onoise {v}, want {expected}");
    }
}

// ── .disto ──

#[test]
fn disto_runs_and_yields_the_second_harmonic_plot() {
    let Some(raw) = run(
        "* disto\nV1 in 0 0 AC 1\nR1 in nb 10k\nD1 nb 0 dmod\n\
         .model dmod D(IS=1e-14)\n.save all\n.disto dec 5 1k 10k\n.end\n",
    ) else {
        return;
    };
    // KNOWN LOSS: ngspice writes "2nd harmonic" *and* "3rd harmonic" plots;
    // `rawfile::parse_raw` reads only the first, so the 3rd is dropped.
    assert_eq!(raw.plot_name, "DISTORTION - 2nd harmonic");
    assert!(raw.is_complex);
    assert!(!raw.complex_data.is_empty());
}

// ── .pz ──

#[test]
fn pz_finds_the_rc_pole_at_minus_one_over_rc() {
    // R = 1k, C = 1u -> pole at -1/(RC) = -1000 rad/s.
    let Some(raw) = run(
        "* pz\nV1 in 0 0 AC 1\nR1 in out 1k\nC1 out 0 1u\n.pz in 0 out 0 vol pol\n.end\n",
    ) else {
        return;
    };
    assert_eq!(raw.plot_name, "Pole-Zero Analysis");
    let pole = raw.complex_data[0][0];
    assert!(close(pole.re, -1000.0, 1e-6), "pole {pole}");
    assert!(pole.im.abs() < 1e-6, "real pole expected, got {pole}");
}

// ── .tf ──

#[test]
fn tf_reports_gain_and_both_impedances() {
    let Some(raw) = run("* tf\nV1 in 0 10\nR1 in out 10k\nR2 out 0 10k\n.tf V(out) V1\n.end\n")
    else {
        return;
    };
    assert_eq!(raw.plot_name, "Transfer Function");
    assert!(close(scalar(&raw, "transfer_function"), 0.5, 1e-9));
    // Thevenin at `out` is 10k || 10k = 5k; input impedance is 10k + 10k.
    assert!(close(scalar(&raw, "output_impedance_at_v(out)"), 5e3, 1e-9));
    assert!(close(scalar(&raw, "v1#input_impedance"), 2e4, 1e-9));
}

// ── .sens ──

#[test]
fn sens_reports_dvout_dr_with_the_right_sign_and_size() {
    // V(out) = 10 * R2/(R1+R2). d/dR1 = -10*R2/(R1+R2)^2 = -2.5e-4 V/ohm.
    let Some(raw) = run("* sens\nV1 in 0 10\nR1 in out 10k\nR2 out 0 10k\n.sens V(out)\n.end\n")
    else {
        return;
    };
    assert_eq!(raw.plot_name, "Sensitivity Analysis");
    let r1 = scalar(&raw, "r1");
    assert!(close(r1, -2.5e-4, 1e-6), "dV(out)/dR1 = {r1}");
    let r2 = scalar(&raw, "r2");
    assert!(close(r2, 2.5e-4, 1e-6), "dV(out)/dR2 = {r2}");
}

// ── .four ──

#[test]
fn four_prints_the_harmonic_table_to_stdout() {
    // Under `-b -r` ngspice says ".fourier line ignored since rawfile was
    // produced." The backend rewrites the deck so `fourier` runs in a
    // `.control` block; the table then lands in stdout.
    let Some(raw) = run(
        "* four\nV1 in 0 SIN(0 1 1k)\nR1 in out 1k\nC1 out 0 1n\n\
         .tran 1u 10m\n.four 1k v(out)\n.end\n",
    ) else {
        return;
    };
    assert!(
        !raw.stdout.contains("fourier line ignored"),
        "four must not be dropped:\n{}",
        raw.stdout
    );
    assert!(
        raw.stdout.contains("Fourier analysis for v(out)"),
        "no fourier table:\n{}",
        raw.stdout
    );
    // The fundamental of a clean 1 V sine is 1 V; a pure sine has ~0 THD.
    let thd: f64 = raw
        .stdout
        .split("THD: ")
        .nth(1)
        .and_then(|s| s.split(' ').next())
        .and_then(|s| s.parse().ok())
        .expect("THD in table");
    assert!(thd < 1e-6, "THD of a pure sine should be ~0, got {thd} %");
    // The transient data must still be in the raw file.
    assert_eq!(raw.plot_name, "Transient Analysis");
    assert!(column(&raw, "out").len() > 100);
}

// ── .measure ──

/// ngspice accepts `.measure` on DC, AC, TRAN and SP. All three of the ones
/// the codegen can emit have to survive the `.control` rewrite.
#[test]
fn measure_works_on_dc_ac_and_tran() {
    let Some(dc) = run(
        "* meas dc\nV1 in 0 0\nR1 in out 10k\nR2 out 0 10k\n\
         .meas dc vhalf FIND V(out) AT=10\n.dc V1 0 10 1\n.end\n",
    ) else {
        return;
    };
    let dc_m = measures(&dc);
    assert!(close(dc_m[0].value, 5.0, 1e-9), "{:?}", dc_m);

    let ac = run(
        "* meas ac\nV1 in 0 0 AC 1\nR1 in out 1k\nC1 out 0 159.1549431n\n\
         .meas ac g FIND vdb(out) AT=1000\n.ac dec 100 100 10k\n.end\n",
    )
    .unwrap();
    let ac_m = measures(&ac);
    let g = ac_m.iter().find(|m| m.name == "g").expect("g");
    assert!((g.value + 3.0103).abs() < 1e-3, "corner gain {}", g.value);

    let tran = run(
        "* meas tran\nV1 in 0 PULSE(0 1 0 1n 1n 1 2)\nR1 in out 1k\nC1 out 0 1u\n\
         .meas tran vend FIND V(out) AT=1m\n.tran 1u 1m\n.end\n",
    )
    .unwrap();
    let tran_m = measures(&tran);
    let v = tran_m.iter().find(|m| m.name == "vend").expect("vend");
    let expected = 1.0 - (-1.0f64).exp();
    assert!(close(v.value, expected, 2e-3), "V(1 tau) = {}", v.value);
}

/// A `.control` block holding only `pre_osdi` (what the codegen emits for OSDI
/// loads) must not block the measure rewrite.
#[test]
fn measure_survives_alongside_an_osdi_control_block() {
    let Some(raw) = run(
        "* osdi + meas\n.control\npre_osdi /nonexistent/model.osdi\n.endc\n\
         V1 in 0 0\nR1 in out 10k\nR2 out 0 10k\n\
         .meas dc vhalf FIND V(out) AT=10\n.dc V1 0 10 1\n.end\n",
    ) else {
        return;
    };
    assert!(
        measures(&raw).iter().any(|m| m.name == "vhalf"),
        "measure lost next to an osdi block:\n{}",
        raw.stdout
    );
}

// ── circuit features claimed in the capability table ──

#[test]
fn xspice_a_elements_run() {
    // `gain` code model, gain = 2 -> a 0.5 V input gives 1.0 V out.
    let Some(raw) = run(
        "* xspice\nV1 in 0 0.5\nA1 in out gainblk\n\
         .model gainblk gain(gain=2.0)\nRl out 0 1meg\n.op\n.end\n",
    ) else {
        return;
    };
    assert!(close(scalar(&raw, "out"), 1.0, 1e-6), "{}", scalar(&raw, "out"));
}

#[test]
fn control_blocks_run_verbatim() {
    let Some(raw) = run("* control\nV1 in 0 3\nR1 in 0 1k\n.control\nop\n.endc\n.end\n") else {
        return;
    };
    assert!(close(scalar(&raw, "in"), 3.0, 1e-9));
}

/// The XSPICE `s_xfer` code model is ngspice's Laplace facility. The
/// `Laplace(...)` B-source text that `Circuit::has_laplace_sources` looks for
/// is *not* ngspice syntax — see the report.
#[test]
fn xspice_s_xfer_is_the_working_laplace_path() {
    // 1/(1 + s*tau), tau = 159.1549431 us -> -3.0103 dB at 1 kHz.
    let Some(raw) = run(
        "* s_xfer\nV1 in 0 0 AC 1\nA1 in out filt\n\
         .model filt s_xfer(num_coeff=[1] den_coeff=[1.591549431e-4 1] int_ic=[0])\n\
         Rl out 0 1meg\n.ac lin 1 1000 1000\n.end\n",
    ) else {
        return;
    };
    let db = 20.0 * raw.complex_data[index_of(&raw, "out")][0].norm().log10();
    assert!((db + 3.0103).abs() < 1e-2, "s_xfer corner {db} dB");
}
