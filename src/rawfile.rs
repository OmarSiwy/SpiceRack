//! Parser for nutmeg-format `.raw` binary and ASCII output files (ngspice, VACASK, LTspice).
//!
//! Format:
//!   Header lines (key: value)
//!   "Variables:" block
//!   "Binary:" or "Values:" marker
//!   Data block

use std::io::{BufRead, BufReader, Read, Cursor};
use num_complex::Complex64;
use crate::result::{RawData, VarInfo};

#[derive(Debug, thiserror::Error)]
pub enum RawFileError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Unexpected format: {0}")]
    Format(String),
}

/// Parse a .raw file from bytes.
/// Handles both ngspice (UTF-8) and LTspice (UTF-16-LE) encodings.
pub fn parse_raw(data: &[u8]) -> Result<RawData, RawFileError> {
    // LTspice may produce UTF-16-LE encoded headers.
    // Detect by checking for BOM (FF FE) or null bytes in first 4 bytes.
    let data = if is_utf16_le(data) {
        let decoded = decode_utf16_le(data);
        return parse_raw_utf8(&decoded);
    } else {
        data
    };

    parse_raw_utf8(data)
}

/// Check if data appears to be UTF-16-LE encoded
fn is_utf16_le(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    // Check for BOM
    if data[0] == 0xFF && data[1] == 0xFE {
        return true;
    }
    // Check for null bytes interleaved with ASCII (T\0i\0t\0l\0e\0)
    data.len() >= 10 && data[1] == 0 && data[3] == 0 && data[5] == 0
}

/// Decode UTF-16-LE to UTF-8 string, stopping at "Binary:" or "Values:" marker.
/// Returns the header as UTF-8 + the remaining binary data appended as-is.
fn decode_utf16_le(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());

    // Skip BOM if present
    let start = if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE { 2 } else { 0 };

    // Decode UTF-16-LE characters until we find "Binary:" or "Values:"
    let mut i = start;
    let mut header_end_byte = 0;
    let mut found_marker = false;

    while i + 1 < data.len() {
        let lo = data[i];
        let hi = data[i + 1];

        if hi == 0 && lo < 128 {
            // ASCII character
            result.push(lo);

            // Check if we just completed "Binary:\r\n" or "Binary:\n" or "Values:\r\n"
            let rlen = result.len();
            if rlen >= 8 {
                let tail = &result[rlen - 8..];
                if tail == b"Binary:\r\n" || tail[1..] == *b"inary:\n" {
                    header_end_byte = i + 2;
                    found_marker = true;
                    break;
                }
            }
            if rlen >= 9 {
                let tail = &result[rlen - 9..];
                if tail == b"Values:\r\n" || tail[1..] == *b"alues:\r\n" {
                    header_end_byte = i + 2;
                    found_marker = true;
                    break;
                }
            }
        } else {
            // Non-ASCII or high byte — just skip (shouldn't appear in headers)
            result.push(lo);
        }

        i += 2;
    }

    if found_marker {
        // Append the binary data section as-is
        result.extend_from_slice(&data[header_end_byte..]);
    }

    result
}

fn parse_raw_utf8(data: &[u8]) -> Result<RawData, RawFileError> {
    let mut reader = BufReader::new(Cursor::new(data));
    let mut result = RawData::empty();

    let mut num_vars = 0usize;
    let mut num_points = 0usize;
    let mut in_variables = false;
    let is_binary;

    // Parse header
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Err(RawFileError::Format("Unexpected EOF in header".into()));
        }
        let line = line.trim_end().to_string();

        if line == "Binary:" {
            is_binary = true;
            break;
        }
        if line == "Values:" {
            is_binary = false;
            break;
        }

        if line.starts_with("Variables:") {
            in_variables = true;
            continue;
        }

        if in_variables {
            // Variable line: "\tindex\tname\ttype"
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                let index: usize = parts[0].parse().map_err(|e| {
                    RawFileError::Parse(format!("Bad variable index: {}", e))
                })?;
                result.variables.push(VarInfo {
                    index,
                    name: parts[1].to_string(),
                    var_type: parts[2].to_string(),
                });
            }
            continue;
        }

        // Header key-value pairs
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "Title" => result.title = value.to_string(),
                "Plotname" => result.plot_name = value.to_string(),
                "Flags" => {
                    result.flags = value.to_string();
                    result.is_complex = value.to_lowercase().contains("complex");
                }
                "No. Variables" => {
                    num_vars = value.parse().map_err(|e| {
                        RawFileError::Parse(format!("Bad num_vars: {}", e))
                    })?;
                }
                "No. Points" => {
                    num_points = value.parse().map_err(|e| {
                        RawFileError::Parse(format!("Bad num_points: {}", e))
                    })?;
                }
                _ => {}
            }
        }
    }

    if num_vars == 0 || num_points == 0 {
        return Err(RawFileError::Format("No variables or points declared".into()));
    }
    if result.variables.len() != num_vars {
        return Err(RawFileError::Format(format!(
            "Header declares {} variables but {} were listed",
            num_vars,
            result.variables.len()
        )));
    }
    if let Some(v) = result.variables.iter().find(|v| v.index >= num_vars) {
        return Err(RawFileError::Format(format!(
            "Variable '{}' declares out-of-range index {} (num_vars = {})",
            v.name, v.index, num_vars
        )));
    }

    // Detect FastAccess layout from flags
    let is_fast_access = result.flags.to_lowercase().contains("fastaccess");

    // Parse data
    if is_binary {
        if is_fast_access {
            parse_binary_data_fast_access(&mut reader, &mut result, num_vars, num_points)?;
        } else {
            parse_binary_data(&mut reader, &mut result, num_vars, num_points)?;
        }
    } else {
        parse_ascii_data(&mut reader, &mut result, num_vars, num_points)?;
    }

    Ok(result)
}

fn parse_binary_data<R: Read>(
    reader: &mut BufReader<R>,
    result: &mut RawData,
    num_vars: usize,
    num_points: usize,
) -> Result<(), RawFileError> {
    if result.is_complex {
        // Complex: each value is 2x f64 (16 bytes)
        result.complex_data = (0..num_vars).map(|_| Vec::with_capacity(num_points)).collect();
        let mut buf = [0u8; 16];
        for _point in 0..num_points {
            for var in 0..num_vars {
                reader.read_exact(&mut buf)?;
                let re = f64::from_le_bytes(buf[0..8].try_into().unwrap());
                let im = f64::from_le_bytes(buf[8..16].try_into().unwrap());
                result.complex_data[var].push(Complex64::new(re, im));
            }
        }
        // Also populate real_data with magnitudes for convenience
        result.real_data = result.complex_data.iter()
            .map(|v| v.iter().map(|c| c.re).collect())
            .collect();
    } else {
        // Real: each value is 1x f64 (8 bytes)
        result.real_data = (0..num_vars).map(|_| Vec::with_capacity(num_points)).collect();
        let mut buf = [0u8; 8];
        for _point in 0..num_points {
            for var in 0..num_vars {
                reader.read_exact(&mut buf)?;
                let val = f64::from_le_bytes(buf);
                result.real_data[var].push(val);
            }
        }
    }

    Ok(())
}

/// Parse binary data in FastAccess (column-major) layout.
///
/// FastAccess layout stores all points for variable 0, then all points for
/// variable 1, etc. — as opposed to the normal interleaved (row-major) layout.
fn parse_binary_data_fast_access<R: Read>(
    reader: &mut BufReader<R>,
    result: &mut RawData,
    num_vars: usize,
    num_points: usize,
) -> Result<(), RawFileError> {
    if result.is_complex {
        result.complex_data = (0..num_vars).map(|_| Vec::with_capacity(num_points)).collect();
        let mut buf = [0u8; 16];
        for var in 0..num_vars {
            for _point in 0..num_points {
                reader.read_exact(&mut buf)?;
                let re = f64::from_le_bytes(buf[0..8].try_into().unwrap());
                let im = f64::from_le_bytes(buf[8..16].try_into().unwrap());
                result.complex_data[var].push(Complex64::new(re, im));
            }
        }
        result.real_data = result.complex_data.iter()
            .map(|v| v.iter().map(|c| c.re).collect())
            .collect();
    } else {
        result.real_data = (0..num_vars).map(|_| Vec::with_capacity(num_points)).collect();
        let mut buf = [0u8; 8];
        for var in 0..num_vars {
            for _point in 0..num_points {
                reader.read_exact(&mut buf)?;
                let val = f64::from_le_bytes(buf);
                result.real_data[var].push(val);
            }
        }
    }

    Ok(())
}

fn parse_ascii_data<R: BufRead>(
    reader: &mut R,
    result: &mut RawData,
    num_vars: usize,
    num_points: usize,
) -> Result<(), RawFileError> {
    result.real_data = (0..num_vars).map(|_| Vec::with_capacity(num_points)).collect();
    if result.is_complex {
        result.complex_data = (0..num_vars).map(|_| Vec::with_capacity(num_points)).collect();
    }

    let mut line = String::new();
    for _point in 0..num_points {
        for var in 0..num_vars {
            loop {
                line.clear();
                reader.read_line(&mut line)?;
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    break;
                }
            }
            let trimmed = line.trim();

            // ASCII format: "index\tvalue" or "index\treal,imag"
            let value_str = if let Some((_idx, val)) = trimmed.split_once('\t') {
                val.trim()
            } else {
                trimmed
            };

            if result.is_complex {
                let (re, im) = if let Some((r, i)) = value_str.split_once(',') {
                    let re: f64 = r.trim().parse().map_err(|e| {
                        RawFileError::Parse(format!("Bad complex real: {}", e))
                    })?;
                    let im: f64 = i.trim().parse().map_err(|e| {
                        RawFileError::Parse(format!("Bad complex imag: {}", e))
                    })?;
                    (re, im)
                } else {
                    let re: f64 = value_str.parse().map_err(|e| {
                        RawFileError::Parse(format!("Bad value: {}", e))
                    })?;
                    (re, 0.0)
                };
                result.complex_data[var].push(Complex64::new(re, im));
                result.real_data[var].push(re);
            } else {
                let val: f64 = value_str.parse().map_err(|e| {
                    RawFileError::Parse(format!("Bad value '{}': {}", value_str, e))
                })?;
                result.real_data[var].push(val);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ascii_raw() {
        let raw_content = b"Title: test\n\
Plotname: Operating Point\n\
Flags: real\n\
No. Variables: 2\n\
No. Points: 1\n\
Variables:\n\
\t0\tv(out)\tvoltage\n\
\t1\tv(in)\tvoltage\n\
Values:\n\
0\t3.300000e+00\n\
\t1.000000e+00\n";

        let result = parse_raw(raw_content).unwrap();
        assert_eq!(result.title, "test");
        assert_eq!(result.variables.len(), 2);
        assert_eq!(result.real_data.len(), 2);
        assert!((result.real_data[0][0] - 3.3).abs() < 1e-10);
        assert!((result.real_data[1][0] - 1.0).abs() < 1e-10);
    }

    /// Build a synthetic binary raw file in normal (row-major/interleaved) layout.
    #[allow(clippy::needless_range_loop)]
    fn build_binary_raw(flags: &str, vars: &[(&str, &str)], data: &[Vec<f64>]) -> Vec<u8> {
        let num_vars = vars.len();
        let num_points = data[0].len();
        let mut buf = Vec::new();

        // Header
        buf.extend_from_slice("Title: test_binary\n".to_string().as_bytes());
        buf.extend_from_slice("Plotname: Transient Analysis\n".to_string().as_bytes());
        buf.extend_from_slice(format!("Flags: {}\n", flags).as_bytes());
        buf.extend_from_slice(format!("No. Variables: {}\n", num_vars).as_bytes());
        buf.extend_from_slice(format!("No. Points: {}\n", num_points).as_bytes());
        buf.extend_from_slice(b"Variables:\n");
        for (i, (name, vtype)) in vars.iter().enumerate() {
            buf.extend_from_slice(format!("\t{}\t{}\t{}\n", i, name, vtype).as_bytes());
        }
        buf.extend_from_slice(b"Binary:\n");

        // Data: row-major (point0_var0, point0_var1, ..., point1_var0, ...)
        for point in 0..num_points {
            for var in 0..num_vars {
                buf.extend_from_slice(&data[var][point].to_le_bytes());
            }
        }

        buf
    }

    /// Build a synthetic binary raw file in FastAccess (column-major) layout.
    #[allow(clippy::needless_range_loop)]
    fn build_fast_access_raw(vars: &[(&str, &str)], data: &[Vec<f64>]) -> Vec<u8> {
        let num_vars = vars.len();
        let num_points = data[0].len();
        let mut buf = Vec::new();

        // Header — note "fastaccess" in flags
        buf.extend_from_slice(b"Title: test_fastaccess\n");
        buf.extend_from_slice(b"Plotname: Transient Analysis\n");
        buf.extend_from_slice(b"Flags: real fastaccess\n");
        buf.extend_from_slice(format!("No. Variables: {}\n", num_vars).as_bytes());
        buf.extend_from_slice(format!("No. Points: {}\n", num_points).as_bytes());
        buf.extend_from_slice(b"Variables:\n");
        for (i, (name, vtype)) in vars.iter().enumerate() {
            buf.extend_from_slice(format!("\t{}\t{}\t{}\n", i, name, vtype).as_bytes());
        }
        buf.extend_from_slice(b"Binary:\n");

        // Data: column-major (var0_point0, var0_point1, ..., var1_point0, ...)
        for var in 0..num_vars {
            for point in 0..num_points {
                buf.extend_from_slice(&data[var][point].to_le_bytes());
            }
        }

        buf
    }

    #[test]
    fn test_parse_binary_normal_layout() {
        let vars = vec![("time", "time"), ("v(out)", "voltage")];
        let time_data = vec![0.0, 1e-6, 2e-6, 3e-6];
        let vout_data = vec![0.0, 1.0, 2.0, 3.0];
        let data = vec![time_data.clone(), vout_data.clone()];

        let raw_bytes = build_binary_raw("real", &vars, &data);
        let result = parse_raw(&raw_bytes).unwrap();

        assert_eq!(result.title, "test_binary");
        assert_eq!(result.variables.len(), 2);
        assert_eq!(result.real_data.len(), 2);
        assert_eq!(result.real_data[0].len(), 4);
        assert_eq!(result.real_data[1].len(), 4);

        for i in 0..4 {
            assert!((result.real_data[0][i] - time_data[i]).abs() < 1e-15);
            assert!((result.real_data[1][i] - vout_data[i]).abs() < 1e-15);
        }
    }

    #[test]
    fn test_parse_binary_fast_access_layout() {
        let vars = vec![("time", "time"), ("v(out)", "voltage"), ("v(in)", "voltage")];
        let time_data = vec![0.0, 1e-6, 2e-6, 3e-6, 4e-6];
        let vout_data = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        let vin_data = vec![3.3, 3.3, 3.3, 3.3, 3.3];
        let data = vec![time_data.clone(), vout_data.clone(), vin_data.clone()];

        let raw_bytes = build_fast_access_raw(&vars, &data);
        let result = parse_raw(&raw_bytes).unwrap();

        assert_eq!(result.title, "test_fastaccess");
        assert!(result.flags.contains("fastaccess"));
        assert_eq!(result.variables.len(), 3);
        assert_eq!(result.real_data.len(), 3);

        // Verify all data was parsed correctly from column-major layout
        for i in 0..5 {
            assert!(
                (result.real_data[0][i] - time_data[i]).abs() < 1e-15,
                "time[{}]: expected {}, got {}", i, time_data[i], result.real_data[0][i]
            );
            assert!(
                (result.real_data[1][i] - vout_data[i]).abs() < 1e-15,
                "vout[{}]: expected {}, got {}", i, vout_data[i], result.real_data[1][i]
            );
            assert!(
                (result.real_data[2][i] - vin_data[i]).abs() < 1e-15,
                "vin[{}]: expected {}, got {}", i, vin_data[i], result.real_data[2][i]
            );
        }
    }

    #[test]
    fn test_fast_access_vs_normal_same_data() {
        // Build both layouts with the same data and verify they parse identically
        let vars = vec![("time", "time"), ("v(out)", "voltage")];
        let time_data = vec![0.0, 0.001, 0.002];
        let vout_data = vec![1.1, 2.2, 3.3];
        let data = vec![time_data.clone(), vout_data.clone()];

        let normal_bytes = build_binary_raw("real", &vars, &data);
        let fast_bytes = build_fast_access_raw(&vars, &data);

        let normal_result = parse_raw(&normal_bytes).unwrap();
        let fast_result = parse_raw(&fast_bytes).unwrap();

        assert_eq!(normal_result.real_data.len(), fast_result.real_data.len());
        for var in 0..2 {
            assert_eq!(normal_result.real_data[var].len(), fast_result.real_data[var].len());
            for pt in 0..3 {
                assert!(
                    (normal_result.real_data[var][pt] - fast_result.real_data[var][pt]).abs() < 1e-15,
                    "Mismatch at var={} pt={}", var, pt
                );
            }
        }
    }

    #[test]
    fn test_fast_access_complex_data() {
        let num_vars = 2;
        let num_points = 3;
        // Build a complex FastAccess raw file manually
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Title: complex_fastaccess\n");
        buf.extend_from_slice(b"Plotname: AC Analysis\n");
        buf.extend_from_slice(b"Flags: complex fastaccess\n");
        buf.extend_from_slice(format!("No. Variables: {}\n", num_vars).as_bytes());
        buf.extend_from_slice(format!("No. Points: {}\n", num_points).as_bytes());
        buf.extend_from_slice(b"Variables:\n");
        buf.extend_from_slice(b"\t0\tfrequency\tfrequency\n");
        buf.extend_from_slice(b"\t1\tv(out)\tvoltage\n");
        buf.extend_from_slice(b"Binary:\n");

        // Column-major complex data: (re, im) pairs
        // var 0 (frequency): (1000,0), (10000,0), (100000,0)
        let freq_data: Vec<(f64, f64)> = vec![(1000.0, 0.0), (10000.0, 0.0), (100000.0, 0.0)];
        // var 1 (v(out)): (0.99, -0.01), (0.95, -0.05), (0.80, -0.10)
        let vout_data: Vec<(f64, f64)> = vec![(0.99, -0.01), (0.95, -0.05), (0.80, -0.10)];

        // Write var 0 points, then var 1 points
        for &(re, im) in &freq_data {
            buf.extend_from_slice(&re.to_le_bytes());
            buf.extend_from_slice(&im.to_le_bytes());
        }
        for &(re, im) in &vout_data {
            buf.extend_from_slice(&re.to_le_bytes());
            buf.extend_from_slice(&im.to_le_bytes());
        }

        let result = parse_raw(&buf).unwrap();
        assert!(result.is_complex);
        assert!(result.flags.contains("fastaccess"));
        assert_eq!(result.complex_data.len(), 2);
        assert_eq!(result.complex_data[0].len(), 3);
        assert_eq!(result.complex_data[1].len(), 3);

        // Verify frequency data
        assert!((result.complex_data[0][0].re - 1000.0).abs() < 1e-10);
        assert!((result.complex_data[0][1].re - 10000.0).abs() < 1e-10);
        assert!((result.complex_data[0][2].re - 100000.0).abs() < 1e-10);

        // Verify v(out) complex data
        assert!((result.complex_data[1][0].re - 0.99).abs() < 1e-10);
        assert!((result.complex_data[1][0].im - (-0.01)).abs() < 1e-10);
        assert!((result.complex_data[1][2].re - 0.80).abs() < 1e-10);
        assert!((result.complex_data[1][2].im - (-0.10)).abs() < 1e-10);
    }
}
