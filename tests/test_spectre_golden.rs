//! Golden-string tests for the Spectre codegen.
//!
//! Spectre is licence-gated and not installed, so nothing here can be executed
//! against the real simulator. The strongest evidence available is that the
//! text we emit matches the statement forms Cadence documents. Every assertion
//! below carries the citation it is checking.
//!
//! Sources:
//! * `[REF]` *Spectre Circuit Simulator Reference*, Product Version 19.1,
//!   January 2020 <https://ee.kpi.ua/~yv/edu/ok/book/spectre_refManual.pdf>
//! * `[CMP]` *Spectre Circuit Simulator Reference*, Product Version 5.0,
//!   September 2003 <http://eece.cu.edu.eg/~fhussien/Spectre_tutorial.pdf>
//!   (component chapters)
//! * `[KUN]` Kundert, *The Designer's Guide to SPICE and Spectre*, Appendix B
//!   <https://designers-guide.org/analysis/dg-spice/chB.pdf>
//!
//! These tests prove only "we emit what the manual describes". They do NOT
//! prove any simulation result.

use spicerack::codegen::{CodeGen, CodeGenError};
use spicerack::codegen::spectre::SpectreCodeGen;
use spicerack::ir::*;

fn empty_top(name: &str) -> Subcircuit {
    Subcircuit {
        name: name.into(),
        ports: vec![],
        parameters: vec![],
        components: vec![],
        instances: vec![],
        models: vec![],
        raw_spice: vec![],
        includes: vec![],
        libs: vec![],
        osdi_loads: vec![],
        verilog_blocks: vec![],
    }
}

fn empty_tb() -> Testbench {
    Testbench {
        dut: "top".into(),
        stimulus: vec![],
        analyses: vec![],
        options: SimOptions::default(),
        saves: vec![],
        measures: vec![],
        temperature: None,
        nominal_temperature: None,
        initial_conditions: vec![],
        node_sets: vec![],
        step_params: vec![],
        extra_lines: vec![],
    }
}

fn ir_with(top: Subcircuit, tb: Option<Testbench>) -> CircuitIR {
    CircuitIR { top, testbench: tb, subcircuit_defs: vec![], model_libraries: vec![] }
}

fn analysis(a: Analysis) -> String {
    SpectreCodeGen.emit_analysis(&a).unwrap()
}

fn component(c: Component) -> String {
    SpectreCodeGen.emit_component(&c).unwrap()
}

// ── Deck structure ──

/// `[KUN]` B.2: "all Spectre netlists must begin with a `lang=spectre`
/// statement"; `//` starts a comment.
/// `[REF]` p.494 `include "filename"`, p.493 `include "file" section=name`.
/// `[REF]` p.541 / `[CMP]` p.888 `ahdl_include "VerilogAfile.va"`.
/// `[REF]` p.506 `parameters <param=value> [param=value]...`.
#[test]
fn spectre_deck_preamble_matches_documented_statements() {
    let mut top = empty_top("Preamble");
    top.osdi_loads.push("/m/bsimcmg.va".into());
    top.includes.push("/pdk/models.scs".into());
    top.libs.push(("/pdk/corners.scs".into(), "tt".into()));
    top.parameters.push(ParamDef { name: "vdd".into(), default: Some("1.8".into()) });

    let netlist = SpectreCodeGen.emit_netlist(&ir_with(top, None)).unwrap();

    assert!(netlist.starts_with("// Preamble\n"), "{netlist}");
    assert!(netlist.contains("\nsimulator lang=spectre\n"), "{netlist}");
    assert!(netlist.contains("ahdl_include \"/m/bsimcmg.va\""), "{netlist}");
    assert!(netlist.contains("include \"/pdk/models.scs\""), "{netlist}");
    assert!(netlist.contains("include \"/pdk/corners.scs\" section=tt"), "{netlist}");
    assert!(netlist.contains("parameters vdd=1.8"), "{netlist}");
    // Spectre has no `.end`.
    assert!(!netlist.contains(".end"), "{netlist}");
}

/// `[REF]` p.535 verbatim:
/// ```text
/// subckt coax (i1 o1 i2 o2)
///      parameters zin=50 zout=50 vin=1 vout=1 len=0
/// ends coax
/// ```
/// plus the instance form `Coax1 pin nin out gnd coax zin=75 zout=150 len=35m`.
#[test]
fn spectre_subckt_and_instance_match_documented_forms() {
    let sc = Subcircuit {
        name: "coax".into(),
        ports: vec![
            Port { name: "i1".into(), direction: PortDirection::Input },
            Port { name: "o1".into(), direction: PortDirection::Output },
        ],
        parameters: vec![ParamDef { name: "zin".into(), default: Some("50".into()) }],
        components: vec![],
        instances: vec![],
        models: vec![],
        raw_spice: vec![],
        includes: vec![],
        libs: vec![],
        osdi_loads: vec![],
        verilog_blocks: vec![],
    };
    let emitted = SpectreCodeGen.emit_subcircuit(&sc).unwrap();
    assert_eq!(emitted, "subckt coax (i1 o1)\nparameters zin=50\nends coax");

    let mut top = empty_top("inst");
    top.instances.push(Instance {
        name: "Coax1".into(),
        subcircuit: "coax".into(),
        port_mapping: vec!["pin".into(), "nin".into()],
        parameters: vec![("zin".into(), "75".into())],
    });
    let netlist = SpectreCodeGen.emit_netlist(&ir_with(top, None)).unwrap();
    assert!(netlist.contains("xcoax1 (pin nin) coax zin=75"), "{netlist}");
}

/// `[CMP]` p.628 `model resmod resistor rsh=150 l=2u w=2u etch=0.05u tc1=0.1`,
/// `[REF]` p.498 `model nch bsim3v3 type=n mobmod=1 capmod=2 version=3.1`.
/// No documented Cadence example parenthesises model parameters.
#[test]
fn spectre_model_statement_has_no_parentheses() {
    let mut top = empty_top("models");
    top.models.push(ModelDef {
        name: "nch".into(),
        kind: "bsim3v3".into(),
        parameters: vec![("type".into(), "n".into()), ("version".into(), "3.1".into())],
    });
    let netlist = SpectreCodeGen.emit_netlist(&ir_with(top, None)).unwrap();
    assert!(netlist.contains("model nch bsim3v3 type=n version=3.1"), "{netlist}");
    assert!(!netlist.contains("bsim3v3 ("), "model params must not be parenthesised: {netlist}");
}

/// `[REF]` p.490 `ic 7=0 out=1 ...`, p.502 `nodeset 7=0 out=1 ...`,
/// p.518 `save 7 out OpAmp1.comp ...` — bare signal names, never `V(x)`.
#[test]
fn spectre_ic_nodeset_save_use_bare_signal_names() {
    let mut tb = empty_tb();
    tb.initial_conditions.push(("V(out)".into(), 1.2));
    tb.node_sets.push(("mid".into(), 0.5));
    tb.saves.push("V(out)".into());

    let netlist = SpectreCodeGen.emit_netlist(&ir_with(empty_top("t"), Some(tb))).unwrap();
    assert!(netlist.contains("\nic out=1.2\n"), "{netlist}");
    assert!(netlist.contains("\nnodeset mid=500m\n"), "{netlist}");
    assert!(netlist.contains("\nsave out\n"), "{netlist}");
}

/// `[REF]` p.17: Spectre supports "standard SPICE measurement functions
/// (.measure)". `.measure` is SPICE syntax, so it must sit inside a
/// `simulator lang=spice` region (`[REF]` p.493, `[KUN]` B.2).
#[test]
fn spectre_measure_is_fenced_by_language_switches() {
    let mut tb = empty_tb();
    tb.measures.push(".meas tran vmax MAX V(out)".into());
    let netlist = SpectreCodeGen.emit_netlist(&ir_with(empty_top("t"), Some(tb))).unwrap();
    assert!(
        netlist.contains("simulator lang=spice\n.meas tran vmax MAX V(out)\nsimulator lang=spectre"),
        "{netlist}",
    );
}

/// `[REF]` p.236 verbatim `myopt options temp=27`; options parameters
/// `reltol` (#1), `vabstol` (#3), `iabstol` (#4), `gmin` (#128),
/// `dcmaxiters` (#56). The p.237-240 parameter index has no plain `maxiters`.
#[test]
fn spectre_options_use_documented_parameter_names() {
    let opts = SimOptions {
        portable: vec![
            ("reltol".into(), "1e-4".into()),
            ("abstol".into(), "1e-13".into()),
            ("vntol".into(), "1e-7".into()),
            ("gmin".into(), "1e-13".into()),
            ("max_iterations".into(), "150".into()),
        ],
        backend_specific: Default::default(),
    };
    let s = SpectreCodeGen.emit_options(&opts).unwrap();
    assert_eq!(
        s,
        "myopts options reltol=1e-4 iabstol=1e-13 vabstol=1e-7 gmin=1e-13 dcmaxiters=150",
    );
    assert!(!s.contains(" maxiters="), "maxiters is a dc parameter, not an option: {s}");
}

// ── Components ──

/// Citations, one per line:
/// `[CMP]` p.628 `r1 (1 2) resistor r=1.2K m=2`
/// `[CMP]` p.283 `c2 (1 0) capacitor c=2.5u tc1=1e-8`
/// `[CMP]` p.372 `l33 (0 net29) inductor l=10e-9 r=1 m=1`
/// `[CMP]` p.577 `ml1 mutual_inductor coupling=1 ind1=l1 ind2=l2`
/// `[CMP]` p.680 `e1 (out1 0 pos neg) vcvs gain=10`
/// `[CMP]` p.678 `Name sink src ps ns ... vccs` with `gm`
/// `[CMP]` p.286 `vcs (pos gnd) cccs gain=2.5 probe=v1`
/// `[CMP]` p.288 `vvs (pos gnd) ccvs rm=1 probe=v1`
/// `[CMP]` p.302 `d0 (dp dn) pdiode ...` (`Name a c ModelName`)
/// `[CMP]` p.645 `t1 (1 0 2 0) lmodel z0=100`, params `z0` and `td`
/// `[CMP]` p.625 `rel1 (1 2 ps ns) my_relay ...` (`Name 1 2 ps ns ModelName`)
#[test]
fn spectre_primitives_match_documented_instance_forms() {
    assert_eq!(
        component(Component::Resistor {
            name: "1".into(), n1: "a".into(), n2: "b".into(),
            value: IrValue::Numeric { value: 1200.0 }, params: vec![("m".into(), "2".into())],
        }),
        "r1 (a b) resistor r=1.2k m=2",
    );
    // NOTE: the doc example is `c=2.5u`, but `crate::circuit::format_spice_number`
    // leaves a float artifact there (2.5e-6/1e-6 == 2.5000000000000004), so
    // this asserts on a value that divides cleanly. See the accompanying
    // report: the artifact is in shared code this change may not touch.
    assert_eq!(
        component(Component::Capacitor {
            name: "2".into(), n1: "1".into(), n2: "0".into(),
            value: IrValue::Numeric { value: 2e-6 }, params: vec![],
        }),
        "c2 (1 0) capacitor c=2u",
    );
    assert_eq!(
        component(Component::Inductor {
            name: "33".into(), n1: "0".into(), n2: "net29".into(),
            value: IrValue::Numeric { value: 10e-9 }, params: vec![],
        }),
        "l33 (0 net29) inductor l=10n",
    );
    assert_eq!(
        component(Component::MutualInductor {
            name: "1".into(), inductor1: "1".into(), inductor2: "2".into(), coupling: 1.0,
        }),
        "k1 mutual_inductor coupling=1 ind1=l1 ind2=l2",
    );
    assert_eq!(
        component(Component::Vcvs {
            name: "1".into(), np: "out1".into(), nm: "0".into(),
            ncp: "pos".into(), ncm: "neg".into(), gain: 10.0,
        }),
        "e1 (out1 0 pos neg) vcvs gain=10",
    );
    assert_eq!(
        component(Component::Vccs {
            name: "1".into(), np: "sink".into(), nm: "src".into(),
            ncp: "ps".into(), ncm: "ns".into(), transconductance: -1.0,
        }),
        "g1 (sink src ps ns) vccs gm=-1",
    );
    // `probe=` names an instance; our vsource emitter lowercases names, so the
    // reference has to be lowercased identically (`[KUN]` B.2: case sensitive).
    assert_eq!(
        component(Component::Cccs {
            name: "cs".into(), np: "pos".into(), nm: "gnd".into(),
            vsense: "V1".into(), gain: 2.5,
        }),
        "fcs (pos gnd) cccs probe=v1 gain=2.5",
    );
    assert_eq!(
        component(Component::Ccvs {
            name: "vs".into(), np: "pos".into(), nm: "gnd".into(),
            vsense: "V1".into(), transresistance: 1.0,
        }),
        "hvs (pos gnd) ccvs probe=v1 rm=1",
    );
    assert_eq!(
        component(Component::Diode {
            name: "0".into(), np: "dp".into(), nm: "dn".into(),
            model: "pdiode".into(), params: vec![("area".into(), "1".into())],
        }),
        "d0 (dp dn) pdiode area=1",
    );
    assert_eq!(
        component(Component::TLine {
            name: "1".into(), inp: "1".into(), inm: "0".into(),
            outp: "2".into(), outm: "0".into(), z0: 100.0, td: 1e-9,
        }),
        "t1 (1 0 2 0) tline z0=100 td=0.000000001",
    );
    assert_eq!(
        component(Component::VSwitch {
            name: "1".into(), np: "1".into(), nm: "2".into(),
            ncp: "ps".into(), ncm: "ns".into(), model: "my_relay".into(),
        }),
        "s1 (1 2 ps ns) my_relay",
    );
}

/// `[CMP]` p.683-685 `vsource`: `dc` (#1), `type` (#2, values
/// `dc pulse pwl sine exp`), `delay` (#4), `val0`/`val1`/`period`/`rise`/
/// `fall`/`width` (#5-#10), `wave` (#12), `sinedc`/`ampl`/`freq`/`sinephase`
/// (#19-#22), `damp` (#32), `td1`/`tau1`/`td2`/`tau2` (#33-#36),
/// `mag`/`phase` (#39/#40). Verbatim sample:
/// `vpulse1 (1 0) vsource type=pulse val0=0 val1=5 period=100n rise=10n fall=10n width=40n`
/// and `vpwl1 (1 0) vsource type=pwl wave=[1n 0 1.1n 2 1.5n 0.5 2n 3 5n 5]`.
#[test]
fn spectre_vsource_waveforms_match_documented_parameters() {
    let v = |wf: Option<IrWaveform>, mag: Option<f64>, ph: Option<f64>| {
        component(Component::VoltageSource {
            name: "1".into(), np: "1".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 },
            ac_magnitude: mag, ac_phase: ph, waveform: wf,
        })
    };

    assert_eq!(
        v(Some(IrWaveform::Pulse {
            initial: 0.0, pulsed: 5.0, delay: 0.0,
            rise_time: 10e-9, fall_time: 10e-9, pulse_width: 40e-9, period: 100e-9,
        }), None, None),
        "v1 (1 0) vsource dc=0 type=pulse val0=0 val1=5 delay=0 rise=10n fall=10n width=40n period=100n",
    );

    assert_eq!(
        v(Some(IrWaveform::Pwl { values: vec![(1e-9, 0.0), (1.5e-9, 0.5)] }), None, None),
        "v1 (1 0) vsource dc=0 type=pwl wave=[1n 0 1.5n 500m]",
    );

    // sine: `delay` and `damp` are the documented names. The previous codegen
    // emitted `sinedelay=` / `sinedamp=`, which vsource does not have.
    assert_eq!(
        v(Some(IrWaveform::Sin {
            offset: 1.0, amplitude: 0.5, frequency: 1e6,
            delay: 1e-9, damping: 2.0, phase: 45.0,
        }), None, None),
        "v1 (1 0) vsource dc=0 type=sine sinedc=1 ampl=500m freq=1M delay=1n damp=2 sinephase=45",
    );

    assert_eq!(
        v(Some(IrWaveform::Exp {
            initial: 0.0, pulsed: 1.0,
            rise_delay: 1e-9, rise_tau: 2e-9, fall_delay: 3e-9, fall_tau: 4e-9,
        }), None, None),
        "v1 (1 0) vsource dc=0 type=exp val0=0 val1=1 td1=1n tau1=2n td2=3n tau2=4n",
    );

    assert_eq!(v(None, Some(1.0), Some(45.0)), "v1 (1 0) vsource dc=0 mag=1 phase=45");
}

/// Things SPICE can express that Spectre has no documented spelling for must
/// error, not silently become a comment or a guessed statement.
#[test]
fn spectre_refuses_constructs_without_a_documented_spelling() {
    let cases: Vec<Component> = vec![
        // XSPICE A-devices are ngspice-only.
        Component::Xspice { name: "1".into(), connections: vec!["a".into()], model: "d_and".into() },
        // Raw SPICE cannot be placed without knowing where a
        // `simulator lang=spice` region may legally open.
        Component::RawSpice { line: ".model sw sw vt=0.5".into() },
        // `[CMP]` p.856: Spectre bsource expressions are `v(a,b)`,
        // `i("inst:idx")`, `$time` — not SPICE's `V()`/`I()`/`time`, and
        // Spectre is case sensitive.
        Component::BehavioralVoltage {
            name: "1".into(), np: "o".into(), nm: "0".into(), expression: "V(a)*2".into(),
        },
        Component::BehavioralCurrent {
            name: "1".into(), np: "o".into(), nm: "0".into(), expression: "I(V1)*2".into(),
        },
        // `[CMP]` p.625 `relay` is voltage controlled; p.643 `switch` changes
        // position only between analyses. Neither is SPICE's W element.
        Component::ISwitch {
            name: "1".into(), np: "a".into(), nm: "b".into(),
            vcontrol: "V1".into(), model: "csw".into(),
        },
        // `[CMP]` p.685 documents ammodindex/ammodfreq but no waveform
        // equation, so the SPICE AM() mapping cannot be established.
        Component::VoltageSource {
            name: "1".into(), np: "a".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 },
            ac_magnitude: None, ac_phase: None,
            waveform: Some(IrWaveform::Am {
                amplitude: 1.0, offset: 0.5, modulating_freq: 1e3,
                carrier_freq: 1e6, delay: 0.0,
            }),
        },
    ];

    for c in cases {
        let r = SpectreCodeGen.emit_component(&c);
        assert!(
            matches!(r, Err(CodeGenError::UnsupportedComponent { .. })),
            "expected UnsupportedComponent, got {r:?}",
        );
    }
}

// ── Analyses ──

/// `[REF]` p.20143 verbatim `dc1 dc` (a `dc` without a sweep parameter is the
/// operating point).
/// `[REF]` p.64: temperature is `param=temp`, a netlist parameter is a bare
/// `param=<name>`, and a device parameter needs `dev=<instance>` alongside
/// `param=<parameter>`. SPICE `.dc Vin ...` sweeps the source's `dc` value
/// (`[CMP]` p.683 vsource parameter #1).
#[test]
fn spectre_dc_sweeps_pick_the_documented_selector() {
    assert_eq!(analysis(Analysis::Op), "op1 dc");

    let dc = |src: &str| analysis(Analysis::Dc {
        sweeps: vec![DcSweep { source: src.into(), start: 0.0, stop: 5.0, step: 0.1 }],
    });
    assert_eq!(dc("Vin"), "dc1 dc dev=vin param=dc start=0 stop=5 step=100m");
    assert_eq!(dc("Iin"), "dc1 dc dev=iin param=dc start=0 stop=5 step=100m");
    assert_eq!(dc("temp"), "dc1 dc param=temp start=0 stop=5 step=100m");
    assert_eq!(dc("rload"), "dc1 dc param=rload start=0 stop=5 step=100m");
}

/// `[REF]` p.43-44 (ac) and p.416-417 (tran): `Name ac ...` /
/// `Name tran ...`. tran parameters `stop` (#1), `start` (#3),
/// `maxstep` (#8), `step` (#9). Verbatim `[REF]` p.433
/// `tran1 tran stop=0.5u noisefmax=10G noiseseed=1`.
#[test]
fn spectre_ac_and_tran_match_documented_parameters() {
    assert_eq!(
        analysis(Analysis::Ac { variation: "dec".into(), points: 100, start: 1.0, stop: 1e9 }),
        "ac1 ac start=1 stop=1G dec=100",
    );
    assert_eq!(
        analysis(Analysis::Transient {
            step: 1e-9, stop: 1e-6, start: Some(1e-7), max_step: Some(5e-9), uic: false,
        }),
        "tran1 tran step=1n stop=1u start=100n maxstep=5n",
    );
}

/// `[REF]` "Sweep interval parameters" (p.44, p.182, p.389, p.394, p.439, ...)
/// list `step lin dec log values valuesfile` only — Spectre has no octave
/// sweep, so SPICE's `oct` must be refused rather than emitted.
#[test]
fn spectre_rejects_octave_frequency_sweeps() {
    let r = SpectreCodeGen.emit_analysis(&Analysis::Ac {
        variation: "oct".into(), points: 10, start: 1.0, stop: 1e6,
    });
    assert!(matches!(r, Err(CodeGenError::UnsupportedAnalysis { .. })), "{r:?}");
}

/// `[REF]` p.181-182 noise: `Name [p] [n] noise parameter=value ...`,
/// "The optional terminals (p and n) specify the output of the circuit",
/// `oprobe` (#16) and `iprobe` (#17) name *components*. The old codegen passed
/// `oprobe=V(out)`, which is neither a component nor Spectre node syntax.
#[test]
fn spectre_noise_puts_the_output_in_the_terminal_list() {
    assert_eq!(
        analysis(Analysis::Noise {
            output: "out".into(), reference: "0".into(), source: "Vin".into(),
            variation: "dec".into(), points: 10, start: 1.0, stop: 1e6,
            points_per_summary: None,
        }),
        "noise1 (out 0) noise start=1 stop=1M dec=10 iprobe=vin",
    );
}

/// `[REF]` p.438-439 xf: "you can simply specify a voltage to be the output by
/// giving a pair of nodes on the xf analysis statement". xf computes the
/// transfer function from *every* independent source, so there is no
/// `source=` parameter — the old codegen emitted one.
///
/// INFERRED, NOT VERIFIED: `freq=0` as the stand-in for SPICE's DC `.tf`.
#[test]
fn spectre_xf_has_no_source_parameter() {
    let s = analysis(Analysis::Tf { output: "V(out)".into(), source: "Vin".into() });
    assert_eq!(s, "xf1 (out 0) xf freq=0");
    assert!(!s.contains("source="), "{s}");
}

/// `[REF]` p.524 verbatim: `sens (1 n2 7) for (analAC)` — `sens` is a control
/// statement naming an analysis, spelled
/// `sens (outputs) to (params) for (analyses)`.
#[test]
fn spectre_sens_matches_documented_control_statement() {
    assert_eq!(
        analysis(Analysis::Sensitivity { output: "V(out)".into(), ac: None }),
        "sensdc1 dc\nsens (out) for (sensdc1)",
    );
    assert_eq!(
        analysis(Analysis::Sensitivity {
            output: "out".into(),
            ac: Some(AcSweepParams { variation: "dec".into(), points: 10, start: 1.0, stop: 1e6 }),
        }),
        "sensac1 ac start=1 stop=1M dec=10\nsens (out) for (sensac1)",
    );
}

/// `[REF]` p.272-274 pss: `fund` (#2), `harms` (#4), `tstab` (#6), and
/// optional `[p] [n]` output terminals. There is no `ppv` and no `probe`.
/// `[REF]` p.90-91 hb: `fundfreqs=[...]` (#2) and `maxharms=[...]` (#3), not
/// `toneN`/`nharmN`.
/// `[REF]` p.387 sp / p.393 stb.
#[test]
fn spectre_rf_large_signal_analyses_match_documented_parameters() {
    assert_eq!(
        analysis(Analysis::Pss {
            fundamental: 1e9, stabilization: 1e-7,
            observe_node: "out".into(), points_per_period: 128, harmonics: 10,
        }),
        "pss1 (out 0) pss fund=1G tstab=100n harms=10",
    );
    assert_eq!(
        analysis(Analysis::HarmonicBalance {
            frequencies: vec![1e9, 1.01e9], harmonics: vec![5, 3],
        }),
        "hb1 hb fundfreqs=[1G 1.01G] maxharms=[5 3]",
    );
    assert_eq!(
        analysis(Analysis::SPar { variation: "dec".into(), points: 20, start: 1e6, stop: 1e9 }),
        "sp1 sp start=1M stop=1G dec=20",
    );
    assert_eq!(
        analysis(Analysis::Stability {
            probe: "IPRB0".into(), variation: "dec".into(), points: 20, start: 1.0, stop: 1e9,
        }),
        "stb1 stb start=1 stop=1G dec=20 probe=iprb0",
    );
}

/// `[REF]` p.245-246 (pac, `sweeptype`), p.253-256 (pnoise — probe parameters
/// are `oprobe`/`iprobe`, there is no `refprobe`), p.303-304 (pxf — the only
/// probe parameter is `probe`, there is no `isrc`), p.298-299 (pstb, `probe`).
#[test]
fn spectre_periodic_small_signal_analyses_drop_invented_parameters() {
    let pac = analysis(Analysis::SpectrePac {
        pss_fundamental: 1e9, pss_stabilization: 1e-7, pss_harmonics: 10,
        variation: "dec".into(), points: 100, start: 1.0, stop: 1e9,
        sweep_type: "relative".into(),
    });
    assert_eq!(
        pac,
        "pss1 pss fund=1G tstab=100n harms=10\n\
         pac1 pac start=1 stop=1G dec=100 sweeptype=relative",
    );

    let pnoise = analysis(Analysis::SpectrePnoise {
        pss_fundamental: 1e9, pss_stabilization: 1e-7, pss_harmonics: 10,
        output: "out".into(), reference: "0".into(),
        variation: "dec".into(), points: 10, start: 1.0, stop: 1e6,
    });
    assert_eq!(
        pnoise,
        "pss1 pss fund=1G tstab=100n harms=10\n\
         pnoise1 (out 0) pnoise start=1 stop=1M dec=10",
    );
    assert!(!pnoise.contains("refprobe"), "{pnoise}");

    let pxf = analysis(Analysis::SpectrePxf {
        pss_fundamental: 1e9, pss_stabilization: 1e-7, pss_harmonics: 10,
        output: "out".into(), source: "Vin".into(),
        variation: "dec".into(), points: 10, start: 1.0, stop: 1e6,
    });
    assert_eq!(
        pxf,
        "pss1 pss fund=1G tstab=100n harms=10\n\
         pxf1 (out 0) pxf start=1 stop=1M dec=10",
    );
    assert!(!pxf.contains("isrc"), "{pxf}");

    let pstb = analysis(Analysis::SpectrePstb {
        pss_fundamental: 1e9, pss_stabilization: 1e-7, pss_harmonics: 10,
        probe: "IPRB0".into(), variation: "dec".into(), points: 10, start: 1.0, stop: 1e6,
    });
    assert_eq!(
        pstb,
        "pss1 pss fund=1G tstab=100n harms=10\n\
         pstb1 pstb start=1 stop=1M dec=10 probe=iprb0",
    );
}

/// `[REF]` p.410 sweep verbatim:
/// ```text
/// swp sweep param=temp values=[-50 0 50 100 125] {
///                oppoint dc oppoint=logfile
/// }
/// ```
/// `[REF]` p.174 montecarlo verbatim:
/// `mc1 montecarlo variations=process seed=1234 numruns=200 { ... }`
/// — every parameter precedes the brace. The old codegen appended `seed=`
/// after the closing brace.
#[test]
fn spectre_sweep_and_montecarlo_blocks_match_documented_forms() {
    assert_eq!(
        analysis(Analysis::SpectreSweep {
            param: "rload".into(), start: 1e3, stop: 10e3, step: 1e3,
            inner: "ac1".into(), inner_type: "ac".into(),
        }),
        "sweep1 sweep param=rload start=1k stop=10k step=1k {\n  ac1 ac\n}",
    );
    assert_eq!(
        analysis(Analysis::SpectreMonteCarlo {
            iterations: 200, inner: "tran1".into(), inner_type: "tran".into(), seed: Some(1234),
        }),
        "mc1 montecarlo numruns=200 seed=1234 {\n  tran1 tran\n}",
    );
    assert_eq!(
        analysis(Analysis::SpectreMonteCarlo {
            iterations: 8, inner: "tran1".into(), inner_type: "tran".into(), seed: None,
        }),
        "mc1 montecarlo numruns=8 {\n  tran1 tran\n}",
    );
}

/// `[CMP]` p.323-325: Spectre's `fourier` is a *component* placed on a node
/// pair — `Name [p] [n] [pr] [nr] fourier parameter=value ...`, verbatim
/// `four1 (1 0) fourmod harms=50`, with `fund` as instance parameter #1. It is
/// not an analysis statement and has no `signal=` parameter.
#[test]
fn spectre_fourier_is_emitted_as_a_component() {
    assert_eq!(
        analysis(Analysis::Fourier {
            fundamental: 1e3,
            outputs: vec!["V(out)".into(), "mid".into()],
            num_harmonics: Some(10),
        }),
        "four1 (out 0) fourier fund=1k harms=10\nfour2 (mid 0) fourier fund=1k harms=10",
    );
}

/// Spectre has no `trnoise` analysis. `[REF]` p.433 verbatim:
/// `tran1 tran stop=0.5u noisefmax=10G noiseseed=1` — transient noise is a
/// `tran` parameter (#92 `noisefmax`, #94 `noiseseed`) and the IR carries no
/// bandwidth, so there is nothing faithful to emit.
#[test]
fn spectre_refuses_transient_noise() {
    let r = SpectreCodeGen.emit_analysis(&Analysis::TransientNoise { step: 1e-9, stop: 1e-6 });
    assert!(matches!(r, Err(CodeGenError::UnsupportedAnalysis { .. })), "{r:?}");
}

/// Analyses belonging to other simulators' dialects must be refused.
#[test]
fn spectre_refuses_foreign_analyses() {
    let r = SpectreCodeGen.emit_analysis(&Analysis::PoleZero {
        node1: "1".into(), node2: "0".into(), node3: "3".into(), node4: "0".into(),
        tf_type: "vol".into(), pz_type: "pz".into(),
    });
    assert!(matches!(r, Err(CodeGenError::UnsupportedAnalysis { .. })), "{r:?}");
}

/// Kundert Appendix B, Table B.1 (SI, used by Spectre) vs Table B.2 (SPICE):
/// SPICE's `meg`/`g`/`t` are not Spectre scale factors; SI spells them
/// `M`/`G`/`T`. `k m u n p f` mean the same in both.
#[test]
fn spectre_numbers_use_si_scale_factors() {
    let r = |v: f64| component(Component::Resistor {
        name: "1".into(), n1: "a".into(), n2: "b".into(),
        value: IrValue::Numeric { value: v }, params: vec![],
    });
    assert!(r(1e6).ends_with("r=1M"), "{}", r(1e6));
    assert!(r(1e9).ends_with("r=1G"), "{}", r(1e9));
    assert!(r(1e12).ends_with("r=1T"), "{}", r(1e12));
    assert!(r(1e3).ends_with("r=1k"), "{}", r(1e3));
    for v in [1e6_f64, 1e9, 1e12] {
        let s = r(v);
        assert!(!s.contains("meg") && !s.ends_with('g') && !s.ends_with('t'), "{s}");
    }
}
