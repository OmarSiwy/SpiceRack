"""
Comprehensive linter tests via Python bindings.

Tests all lint checks: missing .end, missing ground, duplicate elements,
floating nodes, zero-value components, missing models, undefined params,
backend-specific checks.

Run: maturin develop && pytest tests/test_python_lint.py -v
"""
import pytest


def lint():
    try:
        from spicerack import lint
        return lint
    except ImportError:
        pytest.skip("spicerack not built")


# ══════════════════════════════════════════════════════════════════════════════
# Basic lint checks
# ══════════════════════════════════════════════════════════════════════════════

class TestLintBasic:
    def test_clean_netlist(self):
        result = lint()(
            ".title clean\nV1 in 0 DC 1\nR1 in out 1k\nR2 out 0 2k\n.op\n.end\n"
        )
        assert len(result["errors"]) == 0
        assert len(result["warnings"]) == 0

    def test_returns_dict_with_keys(self):
        result = lint()(".title t\nR1 a 0 1k\n.end\n")
        assert "warnings" in result
        assert "errors" in result
        assert isinstance(result["warnings"], list)
        assert isinstance(result["errors"], list)

    def test_warning_has_fields(self):
        # Floating node triggers a warning
        result = lint()(".title t\nV1 in 0 1\nR1 in out 1k\n.end\n")
        if result["warnings"]:
            w = result["warnings"][0]
            assert "line" in w
            assert "message" in w


# ══════════════════════════════════════════════════════════════════════════════
# Missing .end
# ══════════════════════════════════════════════════════════════════════════════

class TestMissingEnd:
    def test_missing_end_error(self):
        result = lint()(".title t\nR1 a 0 1k\n")
        errors = result["errors"]
        assert any(".end" in e["message"] for e in errors)

    def test_has_end_no_error(self):
        result = lint()(".title t\nR1 a 0 1k\n.end\n")
        errors = result["errors"]
        assert not any(".end" in e["message"] for e in errors)


# ══════════════════════════════════════════════════════════════════════════════
# Missing ground
# ══════════════════════════════════════════════════════════════════════════════

class TestMissingGround:
    def test_no_ground_error(self):
        result = lint()(".title t\nR1 a b 1k\n.end\n")
        errors = result["errors"]
        assert any("ground" in e["message"].lower() for e in errors)

    def test_zero_node_ok(self):
        result = lint()(".title t\nR1 a 0 1k\n.end\n")
        errors = result["errors"]
        assert not any("ground" in e["message"].lower() for e in errors)

    def test_gnd_node_ok(self):
        result = lint()(".title t\nR1 a gnd 1k\n.end\n")
        errors = result["errors"]
        assert not any("ground" in e["message"].lower() for e in errors)


# ══════════════════════════════════════════════════════════════════════════════
# Duplicate elements
# ══════════════════════════════════════════════════════════════════════════════

class TestDuplicateElements:
    def test_duplicate_detected(self):
        result = lint()(".title t\nR1 a 0 1k\nR1 b 0 2k\n.end\n")
        errors = result["errors"]
        assert any("Duplicate" in e["message"] or "duplicate" in e["message"] for e in errors)

    def test_no_duplicate(self):
        result = lint()(".title t\nR1 a 0 1k\nR2 b 0 2k\n.end\n")
        errors = result["errors"]
        assert not any("Duplicate" in e["message"] or "duplicate" in e["message"] for e in errors)

    def test_case_insensitive_duplicate(self):
        result = lint()(".title t\nR1 a 0 1k\nr1 b 0 2k\n.end\n")
        errors = result["errors"]
        assert any("Duplicate" in e["message"] or "duplicate" in e["message"] for e in errors)


# ══════════════════════════════════════════════════════════════════════════════
# Floating nodes
# ══════════════════════════════════════════════════════════════════════════════

class TestFloatingNodes:
    def test_floating_node_warning(self):
        result = lint()(".title t\nV1 in 0 1\nR1 in out 1k\n.end\n")
        warnings = result["warnings"]
        assert any("floating" in w["message"].lower() for w in warnings)

    def test_no_floating_node(self):
        result = lint()(".title t\nV1 in 0 1\nR1 in out 1k\nR2 out 0 1k\n.end\n")
        warnings = result["warnings"]
        assert not any("floating" in w["message"].lower() for w in warnings)


# ══════════════════════════════════════════════════════════════════════════════
# Zero-value components
# ══════════════════════════════════════════════════════════════════════════════

class TestZeroValues:
    def test_zero_resistance(self):
        result = lint()(".title t\nR1 a 0 0\n.end\n")
        warnings = result["warnings"]
        assert any("zero" in w["message"].lower() and "resist" in w["message"].lower()
                    for w in warnings)

    def test_zero_capacitance(self):
        result = lint()(".title t\nC1 a 0 0\n.end\n")
        warnings = result["warnings"]
        assert any("zero" in w["message"].lower() and "capac" in w["message"].lower()
                    for w in warnings)

    def test_nonzero_ok(self):
        result = lint()(".title t\nR1 a 0 1k\n.end\n")
        warnings = result["warnings"]
        assert not any("zero" in w["message"].lower() for w in warnings)


# ══════════════════════════════════════════════════════════════════════════════
# Model references
# ══════════════════════════════════════════════════════════════════════════════

class TestModelReferences:
    def test_missing_model_warning(self):
        result = lint()(".title t\nM1 d g s b nmos W=1u L=100n\n.end\n")
        warnings = result["warnings"]
        assert any("model" in w["message"].lower() for w in warnings)

    def test_defined_model_ok(self):
        result = lint()(".title t\n.model nmos NMOS (vth0=0.5)\nM1 d g s b nmos\n.end\n")
        warnings = result["warnings"]
        assert not any("model" in w["message"].lower() and "nmos" in w["message"].lower()
                        for w in warnings)

    def test_include_suppresses_model_check(self):
        result = lint()(".title t\n.include /pdk/models.lib\nM1 d g s b nmos\n.end\n")
        warnings = result["warnings"]
        assert not any("model" in w["message"].lower() for w in warnings)

    def test_lib_suppresses_model_check(self):
        result = lint()(".title t\n.lib /pdk/models.lib tt\nD1 a b mydiode\n.end\n")
        warnings = result["warnings"]
        assert not any("model" in w["message"].lower() for w in warnings)


# ══════════════════════════════════════════════════════════════════════════════
# Undefined parameters
# ══════════════════════════════════════════════════════════════════════════════

class TestUndefinedParams:
    def test_undefined_param_warning(self):
        result = lint()(".title t\nR1 a 0 {rval}\n.end\n")
        warnings = result["warnings"]
        assert any("rval" in w["message"] for w in warnings)

    def test_defined_param_ok(self):
        result = lint()(".title t\n.param rval=1k\nR1 a 0 {rval}\n.end\n")
        warnings = result["warnings"]
        assert not any("rval" in w["message"] for w in warnings)

    def test_expression_param_not_flagged(self):
        """Expressions like {a+b} should not trigger simple param check."""
        result = lint()(".title t\nR1 a 0 {a+b}\n.end\n")
        warnings = result["warnings"]
        # The expression contains operators, so shouldn't be flagged
        assert not any("a+b" in w["message"] for w in warnings)


# ══════════════════════════════════════════════════════════════════════════════
# Backend-specific checks
# ══════════════════════════════════════════════════════════════════════════════

class TestBackendSpecific:
    def test_ngspice_meas_warning(self):
        result = lint()(".title t\nR1 a 0 1k\n.meas tran rise trig v(a)\n.end\n",
                        backend="ngspice")
        warnings = result["warnings"]
        assert any("batch" in w["message"].lower() for w in warnings)

    def test_ltspice_sens_warning(self):
        result = lint()(".title t\nR1 a 0 1k\n.sens v(a)\n.end\n",
                        backend="ltspice")
        warnings = result["warnings"]
        assert any(".sens" in w["message"] for w in warnings)

    def test_ltspice_control_error(self):
        result = lint()(".title t\nR1 a 0 1k\n.control\n.endc\n.end\n",
                        backend="ltspice")
        errors = result["errors"]
        assert any(".control" in e["message"] for e in errors)

    def test_spectre_disto_warning(self):
        result = lint()(".title t\nR1 a 0 1k\n.disto dec 10 100 1e8\n.end\n",
                        backend="spectre")
        warnings = result["warnings"]
        assert any(".disto" in w["message"] for w in warnings)

    def test_spectre_control_error(self):
        result = lint()(".title t\nR1 a 0 1k\n.control\n.endc\n.end\n",
                        backend="spectre")
        errors = result["errors"]
        assert any(".control" in e["message"] for e in errors)

    def test_no_backend_no_specific_warnings(self):
        """Without a backend, no backend-specific warnings should appear."""
        result = lint()(".title t\nR1 a 0 1k\n.meas tran x trig v(a)\n.end\n")
        warnings = result["warnings"]
        assert not any("batch" in w["message"].lower() for w in warnings)


# ══════════════════════════════════════════════════════════════════════════════
# Lint on generated netlists
# ══════════════════════════════════════════════════════════════════════════════

class TestLintOnGenerated:
    """Run linter on netlists generated by the circuit builder."""

    def test_generated_voltage_divider_clean(self):
        import spicerack
        c = spicerack.Circuit("lint_vdiv")
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
        result = lint()(netlist)
        assert len(result["errors"]) == 0
        assert len(result["warnings"]) == 0

    def test_generated_cmos_inv_warns_about_models(self):
        import spicerack
        c = spicerack.Circuit("lint_inv")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=1.8,
        )
        c.MOSFET(
            name="1",
            drain="out",
            gate="in",
            source=c.gnd,
            bulk=c.gnd,
            model="nmos",
        )
        c.MOSFET(
            name="2",
            drain="out",
            gate="in",
            source="vdd",
            bulk="vdd",
            model="pmos",
        )
        netlist = str(c)
        result = lint()(netlist)
        # Should warn about missing nmos/pmos models
        warnings = result["warnings"]
        assert any("model" in w["message"].lower() for w in warnings)

    def test_generated_with_models_clean(self):
        import spicerack
        c = spicerack.Circuit("lint_clean_inv")
        c.V(
            name="dd",
            positive="vdd",
            negative=c.gnd,
            value=1.8,
        )
        c.V(
            name="in",
            positive="vin",
            negative=c.gnd,
            value=0.9,
        )
        c.model("nmos", "NMOS", LEVEL=1, VTO=0.4)
        c.model("pmos", "PMOS", LEVEL=1, VTO=-0.4)
        c.MOSFET(
            name="1",
            drain="out",
            gate="vin",
            source=c.gnd,
            bulk=c.gnd,
            model="nmos",
        )
        c.MOSFET(
            name="2",
            drain="out",
            gate="vin",
            source="vdd",
            bulk="vdd",
            model="pmos",
        )
        c.R(
            name="load",
            positive="out",
            negative=c.gnd,
            value=1e6,
        )
        netlist = str(c)
        result = lint()(netlist)
        assert len(result["errors"]) == 0
