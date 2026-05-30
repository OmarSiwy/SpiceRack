// Issue 01 — Wire IR CodeGen into the run path.
//
// The behaviour under test: when a simulator carries an IR, the netlist it
// actually runs is produced by the selected backend's `CodeGen`, NOT by
// `Circuit::Display`. This is the "round-trip: build IR, emit, assert no
// string-translation pass runs" test from the issue.

use pyspice::backend::ngspice::NgspiceSubprocess;
use pyspice::backend::xyce::XyceSubprocess;
use pyspice::backend::ltspice::LtspiceSubprocess;
use pyspice::backend::spectre::SpectreSubprocess;
use pyspice::backend::vacask::VacaskSubprocess;
use pyspice::backend::Backend;
use pyspice::circuit::Circuit;
use pyspice::codegen::spice3::{Spice3CodeGen, Spice3Dialect};
use pyspice::codegen::CodeGen;
use pyspice::ir::*;

/// Minimal IR: a resistor divider driven by a DC source, operating point.
fn rc_op_ir() -> CircuitIR {
    let top = Subcircuit {
        name: "rc".into(),
        ports: vec![],
        parameters: vec![],
        components: vec![
            Component::Resistor {
                name: "1".into(),
                n1: "vdd".into(),
                n2: "out".into(),
                value: IrValue::Numeric { value: 1000.0 },
                params: vec![],
            },
            Component::Resistor {
                name: "2".into(),
                n1: "out".into(),
                n2: "0".into(),
                value: IrValue::Numeric { value: 1000.0 },
                params: vec![],
            },
        ],
        instances: vec![],
        models: vec![],
        raw_spice: vec![],
        includes: vec![],
        libs: vec![],
        osdi_loads: vec![],
        verilog_blocks: vec![],
    };

    let testbench = Testbench {
        dut: "rc".into(),
        stimulus: vec![Component::VoltageSource {
            name: "dd".into(),
            np: "vdd".into(),
            nm: "0".into(),
            value: IrValue::Numeric { value: 3.3 },
            waveform: None,
        }],
        analyses: vec![Analysis::Op],
        options: SimOptions::default(),
        saves: vec![],
        measures: vec![],
        temperature: None,
        nominal_temperature: None,
        initial_conditions: vec![],
        node_sets: vec![],
        step_params: vec![],
        extra_lines: vec![],
    };

    CircuitIR {
        top,
        testbench: Some(testbench),
        subcircuit_defs: vec![],
        model_libraries: vec![],
    }
}

#[test]
fn ngspice_backend_names_its_codegen() {
    let ng = NgspiceSubprocess;
    assert_eq!(ng.codegen().backend_name(), "ngspice");
}

#[test]
fn run_path_netlist_comes_from_codegen_not_display() {
    let ir = rc_op_ir();

    // A simulator carrying the IR must emit the netlist it runs via the
    // backend's CodeGen — byte-identical to calling the CodeGen directly.
    let mut sim = Circuit::new("rc").simulator();
    sim.set_ir(ir.clone());

    let ng = NgspiceSubprocess;
    let produced = sim.netlist_to_run(&ng).expect("emit netlist for backend");

    let expected = Spice3CodeGen { dialect: Spice3Dialect::Ngspice }
        .emit_netlist(&ir)
        .expect("codegen emit");

    assert_eq!(produced, expected);
    // And it is a real netlist, not a translated string.
    assert!(produced.contains(".op"));
    assert!(produced.contains("R1 vdd out"));
}

// ── RED: each backend names its codegen correctly ──

#[test]
fn xyce_backend_names_its_codegen() {
    let xy = XyceSubprocess { parallel: false };
    assert_eq!(xy.codegen().backend_name(), "xyce");
}

#[test]
fn ltspice_backend_names_its_codegen() {
    let lt = LtspiceSubprocess {
        executable: std::path::PathBuf::from("ltspice"),
        use_wine: false,
        fast_access: false,
    };
    assert_eq!(lt.codegen().backend_name(), "ltspice");
}

#[test]
fn spectre_backend_names_its_codegen() {
    let sp = SpectreSubprocess;
    assert_eq!(sp.codegen().backend_name(), "spectre");
}

// ── RED: same IR → dialect-different netlists ──

#[test]
fn same_ir_produces_different_dialect_netlists() {
    let ir = rc_op_ir();

    let ng = NgspiceSubprocess;
    let xy = XyceSubprocess { parallel: false };
    let sp = SpectreSubprocess;

    let ng_netlist = ng.codegen().emit_netlist(&ir).unwrap();
    let xy_netlist = xy.codegen().emit_netlist(&ir).unwrap();
    let sp_netlist = sp.codegen().emit_netlist(&ir).unwrap();

    // All must be valid netlists containing the resistors + analysis
    assert!(ng_netlist.contains("R1 vdd out"), "ngspice R1");
    assert!(xy_netlist.contains("R1 vdd out"), "xyce R1");

    // Spectre uses different syntax: "r1 (vdd out) resistor r=1k"
    assert!(sp_netlist.contains("r1 (vdd out) resistor"), "spectre r1");

    // ngspice/xyce use .op, spectre uses "op1 dc"
    assert!(ng_netlist.contains(".op"), "ngspice .op");
    assert!(xy_netlist.contains(".op"), "xyce .op");
    assert!(sp_netlist.contains("op1 dc"), "spectre op1 dc");

    // ngspice uses .end, spectre does not
    assert!(ng_netlist.contains(".end"), "ngspice .end");
    assert!(xy_netlist.contains(".end"), "xyce .end");
    assert!(!sp_netlist.contains(".end"), "spectre no .end");

    // Netlists must differ between dialects
    assert_ne!(ng_netlist, sp_netlist, "ngspice vs spectre should differ");
}

#[test]
fn run_path_uses_backend_specific_codegen() {
    let ir = rc_op_ir();

    // Xyce simulator must emit via Xyce codegen, not Ngspice default
    let mut sim = Circuit::new("rc").simulator();
    sim.set_ir(ir.clone());

    let xy = XyceSubprocess { parallel: false };
    let produced = sim.netlist_to_run(&xy).expect("xyce netlist");

    let expected = Spice3CodeGen { dialect: Spice3Dialect::Xyce }
        .emit_netlist(&ir)
        .expect("xyce codegen emit");

    assert_eq!(produced, expected, "run path must use xyce codegen");
}

#[test]
fn run_path_spectre_uses_spectre_codegen() {
    let ir = rc_op_ir();

    let mut sim = Circuit::new("rc").simulator();
    sim.set_ir(ir.clone());

    let sp = SpectreSubprocess;
    let produced = sim.netlist_to_run(&sp).expect("spectre netlist");

    // Must be Spectre syntax, not SPICE
    assert!(produced.contains("simulator lang=spectre"), "spectre lang header");
    assert!(produced.contains("op1 dc"), "spectre op analysis");
    assert!(!produced.contains(".op"), "must not contain SPICE .op");
}

// ── Issue 04: Vacask codegen ──

#[test]
fn vacask_backend_names_its_codegen() {
    let vk = VacaskSubprocess;
    assert_eq!(vk.codegen().backend_name(), "vacask");
}

#[test]
fn vacask_codegen_emits_native_syntax() {
    let ir = rc_op_ir();
    let vk = VacaskSubprocess;
    let netlist = vk.codegen().emit_netlist(&ir).unwrap();

    // Vacask uses Spectre-like syntax
    assert!(netlist.contains("r1 (vdd out) resistor r="), "vacask r1");
    assert!(netlist.contains("r2 (out 0) resistor r="), "vacask r2");
    assert!(netlist.contains("op1 dc"), "vacask op");

    // No UNTRANSLATED lines
    assert!(!netlist.contains("UNTRANSLATED"), "no UNTRANSLATED: {}", netlist);

    // No SPICE syntax leaks
    assert!(!netlist.contains(".op"), "no .op");
    assert!(!netlist.contains(".end"), "no .end");
}

#[test]
fn vacask_codegen_osdi_uses_load() {
    let ir = CircuitIR {
        top: Subcircuit {
            name: "osdi_test".into(),
            ports: vec![], parameters: vec![], components: vec![],
            instances: vec![], models: vec![], raw_spice: vec![],
            includes: vec![], libs: vec![],
            osdi_loads: vec!["/models/bsim4.osdi".into()],
            verilog_blocks: vec![],
        },
        testbench: None, subcircuit_defs: vec![],
        model_libraries: vec![],
    };

    let vk = VacaskSubprocess;
    let netlist = vk.codegen().emit_netlist(&ir).unwrap();

    // Vacask uses `load`, not `ahdl_include` or `.pre_osdi`
    assert!(netlist.contains("load \"/models/bsim4.osdi\""), "load: {}", netlist);
    assert!(!netlist.contains("ahdl_include"), "no ahdl_include");
    assert!(!netlist.contains(".pre_osdi"), "no .pre_osdi");
}

#[test]
fn vacask_codegen_subcircuit_ends_has_name() {
    let ir = CircuitIR {
        top: Subcircuit {
            name: "top".into(),
            ports: vec![], parameters: vec![], components: vec![],
            instances: vec![Instance {
                name: "1".into(),
                subcircuit: "mybuf".into(),
                port_mapping: vec!["in".into(), "out".into()],
                parameters: vec![],
            }],
            models: vec![], raw_spice: vec![],
            includes: vec![], libs: vec![], osdi_loads: vec![],
            verilog_blocks: vec![],
        },
        testbench: None,
        subcircuit_defs: vec![Subcircuit {
            name: "mybuf".into(),
            ports: vec![
                Port { name: "in".into(), direction: PortDirection::Input },
                Port { name: "out".into(), direction: PortDirection::Output },
            ],
            parameters: vec![], components: vec![
                Component::Resistor {
                    name: "1".into(), n1: "in".into(), n2: "out".into(),
                    value: IrValue::Numeric { value: 1000.0 }, params: vec![],
                },
            ],
            instances: vec![], models: vec![], raw_spice: vec![],
            includes: vec![], libs: vec![], osdi_loads: vec![],
            verilog_blocks: vec![],
        }],
        model_libraries: vec![],
    };

    let vk = VacaskSubprocess;
    let netlist = vk.codegen().emit_netlist(&ir).unwrap();

    assert!(netlist.contains("subckt mybuf (in out)"), "subckt header: {}", netlist);
    assert!(netlist.contains("ends mybuf"), "ends with name: {}", netlist);
    // No broken empty `ends `
    assert!(!netlist.contains("ends \n"), "no empty ends");
}
