//! SPICE netlist linter with backend-specific recommendations.
//!
//! Checks for common issues that cause simulation failures or convergence problems,
//! plus backend-specific syntax warnings.

use std::collections::{HashMap, HashSet};

/// Linting result containing warnings and errors.
#[derive(Debug, Clone)]
pub struct LintResult {
    pub warnings: Vec<LintWarning>,
    pub errors: Vec<LintError>,
}

/// A lint warning: may not prevent simulation but is suspicious.
#[derive(Debug, Clone)]
pub struct LintWarning {
    pub line: usize,
    pub message: String,
    pub suggestion: Option<String>,
    pub backends_affected: Vec<String>,
}

/// A lint error: likely to cause simulation failure.
#[derive(Debug, Clone)]
pub struct LintError {
    pub line: usize,
    pub message: String,
}

/// Lint a SPICE netlist. If `target_backend` is provided, include
/// backend-specific checks.
pub fn lint_netlist(netlist: &str, target_backend: Option<&str>) -> LintResult {
    let mut result = LintResult {
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    let lines: Vec<&str> = netlist.lines().collect();

    check_missing_end(&lines, &mut result);
    check_missing_ground(&lines, &mut result);
    check_duplicate_elements(&lines, &mut result);
    check_floating_nodes(&lines, &mut result);
    check_zero_value_components(&lines, &mut result);
    check_missing_model_references(&lines, &mut result);
    check_undefined_parameters(&lines, &mut result);

    if let Some(backend) = target_backend {
        check_backend_specific(&lines, backend, &mut result);
    }

    result
}

/// Check that the netlist ends with .end
fn check_missing_end(lines: &[&str], result: &mut LintResult) {
    let has_end = lines.iter().any(|l| {
        let t = l.trim().to_lowercase();
        t == ".end"
    });

    if !has_end {
        result.errors.push(LintError {
            line: lines.len(),
            message: "Netlist does not end with .end".to_string(),
        });
    }
}

/// Check that node "0" or "gnd" is referenced somewhere
fn check_missing_ground(lines: &[&str], result: &mut LintResult) {
    let mut has_ground = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('*') || trimmed.starts_with('.') {
            continue;
        }
        for token in trimmed.split_whitespace() {
            let lower = token.to_lowercase();
            if lower == "0" || lower == "gnd" {
                has_ground = true;
                break;
            }
        }
        if has_ground {
            break;
        }
    }

    if !has_ground {
        result.errors.push(LintError {
            line: 0,
            message: "No ground node (0 or gnd) found in netlist".to_string(),
        });
    }
}

/// Check for duplicate element names
fn check_duplicate_elements(lines: &[&str], result: &mut LintResult) {
    let mut seen: HashMap<String, usize> = HashMap::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('*') || trimmed.starts_with('.')
            || trimmed.starts_with('+')
        {
            continue;
        }

        let first_token = trimmed.split_whitespace().next().unwrap_or("");
        if first_token.is_empty() {
            continue;
        }

        let first_char = first_token.chars().next().unwrap_or(' ');
        if !first_char.is_ascii_alphabetic() {
            continue;
        }

        let name_lower = first_token.to_lowercase();
        let line_num = idx + 1;

        if let Some(&prev_line) = seen.get(&name_lower) {
            result.errors.push(LintError {
                line: line_num,
                message: format!(
                    "Duplicate element name '{}' (first defined on line {})",
                    first_token, prev_line
                ),
            });
        } else {
            seen.insert(name_lower, line_num);
        }
    }
}

/// Check for floating nodes (connected to only one terminal)
fn check_floating_nodes(lines: &[&str], result: &mut LintResult) {
    let mut node_connections: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('*') || trimmed.starts_with('+') {
            continue;
        }

        if trimmed.starts_with('.') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let first_char = parts[0].chars().next().unwrap_or(' ').to_ascii_uppercase();

        let node_indices = match first_char {
            'R' | 'C' | 'L' | 'V' | 'I' | 'D' => {
                if parts.len() >= 3 { vec![1, 2] } else { vec![] }
            }
            'Q' | 'J' | 'Z' => {
                if parts.len() >= 4 { vec![1, 2, 3] } else { vec![] }
            }
            'M' | 'E' | 'G' | 'S' => {
                if parts.len() >= 5 { vec![1, 2, 3, 4] } else { vec![] }
            }
            'F' | 'H' => {
                if parts.len() >= 3 { vec![1, 2] } else { vec![] }
            }
            'X' => {
                let mut nodes = Vec::new();
                for (i, p) in parts.iter().enumerate().skip(1) {
                    if p.contains('=') {
                        break;
                    }
                    nodes.push(i);
                }
                if !nodes.is_empty() {
                    nodes.pop();
                }
                nodes
            }
            'T' => {
                if parts.len() >= 5 { vec![1, 2, 3, 4] } else { vec![] }
            }
            'K' => vec![],
            'B' | 'W' => {
                if parts.len() >= 3 { vec![1, 2] } else { vec![] }
            }
            _ => vec![],
        };

        let line_num = idx + 1;
        for &ni in &node_indices {
            if ni < parts.len() {
                let node = parts[ni].to_lowercase();
                if node == "0" || node == "gnd" {
                    continue;
                }
                node_connections.entry(node).or_default().push(line_num);
            }
        }
    }

    for (node, lines_vec) in &node_connections {
        if lines_vec.len() == 1 {
            result.warnings.push(LintWarning {
                line: lines_vec[0],
                message: format!("Node '{}' is connected to only one element (floating node)", node),
                suggestion: Some("Add another connection or remove the node".to_string()),
                backends_affected: vec![],
            });
        }
    }
}

/// Check for zero-value components that could cause convergence issues
fn check_zero_value_components(lines: &[&str], result: &mut LintResult) {
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('*') || trimmed.starts_with('.')
            || trimmed.starts_with('+')
        {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let first_char = parts[0].chars().next().unwrap_or(' ').to_ascii_uppercase();
        let line_num = idx + 1;

        match first_char {
            'R'
                if parts.len() >= 4 && is_zero_value(parts[3]) => {
                    result.warnings.push(LintWarning {
                        line: line_num,
                        message: format!("Resistor '{}' has zero resistance", parts[0]),
                        suggestion: Some("Use a small value (e.g., 1m) or a voltage source instead".to_string()),
                        backends_affected: vec!["ngspice".to_string(), "xyce".to_string(), "ltspice".to_string()],
                    });
                }
            'C'
                if parts.len() >= 4 && is_zero_value(parts[3]) => {
                    result.warnings.push(LintWarning {
                        line: line_num,
                        message: format!("Capacitor '{}' has zero capacitance", parts[0]),
                        suggestion: Some("Remove the capacitor or use a small value".to_string()),
                        backends_affected: vec!["ngspice".to_string(), "xyce".to_string()],
                    });
                }
            _ => {}
        }
    }
}

/// Check for model references and missing definitions
fn check_missing_model_references(lines: &[&str], result: &mut LintResult) {
    let mut defined_models: HashSet<String> = HashSet::new();
    let mut has_include = false;

    for line in lines {
        let trimmed = line.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with(".MODEL") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                defined_models.insert(parts[1].to_lowercase());
            }
        }
        if upper.starts_with(".INCLUDE") || upper.starts_with(".LIB") {
            has_include = true;
        }
    }

    if has_include {
        return;
    }

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('*') || trimmed.starts_with('.')
            || trimmed.starts_with('+')
        {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let first_char = parts[0].chars().next().unwrap_or(' ').to_ascii_uppercase();
        let line_num = idx + 1;

        let model_pos = match first_char {
            'D' => if parts.len() >= 4 { Some(3) } else { None },
            'Q' | 'J' | 'Z' => if parts.len() >= 5 { Some(4) } else { None },
            'M' => if parts.len() >= 6 { Some(5) } else { None },
            _ => None,
        };

        if let Some(pos) = model_pos {
            let model_name = parts[pos].to_lowercase();
            if !defined_models.contains(&model_name) {
                result.warnings.push(LintWarning {
                    line: line_num,
                    message: format!(
                        "Element '{}' references model '{}' which is not defined in this netlist",
                        parts[0], parts[pos]
                    ),
                    suggestion: Some("Add a .model definition or .include the model file".to_string()),
                    backends_affected: vec![],
                });
            }
        }
    }
}

/// Check for {expression} parameters without .param definitions
fn check_undefined_parameters(lines: &[&str], result: &mut LintResult) {
    let mut defined_params: HashSet<String> = HashSet::new();

    for line in lines {
        let trimmed = line.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with(".PARAM") {
            let rest = &trimmed[6..];
            for part in rest.split_whitespace() {
                if let Some((name, _)) = part.split_once('=') {
                    defined_params.insert(name.trim().to_lowercase());
                }
            }
        }
    }

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('*') || trimmed.starts_with('.') {
            continue;
        }

        let line_num = idx + 1;
        let mut i = 0;
        let bytes = trimmed.as_bytes();
        while i < bytes.len() {
            if bytes[i] == b'{' {
                if let Some(end) = trimmed[i..].find('}') {
                    let expr = &trimmed[i + 1..i + end];
                    let expr_lower = expr.trim().to_lowercase();
                    if !expr_lower.is_empty()
                        && !expr_lower.contains('+')
                        && !expr_lower.contains('-')
                        && !expr_lower.contains('*')
                        && !expr_lower.contains('/')
                        && !expr_lower.contains('(')
                        && !defined_params.contains(&expr_lower)
                        && expr_lower.parse::<f64>().is_err() {
                            result.warnings.push(LintWarning {
                                line: line_num,
                                message: format!(
                                    "Parameter '{{{}}}' used but not defined with .param",
                                    expr
                                ),
                                suggestion: Some(format!("Add: .param {} = <value>", expr)),
                                backends_affected: vec![],
                            });
                        }
                    i += end + 1;
                } else {
                    break;
                }
            } else {
                i += 1;
            }
        }
    }
}

/// Backend-specific checks
fn check_backend_specific(lines: &[&str], backend: &str, result: &mut LintResult) {
    match backend {
        "ngspice" | "ngspice-subprocess" | "ngspice-shared" => {
            check_ngspice_specific(lines, result);
        }
        "xyce" | "xyce-serial" | "xyce-parallel" => {
            check_xyce_specific(lines, result);
        }
        "ltspice" => {
            check_ltspice_specific(lines, result);
        }
        "spectre" => {
            check_spectre_specific(lines, result);
        }
        _ => {}
    }
}

fn check_ngspice_specific(lines: &[&str], result: &mut LintResult) {
    for (idx, line) in lines.iter().enumerate() {
        let upper = line.trim().to_uppercase();
        let line_num = idx + 1;

        if upper.starts_with(".MEAS") {
            result.warnings.push(LintWarning {
                line: line_num,
                message: ".meas only produces output in batch mode (ngspice -b)".to_string(),
                suggestion: Some("Run with ngspice -b, not interactive mode".to_string()),
                backends_affected: vec!["ngspice".to_string()],
            });
        }
    }
}

fn check_xyce_specific(lines: &[&str], result: &mut LintResult) {
    for (idx, line) in lines.iter().enumerate() {
        let upper = line.trim().to_uppercase();
        let line_num = idx + 1;

        if upper.starts_with(".CONTROL") {
            result.errors.push(LintError {
                line: line_num,
                message: ".control blocks are not supported by Xyce".to_string(),
            });
        }
        if upper.starts_with(".PZ") {
            result.warnings.push(LintWarning {
                line: line_num,
                message: ".pz (pole-zero) analysis is not supported by Xyce".to_string(),
                suggestion: Some("Use .ac analysis and post-process for poles/zeros".to_string()),
                backends_affected: vec!["xyce".to_string()],
            });
        }
        if upper.starts_with(".DISTO") {
            result.warnings.push(LintWarning {
                line: line_num,
                message: ".disto analysis is not supported by Xyce".to_string(),
                suggestion: Some("Use .tran with FFT post-processing instead".to_string()),
                backends_affected: vec!["xyce".to_string()],
            });
        }
    }
}

fn check_ltspice_specific(lines: &[&str], result: &mut LintResult) {
    for (idx, line) in lines.iter().enumerate() {
        let upper = line.trim().to_uppercase();
        let line_num = idx + 1;

        if upper.starts_with(".PZ") {
            result.warnings.push(LintWarning {
                line: line_num,
                message: ".pz analysis is not supported by LTspice".to_string(),
                suggestion: Some("Use .ac analysis instead".to_string()),
                backends_affected: vec!["ltspice".to_string()],
            });
        }
        if upper.starts_with(".DISTO") {
            result.warnings.push(LintWarning {
                line: line_num,
                message: ".disto analysis is not supported by LTspice".to_string(),
                suggestion: Some("Use .tran with .four or .FFT instead".to_string()),
                backends_affected: vec!["ltspice".to_string()],
            });
        }
        if upper.starts_with(".SENS") {
            result.warnings.push(LintWarning {
                line: line_num,
                message: ".sens analysis is not supported by LTspice".to_string(),
                suggestion: Some("Use parametric sweeps with .step instead".to_string()),
                backends_affected: vec!["ltspice".to_string()],
            });
        }
        if upper.starts_with(".CONTROL") {
            result.errors.push(LintError {
                line: line_num,
                message: ".control blocks are not supported by LTspice".to_string(),
            });
        }
    }
}

fn check_spectre_specific(lines: &[&str], result: &mut LintResult) {
    for (idx, line) in lines.iter().enumerate() {
        let upper = line.trim().to_uppercase();
        let line_num = idx + 1;

        if upper.starts_with(".CONTROL") {
            result.errors.push(LintError {
                line: line_num,
                message: ".control blocks are not supported by Spectre".to_string(),
            });
        }
        if upper.starts_with(".PZ") {
            result.warnings.push(LintWarning {
                line: line_num,
                message: ".pz analysis is not supported by Spectre".to_string(),
                suggestion: Some("Use stb or ac analysis for stability/poles".to_string()),
                backends_affected: vec!["spectre".to_string()],
            });
        }
        if upper.starts_with(".DISTO") {
            result.warnings.push(LintWarning {
                line: line_num,
                message: ".disto analysis is not supported by Spectre".to_string(),
                suggestion: Some("Use HB (harmonic balance) analysis for distortion".to_string()),
                backends_affected: vec!["spectre".to_string()],
            });
        }
    }
}

/// Check if a value string represents zero
fn is_zero_value(s: &str) -> bool {
    if let Ok(v) = s.parse::<f64>() {
        return v == 0.0;
    }
    let trimmed = s.trim().to_lowercase();
    trimmed == "0" || trimmed == "0.0"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_end() {
        let netlist = ".title test\nR1 a 0 1k\n";
        let result = lint_netlist(netlist, None);
        assert!(result.errors.iter().any(|e| e.message.contains(".end")));
    }

    #[test]
    fn test_has_end() {
        let netlist = ".title test\nR1 a 0 1k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(!result.errors.iter().any(|e| e.message.contains(".end")));
    }

    #[test]
    fn test_missing_ground() {
        let netlist = ".title test\nR1 a b 1k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(result.errors.iter().any(|e| e.message.contains("ground")));
    }

    #[test]
    fn test_has_ground_zero() {
        let netlist = ".title test\nR1 a 0 1k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(!result.errors.iter().any(|e| e.message.contains("ground")));
    }

    #[test]
    fn test_has_ground_gnd() {
        let netlist = ".title test\nR1 a gnd 1k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(!result.errors.iter().any(|e| e.message.contains("ground")));
    }

    #[test]
    fn test_duplicate_elements() {
        let netlist = ".title test\nR1 a 0 1k\nR1 b 0 2k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(result.errors.iter().any(|e| e.message.contains("Duplicate")));
    }

    #[test]
    fn test_no_duplicate_elements() {
        let netlist = ".title test\nR1 a 0 1k\nR2 b 0 2k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(!result.errors.iter().any(|e| e.message.contains("Duplicate")));
    }

    #[test]
    fn test_floating_node() {
        let netlist = ".title test\nV1 in 0 1\nR1 in out 1k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(result.warnings.iter().any(|w| w.message.contains("floating")));
    }

    #[test]
    fn test_no_floating_node() {
        let netlist = ".title test\nV1 in 0 1\nR1 in out 1k\nR2 out 0 1k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(!result.warnings.iter().any(|w| w.message.contains("floating")));
    }

    #[test]
    fn test_zero_resistance() {
        let netlist = ".title test\nR1 a 0 0\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(result.warnings.iter().any(|w| w.message.contains("zero resistance")));
    }

    #[test]
    fn test_zero_capacitance() {
        let netlist = ".title test\nC1 a 0 0\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(result.warnings.iter().any(|w| w.message.contains("zero capacitance")));
    }

    #[test]
    fn test_missing_model() {
        let netlist = ".title test\nM1 d g s b nmos W=1u L=100n\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(result.warnings.iter().any(|w| w.message.contains("model") && w.message.contains("nmos")));
    }

    #[test]
    fn test_model_defined() {
        let netlist = ".title test\n.model nmos nmos (vth0=0.5)\nM1 d g s b nmos W=1u L=100n\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(!result.warnings.iter().any(|w| w.message.contains("model") && w.message.contains("nmos")));
    }

    #[test]
    fn test_model_with_include_skips_check() {
        let netlist = ".title test\n.include /pdk/models.lib\nM1 d g s b nmos W=1u L=100n\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(!result.warnings.iter().any(|w| w.message.contains("model") && w.message.contains("nmos")));
    }

    #[test]
    fn test_undefined_parameter() {
        let netlist = ".title test\nR1 a 0 {rval}\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(result.warnings.iter().any(|w| w.message.contains("rval")));
    }

    #[test]
    fn test_defined_parameter() {
        let netlist = ".title test\n.param rval=1k\nR1 a 0 {rval}\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(!result.warnings.iter().any(|w| w.message.contains("rval")));
    }

    #[test]
    fn test_ngspice_meas_warning() {
        let netlist = ".title test\nR1 a 0 1k\n.meas tran rise_time trig v(a)\n.end\n";
        let result = lint_netlist(netlist, Some("ngspice"));
        assert!(result.warnings.iter().any(|w| w.message.contains("batch mode")));
    }

    #[test]
    fn test_xyce_control_block() {
        let netlist = ".title test\nR1 a 0 1k\n.control\nrun\n.endc\n.end\n";
        let result = lint_netlist(netlist, Some("xyce"));
        assert!(result.errors.iter().any(|e| e.message.contains(".control")));
    }

    #[test]
    fn test_xyce_pz_warning() {
        let netlist = ".title test\nR1 a 0 1k\n.pz a 0 a 0 vol pz\n.end\n";
        let result = lint_netlist(netlist, Some("xyce"));
        assert!(result.warnings.iter().any(|w| w.message.contains(".pz")));
    }

    #[test]
    fn test_ltspice_sens_warning() {
        let netlist = ".title test\nR1 a 0 1k\n.sens v(a)\n.end\n";
        let result = lint_netlist(netlist, Some("ltspice"));
        assert!(result.warnings.iter().any(|w| w.message.contains(".sens")));
    }

    #[test]
    fn test_spectre_control_block() {
        let netlist = ".title test\nR1 a 0 1k\n.control\nrun\n.endc\n.end\n";
        let result = lint_netlist(netlist, Some("spectre"));
        assert!(result.errors.iter().any(|e| e.message.contains(".control")));
    }

    #[test]
    fn test_spectre_disto_warning() {
        let netlist = ".title test\nR1 a 0 1k\n.disto dec 10 100 1e8\n.end\n";
        let result = lint_netlist(netlist, Some("spectre"));
        assert!(result.warnings.iter().any(|w| w.message.contains(".disto")));
    }

    #[test]
    fn test_clean_netlist() {
        let netlist = "\
.title clean circuit
V1 in 0 DC 1
R1 in out 1k
R2 out 0 2k
.op
.end
";
        let result = lint_netlist(netlist, None);
        assert!(result.errors.is_empty(), "Expected no errors, got: {:?}", result.errors);
        assert!(result.warnings.is_empty(), "Expected no warnings, got: {:?}", result.warnings);
    }

    #[test]
    fn test_lint_with_no_backend() {
        let netlist = ".title test\nR1 a 0 1k\n.end\n";
        let result = lint_netlist(netlist, None);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_is_zero_value() {
        assert!(is_zero_value("0"));
        assert!(is_zero_value("0.0"));
        assert!(is_zero_value("0.00"));
        assert!(!is_zero_value("1k"));
        assert!(!is_zero_value("100"));
        assert!(!is_zero_value("1e-12"));
    }
}
