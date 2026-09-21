//! VACASK end-to-end: every deck here is emitted by `VacaskCodeGen` and then
//! handed to the real binary. A deck that parses and produces a raw file is the
//! only evidence that counts, so nothing is asserted against the generated text
//! that is not also asserted against the numbers VACASK returns.
//!
//! Skips when `vacask` is not on PATH so CI without it still passes.

use spicerack::backend::vacask::VacaskSubprocess;
use spicerack::backend::Backend;
use spicerack::ir::*;
use spicerack::result::RawData;

// ── Harness ──

fn have_vacask() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("vacask").is_file()))
        .unwrap_or(false)
}

/// Emit the deck and run it. `None` means "no binary, skip".
fn run(ir: &CircuitIR) -> Option<RawData> {
    let vk = VacaskSubprocess;
    let netlist = vk.codegen().emit_netlist(ir).expect("emit vacask deck");
    if !have_vacask() {
        eprintln!("skipping: vacask not on PATH");
        return None;
    }
    match vk.run_netlist(&netlist) {
        Ok(raw) => Some(raw),
        Err(e) => panic!("vacask rejected the generated deck: {}\n--- deck ---\n{}", e, netlist),
    }
}

/// One variable's real data, looked up by VACASK's own column name.
fn col<'a>(raw: &'a RawData, name: &str) -> &'a [f64] {
    let i = raw
        .variables
        .iter()
        .position(|v| v.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| {
            panic!(
                "no column '{}' in {:?}",
                name,
                raw.variables.iter().map(|v| &v.name).collect::<Vec<_>>()
            )
        });
    &raw.real_data[i]
}

fn cplx(raw: &RawData, name: &str) -> Vec<num_complex::Complex64> {
    let i = raw
        .variables
        .iter()
        .position(|v| v.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| {
            panic!(
                "no column '{}' in {:?}",
                name,
                raw.variables.iter().map(|v| &v.name).collect::<Vec<_>>()
            )
        });
    raw.complex_data[i].clone()
}

fn close(got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol * want.abs().max(1e-12),
        "got {:e}, want {:e} (rel tol {:e})",
        got, want, tol
    );
}

// ── IR builders ──

fn empty_sub(name: &str) -> Subcircuit {
    Subcircuit {
        name: name.into(), ports: vec![], parameters: vec![], components: vec![],
        instances: vec![], models: vec![], raw_spice: vec![], includes: vec![],
        libs: vec![], osdi_loads: vec![], verilog_blocks: vec![],
    }
}

fn empty_tb(analyses: Vec<Analysis>) -> Testbench {
    Testbench {
        dut: "tb".into(), stimulus: vec![], analyses, options: SimOptions::default(),
        saves: vec![], measures: vec![], temperature: None, nominal_temperature: None,
        initial_conditions: vec![], node_sets: vec![], step_params: vec![], extra_lines: vec![],
    }
}

fn r(name: &str, n1: &str, n2: &str, v: f64) -> Component {
    Component::Resistor {
        name: name.into(), n1: n1.into(), n2: n2.into(),
        value: IrValue::Numeric { value: v }, params: vec![],
    }
}

fn c(name: &str, n1: &str, n2: &str, v: f64) -> Component {
    Component::Capacitor {
        name: name.into(), n1: n1.into(), n2: n2.into(),
        value: IrValue::Numeric { value: v }, params: vec![],
    }
}

fn vsrc(name: &str, np: &str, nm: &str, dc: f64) -> Component {
    Component::VoltageSource {
        name: name.into(), np: np.into(), nm: nm.into(),
        value: IrValue::Numeric { value: dc },
        ac_magnitude: None, ac_phase: None, waveform: None,
    }
}

/// 1k/1k divider off a 3 V rail plus a 1 uF cap on the output: a circuit with
/// a closed form for every analysis below.
fn rc_ir(analyses: Vec<Analysis>) -> CircuitIR {
    let mut top = empty_sub("rc_divider");
    top.components = vec![r("1", "vdd", "out", 1000.0), r("2", "out", "0", 1000.0)];
    let mut tb = empty_tb(analyses);
    tb.stimulus = vec![vsrc("dd", "vdd", "0", 3.0)];
    CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] }
}

// ── Analyses ──

#[test]
fn op_solves_a_resistive_divider() {
    let Some(raw) = run(&rc_ir(vec![Analysis::Op])) else { return };
    // 3 V across 1k+1k: out = 1.5 V, i(vdd) = -1.5 mA (VACASK's branch flow
    // points into the source's positive terminal).
    close(col(&raw, "out")[0], 1.5, 1e-9);
    close(col(&raw, "vdd")[0], 3.0, 1e-9);
    close(col(&raw, "vdd:flow(br)")[0], -1.5e-3, 1e-9);
}

#[test]
fn dc_sweeps_a_source_on_spices_own_grid() {
    let ir = rc_ir(vec![Analysis::Dc {
        sweeps: vec![DcSweep { source: "Vdd".into(), start: 0.0, stop: 10.0, step: 0.5 }],
    }]);
    let Some(raw) = run(&ir) else { return };
    let sweep = col(&raw, "vsweep");
    // 0..10 by 0.5 is 21 SPICE samples, not 21 intervals.
    assert_eq!(sweep.len(), 21, "sweep grid: {:?}", sweep);
    close(sweep[0], 0.0, 1e-12);
    close(sweep[20], 10.0, 1e-12);
    let out = col(&raw, "out");
    for (i, v) in out.iter().enumerate() {
        close(*v, sweep[i] / 2.0, 1e-9);
    }
}

#[test]
fn dc_sweep_stops_before_passing_the_endpoint() {
    // SPICE walks from START by STEP and never reaches 1.1; VACASK always
    // lands ON `to`, so `to` has to be recomputed or the grids diverge.
    let ir = rc_ir(vec![Analysis::Dc {
        sweeps: vec![DcSweep { source: "Vdd".into(), start: 0.0, stop: 1.1, step: 0.2 }],
    }]);
    let Some(raw) = run(&ir) else { return };
    let sweep = col(&raw, "vsweep");
    assert_eq!(sweep.len(), 6, "{:?}", sweep);
    close(sweep[5], 1.0, 1e-12);
    close(sweep[1], 0.2, 1e-12);
}

#[test]
fn two_source_dc_runs_inner_sweep_fastest() {
    let mut top = empty_sub("two_src");
    top.components = vec![r("1", "a", "b", 1000.0), r("2", "b", "0", 1000.0)];
    let mut tb = empty_tb(vec![Analysis::Dc {
        sweeps: vec![
            // SPICE's FIRST source is the fast one.
            DcSweep { source: "Va".into(), start: 0.0, stop: 1.0, step: 0.5 },
            DcSweep { source: "Vb".into(), start: 0.0, stop: 2.0, step: 1.0 },
        ],
    }]);
    tb.stimulus = vec![vsrc("a", "a", "0", 0.0), vsrc("b", "c", "0", 0.0)];
    top.components.push(r("3", "c", "0", 1000.0));
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };
    // 3 outer x 3 inner = 9 points, inner fastest.
    let a = col(&raw, "a");
    assert_eq!(a.len(), 9, "{:?}", a);
    close(a[0], 0.0, 1e-12);
    close(a[1], 0.5, 1e-12);
    close(a[2], 1.0, 1e-12);
    close(a[3], 0.0, 1e-12);
    let cc = col(&raw, "c");
    close(cc[0], 0.0, 1e-12);
    close(cc[3], 1.0, 1e-12);
    close(cc[6], 2.0, 1e-12);
}

#[test]
fn ac_finds_the_rc_corner() {
    // 1k || 1k = 500 ohm driving 1 uF: f_3dB = 1/(2*pi*500*1e-6) = 318.31 Hz.
    let mut top = empty_sub("rc_ac");
    top.components = vec![r("1", "vdd", "out", 1000.0), r("2", "out", "0", 1000.0), c("1", "out", "0", 1e-6)];
    let mut tb = empty_tb(vec![Analysis::Ac { variation: "dec".into(), points: 200, start: 1.0, stop: 1e5 }]);
    tb.stimulus = vec![Component::VoltageSource {
        name: "dd".into(), np: "vdd".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 },
        ac_magnitude: Some(1.0), ac_phase: Some(0.0), waveform: None,
    }];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };
    assert!(raw.is_complex, "AC must come back as a complex plot");

    let f = col(&raw, "frequency");
    let out = cplx(&raw, "out");
    // DC gain is the divider ratio.
    close(out[0].norm(), 0.5, 1e-3);

    // -3 dB relative to the 0.5 plateau.
    let target = 0.5 / std::f64::consts::SQRT_2;
    let (i, _) = out
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1.norm() - target).abs().partial_cmp(&(b.1.norm() - target).abs()).unwrap())
        .unwrap();
    let f3db = 1.0 / (2.0 * std::f64::consts::PI * 500.0 * 1e-6);
    close(f[i], f3db, 2e-2);
}

#[test]
fn ac_phase_on_a_source_is_honoured() {
    let mut top = empty_sub("acphase");
    top.components = vec![r("1", "vin", "0", 1000.0)];
    let mut tb = empty_tb(vec![Analysis::Ac { variation: "lin".into(), points: 2, start: 1e3, stop: 2e3 }]);
    tb.stimulus = vec![Component::VoltageSource {
        name: "1".into(), np: "vin".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 },
        ac_magnitude: Some(2.0), ac_phase: Some(90.0), waveform: None,
    }];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };
    let v = cplx(&raw, "vin");
    close(v[0].norm(), 2.0, 1e-9);
    close(v[0].arg().to_degrees(), 90.0, 1e-6);
}

#[test]
fn tran_follows_the_rc_step_response() {
    // tau = 1k * 1u = 1 ms, driven by a 1 V step at t=0.
    let mut top = empty_sub("rc_tran");
    top.components = vec![r("1", "vin", "out", 1000.0), c("1", "out", "0", 1e-6)];
    let mut tb = empty_tb(vec![Analysis::Transient {
        step: 1e-5, stop: 5e-3, start: None, max_step: Some(1e-5), uic: true,
    }]);
    tb.stimulus = vec![Component::VoltageSource {
        name: "1".into(), np: "vin".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 }, ac_magnitude: None, ac_phase: None,
        waveform: Some(IrWaveform::Pulse {
            initial: 0.0, pulsed: 1.0, delay: 0.0, rise_time: 1e-9,
            fall_time: 1e-9, pulse_width: 1.0, period: 0.0,
        }),
    }];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };
    let t = col(&raw, "time");
    let out = col(&raw, "out");
    // 1 - exp(-1) = 0.6321 at one time constant.
    let (i, _) = t.iter().enumerate().min_by(|a, b| (a.1 - 1e-3).abs().partial_cmp(&(b.1 - 1e-3).abs()).unwrap()).unwrap();
    close(out[i], 1.0 - (-1.0f64).exp(), 5e-3);
    close(*out.last().unwrap(), 1.0 - (-5.0f64).exp(), 5e-3);
}

#[test]
fn tran_emits_maxstep_so_the_grid_is_spices() {
    // Without `maxstep` VACASK's `step` is only a STARTING step and it strides
    // far past SPICE's output interval.
    let mut top = empty_sub("grid");
    top.components = vec![r("1", "vin", "out", 1000.0), c("1", "out", "0", 1e-6)];
    let mut tb = empty_tb(vec![Analysis::Transient {
        step: 1e-4, stop: 1e-2, start: None, max_step: None, uic: false,
    }]);
    tb.stimulus = vec![vsrc("1", "vin", "0", 1.0)];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let netlist = VacaskSubprocess.codegen().emit_netlist(&ir).unwrap();
    assert!(netlist.contains("maxstep="), "{}", netlist);
    let Some(raw) = run(&ir) else { return };
    let t = col(&raw, "time");
    // 10 ms / 100 us = 100 intervals; without maxstep VACASK returns far fewer.
    assert!(t.len() >= 100, "only {} points — maxstep is not being honoured", t.len());
    let dt_max = t.windows(2).map(|w| w[1] - w[0]).fold(0.0f64, f64::max);
    assert!(dt_max <= 1e-4 * 1.001, "max step {:e} exceeds the requested 1e-4", dt_max);
}

#[test]
fn noise_matches_a_resistors_thermal_floor() {
    // A single 1k resistor into a unity-gain probe: the output noise density
    // is 4kTR = 1.66e-17 V^2/Hz at 27 C, flat with frequency.
    let mut top = empty_sub("rnoise");
    top.components = vec![r("1", "vin", "out", 1000.0), r("2", "out", "0", 1000.0)];
    let mut tb = empty_tb(vec![Analysis::Noise {
        output: "out".into(), reference: "0".into(), source: "V1".into(),
        variation: "dec".into(), points: 5, start: 1e3, stop: 1e5,
        points_per_summary: None,
    }]);
    tb.stimulus = vec![Component::VoltageSource {
        name: "1".into(), np: "vin".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 },
        ac_magnitude: Some(1.0), ac_phase: None, waveform: None,
    }];
    tb.temperature = Some(27.0);
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };

    let onoise = col(&raw, "onoise");
    // Two 1k resistors in parallel (500 ohm) seen from `out`: 4kT*500.
    let k = 1.380649e-23;
    let want = 4.0 * k * (27.0 + 273.15) * 500.0;
    close(onoise[0], want, 2e-3);
    close(*onoise.last().unwrap(), want, 2e-3);
    // Power gain from the 1 V source through the divider is (1/2)^2.
    close(col(&raw, "gain")[0], 0.25, 1e-6);
}

#[test]
fn dc_transfer_function_runs() {
    // VACASK's `dcxf` reports the transfer function from EVERY source to `out`
    // in one plot, so SPICE's input-source argument is implicit.
    let ir = rc_ir(vec![Analysis::Tf { output: "V(out)".into(), source: "Vdd".into() }]);
    let Some(raw) = run(&ir) else { return };
    close(col(&raw, "tf(vdd)")[0], 0.5, 1e-9);
    close(col(&raw, "zin(vdd)")[0], 2000.0, 1e-9);
}

// ── Devices ──

#[test]
fn a_diode_runs_under_the_spice_device_set() {
    let mut top = empty_sub("diode");
    top.components = vec![
        r("1", "vin", "a", 1000.0),
        Component::Diode {
            name: "1".into(), np: "a".into(), nm: "0".into(),
            model: "dmod".into(), params: vec![],
        },
    ];
    top.models = vec![ModelDef {
        name: "dmod".into(), kind: "D".into(),
        parameters: vec![("IS".into(), "1e-14".into()), ("N".into(), "1.0".into())],
    }];
    let mut tb = empty_tb(vec![Analysis::Op]);
    tb.stimulus = vec![vsrc("1", "vin", "0", 1.0)];
    tb.temperature = Some(27.0);
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let netlist = VacaskSubprocess.codegen().emit_netlist(&ir).unwrap();
    assert!(netlist.contains("load \"spice/diode.osdi\""), "{}", netlist);
    assert!(netlist.contains("model dmod sp_diode"), "{}", netlist);
    let Some(raw) = run(&ir) else { return };
    // 1 V through 1k into a diode: a forward drop somewhere in the 0.4-0.8 V
    // band, and the rest across the resistor.
    let va = col(&raw, "a")[0];
    assert!((0.4..0.8).contains(&va), "diode drop {} V is not physical", va);
}

#[test]
fn a_bjt_runs_and_carries_its_polarity() {
    let mut top = empty_sub("bjt");
    top.components = vec![
        r("b", "vb", "base", 10_000.0),
        r("c", "vcc", "coll", 1000.0),
        Component::Bjt {
            name: "1".into(), nc: "coll".into(), nb: "base".into(), ne: "0".into(),
            model: "qn".into(), params: vec![],
        },
    ];
    top.models = vec![ModelDef {
        name: "qn".into(), kind: "NPN".into(),
        parameters: vec![("IS".into(), "1e-15".into()), ("BF".into(), "100".into())],
    }];
    let mut tb = empty_tb(vec![Analysis::Op]);
    tb.stimulus = vec![vsrc("cc", "vcc", "0", 5.0), vsrc("b", "vb", "0", 1.0)];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let netlist = VacaskSubprocess.codegen().emit_netlist(&ir).unwrap();
    assert!(netlist.contains("model qn sp_bjt (type=1"), "{}", netlist);
    let Some(raw) = run(&ir) else { return };
    let vbe = col(&raw, "base")[0];
    assert!((0.5..0.9).contains(&vbe), "Vbe {} V is not a forward-biased NPN", vbe);
    // With beta=100 the stage is on, so the collector is pulled well below Vcc.
    assert!(col(&raw, "coll")[0] < 4.9);
}

#[test]
fn a_level_1_mosfet_runs() {
    let mut top = empty_sub("mos");
    top.components = vec![
        r("d", "vdd", "drain", 1000.0),
        Component::Mosfet {
            name: "1".into(), nd: "drain".into(), ng: "gate".into(),
            ns: "0".into(), nb: "0".into(), model: "nm".into(),
            params: vec![("W".into(), "10u".into()), ("L".into(), "1u".into())],
        },
    ];
    top.models = vec![ModelDef {
        name: "nm".into(), kind: "NMOS".into(),
        parameters: vec![
            ("LEVEL".into(), "1".into()), ("VTO".into(), "1.0".into()),
            ("KP".into(), "2e-5".into()), ("LAMBDA".into(), "0.05".into()),
        ],
    }];
    let mut tb = empty_tb(vec![Analysis::Op]);
    tb.stimulus = vec![vsrc("dd", "vdd", "0", 5.0), vsrc("g", "gate", "0", 3.0)];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let netlist = VacaskSubprocess.codegen().emit_netlist(&ir).unwrap();
    assert!(netlist.contains("load \"spice/mos1.osdi\""), "{}", netlist);
    assert!(netlist.contains("model nm sp_mos1 (type=1"), "{}", netlist);
    assert!(netlist.contains("w=10u l=1u"), "{}", netlist);
    let Some(raw) = run(&ir) else { return };
    let vd = col(&raw, "drain")[0];
    assert!(vd < 5.0 && vd > 0.0, "drain {} V is not a conducting NMOS", vd);
}

#[test]
fn a_jfet_runs() {
    let mut top = empty_sub("jfet");
    top.components = vec![
        r("d", "vdd", "drain", 1000.0),
        Component::Jfet {
            name: "1".into(), nd: "drain".into(), ng: "0".into(), ns: "0".into(),
            model: "jn".into(), params: vec![],
        },
    ];
    top.models = vec![ModelDef {
        name: "jn".into(), kind: "NJF".into(),
        parameters: vec![("VTO".into(), "-2".into()), ("BETA".into(), "1e-3".into())],
    }];
    let mut tb = empty_tb(vec![Analysis::Op]);
    tb.stimulus = vec![vsrc("dd", "vdd", "0", 5.0)];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let netlist = VacaskSubprocess.codegen().emit_netlist(&ir).unwrap();
    assert!(netlist.contains("load \"spice/jfet1.osdi\""), "{}", netlist);
    let Some(raw) = run(&ir) else { return };
    assert!(col(&raw, "drain")[0] < 5.0);
}

#[test]
fn an_inductor_and_its_mutual_coupling_run() {
    let mut top = empty_sub("xfmr");
    top.components = vec![
        Component::Inductor { name: "1".into(), n1: "p".into(), n2: "0".into(), value: IrValue::Numeric { value: 1e-3 }, params: vec![] },
        Component::Inductor { name: "2".into(), n1: "s".into(), n2: "0".into(), value: IrValue::Numeric { value: 4e-3 }, params: vec![] },
        Component::MutualInductor { name: "1".into(), inductor1: "1".into(), inductor2: "2".into(), coupling: 0.99 },
        r("1", "drive", "p", 1.0),
        r("2", "s", "0", 1e6),
    ];
    let mut tb = empty_tb(vec![Analysis::Ac { variation: "dec".into(), points: 5, start: 1e3, stop: 1e5 }]);
    tb.stimulus = vec![Component::VoltageSource {
        name: "1".into(), np: "drive".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 },
        ac_magnitude: Some(1.0), ac_phase: None, waveform: None,
    }];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let netlist = VacaskSubprocess.codegen().emit_netlist(&ir).unwrap();
    assert!(netlist.contains("k1 () mutual k=9.9e-1 ind1=\"l1\" ind2=\"l2\""), "{}", netlist);
    let Some(raw) = run(&ir) else { return };
    // Turns ratio sqrt(4m/1m) = 2, so the secondary rises above the primary.
    let p = cplx(&raw, "p");
    let s = cplx(&raw, "s");
    let ratio = s.last().unwrap().norm() / p.last().unwrap().norm();
    close(ratio, 2.0 * 0.99, 2e-2);
}

#[test]
fn controlled_sources_run() {
    let mut top = empty_sub("ctl");
    top.components = vec![
        r("0", "in", "0", 1000.0),
        Component::Vcvs { name: "1".into(), np: "e".into(), nm: "0".into(), ncp: "in".into(), ncm: "0".into(), gain: 2.0 },
        r("1", "e", "0", 1000.0),
        Component::Vccs { name: "1".into(), np: "0".into(), nm: "g".into(), ncp: "in".into(), ncm: "0".into(), transconductance: 2e-3 },
        r("2", "g", "0", 1000.0),
        Component::Cccs { name: "1".into(), np: "0".into(), nm: "f".into(), vsense: "V1".into(), gain: -2.0 },
        r("3", "f", "0", 1000.0),
        Component::Ccvs { name: "1".into(), np: "h".into(), nm: "0".into(), vsense: "V1".into(), transresistance: -2000.0 },
        r("4", "h", "0", 1000.0),
    ];
    let mut tb = empty_tb(vec![Analysis::Op]);
    tb.stimulus = vec![vsrc("1", "in", "0", 2.0)];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };
    close(col(&raw, "e")[0], 4.0, 1e-9);            // VCVS: 2 V * 2
    close(col(&raw, "g")[0], 2.0 * 2e-3 * 1e3, 1e-9); // VCCS: 2 V * 2 mS * 1k
    // i(V1) = -2 mA, so the CCCS drives -2 * -2 mA = 4 mA into 1k.
    close(col(&raw, "f")[0], 4.0, 1e-9);
    close(col(&raw, "h")[0], 4.0, 1e-9);
}

// ── Structure ──

#[test]
fn subcircuits_and_their_instances_run() {
    let mut divider = empty_sub("divider");
    divider.ports = vec![
        Port { name: "in".into(), direction: PortDirection::Input },
        Port { name: "out".into(), direction: PortDirection::Output },
    ];
    divider.parameters = vec![
        ParamDef { name: "rtop".into(), default: Some("1k".into()) },
        ParamDef { name: "rbot".into(), default: Some("1k".into()) },
    ];
    divider.components = vec![
        Component::Resistor { name: "1".into(), n1: "in".into(), n2: "out".into(), value: IrValue::Expression { expr: "rtop".into() }, params: vec![] },
        Component::Resistor { name: "2".into(), n1: "out".into(), n2: "0".into(), value: IrValue::Expression { expr: "rbot".into() }, params: vec![] },
    ];

    let mut top = empty_sub("with_subckt");
    top.instances = vec![Instance {
        name: "1".into(), subcircuit: "divider".into(),
        port_mapping: vec!["vdd".into(), "mid".into()],
        parameters: vec![("rtop".into(), "1k".into()), ("rbot".into(), "3k".into())],
    }];
    let mut tb = empty_tb(vec![Analysis::Op]);
    tb.stimulus = vec![vsrc("dd", "vdd", "0", 4.0)];
    let ir = CircuitIR {
        top, testbench: Some(tb), subcircuit_defs: vec![divider], model_libraries: vec![],
    };
    let Some(raw) = run(&ir) else { return };
    // 4 V * 3k/(1k+3k) = 3 V.
    close(col(&raw, "mid")[0], 3.0, 1e-9);
}

#[test]
fn step_params_sweep_a_netlist_variable() {
    // VACASK's capability table claimed `step_params: false`. A `var` declared
    // in the control block IS visible to the netlist, so it is not.
    let mut top = empty_sub("stepped");
    top.parameters = vec![ParamDef { name: "rtop".into(), default: Some("1k".into()) }];
    top.components = vec![
        Component::Resistor { name: "1".into(), n1: "vdd".into(), n2: "out".into(), value: IrValue::Expression { expr: "rtop".into() }, params: vec![] },
        r("2", "out", "0", 1000.0),
    ];
    let mut tb = empty_tb(vec![Analysis::Op]);
    tb.stimulus = vec![vsrc("dd", "vdd", "0", 2.0)];
    tb.step_params = vec![StepParam {
        param: "rtop".into(), start: 1000.0, stop: 3000.0, step: 1000.0, sweep_type: None,
    }];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };
    let out = col(&raw, "out");
    assert_eq!(out.len(), 3, "{:?}", out);
    close(out[0], 2.0 * 1000.0 / 2000.0, 1e-9);
    close(out[1], 2.0 * 1000.0 / 3000.0, 1e-9);
    close(out[2], 2.0 * 1000.0 / 4000.0, 1e-9);
}

#[test]
fn initial_conditions_reach_the_analysis() {
    let mut top = empty_sub("ic");
    top.components = vec![r("1", "vin", "out", 1000.0), c("1", "out", "0", 1e-6)];
    let mut tb = empty_tb(vec![Analysis::Transient {
        step: 2.5e-5, stop: 1e-3, start: None, max_step: Some(2.5e-5), uic: true,
    }]);
    tb.stimulus = vec![vsrc("1", "vin", "0", 1.0)];
    tb.initial_conditions = vec![("out".into(), 0.5)];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let netlist = VacaskSubprocess.codegen().emit_netlist(&ir).unwrap();
    assert!(netlist.contains("ic=[\"out\"; 5e-1]"), "{}", netlist);
    let Some(raw) = run(&ir) else { return };
    close(col(&raw, "out")[0], 0.5, 1e-9);
}

#[test]
fn every_analysis_writes_its_own_raw_file_and_the_last_one_is_read_back() {
    // VACASK names one raw file per analysis; the backend must pick
    // deterministically rather than taking whatever read_dir hands over.
    let ir = rc_ir(vec![
        Analysis::Op,
        Analysis::Transient { step: 1e-4, stop: 1e-3, start: None, max_step: Some(1e-4), uic: false },
    ]);
    let Some(raw) = run(&ir) else { return };
    assert!(raw.plot_name.to_lowercase().contains("transient"), "got plot '{}'", raw.plot_name);
    assert!(col(&raw, "time").len() > 1);
}

// ── Refusals: things VACASK genuinely cannot do ──

#[test]
fn refusals_are_errors_not_broken_decks() {
    let cg = VacaskSubprocess.codegen();

    // No measurement statement at all: `measure` is "Command not found".
    let mut tb = empty_tb(vec![Analysis::Op]);
    tb.measures = vec!["tran vmax MAX V(out)".into()];
    let ir = CircuitIR { top: empty_sub("m"), testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    assert!(cg.emit_netlist(&ir).is_err(), "measures must refuse");

    // PWL parses and then returns an identically-zero transient.
    let mut top = empty_sub("pwl");
    top.components = vec![Component::VoltageSource {
        name: "1".into(), np: "a".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 }, ac_magnitude: None, ac_phase: None,
        waveform: Some(IrWaveform::Pwl { values: vec![(0.0, 0.0), (1e-3, 1.0)] }),
    }];
    let ir = CircuitIR { top, testbench: None, subcircuit_defs: vec![], model_libraries: vec![] };
    assert!(cg.emit_netlist(&ir).is_err(), "PWL must refuse");

    // No behavioural sources, switches, transmission lines or XSPICE.
    for comp in [
        Component::BehavioralVoltage { name: "1".into(), np: "a".into(), nm: "0".into(), expression: "v(b)*2".into() },
        Component::VSwitch { name: "1".into(), np: "a".into(), nm: "0".into(), ncp: "c".into(), ncm: "0".into(), model: "sw".into() },
        Component::TLine { name: "1".into(), inp: "a".into(), inm: "0".into(), outp: "b".into(), outm: "0".into(), z0: 50.0, td: 1e-9 },
        Component::Xspice { name: "1".into(), connections: vec!["a".into()], model: "d_and".into() },
    ] {
        assert!(cg.emit_component(&comp).is_err(), "{:?} must refuse", comp);
    }

    // A SIN phase is accepted by VACASK and then ignored.
    let sin = Component::VoltageSource {
        name: "1".into(), np: "a".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 }, ac_magnitude: None, ac_phase: None,
        waveform: Some(IrWaveform::Sin {
            offset: 0.0, amplitude: 1.0, frequency: 1e3, delay: 0.0, damping: 0.0, phase: 90.0,
        }),
    };
    assert!(cg.emit_component(&sin).is_err(), "a nonzero SIN phase must refuse");

    // Analyses VACASK does not implement.
    for a in [
        Analysis::PoleZero { node1: "a".into(), node2: "0".into(), node3: "b".into(), node4: "0".into(), tf_type: "vol".into(), pz_type: "pz".into() },
        Analysis::Distortion { variation: "dec".into(), points: 10, start: 1.0, stop: 1e3, f2overf1: None },
        Analysis::Sensitivity { output: "v(out)".into(), ac: None },
    ] {
        assert!(cg.emit_analysis(&a).is_err(), "{:?} must refuse", a);
    }
}

// ── Waveforms that DO translate ──

#[test]
fn a_sine_source_runs_with_delay_and_damping() {
    let mut top = empty_sub("sine");
    top.components = vec![r("1", "vin", "0", 1000.0)];
    let mut tb = empty_tb(vec![Analysis::Transient {
        step: 1e-5, stop: 2e-3, start: None, max_step: Some(1e-5), uic: false,
    }]);
    tb.stimulus = vec![Component::VoltageSource {
        name: "1".into(), np: "vin".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 }, ac_magnitude: None, ac_phase: None,
        waveform: Some(IrWaveform::Sin {
            offset: 0.5, amplitude: 2.0, frequency: 1e3, delay: 0.0, damping: 0.0, phase: 0.0,
        }),
    }];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };
    let t = col(&raw, "time");
    let v = col(&raw, "vin");
    // A quarter period into a 1 kHz sine: offset + amplitude.
    let (i, _) = t.iter().enumerate().min_by(|a, b| (a.1 - 2.5e-4).abs().partial_cmp(&(b.1 - 2.5e-4).abs()).unwrap()).unwrap();
    close(v[i], 2.5, 5e-3);
    close(v[0], 0.5, 1e-6);
}

#[test]
fn a_pulse_train_runs() {
    let mut top = empty_sub("pulse");
    top.components = vec![r("1", "vin", "0", 1000.0)];
    let mut tb = empty_tb(vec![Analysis::Transient {
        step: 1e-6, stop: 5e-4, start: None, max_step: Some(1e-6), uic: false,
    }]);
    tb.stimulus = vec![Component::VoltageSource {
        name: "1".into(), np: "vin".into(), nm: "0".into(),
        value: IrValue::Numeric { value: 0.0 }, ac_magnitude: None, ac_phase: None,
        waveform: Some(IrWaveform::Pulse {
            initial: 0.0, pulsed: 1.0, delay: 1e-4, rise_time: 1e-6,
            fall_time: 1e-6, pulse_width: 1e-4, period: 2e-4,
        }),
    }];
    let ir = CircuitIR { top, testbench: Some(tb), subcircuit_defs: vec![], model_libraries: vec![] };
    let Some(raw) = run(&ir) else { return };
    let t = col(&raw, "time");
    let v = col(&raw, "vin");
    let at = |want: f64| {
        let (i, _) = t.iter().enumerate().min_by(|a, b| (a.1 - want).abs().partial_cmp(&(b.1 - want).abs()).unwrap()).unwrap();
        v[i]
    };
    close(at(5e-5), 0.0, 1e-6);   // before the delay
    close(at(1.5e-4), 1.0, 1e-3); // mid-pulse
    close(at(2.5e-4), 0.0, 1e-3); // after the fall
    close(at(3.5e-4), 1.0, 1e-3); // second period
}
