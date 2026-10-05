//! End-to-end coverage of the analyses the ESPice backend exposes, mirroring
//! tests/test_ngspice_analyses.rs: each test runs a real deck through
//! `EspiceSubprocess` and asserts a closed-form number.
//!
//! Skipped automatically when espice is not on PATH. Not mirrored: XSPICE,
//! `.control` and Laplace (not ESPice features), `.disto`/`.pz`/`.noise`
//! (their result layouts are not mapped by this backend yet), `.four`
//! (ESPice writes the table as a raw plot, not to stdout).

use spicerack::backend::Backend;
use spicerack::backend::espice::EspiceSubprocess;
use spicerack::measure_parse::parse_measures;
use spicerack::result::{MeasureResult, RawData};

fn espice_available() -> bool {
    std::process::Command::new("espice")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a deck, or return `None` when espice is not installed.
fn run(deck: &str) -> Option<RawData> {
    if !espice_available() {
        eprintln!("espice not on PATH — skipping");
        return None;
    }
    Some(
        EspiceSubprocess
            .run(deck)
            .unwrap_or_else(|e| panic!("deck failed: {e}\n{deck}")),
    )
}

fn measures(raw: &RawData) -> Vec<MeasureResult> {
    parse_measures(&raw.stdout, "espice")
}

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
    // ESPice names these columns input_resistance/output_resistance where
    // ngspice writes v1#input_impedance/output_impedance_at_v(out).
    assert!(close(scalar(&raw, "output_resistance"), 5e3, 1e-9));
    assert!(close(scalar(&raw, "input_resistance"), 2e4, 1e-9));
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

// ── what the backend refuses ──

#[test]
fn a_pre_osdi_deck_is_refused_with_a_reason() {
    if !espice_available() {
        return;
    }
    let err = EspiceSubprocess
        .run("* osdi\n.control\npre_osdi /m.osdi\n.endc\nV1 a 0 1\nR1 a 0 1k\n.op\n.end\n")
        .unwrap_err();
    assert!(err.to_string().contains("does not load OSDI"), "{err}");
}
