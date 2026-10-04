"""run_corners / pdk_corners on a resistor divider with closed-form corners.

Run with: maturin develop && pytest tests/test_run_corners.py
"""
import shutil

import pytest

ps = pytest.importorskip("spicerack")
from spicerack.testbenches import CornerCase, pdk_corners, run_corners  # noqa: E402

pytestmark = pytest.mark.skipif(not shutil.which("ngspice"), reason="ngspice not installed")

# Two "process" sections: the bottom resistor is 1k or 3k.
LIB = """\
.lib lo
.param rbot=1k
.endl lo
.lib hi
.param rbot=3k
.endl hi
"""


@pytest.fixture
def lib(tmp_path):
    path = tmp_path / "proc.lib"
    path.write_text(LIB)
    return str(path)


def divider(corner):
    dut = ps.Subcircuit("div", ["vin", "vout"])
    dut.R(name="top", positive="vin", negative="vout", value=1e3)
    # tc1 = 1 %/C about tnom = 27 C makes temperature visible too.
    dut.raw_spice("Rbot vout 0 {rbot} tc1=0.01")
    tb = ps.Testbench(dut)
    tb.V(name="dd", positive="vin", negative="0", value=corner.vdd or 1.0)
    return tb


def vout(tb):
    return {"vout_v": tb.operating_point()["vout"]}


def expected(vdd, rbot, temp):
    rb = rbot * (1 + 0.01 * (temp - 27))
    return vdd * rb / (1e3 + rb)


def test_pdk_corners_crosses_section_temp_vdd(lib):
    corners = pdk_corners(lib, ["lo", "hi"], temperatures=[27, 127], vdds=[1.0, 2.0])
    assert [c.name for c in corners][:3] == ["lo_27C_1V", "lo_27C_2V", "lo_127C_1V"]
    assert len(corners) == 8


def test_every_corner_gets_its_section_temperature_and_vdd(lib):
    corners = pdk_corners(lib, ["lo", "hi"], temperatures=[27, 127], vdds=[1.0, 2.0])
    rows = run_corners(divider, vout, corners)
    assert len(rows) == 8
    for row in rows:
        rbot = 1e3 if row["corner"].startswith("lo") else 3e3
        want = expected(row["vdd"], rbot, row["temperature"])
        assert row["vout_v"] == pytest.approx(want, rel=1e-6), row
        assert row["seed"] is None and "error" not in row


def test_seeds_give_reproducible_distinct_samples():
    def mc(corner):
        dut = ps.Subcircuit("mc", ["a"])
        dut.raw_spice("R1 a 0 {agauss(1k, 100, 1)}")
        tb = ps.Testbench(dut)
        tb.I(name="s", positive="0", negative="a", value=1e-3)
        return tb

    measure = lambda tb: {"r_ohm": tb.operating_point()["a"] * 1e3}
    rows = run_corners(mc, measure, seeds=4)
    assert [r["seed"] for r in rows] == [1, 2, 3, 4]
    assert all(r["corner"] == "nominal" for r in rows)
    assert len({round(r["r_ohm"], 6) for r in rows}) == 4
    again = run_corners(mc, measure, seeds=[3])
    assert again[0]["r_ohm"] == rows[2]["r_ohm"]


def test_a_failing_corner_is_recorded_not_fatal(lib):
    corners = [CornerCase("ok", vdd=1.0, model_libraries=(ps.ModelLibrary(lib, corner="lo"),)),
               CornerCase("broken", vdd=1.0)]  # no section: rbot is undefined
    rows = run_corners(divider, vout, corners)
    assert rows[0]["vout_v"] == pytest.approx(0.5, rel=1e-6)
    assert "error" in rows[1] and "vout_v" not in rows[1]
