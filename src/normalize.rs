/// Variable name normalization for cross-backend consistency.
///
/// Different SPICE backends use different naming conventions for node
/// voltages, branch currents, and hierarchy separators. This module
/// normalizes all names to a canonical lowercase form:
///
/// - Node voltages: strip `v()`/`V()` wrappers, lowercase
/// - Branch currents: normalize to `i(name)` canonical format
/// - Hierarchy separators: Xyce `%` and LTspice `:` become `.`
/// - Sweep variables (`time`, `frequency`) pass through as-is

/// Normalize a variable name from any backend to canonical form.
///
/// Rules:
/// - Node voltages: strip v()/V() wrappers, lowercase the inner name
///   `"v(out)"` -> `"out"`, `"V(OUT)"` -> `"out"`
/// - Branch currents: normalize to `"i(name)"` canonical format
///   `"i(v1)"` -> `"i(v1)"`, `"I(V1)"` -> `"i(v1)"`, `"V1:p"` -> `"i(v1)"`
/// - Hierarchy separators: Xyce `%` and LTspice `:` become `.`
///   `"v(x1%internal)"` -> `"x1.internal"`, `"v(x1:internal)"` -> `"x1.internal"`
/// - Sweep variables: `"time"`, `"frequency"` stay as-is (already lowercase)
/// - Noise spectra: `"inoise_spectrum"`, `"onoise_spectrum"` preserved
/// - Spectre bare node names: `"out"` -> `"out"`, `"OUT"` -> `"out"`
pub fn normalize_var_name(name: &str, backend: &str) -> String {
    let lower = name.to_lowercase();

    // Sweep variables pass through
    if lower == "time" || lower == "frequency" || lower == "sweep" {
        return lower;
    }

    // Noise spectra pass through (lowercased)
    if lower == "inoise_spectrum" || lower == "onoise_spectrum" {
        return lower;
    }

    // Check if this is a current variable
    if is_current_name(name, backend) {
        return normalize_current(name, backend);
    }

    // Voltage wrapped in v()/V()
    if (lower.starts_with("v(") || lower.starts_with("v ("))
        && lower.ends_with(')')
    {
        let start = lower.find('(').unwrap() + 1;
        let inner = &lower[start..lower.len() - 1];
        // Strip reference node if present: v(out,0) -> out
        let node = inner.split(',').next().unwrap_or(inner).trim();
        return normalize_hierarchy(node, backend);
    }

    // Spectre/Vacask bare node name (no wrapper)
    if backend == "spectre" || backend == "vacask" || backend == "vacask-shared" {
        return normalize_hierarchy(&lower, backend);
    }

    // Fallback: just lowercase
    normalize_hierarchy(&lower, backend)
}

/// Detect whether a variable name represents a current measurement.
pub fn is_current_name(name: &str, backend: &str) -> bool {
    let lower = name.to_lowercase();

    // Explicit i() wrapper (all backends)
    if lower.starts_with("i(") && lower.ends_with(')') {
        return true;
    }

    // ngspice #branch suffix (e.g. "vdd#branch", "v1#branch")
    if lower.contains("#branch") {
        return true;
    }

    // Spectre/Vacask current notation: "V1:p", "I0:src"
    // These are terminal currents, identified by <name>:<terminal>
    // But only when NOT inside a v() wrapper (which would be hierarchy)
    if backend == "spectre" || backend == "vacask" || backend == "vacask-shared" {
        if !lower.starts_with("v(") && name.contains(':') {
            return true;
        }
    }

    false
}

/// Normalize a branch/current name to canonical `i(name)` format.
///
/// `"i(v1)"` -> `"i(v1)"`, `"I(V1)"` -> `"i(v1)"`,
/// `"V1:p"` -> `"i(v1)"`, `"I0:src"` -> `"i(i0)"`
/// `"vdd#branch"` -> `"i(vdd)"`, `"v1#branch"` -> `"i(v1)"`
fn normalize_current(name: &str, backend: &str) -> String {
    let lower = name.to_lowercase();

    // Already in i() format
    if lower.starts_with("i(") && lower.ends_with(')') {
        let inner = &lower[2..lower.len() - 1];
        let normalized_inner = normalize_hierarchy(inner, backend);
        return format!("i({})", normalized_inner);
    }

    // ngspice #branch suffix: "vdd#branch" -> "i(vdd)"
    if let Some(hash_pos) = lower.find("#branch") {
        let device = &lower[..hash_pos];
        return format!("i({})", device);
    }

    // Spectre/Vacask terminal current: "V1:p" -> "i(v1)"
    if backend == "spectre" || backend == "vacask" || backend == "vacask-shared" {
        if let Some(colon_pos) = name.find(':') {
            let device = name[..colon_pos].to_lowercase();
            return format!("i({})", device);
        }
    }

    // Fallback: wrap in i()
    format!("i({})", lower)
}

/// Normalize hierarchy separators to `.`.
///
/// - Xyce uses `%` for hierarchy: `x1%internal` -> `x1.internal`
/// - LTspice uses `:` for hierarchy: `x1:internal` -> `x1.internal`
///   (But in Spectre, `:` denotes terminal current, handled elsewhere)
/// - ngspice/Spectre already use `.`: no change needed
fn normalize_hierarchy(name: &str, backend: &str) -> String {
    match backend {
        "xyce" | "xyce-serial" | "xyce-parallel" => {
            name.replace('%', ".")
        }
        "ltspice" => {
            // LTspice uses `:` for hierarchy separators in node names
            name.replace(':', ".")
        }
        _ => {
            // ngspice, spectre, vacask already use `.` for hierarchy
            name.to_string()
        }
    }
}

/// Normalize a `.meas` result name to lowercase.
pub fn normalize_measure_name(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ngspice ──

    #[test]
    fn test_ngspice_voltage() {
        assert_eq!(normalize_var_name("v(out)", "ngspice"), "out");
        assert_eq!(normalize_var_name("V(OUT)", "ngspice"), "out");
    }

    #[test]
    fn test_ngspice_voltage_with_ref() {
        assert_eq!(normalize_var_name("v(out,0)", "ngspice"), "out");
    }

    #[test]
    fn test_ngspice_current() {
        assert_eq!(normalize_var_name("i(v1)", "ngspice"), "i(v1)");
        assert_eq!(normalize_var_name("I(V1)", "ngspice"), "i(v1)");
    }

    #[test]
    fn test_ngspice_hierarchy() {
        assert_eq!(normalize_var_name("v(x1.internal)", "ngspice"), "x1.internal");
        assert_eq!(normalize_var_name("V(X1.INTERNAL)", "ngspice"), "x1.internal");
    }

    // ── Xyce ──

    #[test]
    fn test_xyce_voltage() {
        assert_eq!(normalize_var_name("V(OUT)", "xyce"), "out");
    }

    #[test]
    fn test_xyce_current() {
        assert_eq!(normalize_var_name("I(V1)", "xyce"), "i(v1)");
    }

    #[test]
    fn test_xyce_hierarchy() {
        assert_eq!(normalize_var_name("V(X1%INTERNAL)", "xyce"), "x1.internal");
    }

    #[test]
    fn test_xyce_hierarchy_nested() {
        assert_eq!(normalize_var_name("V(X1%X2%NET3)", "xyce"), "x1.x2.net3");
    }

    #[test]
    fn test_xyce_current_hierarchy() {
        assert_eq!(normalize_var_name("I(X1%V1)", "xyce"), "i(x1.v1)");
    }

    // ── LTspice ──

    #[test]
    fn test_ltspice_voltage() {
        assert_eq!(normalize_var_name("V(out)", "ltspice"), "out");
    }

    #[test]
    fn test_ltspice_hierarchy() {
        assert_eq!(normalize_var_name("V(x1:internal)", "ltspice"), "x1.internal");
    }

    #[test]
    fn test_ltspice_current() {
        assert_eq!(normalize_var_name("I(V1)", "ltspice"), "i(v1)");
    }

    // ── Spectre ──

    #[test]
    fn test_spectre_bare_node() {
        assert_eq!(normalize_var_name("out", "spectre"), "out");
        assert_eq!(normalize_var_name("OUT", "spectre"), "out");
    }

    #[test]
    fn test_spectre_current() {
        // Spectre uses "V1:p" for current through V1 positive terminal
        assert_eq!(normalize_var_name("V1:p", "spectre"), "i(v1)");
    }

    #[test]
    fn test_spectre_current_isrc() {
        assert_eq!(normalize_var_name("I0:src", "spectre"), "i(i0)");
    }

    #[test]
    fn test_spectre_hierarchy() {
        // Spectre uses `.` for hierarchy (same as canonical)
        assert_eq!(normalize_var_name("x1.internal", "spectre"), "x1.internal");
    }

    // ── Vacask ──

    #[test]
    fn test_vacask_bare_node() {
        assert_eq!(normalize_var_name("out", "vacask"), "out");
    }

    #[test]
    fn test_vacask_current() {
        assert_eq!(normalize_var_name("V1:p", "vacask"), "i(v1)");
    }

    // ── Sweep variables ──

    #[test]
    fn test_sweep_vars_unchanged() {
        assert_eq!(normalize_var_name("time", "ngspice"), "time");
        assert_eq!(normalize_var_name("frequency", "ngspice"), "frequency");
        assert_eq!(normalize_var_name("TIME", "xyce"), "time");
        assert_eq!(normalize_var_name("FREQUENCY", "xyce"), "frequency");
    }

    // ── Noise spectra ──

    #[test]
    fn test_noise_vars_preserved() {
        assert_eq!(normalize_var_name("inoise_spectrum", "ngspice"), "inoise_spectrum");
        assert_eq!(normalize_var_name("onoise_spectrum", "ngspice"), "onoise_spectrum");
    }

    // ── Measure normalization ──

    #[test]
    fn test_measure_normalization() {
        assert_eq!(normalize_measure_name("Gain_dB"), "gain_db");
        assert_eq!(normalize_measure_name("BANDWIDTH"), "bandwidth");
        assert_eq!(normalize_measure_name("rise_time"), "rise_time");
    }

    // ── is_current_name ──

    #[test]
    fn test_is_current_explicit() {
        assert!(is_current_name("i(v1)", "ngspice"));
        assert!(is_current_name("I(V1)", "xyce"));
    }

    #[test]
    fn test_is_current_spectre_terminal() {
        assert!(is_current_name("V1:p", "spectre"));
        assert!(is_current_name("I0:src", "vacask"));
    }

    #[test]
    fn test_is_not_current_voltage() {
        assert!(!is_current_name("v(out)", "ngspice"));
        assert!(!is_current_name("V(OUT)", "xyce"));
    }

    #[test]
    fn test_is_not_current_spectre_voltage_wrapper() {
        // v(x1:internal) in Spectre context -- the `:` is inside v(), so
        // it's hierarchy, not a current
        assert!(!is_current_name("v(x1:internal)", "spectre"));
    }

    // ── Backward compatibility with clean_name behavior ──

    #[test]
    fn test_backward_compat_ngspice_defaults() {
        // These mirror the old clean_name behavior
        assert_eq!(normalize_var_name("v(out)", "ngspice"), "out");
        assert_eq!(normalize_var_name("V(NET1)", "ngspice"), "net1");
        assert_eq!(normalize_var_name("i(Vin)", "ngspice"), "i(vin)");
    }

    // ── ngspice-shared / ngspice-subprocess variants ──

    #[test]
    fn test_ngspice_variant_names() {
        assert_eq!(normalize_var_name("v(out)", "ngspice-subprocess"), "out");
        assert_eq!(normalize_var_name("v(out)", "ngspice-shared"), "out");
    }
}
