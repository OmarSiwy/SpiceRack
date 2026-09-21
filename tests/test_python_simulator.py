"""
Comprehensive simulator configuration and analysis tests.

Tests simulator creation, option setting, netlist building,
temperature, initial conditions, saves, measures, step sweeps.

These tests do NOT require a simulator backend -- they only test
the Python binding layer and netlist generation.

Run: maturin develop && pytest tests/test_python_simulator.py -v
"""
import pytest


def ps():
    try:
        import spicerack
        return spicerack
    except ImportError:
        pytest.skip("spicerack not built")


# ======================================================================
# Simulator creation
# ======================================================================

class TestSimulatorCreation:
    def test_default_simulator(self):
        c = ps().Circuit("t")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=3.3,
        )
        sim = c.simulator()
        assert repr(sim) == "CircuitSimulator"

    def test_simulator_with_backend(self):
        c = ps().Circuit("t")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=3.3,
        )
        sim = c.simulator(simulator="ngspice-subprocess")
        assert repr(sim) == "CircuitSimulator"

    def test_simulator_with_various_backends(self):
        """Test that we can create simulators with various backend names."""
        c = ps().Circuit("t")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=3.3,
        )
        for backend in ["ngspice-subprocess", "ngspice-shared", "ltspice", "vacask", "spectre"]:
            sim = c.simulator(simulator=backend)
            assert repr(sim) == "CircuitSimulator"


# ======================================================================
# Simulator configuration
# ======================================================================

class TestSimulatorConfig:
    def _make_sim(self):
        c = ps().Circuit("cfg")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=3.3,
        )
        c.R(
            name="1",
            positive="vdd",
            negative=c.gnd,
            value=1e3,
        )
        return c.simulator()

    def test_set_temperature(self):
        sim = self._make_sim()
        sim.temperature = 85.0
        # Should not raise

    def test_set_nominal_temperature(self):
        sim = self._make_sim()
        sim.nominal_temperature = 27.0

    def test_save_currents(self):
        sim = self._make_sim()
        sim.save_currents = True

    def test_save_nodes(self):
        sim = self._make_sim()
        sim.save("V(out)", "I(Vdd)")

    def test_options(self):
        sim = self._make_sim()
        sim.options(reltol="0.001", abstol="1e-12")

    def test_initial_condition(self):
        sim = self._make_sim()
        sim.initial_condition(out=0.0, vdd=3.3)

    def test_node_set(self):
        sim = self._make_sim()
        sim.node_set(out=1.65, bias=0.7)

    def test_measure(self):
        sim = self._make_sim()
        sim.measure("tran", "rise_time", "trig", "V(out)", "val=0.1", "rise=1",
                     "targ", "V(out)", "val=0.9", "rise=1")

    def test_step_linear(self):
        sim = self._make_sim()
        sim.step("R1", 500.0, 2000.0, 100.0)

    def test_step_sweep(self):
        sim = self._make_sim()
        sim.step_sweep("C1", 1e-12, 100e-12, 10e-12, "lin")


# ======================================================================
# Analysis method signatures (these will fail without a backend,
# but we test that the methods exist and accept correct args)
# ======================================================================

class TestAnalysisMethods:
    """Verify all analysis methods exist on the simulator and accept args.

    These will raise RuntimeError (no backend) -- we just check
    the method exists and the binding doesn't crash on arg parsing.
    """

    def _sim(self):
        c = ps().Circuit("t")
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
            value=1e3,
        )
        c.R(
            name="2",
            positive="out",
            negative=c.gnd,
            value=1e3,
        )
        return c.simulator()

    def test_operating_point_exists(self):
        sim = self._sim()
        assert hasattr(sim, "operating_point")
        # Will succeed if ngspice is available, RuntimeError otherwise
        try:
            sim.operating_point()
        except RuntimeError:
            pass

    def test_dc_exists(self):
        sim = self._sim()
        assert hasattr(sim, "dc")

    def test_ac_exists(self):
        sim = self._sim()
        assert hasattr(sim, "ac")
        try:
            sim.ac(variation="dec", number_of_points=10,
                   start_frequency=1.0, stop_frequency=1e9)
        except RuntimeError:
            pass

    def test_transient_exists(self):
        sim = self._sim()
        assert hasattr(sim, "transient")
        try:
            sim.transient(step_time=1e-6, end_time=1e-3)
        except RuntimeError:
            pass

    def test_noise_exists(self):
        sim = self._sim()
        assert hasattr(sim, "noise")

    def test_transfer_function_exists(self):
        sim = self._sim()
        assert hasattr(sim, "transfer_function")
        assert hasattr(sim, "tf")

    def test_dc_sensitivity_exists(self):
        sim = self._sim()
        assert hasattr(sim, "dc_sensitivity")

    def test_ac_sensitivity_exists(self):
        sim = self._sim()
        assert hasattr(sim, "ac_sensitivity")

    def test_polezero_exists(self):
        sim = self._sim()
        assert hasattr(sim, "polezero")

    def test_distortion_exists(self):
        sim = self._sim()
        assert hasattr(sim, "distortion")

    def test_pss_exists(self):
        sim = self._sim()
        assert hasattr(sim, "pss")

    def test_s_param_exists(self):
        sim = self._sim()
        assert hasattr(sim, "s_param")

    def test_harmonic_balance_exists(self):
        sim = self._sim()
        assert hasattr(sim, "harmonic_balance")

    def test_stability_exists(self):
        sim = self._sim()
        assert hasattr(sim, "stability")

    def test_transient_noise_exists(self):
        sim = self._sim()
        assert hasattr(sim, "transient_noise")

    def test_network_params_exists(self):
        sim = self._sim()
        assert hasattr(sim, "network_params")

    # Spectre-specific
    def test_spectre_sweep_exists(self):
        sim = self._sim()
        assert hasattr(sim, "spectre_sweep")

    def test_spectre_montecarlo_exists(self):
        sim = self._sim()
        assert hasattr(sim, "spectre_montecarlo")

    def test_spectre_pac_exists(self):
        sim = self._sim()
        assert hasattr(sim, "spectre_pac")

    def test_spectre_pnoise_exists(self):
        sim = self._sim()
        assert hasattr(sim, "spectre_pnoise")

    def test_spectre_pxf_exists(self):
        sim = self._sim()
        assert hasattr(sim, "spectre_pxf")

    def test_spectre_pstb_exists(self):
        sim = self._sim()
        assert hasattr(sim, "spectre_pstb")

    def test_available_backends_static(self):
        sim = self._sim()
        assert hasattr(sim, "available_backends")
        backends = type(sim).available_backends()
        assert isinstance(backends, list)


# ======================================================================
# Live simulation (ngspice only, skipped if not available)
# ======================================================================

class TestLiveSimulation:
    """Tests that actually run simulations. Require ngspice."""

    @pytest.fixture(autouse=True)
    def check_ngspice(self):
        import shutil
        if not shutil.which("ngspice"):
            pytest.skip("ngspice not installed")

    def _divider_sim(self):
        c = ps().Circuit("live_test")
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
        return c.simulator(simulator="ngspice-subprocess")

    def test_operating_point(self):
        sim = self._divider_sim()
        op = sim.operating_point()
        vout = op["out"]
        assert abs(vout - 1.65) < 0.01

    def test_dc_sweep(self):
        c = ps().Circuit("dc_live")
        c.V(
            name="in",
            positive="vin",
            negative=c.gnd,
            value=0.0,
        )
        c.R(
            name="1",
            positive="vin",
            negative="out",
            value=1e3,
        )
        c.R(
            name="2",
            positive="out",
            negative=c.gnd,
            value=1e3,
        )
        sim = c.simulator(simulator="ngspice-subprocess")
        dc = sim.dc(Vin=slice(0, 10, 1))
        sweep = dc.sweep
        assert len(sweep) > 5
        # At Vin=10V, Vout should be 5V
        vout = dc["out"]
        assert abs(vout[-1] - 5.0) < 0.1

    def test_transient(self):
        c = ps().Circuit("tran_live")
        c.PulseVoltageSource(
            name="in",
            positive="in",
            negative=c.gnd,
            initial_value=0.0,
            pulsed_value=1.0,
            pulse_width=0.5e-3,
            period=1e-3,
        )
        c.R(
            name="1",
            positive="in",
            negative="out",
            value=1e3,
        )
        c.C(
            name="1",
            positive="out",
            negative=c.gnd,
            value=1e-6,
        )
        sim = c.simulator(simulator="ngspice-subprocess")
        tran = sim.transient(step_time=1e-5, end_time=2e-3)
        time = tran.time
        assert len(time) > 10
        assert time[-1] >= 1.9e-3

    def test_ac_analysis(self):
        c = ps().Circuit("ac_live")
        c.V(
            name="in",
            positive="in",
            negative=c.gnd,
            value=1.0,
        )
        c.R(
            name="1",
            positive="in",
            negative="out",
            value=1e3,
        )
        c.C(
            name="1",
            positive="out",
            negative=c.gnd,
            value=1e-6,
        )
        sim = c.simulator(simulator="ngspice-subprocess")
        ac = sim.ac(variation="dec", number_of_points=10,
                    start_frequency=1.0, stop_frequency=1e6)
        freq = ac.frequency
        assert len(freq) > 10
        assert freq[0] >= 1.0


def test_ac_magnitude_and_phase_match_rc_theory():
    """AC must expose |H| and phase, not just the real part (they differ)."""
    import math
    import spicerack as ps
    from spicerack.unit import u_kOhm, u_uF

    circuit = ps.Circuit("rc_bode")
    circuit.V(name="in", positive="vin", negative=circuit.gnd, value=0.0, ac=1.0)
    circuit.R(name="r1", positive="vin", negative="vout", value=1 @ u_kOhm)
    circuit.C(name="c1", positive="vout", negative=circuit.gnd, value=1 @ u_uF)

    sim = circuit.simulator(simulator="ngspice")
    ac = sim.ac(variation="dec", number_of_points=10, start_frequency=1, stop_frequency=1e5)

    fc = 1.0 / (2 * math.pi * 1e3 * 1e-6)
    for f, mag, phase, real in zip(ac.frequency, ac.magnitude("vout"),
                                   ac.phase("vout"), ac["vout"]):
        expected_mag = 1.0 / math.sqrt(1 + (f / fc) ** 2)
        expected_phase = -math.degrees(math.atan(f / fc))
        assert abs(mag - expected_mag) < 1e-6 * max(1.0, expected_mag)
        assert abs(phase - expected_phase) < 1e-3
        # Real part is not the magnitude anywhere past DC.
        if f > fc:
            assert real < mag


def test_noise_runs_when_saves_are_set():
    """A node-name .save cannot name onoise_spectrum; emitting it fails the run."""
    import math

    import spicerack as ps

    dut = ps.Subcircuit("nz", ["vin", "vout"])
    dut.R(name="r1", positive="vin", negative="vout", value=1000.0)
    dut.C(name="c1", positive="vout", negative="0", value=1e-6)

    tb = ps.Testbench(dut)
    tb.V(name="in", positive="vin", negative="0", value=1.0, ac=1.0)
    tb.save("V(vout)")
    noise = tb.noise(output_node="vout", ref_node="0", src="Vin",
                     variation="dec", points=10, start_frequency=1.0,
                     stop_frequency=1e5)

    spectrum = noise["onoise_spectrum"]
    assert len(spectrum) == len(noise.frequency)
    # Flat band is the 1k resistor's thermal noise, sqrt(4kTR).
    expected = math.sqrt(4 * 1.380649e-23 * 300.15 * 1000.0)
    assert spectrum[0] == pytest.approx(expected, rel=0.02)
