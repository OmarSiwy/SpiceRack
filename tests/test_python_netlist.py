"""
Comprehensive netlist generation tests.

Covers: every component type, waveform sources, directives, edge cases,
unit integration, element access, subcircuit instances, ground aliasing.

Run: maturin develop && pytest tests/test_python_netlist.py -v
"""
import pytest


def ps():
    try:
        import spicerack
        return spicerack
    except ImportError:
        pytest.skip("spicerack not built")


def unit():
    try:
        from spicerack import unit
        return unit
    except ImportError:
        pytest.skip("spicerack not built")


# ======================================================================
# Circuit creation & basic properties
# ======================================================================

class TestCircuitBasics:
    def test_empty_circuit_has_title_and_end(self):
        c = ps().Circuit("empty")
        netlist = str(c)
        assert ".title empty" in netlist
        assert ".end" in netlist

    def test_repr(self):
        c = ps().Circuit("repr_test")
        assert repr(c) == "Circuit('repr_test')"

    def test_gnd_is_zero(self):
        c = ps().Circuit("gnd")
        assert c.gnd == "0"

    def test_special_chars_in_title(self):
        c = ps().Circuit("Test: special & chars!")
        assert "Test: special & chars!" in str(c)

    def test_multiple_circuits_independent(self):
        c1 = ps().Circuit("c1")
        c2 = ps().Circuit("c2")
        c1.R(
            name="1",
            positive="a",
            negative="b",
            value=1000.0,
        )
        assert "R1" in str(c1)
        assert "R1" not in str(c2)


# ======================================================================
# Passive components
# ======================================================================

class TestPassives:
    def test_resistor_numeric(self):
        c = ps().Circuit("t")
        c.R(
            name="1",
            positive="a",
            negative="b",
            value=4700.0,
        )
        assert "R1 a b 4.7k" in str(c)

    def test_resistor_with_unit(self):
        u = unit()
        c = ps().Circuit("t")
        c.R(
            name="1",
            positive="a",
            negative="b",
            value=4.7 @ u.u_kOhm,
        )
        assert "R1 a b 4.7k" in str(c)

    def test_resistor_raw_spice(self):
        c = ps().Circuit("t")
        c.R(
            name="load",
            positive="out",
            negative=c.gnd,
            value=0.0,
            raw_spice="1Meg",
        )
        assert "Rload out 0 1Meg" in str(c)

    def test_capacitor_picofarad(self):
        c = ps().Circuit("t")
        c.C(
            name="1",
            positive="a",
            negative="b",
            value=100e-12,
        )
        assert "C1 a b 100p" in str(c)

    def test_capacitor_with_unit(self):
        u = unit()
        c = ps().Circuit("t")
        c.C(
            name="1",
            positive="a",
            negative="b",
            value=100 @ u.u_pF,
        )
        assert "C1 a b 100p" in str(c)

    def test_inductor_microhenry(self):
        c = ps().Circuit("t")
        c.L(
            name="1",
            positive="a",
            negative="b",
            value=2.2e-6,
        )
        assert "L1 a b 2.2u" in str(c)

    def test_mutual_inductance(self):
        c = ps().Circuit("t")
        c.L(
            name="1",
            positive="a",
            negative="b",
            value=1e-6,
        )
        c.L(
            name="2",
            positive="c",
            negative="d",
            value=1e-6,
        )
        c.K(
            name="1",
            inductor1="1",
            inductor2="2",
            coupling=0.95,
        )
        netlist = str(c)
        assert "K1 L1 L2 0.95" in netlist

    def test_very_small_value(self):
        c = ps().Circuit("t")
        c.C(
            name="1",
            positive="a",
            negative="b",
            value=1e-15,
        )
        assert "C1 a b 1f" in str(c)

    def test_very_large_value(self):
        c = ps().Circuit("t")
        c.R(
            name="1",
            positive="a",
            negative="b",
            value=1e6,
        )
        netlist = str(c)
        assert "R1 a b" in netlist


# ======================================================================
# Sources
# ======================================================================

class TestSources:
    def test_dc_voltage(self):
        c = ps().Circuit("t")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=5.0,
        )
        assert "Vdd vdd 0 5" in str(c)

    def test_dc_voltage_with_unit(self):
        u = unit()
        c = ps().Circuit("t")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=3.3 @ u.u_V,
        )
        assert "Vdd vdd 0 3.3" in str(c)

    def test_dc_current(self):
        c = ps().Circuit("t")
        c.I(
            name="ref",
            positive=c.gnd,
            negative="drain",
            value=100e-6,
        )
        assert "Iref 0 drain 100u" in str(c)

    def test_behavioral_voltage(self):
        c = ps().Circuit("t")
        c.BV(
            name="1",
            positive="out",
            negative=c.gnd,
            expression="V(in)*2 + 0.5",
        )
        netlist = str(c)
        assert "B1 out 0 V=V(in)*2 + 0.5" in netlist

    def test_behavioral_current(self):
        c = ps().Circuit("t")
        c.BI(
            name="1",
            positive="out",
            negative=c.gnd,
            expression="V(ctrl)/1k",
        )
        assert "B1 out 0 I=V(ctrl)/1k" in str(c)


# ======================================================================
# Controlled sources
# ======================================================================

class TestControlledSources:
    def test_vcvs(self):
        c = ps().Circuit("t")
        c.E(
            name="1",
            positive="op",
            negative="om",
            control_positive="ip",
            control_negative="im",
            voltage_gain=100.0,
        )
        assert "E1 op om ip im 100" in str(c)

    def test_vccs(self):
        c = ps().Circuit("t")
        c.G(
            name="m1",
            positive="op",
            negative="om",
            control_positive="ip",
            control_negative="im",
            transconductance=0.01,
        )
        assert "Gm1 op om ip im 0.01" in str(c)

    def test_cccs(self):
        c = ps().Circuit("t")
        c.F(
            name="1",
            positive="op",
            negative="om",
            vsense="Vsense",
            current_gain=50.0,
        )
        assert "F1 op om Vsense 50" in str(c)

    def test_ccvs(self):
        c = ps().Circuit("t")
        c.H(
            name="1",
            positive="op",
            negative="om",
            vsense="Vsense",
            transresistance=500.0,
        )
        assert "H1 op om Vsense 500" in str(c)


# ======================================================================
# Semiconductors
# ======================================================================

class TestSemiconductors:
    def test_diode(self):
        c = ps().Circuit("t")
        c.D(
            name="1",
            anode="anode",
            cathode="cathode",
            model="1N4148",
        )
        assert "D1 anode cathode 1N4148" in str(c)

    def test_bjt_npn(self):
        c = ps().Circuit("t")
        c.Q(
            name="1",
            collector="c",
            base="b",
            emitter="e",
            model="2N2222",
        )
        assert "Q1 c b e 2N2222" in str(c)

    def test_bjt_alias(self):
        c = ps().Circuit("t")
        c.BJT(
            name="1",
            collector="c",
            base="b",
            emitter="e",
            model="BC547",
        )
        assert "Q1 c b e BC547" in str(c)

    def test_mosfet(self):
        c = ps().Circuit("t")
        c.M(
            name="1",
            drain="d",
            gate="g",
            source="s",
            bulk="b",
            model="nmos3p3",
        )
        assert "M1 d g s b nmos3p3" in str(c)

    def test_mosfet_alias(self):
        c = ps().Circuit("t")
        c.MOSFET(
            name="1",
            drain="d",
            gate="g",
            source="s",
            bulk="b",
            model="pmos",
        )
        assert "M1 d g s b pmos" in str(c)

    def test_jfet(self):
        c = ps().Circuit("t")
        c.J(
            name="1",
            drain="d",
            gate="g",
            source="s",
            model="njf_mod",
        )
        assert "J1 d g s njf_mod" in str(c)

    def test_mesfet(self):
        c = ps().Circuit("t")
        c.Z(
            name="1",
            drain="d",
            gate="g",
            source="s",
            model="mes_mod",
        )
        assert "Z1 d g s mes_mod" in str(c)


# ======================================================================
# Switches and transmission lines
# ======================================================================

class TestSwitchesAndTLines:
    def test_voltage_switch(self):
        c = ps().Circuit("t")
        c.S(
            name="1",
            positive="out",
            negative=c.gnd,
            control_positive="cp",
            control_negative="cm",
            model="sw1",
        )
        assert "S1 out 0 cp cm sw1" in str(c)

    def test_current_switch(self):
        c = ps().Circuit("t")
        c.W(
            name="1",
            positive="out",
            negative=c.gnd,
            vcontrol="Vctrl",
            model="csw1",
        )
        assert "W1 out 0 Vctrl csw1" in str(c)

    def test_transmission_line(self):
        c = ps().Circuit("t")
        c.T(
            name="1",
            input_positive="ip",
            input_negative="im",
            output_positive="op",
            output_negative="om",
            Z0=50.0,
            TD=1e-9,
        )
        netlist = str(c)
        assert "T1 ip im op om" in netlist
        assert "Z0=50" in netlist
        assert "TD=" in netlist


# ======================================================================
# Waveform sources
# ======================================================================

class TestWaveformSources:
    def test_sinusoidal_voltage_default(self):
        c = ps().Circuit("t")
        c.SinusoidalVoltageSource(
            name="1",
            positive="in",
            negative=c.gnd,
        )
        netlist = str(c)
        assert "V1 in 0" in netlist
        assert "SIN(" in netlist

    def test_sinusoidal_voltage_custom(self):
        c = ps().Circuit("t")
        c.SinusoidalVoltageSource(
            name="1",
            positive="in",
            negative=c.gnd,
            dc_offset=1.0,
            offset=0.5,
            amplitude=2.0,
            frequency=5000.0,
        )
        netlist = str(c)
        assert "SIN(" in netlist

    def test_pulse_voltage(self):
        c = ps().Circuit("t")
        c.PulseVoltageSource(
            name="clk",
            positive="clk",
            negative=c.gnd,
            initial_value=0.0,
            pulsed_value=1.8,
            pulse_width=5e-9,
            period=10e-9,
            rise_time=0.1e-9,
            fall_time=0.1e-9,
        )
        netlist = str(c)
        assert "Vclk clk 0" in netlist
        assert "PULSE(" in netlist

    def test_pwl_voltage(self):
        c = ps().Circuit("t")
        c.PieceWiseLinearVoltageSource(
            name="1",
            positive="in",
            negative=c.gnd,
            values=[(0, 0), (1e-6, 1.0), (2e-6, 0.5), (3e-6, 0)],
        )
        netlist = str(c)
        assert "PWL(" in netlist

    def test_sinusoidal_current(self):
        c = ps().Circuit("t")
        c.SinusoidalCurrentSource(
            name="1",
            positive="in",
            negative=c.gnd,
            dc_offset=0.0,
            offset=0.0,
            amplitude=1e-3,
            frequency=10e3,
        )
        netlist = str(c)
        assert "I1 in 0" in netlist
        assert "SIN(" in netlist

    def test_pulse_current(self):
        c = ps().Circuit("t")
        c.PulseCurrentSource(
            name="1",
            positive="in",
            negative=c.gnd,
            initial_value=0.0,
            pulsed_value=1e-3,
            pulse_width=1e-6,
            period=2e-6,
        )
        netlist = str(c)
        assert "I1 in 0" in netlist
        assert "PULSE(" in netlist


# ======================================================================
# Directives
# ======================================================================

class TestDirectives:
    def test_model_with_params(self):
        c = ps().Circuit("t")
        c.model("nmos1", "NMOS", LEVEL=1, VTO=0.7, KP=110e-6)
        netlist = str(c)
        assert ".model nmos1 NMOS" in netlist
        assert "LEVEL=1" in netlist
        assert "VTO=0.7" in netlist

    def test_model_no_params(self):
        c = ps().Circuit("t")
        c.model("simple_d", "D")
        assert ".model simple_d D" in str(c)

    def test_include(self):
        c = ps().Circuit("t")
        c.include("/pdk/sky130/models.spice")
        assert ".include /pdk/sky130/models.spice" in str(c)

    def test_lib(self):
        c = ps().Circuit("t")
        c.lib("/pdk/models.lib", "tt")
        assert ".lib /pdk/models.lib tt" in str(c)

    def test_parameter(self):
        c = ps().Circuit("t")
        c.parameter("width", "1u")
        assert ".param width=1u" in str(c)

    def test_multiple_parameters(self):
        c = ps().Circuit("t")
        c.parameter("vdd", "3.3")
        c.parameter("length", "100n")
        netlist = str(c)
        assert ".param vdd=3.3" in netlist
        assert ".param length=100n" in netlist

    def test_subcircuit_instance(self):
        c = ps().Circuit("t")
        c.X("1", "NAND2", "a", "b", "out", "vdd", "gnd")
        netlist = str(c)
        assert "X1" in netlist
        assert "NAND2" in netlist

    def test_subcircuit_gnd_conversion(self):
        """Nodes named 'gnd' should become '0'."""
        c = ps().Circuit("t")
        c.X("1", "Buf", "in", "out", "gnd")
        assert "0" in str(c)


# ======================================================================
# Element access
# ======================================================================

class TestElementAccess:
    def test_getitem_exists(self):
        c = ps().Circuit("t")
        c.R(
            name="1",
            positive="a",
            negative="b",
            value=1e3,
        )
        result = c["1"]
        assert "R1" in result

    def test_getitem_not_found_raises(self):
        c = ps().Circuit("t")
        with pytest.raises(KeyError):
            c["nonexistent"]

    def test_element_method(self):
        c = ps().Circuit("t")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=3.3,
        )
        result = c.element("dd")
        assert "Vdd" in result

    def test_access_after_multiple_adds(self):
        c = ps().Circuit("t")
        c.R(
            name="1",
            positive="a",
            negative="b",
            value=1e3,
        )
        c.C(
            name="2",
            positive="b",
            negative=c.gnd,
            value=1e-12,
        )
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=5.0,
        )
        assert "R1" in c["1"]
        assert "C2" in c["2"]
        assert "Vdd" in c["dd"]


# ======================================================================
# Ground node aliasing
# ======================================================================

class TestGroundAliasing:
    def test_gnd_string_becomes_zero(self):
        c = ps().Circuit("t")
        c.R(
            name="1",
            positive="a",
            negative="gnd",
            value=1e3,
        )
        assert "R1 a 0" in str(c)

    def test_zero_stays_zero(self):
        c = ps().Circuit("t")
        c.R(
            name="1",
            positive="a",
            negative="0",
            value=1e3,
        )
        assert "R1 a 0" in str(c)

    def test_circuit_gnd_property(self):
        c = ps().Circuit("t")
        c.R(
            name="1",
            positive="a",
            negative=c.gnd,
            value=1e3,
        )
        assert "R1 a 0" in str(c)


# ======================================================================
# Complex realistic circuits
# ======================================================================

class TestRealisticCircuits:
    def test_voltage_divider(self):
        c = ps().Circuit("vdiv")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=3.3,
        )
        c.R(
            name="1",
            positive="vdd",
            negative="out",
            value=10e3,
        )
        c.R(
            name="2",
            positive="out",
            negative=c.gnd,
            value=10e3,
        )
        netlist = str(c)
        assert "Vdd vdd 0 3.3" in netlist
        assert "R1 vdd out 10k" in netlist
        assert "R2 out 0 10k" in netlist

    def test_diff_pair(self):
        c = ps().Circuit("dp")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=3.3,
        )
        c.R(
            name="d1",
            positive="vdd",
            negative="vo_m",
            value=5e3,
        )
        c.R(
            name="d2",
            positive="vdd",
            negative="vo_p",
            value=5e3,
        )
        c.MOSFET(
            name="1",
            drain="vo_m",
            gate="vi_m",
            source="tail",
            bulk=c.gnd,
            model="nmos",
        )
        c.MOSFET(
            name="2",
            drain="vo_p",
            gate="vi_p",
            source="tail",
            bulk=c.gnd,
            model="nmos",
        )
        c.I(
            name="ss",
            positive=c.gnd,
            negative="tail",
            value=200e-6,
        )
        netlist = str(c)
        assert "M1" in netlist
        assert "M2" in netlist
        assert "Iss" in netlist

    def test_bridge_rectifier(self):
        c = ps().Circuit("br")
        c.model("D1N", "D", IS=2.52e-9)
        c.SinusoidalVoltageSource(
            name="in",
            positive="ac_p",
            negative="ac_m",
            amplitude=12.0,
            frequency=60.0,
        )
        c.D(
            name="1",
            anode="ac_p",
            cathode="out_p",
            model="D1N",
        )
        c.D(
            name="2",
            anode="out_m",
            cathode="ac_p",
            model="D1N",
        )
        c.D(
            name="3",
            anode="ac_m",
            cathode="out_p",
            model="D1N",
        )
        c.D(
            name="4",
            anode="out_m",
            cathode="ac_m",
            model="D1N",
        )
        c.C(
            name="1",
            positive="out_p",
            negative="out_m",
            value=100e-6,
        )
        c.R(
            name="load",
            positive="out_p",
            negative="out_m",
            value=1e3,
        )
        c.V(
            name="ref",
            positive="out_m",
            negative=c.gnd,
            value=0.0,
        )
        netlist = str(c)
        assert "D1" in netlist
        assert "D2" in netlist
        assert "D3" in netlist
        assert "D4" in netlist
        assert ".model D1N D" in netlist

    def test_ring_oscillator_netlist(self):
        c = ps().Circuit("ro")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=1.8,
        )
        c.model("nm", "NMOS", LEVEL=1, VTO=0.4)
        c.model("pm", "PMOS", LEVEL=1, VTO=-0.4)
        for i in range(1, 4):
            prev = f"n{(i-2)%3+1}" if i > 1 else "n3"
            out = f"n{i}"
            c.MOSFET(
                name=f"{i}n",
                drain=out,
                gate=prev,
                source=c.gnd,
                bulk=c.gnd,
                model="nm",
            )
            c.MOSFET(
                name=f"{i}p",
                drain=out,
                gate=prev,
                source="vdd",
                bulk="vdd",
                model="pm",
            )
        netlist = str(c)
        for i in range(1, 4):
            assert f"M{i}n" in netlist
            assert f"M{i}p" in netlist

    def test_many_components(self):
        """Stress test: 100 resistors in series."""
        c = ps().Circuit("stress")
        c.V(
            name="in",
            positive="n0",
            negative=c.gnd,
            value=10.0,
        )
        for i in range(100):
            c.R(
                name=str(i+1),
                positive=f"n{i}",
                negative=f"n{i+1}",
                value=100.0,
            )
        c.R(
            name="load",
            positive="n100",
            negative=c.gnd,
            value=1e3,
        )
        netlist = str(c)
        assert "R1 n0 n1 100" in netlist
        assert "R100 n99 n100 100" in netlist
        assert "Rload n100 0" in netlist
