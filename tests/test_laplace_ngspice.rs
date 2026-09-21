//! Laplace-domain sources, end to end, against real ngspice 44.2.
//!
//! Every assertion here is a closed-form number, not "it exited 0". The deck
//! the library used to emit (`B1 out 0 V=Laplace(v(in), ...)`) is fatal —
//! `Undefined parameter [s]`, `exit(1)` — so "it ran" is exactly the claim
//! that needed evidence.
//!
//! Skipped automatically when ngspice is not on PATH.

use spicerack::backend::Backend;
use spicerack::backend::ngspice::NgspiceSubprocess;
use spicerack::circuit::{Circuit, Node};
use spicerack::codegen::{CodeGen, spice3::{Spice3CodeGen, Spice3Dialect}};
use spicerack::ir::CircuitIR;
use spicerack::result::RawData;

fn ngspice_available() -> bool {
    std::process::Command::new("ngspice")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ngspice() -> Spice3CodeGen {
    Spice3CodeGen { dialect: Spice3Dialect::Ngspice }
}

fn ltspice() -> Spice3CodeGen {
    Spice3CodeGen { dialect: Spice3Dialect::Ltspice }
}

fn netlist(cg: &Spice3CodeGen, c: &Circuit) -> Result<String, String> {
    cg.emit_netlist(&CircuitIR::from_circuit(c)).map_err(|e| e.to_string())
}

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

fn index_of(raw: &RawData, name: &str) -> usize {
    let wrapped = format!("v({name})");
    raw.variables
        .iter()
        .position(|v| v.name.eq_ignore_ascii_case(name) || v.name.eq_ignore_ascii_case(&wrapped))
        .unwrap_or_else(|| panic!("no vector '{name}'"))
}

/// `(magnitude, phase in degrees)` of an AC result at sweep point `i`.
fn ac(raw: &RawData, name: &str, i: usize) -> (f64, f64) {
    let c = raw.complex_data[index_of(raw, name)][i];
    (c.norm(), c.arg().to_degrees())
}

/// A one-pole lowpass at `f0`, driven by a 1 V AC source, read at `out`.
fn onepole_deck(cg: &Spice3CodeGen, tau: f64, sweep: &str) -> String {
    let mut c = Circuit::new("laplace onepole");
    c.raw_spice("V1 in 0 AC 1");
    c.bv("1", "out", Node::Ground, format!("Laplace(V(in), 1/(1+s*{tau}))"));
    c.r("l", "out", Node::Ground, 1e6);
    c.raw_spice(sweep);
    netlist(cg, &c).unwrap()
}

// ── the deck the library used to emit ──

#[test]
fn the_old_verbatim_bsource_really_is_fatal() {
    // Guards the reason this module exists: if ngspice ever grows a
    // `Laplace()` built-in, the translation below stops being necessary.
    if !ngspice_available() {
        return;
    }
    let deck = "* fatal\nV1 in 0 AC 1\n\
                B1 out 0 V=Laplace(v(in), 1/(1+s*1.59e-4))\n\
                Rl out 0 1meg\n.ac lin 1 1000 1000\n.end\n";
    assert!(
        NgspiceSubprocess.run(deck).is_err(),
        "ngspice accepted `V=Laplace(...)` — re-check the translation"
    );
}

// ── ngspice: numbers ──

#[test]
fn single_pole_is_minus_3db_and_minus_45_degrees_at_the_corner() {
    // tau = 1/(2*pi*1000) -> corner at exactly 1 kHz.
    let deck = onepole_deck(&ngspice(), 1.591_549_431e-4, ".ac lin 1 1000 1000");
    assert!(deck.contains("s_xfer"), "{deck}");
    let Some(raw) = run(&deck) else { return };
    let (mag, phase) = ac(&raw, "out", 0);
    assert!((mag - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-4, "|H| = {mag}");
    assert!((phase + 45.0).abs() < 1e-3, "arg H = {phase} deg");
}

#[test]
fn single_pole_rolls_off_at_20db_per_decade() {
    let deck = onepole_deck(&ngspice(), 1.591_549_431e-4, ".ac dec 1 100 100000");
    let Some(raw) = run(&deck) else { return };
    // 100 Hz (a decade below the corner) -> ~unity.
    let (m100, p100) = ac(&raw, "out", 0);
    assert!((m100 - 0.995_037).abs() < 1e-4, "|H|(100) = {m100}");
    assert!((p100 + 5.7106).abs() < 1e-2, "arg H(100) = {p100}");
    // 100 kHz (two decades above) -> 1/100 of the corner value, i.e. -40 dB.
    let (m100k, p100k) = ac(&raw, "out", 3);
    assert!((m100k - 9.999_500e-3).abs() < 1e-6, "|H|(100k) = {m100k}");
    assert!((p100k + 89.427).abs() < 1e-2, "arg H(100k) = {p100k}");
}

#[test]
fn lead_lag_matches_the_closed_form() {
    // (1 + s*1e-3)/(1 + s*1e-6) at 1 kHz: (1+j6.2832)/(1+j0.0062832).
    let mut c = Circuit::new("lead lag");
    c.raw_spice("V1 in 0 AC 1");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), (1+s*1e-3)/(1+s*1e-6))");
    c.r("l", "out", Node::Ground, 1e6);
    c.raw_spice(".ac lin 1 1000 1000");
    let Some(raw) = run(&netlist(&ngspice(), &c).unwrap()) else { return };
    let (mag, phase) = ac(&raw, "out", 0);
    let w = 2.0 * std::f64::consts::PI * 1000.0;
    let expect = num_complex::Complex64::new(1.0, w * 1e-3) / num_complex::Complex64::new(1.0, w * 1e-6);
    assert!((mag - expect.norm()).abs() < 1e-4 * expect.norm(), "|H| = {mag}, want {}", expect.norm());
    assert!((phase - expect.arg().to_degrees()).abs() < 1e-2, "arg H = {phase}");
}

#[test]
fn two_pole_butterworth_is_minus_3db_at_the_corner_and_minus_90_degrees() {
    // 1/(1 + s*sqrt(2)/w0 + s^2/w0^2), w0 = 2*pi*1000.
    let w0 = 2.0 * std::f64::consts::PI * 1000.0;
    let (a1, a2) = (std::f64::consts::SQRT_2 / w0, 1.0 / (w0 * w0));
    let mut c = Circuit::new("butterworth");
    c.raw_spice("V1 in 0 AC 1");
    c.bv("1", "out", Node::Ground, format!("Laplace(V(in), 1/(1+s*{a1}+s^2*{a2}))"));
    c.r("l", "out", Node::Ground, 1e6);
    c.raw_spice(".ac lin 1 1000 1000");
    let deck = netlist(&ngspice(), &c).unwrap();
    assert!(deck.contains("int_ic=[0 0]"), "two integrator stages expected:\n{deck}");
    let Some(raw) = run(&deck) else { return };
    let (mag, phase) = ac(&raw, "out", 0);
    assert!((mag - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-5, "|H| = {mag}");
    assert!((phase + 90.0).abs() < 1e-3, "arg H = {phase} deg");
}

#[test]
fn differentiator_with_a_pole_gives_plus_20db_per_decade() {
    // s/(1 + s*1e-6): |H| = w/|1+jw*1e-6|, ~ w below 159 kHz.
    let mut c = Circuit::new("hpf");
    c.raw_spice("V1 in 0 AC 1");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), s/(1+s*1e-6))");
    c.r("l", "out", Node::Ground, 1e6);
    c.raw_spice(".ac dec 1 100 1000");
    let Some(raw) = run(&netlist(&ngspice(), &c).unwrap()) else { return };
    for (i, f) in [100.0_f64, 1000.0].into_iter().enumerate() {
        let w = 2.0 * std::f64::consts::PI * f;
        let (mag, phase) = ac(&raw, "out", i);
        let expect = num_complex::Complex64::new(0.0, w) / num_complex::Complex64::new(1.0, w * 1e-6);
        assert!((mag - expect.norm()).abs() < 1e-4 * expect.norm(), "|H|({f}) = {mag}");
        assert!((phase - expect.arg().to_degrees()).abs() < 1e-2, "arg H({f}) = {phase}");
    }
}

#[test]
fn differential_input_subtracts_its_two_nodes() {
    // H = 2, differential input: 2 * (1.0 - 0.25) = 1.5, through a real pole
    // well below the sweep frequency so the magnitude is the DC gain.
    let mut c = Circuit::new("diff in");
    c.raw_spice("V1 a 0 AC 1");
    c.raw_spice("V2 b 0 AC 0.25");
    c.r("b", "b", Node::Ground, 1e6);
    c.bv("1", "out", Node::Ground, "Laplace(V(a,b), 2/(1+s*1.591549431e-9))");
    c.r("l", "out", Node::Ground, 1e6);
    c.raw_spice(".ac lin 1 1000 1000");
    let Some(raw) = run(&netlist(&ngspice(), &c).unwrap()) else { return };
    let (mag, _) = ac(&raw, "out", 0);
    assert!((mag - 1.5).abs() < 1e-5, "|H| = {mag}");
}

#[test]
fn current_output_keeps_the_spice_b_source_sign_convention() {
    // `B np nm I=...` sinks current out of np, so a 1 mA/V block into 1k gives
    // -1 V. The XSPICE %id port agrees; this test is what proves it.
    let mut c = Circuit::new("bi laplace");
    c.raw_spice("V1 in 0 AC 1");
    c.bi("1", "out", Node::Ground, "Laplace(V(in), 1e-3/(1+s*1.591549431e-9))");
    c.r("l", "out", Node::Ground, 1e3);
    c.raw_spice(".ac lin 1 1000 1000");
    let deck = netlist(&ngspice(), &c).unwrap();
    assert!(deck.contains("%id(out 0)"), "{deck}");
    let Some(raw) = run(&deck) else { return };
    let (mag, phase) = ac(&raw, "out", 0);
    assert!((mag - 1.0).abs() < 1e-5, "|H| = {mag}");
    assert!((phase.abs() - 180.0).abs() < 1e-3, "arg H = {phase} deg (expected +/-180)");
}

#[test]
fn constant_transfer_function_becomes_a_plain_vcvs() {
    // den_coeff=[1] segfaults ngspice 44.2 (exit 139), so a constant H must
    // not go through s_xfer at all.
    let mut c = Circuit::new("gain");
    c.raw_spice("V1 in 0 2");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), 2.5)");
    c.r("l", "out", Node::Ground, 1e6);
    c.raw_spice(".op");
    let deck = netlist(&ngspice(), &c).unwrap();
    assert!(!deck.contains("s_xfer"), "{deck}");
    assert!(deck.contains("EB1 out 0 in 0 2.5"), "{deck}");
    let Some(raw) = run(&deck) else { return };
    assert!((raw.real_data[index_of(&raw, "out")][0] - 5.0).abs() < 1e-9);
}

#[test]
fn transient_step_response_follows_the_exponential() {
    let tau = 1.591_549_431e-4;
    let mut c = Circuit::new("step");
    c.raw_spice("V1 in 0 PULSE(0 1 0 1n 1n 1 2)");
    c.bv("1", "out", Node::Ground, format!("Laplace(V(in), 1/(1+s*{tau}))"));
    c.r("l", "out", Node::Ground, 1e6);
    c.raw_spice(".tran 100n 1m");
    let Some(raw) = run(&netlist(&ngspice(), &c).unwrap()) else { return };
    let time = &raw.real_data[index_of(&raw, "time")];
    let out = &raw.real_data[index_of(&raw, "out")];
    for (t, v) in time.iter().zip(out).filter(|(t, _)| **t > 1e-5) {
        let want = 1.0 - (-t / tau).exp();
        assert!((v - want).abs() < 2e-3, "t={t}: {v}, want {want}");
    }
}

/// KNOWN CEILING, pinned so it cannot change unnoticed: the `s_xfer` code
/// model contributes nothing to the DC solution. A block with a DC gain of 3
/// driven by 2 V reports `v(out) = 0` under `.op` and `.dc`, while `.tran`
/// settles on the correct 6 V (the test above). Nothing in this translation
/// can fix that — it is ngspice's code model.
///
/// ponytail: documented, not refused. Refusing `.op`/`.dc` whenever a Laplace
/// block exists would also block circuits where the block sits off the bias
/// path; wire the refusal into analysis dispatch if a wrong bias ever bites.
#[test]
fn dc_through_a_laplace_block_reads_zero_not_the_dc_gain() {
    let mut c = Circuit::new("dc ceiling");
    c.raw_spice("V1 in 0 2");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), 3/(1+s*1.591549431e-6))");
    c.r("l", "out", Node::Ground, 1e6);
    c.raw_spice(".op");
    let Some(raw) = run(&netlist(&ngspice(), &c).unwrap()) else { return };
    let out = raw.real_data[index_of(&raw, "out")][0];
    assert_eq!(out, 0.0, "s_xfer started contributing at DC — revisit the ceiling note");
}

// ── refusals ──

#[test]
fn symbolic_coefficients_are_refused_not_emitted() {
    let mut c = Circuit::new("symbolic");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), 1/(1+s*tau))");
    let err = netlist(&ngspice(), &c).unwrap_err();
    assert!(err.contains("tau"), "{err}");
    assert!(err.contains("B1"), "{err}");
}

#[test]
fn improper_transfer_functions_are_refused_not_emitted() {
    // s_xfer: "Numerator coefficient array size greater than denominator
    // coefficiant array size." Refuse rather than emit a deck that errors.
    for h in ["s", "s^2", "s*1e-3"] {
        let mut c = Circuit::new("improper");
        c.bv("1", "out", Node::Ground, format!("Laplace(V(in), {h})"));
        let err = netlist(&ngspice(), &c).unwrap_err();
        assert!(err.contains("improper"), "{h}: {err}");
    }
}

#[test]
fn current_inputs_are_refused_not_emitted() {
    let mut c = Circuit::new("iin");
    c.bv("1", "out", Node::Ground, "Laplace(I(V1), 1/(1+s*1e-6))");
    let err = netlist(&ngspice(), &c).unwrap_err();
    assert!(err.contains("V(node)"), "{err}");
}

#[test]
fn spectre_and_vacask_refuse_every_behavioural_source() {
    use spicerack::codegen::{spectre::SpectreCodeGen, vacask::VacaskCodeGen};
    let mut c = Circuit::new("no bsource");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), 1/(1+s*1e-6))");
    let ir = CircuitIR::from_circuit(&c);
    assert!(SpectreCodeGen.emit_netlist(&ir).is_err());
    assert!(VacaskCodeGen.emit_netlist(&ir).is_err());
}

// ── the legacy `Circuit::Display` path ──
//
// `Circuit::simulator()` attaches no IR, so its decks come from
// `Circuit::Display`, not from `CodeGen`. That is the path the Python
// `circuit.simulator().ac(...)` API uses, and fixing only the codegen left it
// still emitting the fatal `V=Laplace(...)` line.

#[test]
fn the_display_netlist_runs_on_ngspice_too() {
    let mut c = Circuit::new("legacy path");
    c.raw_spice("V1 in 0 AC 1");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), 1/(1+s*1.591549431e-4))");
    c.r("l", "out", Node::Ground, 1e6);
    let deck = c.to_string();
    assert!(deck.contains("s_xfer"), "{deck}");
    assert!(!deck.contains("V=Laplace"), "{deck}");

    if !ngspice_available() {
        return;
    }
    let ac_res = c
        .simulator()
        .ac("lin", 1, 1000.0, 1000.0)
        .expect("legacy Laplace deck must run");
    let out = ac_res.base.nodes["out"].complex.as_ref().expect("AC is complex")[0];
    assert!((out.norm() - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-4, "|H| = {}", out.norm());
    assert!((out.arg().to_degrees() + 45.0).abs() < 1e-3, "arg H = {}", out.arg().to_degrees());
}

#[test]
fn the_display_path_refuses_what_it_cannot_translate() {
    // `Display` has no way to report an error, so the check has to happen
    // before the deck is handed to a simulator.
    let mut c = Circuit::new("legacy symbolic");
    c.raw_spice("V1 in 0 AC 1");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), 1/(1+s*tau))");
    c.r("l", "out", Node::Ground, 1e6);
    let err = c.laplace_error("ngspice-subprocess").expect("must refuse");
    assert!(err.contains("tau"), "{err}");
    if ngspice_available() {
        assert!(c.simulator().ac("lin", 1, 1000.0, 1000.0).is_err());
    }
}

#[test]
fn the_display_path_refuses_non_ngspice_backends() {
    // `Display` writes ngspice XSPICE syntax unconditionally. Sending that to
    // LTspice/Spectre/VACASK would be a different fatal deck, not a fix.
    let mut c = Circuit::new("legacy ltspice");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), 1/(1+s*1e-6))");
    for backend in ["ltspice", "spectre", "vacask"] {
        let err = c.laplace_error(backend).unwrap_or_else(|| panic!("{backend} not refused"));
        assert!(err.contains(backend), "{err}");
    }
    assert_eq!(c.laplace_error("ngspice-subprocess"), None);
    // No Laplace source, no opinion.
    assert_eq!(Circuit::new("plain").laplace_error("ltspice"), None);
}

// ── LTspice: syntax only, UNVERIFIED ──

#[test]
fn ltspice_emits_documented_laplace_syntax() {
    // LTspice is not installed and cannot be run here. This asserts the shape
    // the LTspice help file documents for `E`/`G` sources, nothing more:
    //   Exxx n+ n- nc+ nc- Laplace=<func(s)>
    // No numeric claim is made about LTspice anywhere in this repo.
    let deck = onepole_deck(&ltspice(), 1.59e-4, ".ac lin 1 1000 1000");
    assert!(deck.contains("EB1 out 0 in 0 Laplace=1e0/(1.59e-4*s+1e0)"), "{deck}");
    assert!(!deck.contains("s_xfer"), "{deck}");

    let mut c = Circuit::new("bi");
    c.bi("1", "out", Node::Ground, "Laplace(V(in), 1/(1+s*1e-6))");
    let deck = netlist(&ltspice(), &c).unwrap();
    assert!(deck.contains("GB1 out 0 in 0 Laplace="), "{deck}");

    // The refusals are dialect-independent: LTspice never sees an expression
    // this crate has not parsed.
    let mut c = Circuit::new("symbolic");
    c.bv("1", "out", Node::Ground, "Laplace(V(in), 1/(1+s*tau))");
    assert!(netlist(&ltspice(), &c).is_err());
}
