use pyspice::circuit::*;
use pyspice::unit::*;

#[test]
fn test_resistor_divider() {
    let mut c = Circuit::new("resistor_divider");
    c.v("in", "vin", Node::Ground, 5.0);
    c.r("1", "vin", "vout", UnitValue::new(1.0, U_KOHM));
    c.r("2", "vout", Node::Ground, UnitValue::new(1.0, U_KOHM));

    let netlist = c.to_string();
    assert!(netlist.contains(".title resistor_divider"));
    assert!(netlist.contains("Vin vin 0 5"));
    assert!(netlist.contains("R1 vin vout 1k"));
    assert!(netlist.contains("R2 vout 0 1k"));
    assert!(netlist.ends_with(".end"));
}

#[test]
fn test_all_passive_elements() {
    let mut c = Circuit::new("passives");
    c.r("1", "a", "b", 1000.0);
    c.c("1", "b", Node::Ground, 10e-12);
    c.l("1", "a", "c", 1e-6);
    c.k("1", "1", "2", 0.99);

    let netlist = c.to_string();
    assert!(netlist.contains("R1 a b 1k"));
    assert!(netlist.contains("C1 b 0 10p"));
    assert!(netlist.contains("L1 a c 1u"));
    assert!(netlist.contains("K1 L1 L2 0.99"));
}

#[test]
fn test_all_source_types() {
    let mut c = Circuit::new("sources");

    // DC sources
    c.v("dd", "vdd", Node::Ground, 3.3);
    c.i("bias", Node::Ground, "base", 10e-6);

    // Behavioral sources
    c.bv("1", "out", Node::Ground, "V(in)*2");
    c.bi("1", "out2", Node::Ground, "V(in)/1k");

    let netlist = c.to_string();
    assert!(netlist.contains("Vdd vdd 0 3.3"));
    assert!(netlist.contains("Ibias 0 base 10u"));
    assert!(netlist.contains("B1 out 0 V=V(in)*2"));
    assert!(netlist.contains("B1 out2 0 I=V(in)/1k"));
}

#[test]
fn test_all_controlled_sources() {
    let mut c = Circuit::new("controlled");
    c.e("1", "out_p", "out_m", "in_p", "in_m", 10.0);
    c.g("1", "out_p", "out_m", "in_p", "in_m", 1e-3);
    c.f("1", "out_p", "out_m", "Vsense", 100.0);
    c.h("1", "out_p", "out_m", "Vsense", 1e3);

    let netlist = c.to_string();
    assert!(netlist.contains("E1 out_p out_m in_p in_m 10"));
    assert!(netlist.contains("G1 out_p out_m in_p in_m 0.001"));
    assert!(netlist.contains("F1 out_p out_m Vsense 100"));
    assert!(netlist.contains("H1 out_p out_m Vsense 1000"));
}

#[test]
fn test_all_semiconductor_devices() {
    let mut c = Circuit::new("semiconductors");

    c.d("1", "anode", "cathode", "1N4148");
    c.q("1", "collector", "base", Node::Ground, "2n2222a");
    c.m("1", "drain", "gate", "source", "bulk", "nmos_3p3");
    c.j("1", "drain", "gate", "source", "njf");
    c.z("1", "drain", "gate", "source", "mes");

    let netlist = c.to_string();
    assert!(netlist.contains("D1 anode cathode 1N4148"));
    assert!(netlist.contains("Q1 collector base 0 2n2222a"));
    assert!(netlist.contains("M1 drain gate source bulk nmos_3p3"));
    assert!(netlist.contains("J1 drain gate source njf"));
    assert!(netlist.contains("Z1 drain gate source mes"));
}

#[test]
fn test_mosfet_aliases() {
    let mut c = Circuit::new("aliases");
    c.mosfet("1", "d", "g", "s", "b", "nmos");
    c.bjt("1", "c", "b", "e", "npn");

    let netlist = c.to_string();
    assert!(netlist.contains("M1 d g s b nmos"));
    assert!(netlist.contains("Q1 c b e npn"));
}

#[test]
fn test_switches_and_tline() {
    let mut c = Circuit::new("switches");
    c.s("1", "out", Node::Ground, "ctrl_p", "ctrl_m", "sw1");
    c.w("1", "out", Node::Ground, "Vctrl", "csw1");
    c.t("1", "in_p", "in_m", "out_p", "out_m", 50.0, 1e-9);

    let netlist = c.to_string();
    assert!(netlist.contains("S1 out 0 ctrl_p ctrl_m sw1"));
    assert!(netlist.contains("W1 out 0 Vctrl csw1"));
    assert!(netlist.contains("T1 in_p in_m out_p out_m Z0=50 TD=0.000000001"));
}

#[test]
fn test_model_definition() {
    let mut c = Circuit::new("models");
    c.model("nmos_3p3", "NMOS", vec![
        Param::new("LEVEL", "1"),
        Param::new("VTO", "0.7"),
        Param::new("KP", "110e-6"),
    ]);
    c.model("sw1", "SW", vec![
        Param::new("VT", "0.5"),
        Param::new("VH", "0.1"),
        Param::new("RON", "1"),
        Param::new("ROFF", "1e6"),
    ]);

    let netlist = c.to_string();
    assert!(netlist.contains(".model nmos_3p3 NMOS(LEVEL=1 VTO=0.7 KP=110e-6)"));
    assert!(netlist.contains(".model sw1 SW(VT=0.5 VH=0.1 RON=1 ROFF=1e6)"));
}

#[test]
fn test_includes_and_libs() {
    let mut c = Circuit::new("includes");
    c.include("/path/to/model.lib");
    c.lib("/path/to/pdk.lib", "tt");
    c.parameter("vdd_val", "3.3");

    let netlist = c.to_string();
    assert!(netlist.contains(".include \"/path/to/model.lib\""));
    assert!(netlist.contains(".lib \"/path/to/pdk.lib\" tt"));
    assert!(netlist.contains(".param vdd_val=3.3"));
}

#[test]
fn test_subcircuit_definition_and_instance() {
    let mut c = Circuit::new("subckt_test");

    c.subcircuit(SubCircuitDef {
        name: "inverter".into(),
        pins: vec!["in".into(), "out".into(), "vdd".into(), "vss".into()],
        elements: vec![
            Element::M(Mosfet {
                name: "p".into(),
                nd: Node::named("out"),
                ng: Node::named("in"),
                ns: Node::named("vdd"),
                nb: Node::named("vdd"),
                model: "pmos".into(),
                params: vec![],
            }),
            Element::M(Mosfet {
                name: "n".into(),
                nd: Node::named("out"),
                ng: Node::named("in"),
                ns: Node::named("vss"),
                nb: Node::named("vss"),
                model: "nmos".into(),
                params: vec![],
            }),
        ],
        models: vec![],
        params: vec![],
    });

    c.x("1", "inverter", vec!["a", "b", "vdd", "vss"]);
    c.x("2", "inverter", vec!["b", "c", "vdd", "vss"]);

    let netlist = c.to_string();
    assert!(netlist.contains(".subckt inverter in out vdd vss"));
    assert!(netlist.contains("Mp out in vdd vdd pmos"));
    assert!(netlist.contains("Mn out in vss vss nmos"));
    assert!(netlist.contains(".ends inverter"));
    assert!(netlist.contains("X1 a b vdd vss inverter"));
    assert!(netlist.contains("X2 b c vdd vss inverter"));
}

#[test]
fn test_raw_spice_escape() {
    let mut c = Circuit::new("raw_escape");
    c.r_raw("1", "in", "out", "9kOhm");

    let netlist = c.to_string();
    assert!(netlist.contains("R1 in out 9kOhm"));
}

#[test]
fn test_sinusoidal_voltage_source() {
    let mut c = Circuit::new("sin_test");
    c.sinusoidal_voltage_source("1", "in", Node::Ground, 0.0, 0.0, 1.0, 1000.0);

    let netlist = c.to_string();
    assert!(netlist.contains("V1 in 0"));
    assert!(netlist.contains("SIN("));
}

#[test]
fn test_pulse_voltage_source() {
    let mut c = Circuit::new("pulse_test");
    c.pulse_voltage_source("1", "clk", Node::Ground, 0.0, 3.3, 50e-9, 100e-9, 1e-9, 1e-9);

    let netlist = c.to_string();
    assert!(netlist.contains("V1 clk 0"));
    assert!(netlist.contains("PULSE("));
}

#[test]
fn test_pwl_voltage_source() {
    let mut c = Circuit::new("pwl_test");
    c.pwl_voltage_source("1", "in", Node::Ground, vec![
        (0.0, 0.0),
        (1e-6, 1.0),
        (2e-6, 0.0),
    ]);

    let netlist = c.to_string();
    assert!(netlist.contains("V1 in 0"));
    assert!(netlist.contains("PWL("));
}

#[test]
fn test_element_lookup() {
    let mut c = Circuit::new("lookup");
    c.r("1", "a", "b", 1000.0);
    c.c("1", "b", Node::Ground, 1e-12);

    assert!(c.element("1").is_some()); // finds R1 (name "1")
    assert!(c.element_by_spice_name("R1").is_some());
    assert!(c.element_by_spice_name("C1").is_some());
    assert!(c.element_by_spice_name("X99").is_none());
}

#[test]
fn test_ground_node() {
    let c = Circuit::new("gnd_test");
    assert_eq!(c.gnd(), Node::Ground);
    assert_eq!(c.gnd().spice_name(), "0");
}

#[test]
fn test_unit_values_in_components() {
    let mut c = Circuit::new("units");
    c.r("1", "in", "out", UnitValue::new(1.0, U_KOHM));
    c.c("1", "out", Node::Ground, UnitValue::new(10.0, U_PF));
    c.l("1", "in", "out2", UnitValue::new(1.0, U_UH));
    c.v("dd", "vdd", Node::Ground, UnitValue::new(3.3, U_V));
    c.i("bias", Node::Ground, "base", UnitValue::new(10.0, U_UA));

    let netlist = c.to_string();
    assert!(netlist.contains("R1 in out 1k"));
    assert!(netlist.contains("C1 out 0 10p"));
    assert!(netlist.contains("L1 in out2 1u"));
    assert!(netlist.contains("Vdd vdd 0 3.3"));
    assert!(netlist.contains("Ibias 0 base 10u"));
}

#[test]
fn test_full_circuit_from_todo() {
    // Reproduce the exact circuit from TODO.md section 1
    let mut c = Circuit::new("folded_cascode");
    c.m("1", "drain_1", "gate_1", "source_1", "bulk", "nmos_3p3");
    c.m("2", "drain_2", "gate_2", "source_2", "bulk", "pmos_3p3");
    c.r("1", "vdd", "drain_1", UnitValue::new(1.0, U_KOHM));
    c.c("1", "out", Node::Ground, UnitValue::new(10.0, U_PF));
    c.v("dd", "vdd", Node::Ground, UnitValue::new(3.3, U_V));

    let netlist = c.to_string();
    assert!(netlist.contains("M1 drain_1 gate_1 source_1 bulk nmos_3p3"));
    assert!(netlist.contains("M2 drain_2 gate_2 source_2 bulk pmos_3p3"));
    assert!(netlist.contains("R1 vdd drain_1 1k"));
    assert!(netlist.contains("C1 out 0 10p"));
    assert!(netlist.contains("Vdd vdd 0 3.3"));
}
