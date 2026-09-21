use spicerack::unit::*;

macro_rules! assert_approx {
    ($a:expr, $b:expr) => {
        let (a, b) = ($a as f64, $b as f64);
        assert!(
            (a - b).abs() < 1e-20_f64.max(b.abs() * 1e-10),
            "assert_approx failed: {} != {} (diff={})", a, b, (a - b).abs()
        );
    };
}

#[test]
fn test_all_voltage_units() {
    let v1 = UnitValue::new(3.3, U_V);
    assert_approx!(v1.value, 3.3);
    assert_eq!(v1.str_spice(), "3.3");

    let v2 = UnitValue::new(100.0, U_MV);
    assert_approx!(v2.value, 0.1);
    assert_eq!(v2.str_spice(), "100m");

    let v3 = UnitValue::new(500.0, U_UV);
    assert_approx!(v3.value, 500e-6);
    assert_eq!(v3.str_spice(), "500u");
}

#[test]
fn test_all_current_units() {
    let i1 = UnitValue::new(1.0, U_A);
    assert_approx!(i1.value, 1.0);

    let i2 = UnitValue::new(10.0, U_MA);
    assert_approx!(i2.value, 0.01);
    assert_eq!(i2.str_spice(), "10m");

    let i3 = UnitValue::new(10.0, U_UA);
    assert_approx!(i3.value, 10e-6);
    assert_eq!(i3.str_spice(), "10u");

    let i4 = UnitValue::new(5.0, U_NA);
    assert_approx!(i4.value, 5e-9);
    assert_eq!(i4.str_spice(), "5n");
}

#[test]
fn test_all_resistance_units() {
    let r1 = UnitValue::new(100.0, U_OHM);
    assert_approx!(r1.value, 100.0);
    assert_eq!(r1.str_spice(), "100");

    let r2 = UnitValue::new(1.0, U_KOHM);
    assert_approx!(r2.value, 1000.0);
    assert_eq!(r2.str_spice(), "1k");

    let r3 = UnitValue::new(1.0, U_MOHM);
    assert_approx!(r3.value, 1e6);
    assert_eq!(r3.str_spice(), "1meg");
}

#[test]
fn test_all_capacitance_units() {
    let c1 = UnitValue::new(1.0, U_UF);
    assert_approx!(c1.value, 1e-6);
    assert_eq!(c1.str_spice(), "1u");

    let c2 = UnitValue::new(1.0, U_NF);
    assert_approx!(c2.value, 1e-9);
    assert_eq!(c2.str_spice(), "1n");

    let c3 = UnitValue::new(10.0, U_PF);
    assert_approx!(c3.value, 10e-12);
    assert_eq!(c3.str_spice(), "10p");

    let c4 = UnitValue::new(100.0, U_FF);
    assert_approx!(c4.value, 100e-15);
    assert_eq!(c4.str_spice(), "100f");
}

#[test]
fn test_all_inductance_units() {
    let l1 = UnitValue::new(1.0, U_H);
    assert_approx!(l1.value, 1.0);

    let l2 = UnitValue::new(10.0, U_MH);
    assert_approx!(l2.value, 0.01);
    assert_eq!(l2.str_spice(), "10m");

    let l3 = UnitValue::new(1.0, U_UH);
    assert_approx!(l3.value, 1e-6);
    assert_eq!(l3.str_spice(), "1u");

    let l4 = UnitValue::new(100.0, U_NH);
    assert_approx!(l4.value, 100e-9);
    assert_eq!(l4.str_spice(), "100n");
}

#[test]
fn test_all_frequency_units() {
    let f1 = UnitValue::new(1.0, U_HZ);
    assert_approx!(f1.value, 1.0);

    let f2 = UnitValue::new(10.0, U_KHZ);
    assert_approx!(f2.value, 10e3);
    assert_eq!(f2.str_spice(), "10k");

    let f3 = UnitValue::new(1.0, U_MHZ);
    assert_approx!(f3.value, 1e6);
    assert_eq!(f3.str_spice(), "1meg");

    let f4 = UnitValue::new(1.0, U_GHZ);
    assert_approx!(f4.value, 1e9);
    assert_eq!(f4.str_spice(), "1g");
}

#[test]
fn test_all_time_units() {
    let t1 = UnitValue::new(1.0, U_S);
    assert_approx!(t1.value, 1.0);

    let t2 = UnitValue::new(1.0, U_MS);
    assert_approx!(t2.value, 1e-3);
    assert_eq!(t2.str_spice(), "1m");

    let t3 = UnitValue::new(1.0, U_US);
    assert_approx!(t3.value, 1e-6);
    assert_eq!(t3.str_spice(), "1u");

    let t4 = UnitValue::new(100.0, U_NS);
    assert_approx!(t4.value, 100e-9);
    assert_eq!(t4.str_spice(), "100n");

    let t5 = UnitValue::new(1.0, U_PS);
    assert_approx!(t5.value, 1e-12);
    assert_eq!(t5.str_spice(), "1p");
}

#[test]
fn test_display_with_unit_symbol() {
    let v = UnitValue::new(3.3, U_V);
    assert_eq!(format!("{}", v), "3.3V");

    let r = UnitValue::new(1.0, U_KOHM);
    assert_eq!(format!("{}", r), "1kOhm");

    let c = UnitValue::new(10.0, U_PF);
    assert_eq!(format!("{}", c), "10pF");
}

#[test]
fn test_f64_conversion() {
    let v = UnitValue::new(3.3, U_V);
    let f: f64 = v.into();
    assert_approx!(f, 3.3);

    let r = UnitValue::new(1.0, U_KOHM);
    assert_approx!(r.as_f64(), 1000.0);
}

#[test]
fn test_val_convenience() {
    let v = val(3.3, U_V);
    assert_approx!(v.value, 3.3);

    let r = val(10.0, U_KOHM);
    assert_approx!(r.value, 10000.0);
}

#[test]
fn test_degree_unit() {
    let temp = UnitValue::new(27.0, U_DEGREE);
    assert_approx!(temp.value, 27.0);
}

#[test]
fn test_watt_units() {
    let p1 = UnitValue::new(1.0, U_W);
    assert_approx!(p1.value, 1.0);

    let p2 = UnitValue::new(100.0, U_MW);
    assert_approx!(p2.value, 0.1);

    let p3 = UnitValue::new(50.0, U_UW);
    assert_approx!(p3.value, 50e-6);
}

#[test]
fn test_best_prefix_selection() {
    assert_eq!(SiPrefix::best_for(1000.0), SiPrefix::Kilo);
    assert_eq!(SiPrefix::best_for(1e-12), SiPrefix::Pico);
    assert_eq!(SiPrefix::best_for(1e6), SiPrefix::Mega);
    assert_eq!(SiPrefix::best_for(0.001), SiPrefix::Milli);
    assert_eq!(SiPrefix::best_for(0.0), SiPrefix::None);
}
