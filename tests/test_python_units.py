"""
Python tests for the unit system via PyO3 bindings.

Run with: maturin develop && pytest tests/
"""
import pytest


def import_units():
    try:
        from spicerack import unit
        return unit
    except ImportError:
        pytest.skip("spicerack not built — run 'maturin develop' first")


class TestUnitMatmul:
    """Test the value @ unit syntax."""

    def test_voltage(self):
        u = import_units()
        v = 3.3 @ u.u_V
        assert float(v) == pytest.approx(3.3)

    def test_kohm(self):
        u = import_units()
        r = 1 @ u.u_kOhm
        assert float(r) == pytest.approx(1000.0)

    def test_picofarad(self):
        u = import_units()
        c = 10 @ u.u_pF
        assert float(c) == pytest.approx(10e-12)

    def test_microhenry(self):
        u = import_units()
        l = 1 @ u.u_uH
        assert float(l) == pytest.approx(1e-6)

    def test_microsecond(self):
        u = import_units()
        t = 1 @ u.u_us
        assert float(t) == pytest.approx(1e-6)

    def test_nanosecond(self):
        u = import_units()
        t = 100 @ u.u_ns
        assert float(t) == pytest.approx(100e-9)

    def test_kilohertz(self):
        u = import_units()
        f = 10 @ u.u_kHz
        assert float(f) == pytest.approx(10e3)

    def test_gigahertz(self):
        u = import_units()
        f = 1 @ u.u_GHz
        assert float(f) == pytest.approx(1e9)

    def test_microamp(self):
        u = import_units()
        i = 10 @ u.u_uA
        assert float(i) == pytest.approx(10e-6)

    def test_millivolt(self):
        u = import_units()
        v = 100 @ u.u_mV
        assert float(v) == pytest.approx(0.1)

    def test_degree(self):
        u = import_units()
        t = 27 @ u.u_Degree
        assert float(t) == pytest.approx(27.0)


class TestUnitStrSpice:
    def test_kohm_str(self):
        u = import_units()
        r = 1 @ u.u_kOhm
        assert r.str_spice() == "1k"

    def test_picofarad_str(self):
        u = import_units()
        c = 10 @ u.u_pF
        assert c.str_spice() == "10p"

    def test_nanosecond_str(self):
        u = import_units()
        t = 100 @ u.u_ns
        assert t.str_spice() == "100n"

    def test_voltage_str(self):
        u = import_units()
        v = 3.3 @ u.u_V
        assert v.str_spice() == "3.3"


class TestUnitRepr:
    def test_repr(self):
        u = import_units()
        v = 3.3 @ u.u_V
        s = repr(v)
        assert "3.3" in s
        assert "V" in s

    def test_str(self):
        u = import_units()
        v = 1 @ u.u_kOhm
        s = str(v)
        assert "1k" in s
        assert "Ohm" in s


class TestAllUnitConstants:
    """Verify all unit constants exist and are usable."""

    def test_volts(self):
        u = import_units()
        assert float(1 @ u.u_V) == pytest.approx(1.0)
        assert float(1 @ u.u_mV) == pytest.approx(1e-3)
        assert float(1 @ u.u_uV) == pytest.approx(1e-6)

    def test_amps(self):
        u = import_units()
        assert float(1 @ u.u_A) == pytest.approx(1.0)
        assert float(1 @ u.u_mA) == pytest.approx(1e-3)
        assert float(1 @ u.u_uA) == pytest.approx(1e-6)
        assert float(1 @ u.u_nA) == pytest.approx(1e-9)

    def test_ohms(self):
        u = import_units()
        assert float(1 @ u.u_Ohm) == pytest.approx(1.0)
        assert float(1 @ u.u_kOhm) == pytest.approx(1e3)
        assert float(1 @ u.u_MOhm) == pytest.approx(1e6)

    def test_farads(self):
        u = import_units()
        assert float(1 @ u.u_F) == pytest.approx(1.0)
        assert float(1 @ u.u_mF) == pytest.approx(1e-3)
        assert float(1 @ u.u_uF) == pytest.approx(1e-6)
        assert float(1 @ u.u_nF) == pytest.approx(1e-9)
        assert float(1 @ u.u_pF) == pytest.approx(1e-12)
        assert float(1 @ u.u_fF) == pytest.approx(1e-15)

    def test_henrys(self):
        u = import_units()
        assert float(1 @ u.u_H) == pytest.approx(1.0)
        assert float(1 @ u.u_mH) == pytest.approx(1e-3)
        assert float(1 @ u.u_uH) == pytest.approx(1e-6)
        assert float(1 @ u.u_nH) == pytest.approx(1e-9)

    def test_hertz(self):
        u = import_units()
        assert float(1 @ u.u_Hz) == pytest.approx(1.0)
        assert float(1 @ u.u_kHz) == pytest.approx(1e3)
        assert float(1 @ u.u_MHz) == pytest.approx(1e6)
        assert float(1 @ u.u_GHz) == pytest.approx(1e9)

    def test_seconds(self):
        u = import_units()
        assert float(1 @ u.u_s) == pytest.approx(1.0)
        assert float(1 @ u.u_ms) == pytest.approx(1e-3)
        assert float(1 @ u.u_us) == pytest.approx(1e-6)
        assert float(1 @ u.u_ns) == pytest.approx(1e-9)
        assert float(1 @ u.u_ps) == pytest.approx(1e-12)

    def test_watts(self):
        u = import_units()
        assert float(1 @ u.u_W) == pytest.approx(1.0)
        assert float(1 @ u.u_mW) == pytest.approx(1e-3)
        assert float(1 @ u.u_uW) == pytest.approx(1e-6)

    def test_degree(self):
        u = import_units()
        assert float(27 @ u.u_Degree) == pytest.approx(27.0)
