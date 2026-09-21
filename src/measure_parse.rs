//! Parsers for `.meas` results from simulator stdout/log output.
//!
//! Each simulator prints measurement results in a different format:
//!
//! **NGSpice** (stdout, batch mode):
//! ```text
//! rise_time        =  2.345000e-09
//! fall_time        =  1.987000e-09
//! ```
//!
//! **Xyce** (stdout):
//! ```text
//! .MEASURE TRAN rise_time = 2.345000e-09
//! ```
//!
//! **LTspice** (log file):
//! ```text
//! Measurement: rise_time
//!   rise_time: AVG=2.345e-09
//! ```
//! Or single-line: `rise_time: 2.345e-009 ...`

use crate::result::MeasureResult;

/// Parse measure results from simulator output, auto-detecting format
/// based on `backend_name`.
pub fn parse_measures(text: &str, backend_name: &str) -> Vec<MeasureResult> {
    match backend_name {
        "ngspice-subprocess" | "ngspice" | "ngspice-shared" => parse_ngspice(text),
        "xyce-serial" | "xyce-parallel" | "xyce" => parse_xyce(text),
        "ltspice" => parse_ltspice(text),
        _ => {
            // Try all parsers, return whichever finds results
            let results = parse_ngspice(text);
            if !results.is_empty() {
                return results;
            }
            let results = parse_xyce(text);
            if !results.is_empty() {
                return results;
            }
            parse_ltspice(text)
        }
    }
}

/// Parse NGSpice batch-mode stdout for .meas results.
///
/// Format: `name = value` (possibly with leading whitespace)
/// Lines containing "=" that look like measure results.
/// NGSpice also prints "failed" for measures that didn't trigger.
fn parse_ngspice(text: &str) -> Vec<MeasureResult> {
    // ngspice groups real results under a "Measurements for <Analysis>" header:
    //
    //     Measurements for Transient Analysis
    //
    //     tplh   =  9.975241e-12 targ=  1.059975e-09 trig=  1.050000e-09
    //
    // Anchoring to that block matters because ngspice's resource footer ends
    // with `Stack = 0 bytes.`, which a bare `name = value` scan reports as a
    // measurement named "Stack". Trailing text after the value is legitimate
    // (targ/trig/at/from/to), so the value cannot be required to stand alone.
    let mut results: Vec<MeasureResult> = Vec::new();
    let mut in_block = false;
    let mut saw_block = false;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.contains("Measurements for") {
            in_block = true;
            saw_block = true;
            continue;
        }
        if !in_block {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        match measure_line(trimmed) {
            Some(result) => push_measure(&mut results, result),
            // First non-measure line closes the block.
            None => in_block = false,
        }
    }

    if saw_block {
        return results;
    }

    // No header (non-batch output, or a caller passing a bare fragment):
    // fall back to scanning every line.
    for line in text.lines() {
        if let Some(result) = measure_line(line.trim()) {
            push_measure(&mut results, result);
        }
    }
    results
}

/// Parse a single `name = value [extra...]` measure line.
fn measure_line(trimmed: &str) -> Option<MeasureResult> {
    let (name_part, value_part) = trimmed.split_once('=')?;
    let name = name_part.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let value_str = value_part.trim();
    if value_str.starts_with("failed") {
        return None;
    }
    let value = value_str.split_whitespace().next()?.parse::<f64>().ok()?;
    Some(MeasureResult { name: name.to_string(), value })
}

/// Later values win: a deck that reports a measure twice should not yield
/// duplicate entries.
fn push_measure(results: &mut Vec<MeasureResult>, result: MeasureResult) {
    if let Some(existing) = results.iter_mut().find(|r| r.name == result.name) {
        existing.value = result.value;
    } else {
        results.push(result);
    }
}

/// Parse Xyce stdout for .meas results.
///
/// Format: `.MEASURE TRAN name = value` or `.MEASURE DC name = value`
fn parse_xyce(text: &str) -> Vec<MeasureResult> {
    let mut results = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_uppercase();

        if !upper.starts_with(".MEASURE") {
            continue;
        }

        // .MEASURE <analysis_type> <name> = <value>
        // Split at '=' to get name and value
        if let Some((before_eq, after_eq)) = trimmed.split_once('=') {
            let value_str = after_eq.trim();
            let first_token = value_str.split_whitespace().next().unwrap_or("");

            if let Ok(value) = first_token.parse::<f64>() {
                // Extract name: last word before '='
                let name = before_eq.split_whitespace().last().unwrap_or("").to_string();
                if !name.is_empty() {
                    results.push(MeasureResult { name, value });
                }
            }
        }
    }

    results
}

/// Parse LTspice log file for .meas results.
///
/// LTspice has several formats:
/// - Single-line: `rise_time: 2.345e-009 ...`  (name: value ...)
/// - Multi-line: `Measurement: rise_time` followed by `  rise_time: AVG=value`
/// - Simple: `name: value=number FROM ...`
fn parse_ltspice(text: &str) -> Vec<MeasureResult> {
    let mut results = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Skip non-measurement lines
        if trimmed.starts_with("Circuit:") || trimmed.starts_with("Date:") ||
           trimmed.starts_with("Total elapsed") || trimmed.starts_with(".step") ||
           trimmed.starts_with("Measurement:") {
            continue;
        }

        // Pattern: "name: value" or "name: KEY=value"
        if let Some((name_part, rest)) = trimmed.split_once(':') {
            let name = name_part.trim();

            // Skip if name has spaces (not a simple measure name) or is empty
            if name.is_empty() || name.contains(' ') {
                continue;
            }

            let rest = rest.trim();

            // Try direct numeric value: "name: 1.23e-09"
            let first_token = rest.split_whitespace().next().unwrap_or("");
            if let Ok(value) = first_token.parse::<f64>() {
                results.push(MeasureResult {
                    name: name.to_string(),
                    value,
                });
                continue;
            }

            // Try "KEY=value" format: "name: AVG=1.23e-09" or "name: FROM=... TO=... AVG=..."
            for part in rest.split_whitespace() {
                if let Some((_key, val_str)) = part.split_once('=')
                    && let Ok(value) = val_str.parse::<f64>() {
                        results.push(MeasureResult {
                            name: name.to_string(),
                            value,
                        });
                        break; // Take first parseable value
                    }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ngspice_measures() {
        let stdout = "\
Circuit: test circuit

Doing analysis at TEMP = 27.000000 and target TNOM = 27.000000

rise_time        =  2.345000e-09
fall_time        =  1.987000e-09
vout_dc          =  1.650000e+00
gain_failed      =  failed
";
        let results = parse_ngspice(stdout);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "rise_time");
        assert!((results[0].value - 2.345e-9).abs() < 1e-20);
        assert_eq!(results[1].name, "fall_time");
        assert!((results[1].value - 1.987e-9).abs() < 1e-20);
        assert_eq!(results[2].name, "vout_dc");
        assert!((results[2].value - 1.65).abs() < 1e-10);
    }

    #[test]
    fn test_parse_ngspice_ignores_resource_footer() {
        // Verbatim ngspice -b output shape. `Stack = 0 bytes.` parses as a
        // `name = value` pair and was previously reported as a measurement.
        let stdout = "\
Circuit: * rc

  Measurements for AC Analysis

gain_100            =  -1.445070e+00
tplh                =  9.975241e-12 targ=  1.059975e-09 trig=  1.050000e-09

binary raw file \"/tmp/x.raw\"

Total analysis time (seconds) = 0.0632234
Stack = 0 bytes.
Library pages =    2.059 MB.
";
        let results = parse_ngspice(stdout);
        assert_eq!(results.len(), 2, "got {:?}", results);
        assert_eq!(results[0].name, "gain_100");
        assert!((results[0].value - -1.445070).abs() < 1e-9);
        // Trailing targ=/trig= text must not break the value parse.
        assert_eq!(results[1].name, "tplh");
        assert!((results[1].value - 9.975241e-12).abs() < 1e-20);
        assert!(!results.iter().any(|r| r.name == "Stack"));
    }

    #[test]
    fn test_parse_ngspice_repeated_measure_keeps_last() {
        let stdout = "  Measurements for Transient Analysis\n\ng = 1.0\ng = 2.0\n";
        let results = parse_ngspice(stdout);
        assert_eq!(results.len(), 1);
        assert!((results[0].value - 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_parse_ngspice_empty() {
        let stdout = "Circuit: test\nDoing analysis at TEMP = 27\n";
        let results = parse_ngspice(stdout);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_xyce_measures() {
        let stdout = "\
Xyce Release 7.6
.MEASURE TRAN rise_time = 2.345000e-09
.MEASURE TRAN fall_time = 1.987000e-09
.MEASURE DC vout_max = 3.300000e+00
";
        let results = parse_xyce(stdout);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "rise_time");
        assert!((results[0].value - 2.345e-9).abs() < 1e-20);
        assert_eq!(results[1].name, "fall_time");
        assert!((results[1].value - 1.987e-9).abs() < 1e-20);
        assert_eq!(results[2].name, "vout_max");
        assert!((results[2].value - 3.3).abs() < 1e-10);
    }

    #[test]
    fn test_parse_xyce_empty() {
        let stdout = "Xyce Release 7.6\nSimulation complete.\n";
        let results = parse_xyce(stdout);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_ltspice_simple() {
        let log = "\
Circuit: test
Date: Mon May 12 10:00:00 2026
rise_time: 2.345e-009
fall_time: 1.987e-009
";
        let results = parse_ltspice(log);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "rise_time");
        assert!((results[0].value - 2.345e-9).abs() < 1e-20);
        assert_eq!(results[1].name, "fall_time");
    }

    #[test]
    fn test_parse_ltspice_key_value() {
        let log = "\
Measurement: avg_vout
  avg_vout: AVG=1.650000e+00 FROM=0 TO=1e-06
";
        let results = parse_ltspice(log);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "avg_vout");
        assert!((results[0].value - 1.65).abs() < 1e-10);
    }

    #[test]
    fn test_parse_ltspice_empty() {
        let log = "Circuit: test\nTotal elapsed time: 0.5s\n";
        let results = parse_ltspice(log);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_measures_auto_detect() {
        // NGSpice format with unknown backend falls back to trying all
        let stdout = "rise_time        =  2.345000e-09\n";
        let results = parse_measures(stdout, "unknown");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "rise_time");
    }

    #[test]
    fn test_parse_measures_dispatches_correctly() {
        let ngspice_out = "rise_time        =  2.345000e-09\n";
        let xyce_out = ".MEASURE TRAN rise_time = 2.345000e-09\n";
        let ltspice_out = "rise_time: 2.345e-009\n";

        let r1 = parse_measures(ngspice_out, "ngspice");
        let r2 = parse_measures(xyce_out, "xyce");
        let r3 = parse_measures(ltspice_out, "ltspice");

        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
        assert_eq!(r3.len(), 1);
        assert_eq!(r1[0].name, "rise_time");
        assert_eq!(r2[0].name, "rise_time");
        assert_eq!(r3[0].name, "rise_time");
    }

    #[test]
    fn test_parse_ngspice_scientific_notation() {
        let stdout = "bw =  5.67890e+06\n";
        let results = parse_ngspice(stdout);
        assert_eq!(results.len(), 1);
        assert!((results[0].value - 5.6789e6).abs() < 1.0);
    }

    #[test]
    fn test_parse_xyce_different_analysis_types() {
        let stdout = "\
.MEASURE TRAN delay = 1.5e-09
.MEASURE AC bw_3db = 1.0e+07
.MEASURE DC vth = 0.45
";
        let results = parse_xyce(stdout);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "delay");
        assert_eq!(results[1].name, "bw_3db");
        assert_eq!(results[2].name, "vth");
    }
}
