"""
Python tests for IR type bindings: Subcircuit, Testbench, ModelLibrary.

Run with: maturin develop && pytest tests/test_python_subcircuit.py
"""
import pytest
import json
import tempfile
import os


def import_pyspice():
    """Import pyspice_rs, skip if not built yet."""
    try:
        import pyspice_rs
        return pyspice_rs
    except ImportError:
        pytest.skip("pyspice_rs not built -- run 'maturin develop' first")


# ── Subcircuit Tests ──


class TestSubcircuitCreation:
    def test_create_subcircuit(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("inverter", ["vdd", "vss", "vin", "vout"])
        assert "inverter" in str(sc)

    def test_create_subcircuit_no_ports(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("top")
        assert "top" in str(sc)

    def test_create_subcircuit_with_params(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("amp", ["in", "out"], W="1u", L="180n")
        json_str = sc.to_json()
        data = json.loads(json_str)
        param_names = [p["name"] for p in data["parameters"]]
        assert "W" in param_names
        assert "L" in param_names

    def test_repr(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("buf", ["a", "b"])
        r = repr(sc)
        assert "Subcircuit" in r
        assert "buf" in r

    def test_gnd(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("test")
        assert sc.gnd == "0"


class TestSubcircuitComponents:
    def test_resistor(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("r_test", ["in", "out"])
        sc.R(
            name="1",
            positive="in",
            negative="out",
            value=1000.0,
        )
        data = json.loads(sc.to_json())
        assert len(data["components"]) == 1
        assert data["components"][0]["type"] == "Resistor"
        assert data["components"][0]["name"] == "1"

    def test_capacitor(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("c_test")
        sc.C(
            name="1",
            positive="out",
            negative=sc.gnd,
            value=10e-12,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Capacitor"

    def test_inductor(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("l_test")
        sc.L(
            name="1",
            positive="in",
            negative="out",
            value=1e-6,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Inductor"

    def test_mutual_inductor(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("k_test")
        sc.L(
            name="1",
            positive="a",
            negative="b",
            value=1e-3,
        )
        sc.L(
            name="2",
            positive="c",
            negative="d",
            value=1e-3,
        )
        sc.K(
            name="1",
            inductor1="1",
            inductor2="2",
            coupling=0.99,
        )
        data = json.loads(sc.to_json())
        assert len(data["components"]) == 3
        assert data["components"][2]["type"] == "MutualInductor"

    def test_voltage_source(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("v_test")
        sc.V(
            name="dd",
            positive="vdd",
            negative=sc.gnd,
            value=3.3,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "VoltageSource"
        assert data["components"][0]["value"]["value"] == 3.3

    def test_current_source(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("i_test")
        sc.I(
            name="bias",
            positive="vdd",
            negative="out",
            value=100e-6,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "CurrentSource"

    def test_behavioral_voltage(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("bv_test")
        sc.BV(
            name="1",
            positive="out",
            negative=sc.gnd,
            expression="V(in)*2",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "BehavioralVoltage"
        assert data["components"][0]["expression"] == "V(in)*2"

    def test_behavioral_current(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("bi_test")
        sc.BI(
            name="1",
            positive="out",
            negative=sc.gnd,
            expression="V(in)/1k",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "BehavioralCurrent"

    def test_vcvs(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("e_test")
        sc.E(
            name="1",
            positive="out",
            negative=sc.gnd,
            control_positive="inp",
            control_negative="inm",
            voltage_gain=10.0,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Vcvs"
        assert data["components"][0]["gain"] == 10.0

    def test_vccs(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("g_test")
        sc.G(
            name="1",
            positive="out",
            negative=sc.gnd,
            control_positive="inp",
            control_negative="inm",
            transconductance=1e-3,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Vccs"

    def test_cccs(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("f_test")
        sc.F(
            name="1",
            positive="out",
            negative=sc.gnd,
            vsense="Vsense",
            current_gain=100.0,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Cccs"

    def test_ccvs(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("h_test")
        sc.H(
            name="1",
            positive="out",
            negative=sc.gnd,
            vsense="Vsense",
            transresistance=1e3,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Ccvs"

    def test_diode(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("d_test")
        sc.D(
            name="1",
            anode="anode",
            cathode="cathode",
            model="D1N4148",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Diode"
        assert data["components"][0]["model"] == "D1N4148"

    def test_bjt(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("q_test")
        sc.Q(
            name="1",
            collector="collector",
            base="base",
            emitter="emitter",
            model="2N2222",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Bjt"

    def test_bjt_alias(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("bjt_test")
        sc.BJT(
            name="1",
            collector="c",
            base="b",
            emitter="e",
            model="npn_model",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Bjt"

    def test_mosfet(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("m_test")
        sc.M(
            name="p1",
            drain="vout",
            gate="vin",
            source="vdd",
            bulk="vdd",
            model="pmos",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Mosfet"
        assert data["components"][0]["model"] == "pmos"

    def test_mosfet_alias(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("mos_test")
        sc.MOSFET(
            name="1",
            drain="d",
            gate="g",
            source="s",
            bulk="b",
            model="nmos",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Mosfet"

    def test_jfet(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("j_test")
        sc.J(
            name="1",
            drain="drain",
            gate="gate",
            source="source",
            model="jfet_model",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Jfet"

    def test_mesfet(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("z_test")
        sc.Z(
            name="1",
            drain="drain",
            gate="gate",
            source="source",
            model="mesfet_model",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Mesfet"

    def test_vswitch(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("s_test")
        sc.S(
            name="1",
            positive="out",
            negative=sc.gnd,
            control_positive="ctrl_p",
            control_negative="ctrl_m",
            model="sw_model",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "VSwitch"

    def test_iswitch(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("w_test")
        sc.W(
            name="1",
            positive="out",
            negative=sc.gnd,
            vcontrol="Vctrl",
            model="isw_model",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "ISwitch"

    def test_tline(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("t_test")
        sc.T(
            name="1",
            input_positive="inp",
            input_negative="inm",
            output_positive="outp",
            output_negative="outm",
            Z0=50.0,
            TD=1e-9,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "TLine"
        assert data["components"][0]["z0"] == 50.0

    def test_xspice(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("a_test")
        sc.A(
            name="adc1",
            connections=["[vin]", "[dout]"],
            model="adc_buf",
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["type"] == "Xspice"

    def test_raw_resistor(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("raw_r_test")
        sc.R(
            name="1",
            positive="in",
            negative="out",
            value=0.0,
            raw_spice="1k tc=0.01",
        )
        data = json.loads(sc.to_json())
        comp = data["components"][0]
        assert comp["type"] == "Resistor"
        assert comp["value"]["type"] == "Raw"


class TestSubcircuitDirectives:
    def test_model(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("model_test")
        sc.model("nmos_3p3", "NMOS", VTO="0.7", KP="110e-6")
        data = json.loads(sc.to_json())
        assert len(data["models"]) == 1
        assert data["models"][0]["name"] == "nmos_3p3"

    def test_raw_spice(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("raw_test")
        sc.raw_spice(".options reltol=1e-6")
        data = json.loads(sc.to_json())
        assert ".options reltol=1e-6" in data["raw_spice"]

    def test_include(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("inc_test")
        sc.include("/path/to/model.lib")
        data = json.loads(sc.to_json())
        assert "/path/to/model.lib" in data["includes"]

    def test_lib(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("lib_test")
        sc.lib("/path/to/pdk.lib", "tt")
        data = json.loads(sc.to_json())
        assert ["/path/to/pdk.lib", "tt"] in data["libs"]

    def test_osdi(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("osdi_test")
        sc.osdi("/path/to/model.osdi")
        data = json.loads(sc.to_json())
        assert "/path/to/model.osdi" in data["osdi_loads"]

    def test_parameter(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("param_test")
        sc.parameter("vdd", "3.3")
        data = json.loads(sc.to_json())
        param_names = [p["name"] for p in data["parameters"]]
        assert "vdd" in param_names


class TestSubcircuitInstances:
    def test_instance(self):
        ps = import_pyspice()
        inv = ps.Subcircuit("inverter", ["vdd", "vss", "vin", "vout"])
        inv.M(
            name="p1",
            drain="vout",
            gate="vin",
            source="vdd",
            bulk="vdd",
            model="pmos",
        )
        inv.M(
            name="n1",
            drain="vout",
            gate="vin",
            source="vss",
            bulk="vss",
            model="nmos",
        )

        top = ps.Subcircuit("top")
        top.instance(inv, "inv1", "vdd", "vss", "a", "b")
        data = json.loads(top.to_json())
        assert len(data["instances"]) == 1
        assert data["instances"][0]["subcircuit"] == "inverter"

    def test_x_instance(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("top")
        sc.X("1", "MyBuf", "in", "out", "vdd", "gnd")
        data = json.loads(sc.to_json())
        assert len(data["instances"]) == 1
        assert data["instances"][0]["subcircuit"] == "MyBuf"


class TestSubcircuitSerialization:
    def test_json_roundtrip(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("amp", ["in", "out"])
        sc.R(
            name="1",
            positive="in",
            negative="out",
            value=1000.0,
        )
        sc.C(
            name="1",
            positive="out",
            negative=sc.gnd,
            value=10e-12,
        )
        json_str = sc.to_json()
        sc2 = ps.Subcircuit.from_json(json_str)
        assert sc2.to_json() == json_str

    def test_save_and_load(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("roundtrip", ["a", "b"])
        sc.R(
            name="1",
            positive="a",
            negative="b",
            value=470.0,
        )
        sc.model("d_model", "D", IS="1e-14")

        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
            path = f.name
        try:
            sc.save_json(path)
            sc2 = ps.Subcircuit.load_json(path)
            assert sc2.to_json() == sc.to_json()
        finally:
            os.unlink(path)

    def test_from_json_invalid(self):
        ps = import_pyspice()
        with pytest.raises(ValueError):
            ps.Subcircuit.from_json("not valid json")


class TestSubcircuitWaveforms:
    def test_sinusoidal_voltage(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("sin_test")
        sc.SinusoidalVoltageSource(
            name="in",
            positive="inp",
            negative=sc.gnd,
            amplitude=1.0,
            frequency=1e6,
        )
        data = json.loads(sc.to_json())
        comp = data["components"][0]
        assert comp["waveform"]["type"] == "Sin"

    def test_pulse_voltage(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("pulse_test")
        sc.PulseVoltageSource(
            name="clk",
            positive="clk",
            negative=sc.gnd,
            pulsed_value=3.3,
            period=10e-9,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["waveform"]["type"] == "Pulse"

    def test_pwl_voltage(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("pwl_test")
        sc.PieceWiseLinearVoltageSource(
            name="ramp",
            positive="ramp",
            negative=sc.gnd,
            values=[(0, 0), (1e-3, 1.0)],
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["waveform"]["type"] == "Pwl"

    def test_sinusoidal_current(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("sin_i_test")
        sc.SinusoidalCurrentSource(
            name="in",
            positive="inp",
            negative=sc.gnd,
            amplitude=1e-3,
            frequency=1e3,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["waveform"]["type"] == "Sin"

    def test_pulse_current(self):
        ps = import_pyspice()
        sc = ps.Subcircuit("pulse_i_test")
        sc.PulseCurrentSource(
            name="pulse",
            positive="out",
            negative=sc.gnd,
            pulsed_value=1e-3,
        )
        data = json.loads(sc.to_json())
        assert data["components"][0]["waveform"]["type"] == "Pulse"


# ── Testbench Tests ──


class TestTestbenchCreation:
    def test_create_testbench(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("my_amp", ["inp", "out"])
        dut.R(
            name="1",
            positive="inp",
            negative="out",
            value=10e3,
        )
        tb = ps.Testbench(dut)
        assert "my_amp" in str(tb)

    def test_stimulus(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("rc", ["in", "out"])
        dut.R(
            name="1",
            positive="in",
            negative="out",
            value=1000.0,
        )
        dut.C(
            name="1",
            positive="out",
            negative=dut.gnd,
            value=1e-9,
        )

        tb = ps.Testbench(dut)
        tb.V(
            name="in",
            positive="in",
            negative=dut.gnd,
            value=1.0,
        )
        json_str = tb.to_json()
        data = json.loads(json_str)
        assert len(data["testbench"]["stimulus"]) == 1

    def test_add_subcircuit(self):
        ps = import_pyspice()
        inv = ps.Subcircuit("inverter", ["vdd", "vss", "vin", "vout"])
        inv.M(
            name="p1",
            drain="vout",
            gate="vin",
            source="vdd",
            bulk="vdd",
            model="pmos",
        )

        dut = ps.Subcircuit("top")
        dut.instance(inv, "inv1", "vdd", "vss", "a", "b")

        tb = ps.Testbench(dut)
        tb.add_subcircuit(inv)
        json_str = tb.to_json()
        data = json.loads(json_str)
        assert len(data["subcircuit_defs"]) == 1

    def test_options(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("test")
        dut.R(
            name="1",
            positive="a",
            negative="b",
            value=100.0,
        )
        tb = ps.Testbench(dut)
        tb.options(reltol="1e-6", abstol="1e-12")
        json_str = tb.to_json()
        data = json.loads(json_str)
        opts = data["testbench"]["options"]["portable"]
        opt_keys = [o[0] for o in opts]
        assert "reltol" in opt_keys

    def test_temperature(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("test")
        dut.R(
            name="1",
            positive="a",
            negative="b",
            value=100.0,
        )
        tb = ps.Testbench(dut)
        tb.temperature = 85.0
        json_str = tb.to_json()
        data = json.loads(json_str)
        assert data["testbench"]["temperature"] == 85.0

    def test_save(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("test")
        dut.R(
            name="1",
            positive="a",
            negative="b",
            value=100.0,
        )
        tb = ps.Testbench(dut)
        tb.save("v(out)", "i(Vin)")
        json_str = tb.to_json()
        data = json.loads(json_str)
        assert "v(out)" in data["testbench"]["saves"]

    def test_measure(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("test")
        dut.R(
            name="1",
            positive="a",
            negative="b",
            value=100.0,
        )
        tb = ps.Testbench(dut)
        tb.measure("TRAN", "vmax", "MAX", "V(out)")
        json_str = tb.to_json()
        data = json.loads(json_str)
        assert len(data["testbench"]["measures"]) == 1

    def test_step(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("test")
        dut.R(
            name="1",
            positive="a",
            negative="b",
            value=100.0,
        )
        tb = ps.Testbench(dut)
        tb.step("R1", 100.0, 10000.0, 100.0)
        json_str = tb.to_json()
        data = json.loads(json_str)
        assert len(data["testbench"]["step_params"]) == 1

    def test_waveform_sources(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("test")
        dut.R(
            name="1",
            positive="a",
            negative="b",
            value=100.0,
        )
        tb = ps.Testbench(dut)
        tb.SinusoidalVoltageSource(
            name="in",
            positive="a",
            negative="0",
            amplitude=1.0,
            frequency=1e6,
        )
        json_str = tb.to_json()
        data = json.loads(json_str)
        stim = data["testbench"]["stimulus"]
        assert len(stim) == 1
        assert stim[0]["waveform"]["type"] == "Sin"

    def test_current_waveform_sources(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("current_tb", ["in", "out"])
        tb = ps.Testbench(dut)
        tb.PulseCurrentSource(
            name="q",
            positive="in",
            negative="0",
            pulsed_value=1e-6,
            pulse_width=1e-9,
        )
        data = json.loads(tb.to_json())
        stim = data["testbench"]["stimulus"]
        assert stim[0]["type"] == "CurrentSource"
        assert stim[0]["waveform"]["type"] == "Pulse"

    def test_add_multi_analysis_and_netlist(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("multi", ["in", "out"])
        dut.R(name="1", positive="in", negative="out", value=1000.0)
        tb = ps.Testbench(dut)
        tb.V(name="in", positive="in", negative="0", value=1.0, ac=1.0)
        tb.add_operating_point()
        tb.add_ac(variation="dec", number_of_points=10, start_frequency=1.0, stop_frequency=1e6)
        tb.measure("tran", "vmax", "MAX", "V(out)")

        data = json.loads(tb.to_json())
        assert [a["type"] for a in data["testbench"]["analyses"]] == ["Op", "Ac"]
        netlist = tb.netlist("ngspice")
        assert ".op" in netlist
        assert ".ac dec 10 1 1meg" in netlist
        assert ".meas tran vmax MAX V(out)" in netlist

    def test_testbench_statistical_analysis_builders(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("amp_mc", ["vin", "vout"])
        tb = ps.Testbench(dut)
        tb.add_xyce_sampling(25, {"Rload": "normal(1000,50)"})

        data = json.loads(tb.to_json())
        assert data["testbench"]["analyses"][0]["type"] == "XyceSampling"
        xyce = tb.netlist("xyce")
        assert ".SAMPLING" in xyce
        assert "+ Rload=normal(1000,50)" in xyce

        tb2 = ps.Testbench(dut)
        tb2.add_xyce_pce(10, {"Cload": "uniform(0.9p,1.1p)"}, order=3)
        assert ".PCE" in tb2.netlist("xyce")

        tb3 = ps.Testbench(dut)
        tb3.add_spectre_monte_carlo(20, "tran1", "tran", seed=42)
        spectre = tb3.netlist("spectre")
        assert "mc1 montecarlo numruns=20" in spectre
        assert "seed=42" in spectre


class TestTestbenchCheckBackend:
    def test_check_backend_clean(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("simple")
        dut.R(
            name="1",
            positive="in",
            negative="out",
            value=1000.0,
        )
        dut.V(
            name="1",
            positive="in",
            negative=dut.gnd,
            value=1.0,
        )
        tb = ps.Testbench(dut)
        issues = tb.check_backend("ngspice")
        assert len(issues) == 0

    def test_check_backend_xspice_on_xyce(self):
        ps = import_pyspice()
        dut = ps.Subcircuit("xspice_test")
        dut.A(
            name="adc1",
            connections=["[vin]", "[dout]"],
            model="adc_buf",
        )
        tb = ps.Testbench(dut)
        issues = tb.check_backend("xyce")
        assert any("XSPICE" in issue for issue in issues)


# ── ModelLibrary Tests ──


class TestModelLibrary:
    def test_create(self):
        ps = import_pyspice()
        lib = ps.ModelLibrary("/path/to/sky130.lib", corner="tt")
        assert lib.name == "sky130"
        assert lib.path == "/path/to/sky130.lib"
        assert lib.corner == "tt"

    def test_create_no_corner(self):
        ps = import_pyspice()
        lib = ps.ModelLibrary("/models/custom.lib")
        assert lib.name == "custom"
        assert lib.corner is None

    def test_repr(self):
        ps = import_pyspice()
        lib = ps.ModelLibrary("/path/to/sky130.lib", corner="tt")
        r = repr(lib)
        assert "ModelLibrary" in r
        assert "sky130" in r


# ── Backwards Compatibility ──


class TestBackwardsCompat:
    """Ensure existing Circuit API still works unchanged."""

    def test_circuit_basic(self):
        ps = import_pyspice()
        c = ps.Circuit("test")
        c.V(
            name="1",
            positive="in",
            negative=c.gnd,
            value=5.0,
        )
        c.R(
            name="1",
            positive="in",
            negative="out",
            value=1000.0,
        )
        c.R(
            name="2",
            positive="out",
            negative=c.gnd,
            value=1000.0,
        )
        netlist = str(c)
        assert "V1" in netlist
        assert "R1" in netlist
        assert "R2" in netlist

    def test_circuit_model(self):
        ps = import_pyspice()
        c = ps.Circuit("mos")
        c.M(
            name="1",
            drain="d",
            gate="g",
            source="s",
            bulk="b",
            model="nmos",
        )
        c.model("nmos", "NMOS", VTO="0.7")
        netlist = str(c)
        assert "M1" in netlist
        assert ".model" in netlist

    def test_circuit_simulator(self):
        ps = import_pyspice()
        c = ps.Circuit("sim_test")
        c.V(
            name="1",
            positive="in",
            negative=c.gnd,
            value=1.0,
        )
        c.R(
            name="1",
            positive="in",
            negative=c.gnd,
            value=1000.0,
        )
        sim = c.simulator()
        assert repr(sim) == "CircuitSimulator"

    def test_circuit_getitem(self):
        ps = import_pyspice()
        c = ps.Circuit("lookup")
        c.R(
            name="1",
            positive="a",
            negative="b",
            value=1000.0,
        )
        elem = c["1"]
        assert "R1" in elem

    def test_circuit_subcircuit_instance(self):
        ps = import_pyspice()
        c = ps.Circuit("x_test")
        c.X("1", "MyBuf", "in", "out", "vdd")
        netlist = str(c)
        assert "X1" in netlist
        assert "MyBuf" in netlist


class TestSimulatorCheckBackend:
    """Test check_backend method on existing CircuitSimulator."""

    def test_check_backend_on_simulator(self):
        ps = import_pyspice()
        c = ps.Circuit("check_test")
        c.R(
            name="1",
            positive="in",
            negative="out",
            value=1000.0,
        )
        c.V(
            name="1",
            positive="in",
            negative=c.gnd,
            value=1.0,
        )
        sim = c.simulator()
        issues = sim.check_backend("ngspice")
        assert isinstance(issues, list)
