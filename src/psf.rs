//! Parser for Cadence Spectre PSF (Parameter Storage Format) binary files.
//!
//! PSF is produced when Spectre runs without `-format nutbin`. The format is:
//!   - Big-endian byte order throughout
//!   - Magic: "Clarissa" (8 bytes)
//!   - Sections identified by 4-byte section IDs
//!   - Sections: HEADER, TYPE, SWEEP, TRACE, VALUE
//!   - Data types: real (f64, 8 bytes) or complex (2x f64, 16 bytes)
//!
//! This parser handles:
//!   - Non-swept PSF (e.g., .op results) -- single values per trace
//!   - Swept PSF (e.g., .ac, .tran results) -- arrays per trace
//!   - Real and complex data types

use num_complex::Complex64;
use crate::result::{RawData, VarInfo};

// ── Section IDs (big-endian u32) ──

const SECTION_HEADER: u32 = 0x00000000;
const SECTION_TYPE: u32   = 0x00000001;
const SECTION_SWEEP: u32  = 0x00000002;
const SECTION_TRACE: u32  = 0x00000003;
const SECTION_VALUE: u32  = 0x00000004;

// ── PSF type IDs ──

const PSF_TYPE_COMPLEX: u32 = 0x0000000C;  // pair of 64-bit IEEE doubles

#[cfg(test)]
const PSF_TYPE_REAL: u32    = 0x0000000B;  // 64-bit IEEE double

const PSF_MAGIC: &[u8] = b"Clarissa";

#[derive(Debug, thiserror::Error)]
pub enum PsfError {
    #[error("PSF parse error: {0}")]
    Parse(String),
    #[error("Bad magic: expected 'Clarissa' header")]
    BadMagic,
    #[error("Unexpected EOF at offset {0}")]
    UnexpectedEof(usize),
    #[error("Unknown type ID: 0x{0:08X}")]
    UnknownType(u32),
}

/// A cursor over a byte slice with big-endian reads.
struct PsfReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> PsfReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], PsfError> {
        if self.pos + n > self.data.len() {
            return Err(PsfError::UnexpectedEof(self.pos));
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u32(&mut self) -> Result<u32, PsfError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
    }

    fn read_i32(&mut self) -> Result<i32, PsfError> {
        let bytes = self.read_bytes(4)?;
        Ok(i32::from_be_bytes(bytes.try_into().unwrap()))
    }

    fn read_f64(&mut self) -> Result<f64, PsfError> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_be_bytes(bytes.try_into().unwrap()))
    }

    fn read_complex64(&mut self) -> Result<Complex64, PsfError> {
        let re = self.read_f64()?;
        let im = self.read_f64()?;
        Ok(Complex64::new(re, im))
    }

    /// Read a PSF string: 4-byte length prefix (big-endian) + UTF-8 bytes + padding to 4-byte boundary.
    fn read_string(&mut self) -> Result<String, PsfError> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;
        let s = String::from_utf8_lossy(bytes).to_string();
        // Pad to 4-byte alignment
        let pad = (4 - (len % 4)) % 4;
        if pad > 0 {
            self.read_bytes(pad)?;
        }
        Ok(s)
    }

    /// Skip n bytes.
    fn skip(&mut self, n: usize) -> Result<(), PsfError> {
        if self.pos + n > self.data.len() {
            return Err(PsfError::UnexpectedEof(self.pos));
        }
        self.pos += n;
        Ok(())
    }
}

// ── Internal types for parsing ──

#[derive(Debug, Clone)]
struct PsfTraceInfo {
    name: String,
    type_id: u32,
}

#[derive(Debug, Clone)]
struct PsfSweepInfo {
    name: String,
    type_id: u32,
}

/// Read a PSF property from the stream.
/// PSF properties: string key, type byte, then value.
/// Returns (key, value_as_string).
fn read_property(reader: &mut PsfReader<'_>) -> Result<Option<(String, String)>, PsfError> {
    if reader.remaining() < 4 {
        return Ok(None);
    }
    let key = reader.read_string()?;
    if key.is_empty() {
        return Ok(None);
    }
    let prop_type = reader.read_u32()?;
    let value = match prop_type {
        // String property
        0x21 | 0x22 => reader.read_string()?,
        // Integer property
        0x01 => {
            let v = reader.read_i32()?;
            format!("{}", v)
        }
        // Real property
        0x0B => {
            let v = reader.read_f64()?;
            format!("{}", v)
        }
        _ => {
            // Unknown property type -- try to skip gracefully
            return Ok(Some((key, String::new())));
        }
    };
    Ok(Some((key, value)))
}

/// Parse a PSF binary file and convert to our RawData format.
pub fn parse_psf(data: &[u8]) -> Result<RawData, PsfError> {
    if data.len() < 8 {
        return Err(PsfError::BadMagic);
    }

    let mut reader = PsfReader::new(data);

    // Validate magic
    let magic = reader.read_bytes(8)?;
    if magic != PSF_MAGIC {
        return Err(PsfError::BadMagic);
    }

    let mut header_props: Vec<(String, String)> = Vec::new();
    let mut traces: Vec<PsfTraceInfo> = Vec::new();
    let mut sweeps: Vec<PsfSweepInfo> = Vec::new();
    let mut is_swept = false;

    // Skip version/file-type info after magic (typically 4 bytes)
    if reader.remaining() >= 4 {
        let _version = reader.read_u32()?;
    }

    // Parse sections. PSF files have section markers followed by section data.
    // We scan for section boundaries.
    while reader.remaining() >= 8 {
        let section_id = reader.read_u32()?;
        let section_end_marker = reader.read_u32()?;

        match section_id {
            SECTION_HEADER => {
                // Header section: key-value properties until we hit the end marker
                // The end marker for this section is the next section_id boundary.
                // Read properties until we can't anymore or hit a known section marker.
                loop {
                    if reader.remaining() < 8 {
                        break;
                    }
                    // Peek at next 4 bytes to see if it's a section marker
                    let peek = u32::from_be_bytes(
                        reader.data[reader.pos..reader.pos + 4].try_into().unwrap()
                    );
                    // If we see a known section ID, we're done with this section
                    if peek <= SECTION_VALUE && peek > SECTION_HEADER {
                        break;
                    }
                    match read_property(&mut reader)? {
                        Some((k, v)) => header_props.push((k, v)),
                        None => break,
                    }
                }
            }
            SECTION_TYPE => {
                // Type section defines named data types.
                // For now we skip this -- we get type info from traces.
                // Type defs are: id, name, type_id, properties...
                // We skip to next section by scanning.
                loop {
                    if reader.remaining() < 4 {
                        break;
                    }
                    let peek = u32::from_be_bytes(
                        reader.data[reader.pos..reader.pos + 4].try_into().unwrap()
                    );
                    if peek == SECTION_SWEEP || peek == SECTION_TRACE || peek == SECTION_VALUE {
                        break;
                    }
                    // Skip one word
                    reader.skip(4)?;
                }
            }
            SECTION_SWEEP => {
                // Sweep section defines the sweep variable (time, frequency, etc.)
                is_swept = true;
                // Read sweep definitions
                loop {
                    if reader.remaining() < 4 {
                        break;
                    }
                    let peek = u32::from_be_bytes(
                        reader.data[reader.pos..reader.pos + 4].try_into().unwrap()
                    );
                    if peek == SECTION_TRACE || peek == SECTION_VALUE {
                        break;
                    }
                    // Each sweep def: id(4) + name(str) + type_id(4) + properties
                    let _sweep_id = reader.read_u32()?;
                    if reader.remaining() < 4 {
                        break;
                    }
                    let name = reader.read_string()?;
                    if reader.remaining() < 4 {
                        break;
                    }
                    let type_id = reader.read_u32()?;
                    sweeps.push(PsfSweepInfo { name, type_id });

                    // Skip any trailing properties for this sweep definition
                    skip_properties(&mut reader)?;
                }
            }
            SECTION_TRACE => {
                // Trace section defines output variables (names + types)
                loop {
                    if reader.remaining() < 4 {
                        break;
                    }
                    let peek = u32::from_be_bytes(
                        reader.data[reader.pos..reader.pos + 4].try_into().unwrap()
                    );
                    if peek == SECTION_VALUE {
                        break;
                    }
                    // Each trace: id(4) + name(str) + type_id(4) + properties
                    let _trace_id = reader.read_u32()?;
                    if reader.remaining() < 4 {
                        break;
                    }
                    let name = reader.read_string()?;
                    if reader.remaining() < 4 {
                        break;
                    }
                    let type_id = reader.read_u32()?;
                    traces.push(PsfTraceInfo { name, type_id });

                    // Skip any trailing properties
                    skip_properties(&mut reader)?;
                }
            }
            SECTION_VALUE => {
                // Value section contains the actual data.
                // For non-swept: one value per trace.
                // For swept: interleaved sweep_value + trace_values repeated.
                let _ = section_end_marker; // total byte count or point count
                return parse_value_section(
                    &mut reader,
                    &header_props,
                    &sweeps,
                    &traces,
                    is_swept,
                );
            }
            _ => {
                // Unknown section -- skip 4 bytes and hope for the best
                // (the section_end_marker we already consumed may have been data)
            }
        }
    }

    // If we got here without hitting VALUE section, build result from what we have
    build_empty_result(&header_props, &sweeps, &traces)
}

/// Skip properties until we see a section boundary or a trace/sweep ID marker.
fn skip_properties(reader: &mut PsfReader<'_>) -> Result<(), PsfError> {
    // PSF properties after a trace/sweep def have various formats.
    // We use a heuristic: keep reading until the next word looks like
    // a known section ID or another trace/sweep definition.
    // This is fragile but works for common PSF files.
    loop {
        if reader.remaining() < 4 {
            break;
        }
        let peek = u32::from_be_bytes(
            reader.data[reader.pos..reader.pos + 4].try_into().unwrap()
        );
        // Known section boundaries
        if peek <= SECTION_VALUE {
            break;
        }
        // If the word is small enough to be another trace/sweep ID, bail
        // Heuristic: property strings have length >= 1, so a word in 1..256 range
        // could be a string length. We attempt to read one property.
        match read_property(reader) {
            Ok(Some(_)) => continue,
            _ => break,
        }
    }
    Ok(())
}

fn parse_value_section(
    reader: &mut PsfReader<'_>,
    header_props: &[(String, String)],
    sweeps: &[PsfSweepInfo],
    traces: &[PsfTraceInfo],
    is_swept: bool,
) -> Result<RawData, PsfError> {
    let has_complex = traces.iter().any(|t| t.type_id == PSF_TYPE_COMPLEX);

    // Determine plot_name from header properties
    let plot_name = header_props
        .iter()
        .find(|(k, _)| k == "PSFversion" || k == "simulator" || k == "analysis")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let title = header_props
        .iter()
        .find(|(k, _)| k == "title" || k == "design")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    if is_swept {
        parse_swept_values(reader, title, plot_name, sweeps, traces, has_complex)
    } else {
        parse_nonsweep_values(reader, title, plot_name, traces, has_complex)
    }
}

/// Parse non-swept PSF data (e.g., .op results): one value per trace.
fn parse_nonsweep_values(
    reader: &mut PsfReader<'_>,
    title: String,
    plot_name: String,
    traces: &[PsfTraceInfo],
    has_complex: bool,
) -> Result<RawData, PsfError> {
    let num_vars = traces.len();
    let mut variables = Vec::with_capacity(num_vars);
    let mut real_data = Vec::with_capacity(num_vars);
    let mut complex_data = if has_complex {
        Vec::with_capacity(num_vars)
    } else {
        Vec::new()
    };

    for (i, trace) in traces.iter().enumerate() {
        let var_type = infer_var_type(&trace.name);
        variables.push(VarInfo {
            index: i,
            name: trace.name.clone(),
            var_type,
        });

        match trace.type_id {
            PSF_TYPE_COMPLEX => {
                if reader.remaining() >= 16 {
                    let c = reader.read_complex64()?;
                    real_data.push(vec![c.re]);
                    if has_complex {
                        complex_data.push(vec![c]);
                    }
                } else {
                    real_data.push(vec![0.0]);
                    if has_complex {
                        complex_data.push(vec![Complex64::new(0.0, 0.0)]);
                    }
                }
            }
            _ => {
                // Default to real
                if reader.remaining() >= 8 {
                    let v = reader.read_f64()?;
                    real_data.push(vec![v]);
                    if has_complex {
                        complex_data.push(vec![Complex64::new(v, 0.0)]);
                    }
                } else {
                    real_data.push(vec![0.0]);
                    if has_complex {
                        complex_data.push(vec![Complex64::new(0.0, 0.0)]);
                    }
                }
            }
        }
    }

    Ok(RawData {
        title,
        plot_name,
        flags: if has_complex { "complex".to_string() } else { "real".to_string() },
        variables,
        real_data,
        complex_data,
        is_complex: has_complex,
        stdout: String::new(),
        log_content: String::new(),
        measures: Vec::new(),
        backend_hint: String::new(),
    })
}

/// Parse swept PSF data (e.g., .ac, .tran results): arrays per trace.
fn parse_swept_values(
    reader: &mut PsfReader<'_>,
    title: String,
    plot_name: String,
    sweeps: &[PsfSweepInfo],
    traces: &[PsfTraceInfo],
    has_complex: bool,
) -> Result<RawData, PsfError> {
    // Total variables = sweep vars + trace vars
    let num_sweep = sweeps.len();
    let num_trace = traces.len();
    let total_vars = num_sweep + num_trace;

    let bytes_per_point = compute_bytes_per_point(sweeps, traces);
    let est_points = reader.remaining().checked_div(bytes_per_point).unwrap_or(0);

    let mut variables = Vec::with_capacity(total_vars);
    let mut real_data: Vec<Vec<f64>> =
        (0..total_vars).map(|_| Vec::with_capacity(est_points)).collect();
    let mut complex_data: Vec<Vec<Complex64>> = if has_complex {
        (0..total_vars).map(|_| Vec::with_capacity(est_points)).collect()
    } else {
        Vec::new()
    };

    // Build variable info: sweep vars first, then trace vars
    for (i, sweep) in sweeps.iter().enumerate() {
        let var_type = infer_var_type(&sweep.name);
        variables.push(VarInfo {
            index: i,
            name: sweep.name.clone(),
            var_type,
        });
    }
    for (i, trace) in traces.iter().enumerate() {
        let var_type = infer_var_type(&trace.name);
        variables.push(VarInfo {
            index: num_sweep + i,
            name: trace.name.clone(),
            var_type,
        });
    }

    // Read data points: each point has sweep values followed by trace values
    // We read until EOF or not enough data for a full point.
    while reader.remaining() >= bytes_per_point {
        // Read sweep values (always real)
        for (i, sweep) in sweeps.iter().enumerate() {
            let val = match sweep.type_id {
                PSF_TYPE_COMPLEX => {
                    let c = reader.read_complex64()?;
                    if has_complex {
                        complex_data[i].push(c);
                    }
                    c.re
                }
                _ => {
                    let v = reader.read_f64()?;
                    if has_complex {
                        complex_data[i].push(Complex64::new(v, 0.0));
                    }
                    v
                }
            };
            real_data[i].push(val);
        }

        // Read trace values
        for (j, trace) in traces.iter().enumerate() {
            let idx = num_sweep + j;
            match trace.type_id {
                PSF_TYPE_COMPLEX => {
                    let c = reader.read_complex64()?;
                    real_data[idx].push(c.re);
                    if has_complex {
                        complex_data[idx].push(c);
                    }
                }
                _ => {
                    let v = reader.read_f64()?;
                    real_data[idx].push(v);
                    if has_complex {
                        complex_data[idx].push(Complex64::new(v, 0.0));
                    }
                }
            }
        }
    }

    Ok(RawData {
        title,
        plot_name,
        flags: if has_complex { "complex".to_string() } else { "real".to_string() },
        variables,
        real_data,
        complex_data,
        is_complex: has_complex,
        stdout: String::new(),
        log_content: String::new(),
        measures: Vec::new(),
        backend_hint: String::new(),
    })
}

fn compute_bytes_per_point(sweeps: &[PsfSweepInfo], traces: &[PsfTraceInfo]) -> usize {
    let mut total = 0;
    for s in sweeps {
        total += if s.type_id == PSF_TYPE_COMPLEX { 16 } else { 8 };
    }
    for t in traces {
        total += if t.type_id == PSF_TYPE_COMPLEX { 16 } else { 8 };
    }
    // Minimum 8 bytes so we don't loop forever on empty trace lists
    total.max(8)
}

fn build_empty_result(
    header_props: &[(String, String)],
    sweeps: &[PsfSweepInfo],
    traces: &[PsfTraceInfo],
) -> Result<RawData, PsfError> {
    let has_complex = traces.iter().any(|t| t.type_id == PSF_TYPE_COMPLEX);

    let title = header_props
        .iter()
        .find(|(k, _)| k == "title" || k == "design")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let total_vars = sweeps.len() + traces.len();
    let mut variables = Vec::with_capacity(total_vars);

    for (i, sweep) in sweeps.iter().enumerate() {
        variables.push(VarInfo {
            index: i,
            name: sweep.name.clone(),
            var_type: infer_var_type(&sweep.name),
        });
    }
    for (i, trace) in traces.iter().enumerate() {
        variables.push(VarInfo {
            index: sweeps.len() + i,
            name: trace.name.clone(),
            var_type: infer_var_type(&trace.name),
        });
    }

    Ok(RawData {
        title,
        plot_name: String::new(),
        flags: if has_complex { "complex".to_string() } else { "real".to_string() },
        variables,
        real_data: vec![Vec::new(); total_vars],
        complex_data: if has_complex { vec![Vec::new(); total_vars] } else { Vec::new() },
        is_complex: has_complex,
        stdout: String::new(),
        log_content: String::new(),
        measures: Vec::new(),
        backend_hint: String::new(),
    })
}

/// Infer SPICE variable type from the trace name.
fn infer_var_type(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower == "time" || lower == "t" {
        "time".to_string()
    } else if lower == "frequency" || lower == "freq" || lower == "f" {
        "frequency".to_string()
    } else if lower.starts_with("i(") || lower.starts_with("i_") || lower == "current" {
        "current".to_string()
    } else {
        "voltage".to_string()
    }
}

/// Check if data looks like a PSF file (starts with "Clarissa").
pub fn is_psf(data: &[u8]) -> bool {
    data.len() >= 8 && &data[..8] == PSF_MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bad_magic() {
        let data = b"NotPSFxx";
        let result = parse_psf(data);
        assert!(matches!(result, Err(PsfError::BadMagic)));
    }

    #[test]
    fn test_too_short() {
        let data = b"Clar";
        let result = parse_psf(data);
        assert!(matches!(result, Err(PsfError::BadMagic)));
    }

    #[test]
    fn test_is_psf() {
        assert!(is_psf(b"Clarissa\x00\x00\x00\x01"));
        assert!(!is_psf(b"Title: test\n"));
        assert!(!is_psf(b"short"));
    }

    #[test]
    fn test_infer_var_type() {
        assert_eq!(infer_var_type("time"), "time");
        assert_eq!(infer_var_type("frequency"), "frequency");
        assert_eq!(infer_var_type("freq"), "frequency");
        assert_eq!(infer_var_type("I(Vin)"), "current");
        assert_eq!(infer_var_type("v(out)"), "voltage");
        assert_eq!(infer_var_type("out"), "voltage");
    }

    #[test]
    fn test_reader_basics() {
        let data = [
            0x00, 0x00, 0x00, 0x2A, // u32 = 42
            0x40, 0x09, 0x21, 0xFB, 0x54, 0x44, 0x2D, 0x18, // f64 = pi
        ];
        let mut reader = PsfReader::new(&data);
        assert_eq!(reader.read_u32().unwrap(), 42);
        let pi = reader.read_f64().unwrap();
        assert!((pi - std::f64::consts::PI).abs() < 1e-10);
    }

    #[test]
    fn test_reader_string() {
        // Length 3 "abc" + 1 byte padding
        let data = [
            0x00, 0x00, 0x00, 0x03, // length = 3
            b'a', b'b', b'c',       // string data
            0x00,                    // padding to 4-byte boundary
        ];
        let mut reader = PsfReader::new(&data);
        let s = reader.read_string().unwrap();
        assert_eq!(s, "abc");
        assert_eq!(reader.pos, 8);
    }

    #[test]
    fn test_reader_string_aligned() {
        // Length 4 "abcd" -- no padding needed
        let data = [
            0x00, 0x00, 0x00, 0x04, // length = 4
            b'a', b'b', b'c', b'd', // string data (already aligned)
        ];
        let mut reader = PsfReader::new(&data);
        let s = reader.read_string().unwrap();
        assert_eq!(s, "abcd");
        assert_eq!(reader.pos, 8);
    }

    /// Build a minimal non-swept PSF binary with 2 real traces and verify parsing.
    #[test]
    fn test_parse_minimal_nonsweep_psf() {
        let mut buf: Vec<u8> = Vec::new();

        // Magic
        buf.extend_from_slice(PSF_MAGIC);
        // Version
        buf.extend_from_slice(&1u32.to_be_bytes());

        // HEADER section
        buf.extend_from_slice(&SECTION_HEADER.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes()); // end marker

        // TRACE section (skip TYPE and SWEEP)
        buf.extend_from_slice(&SECTION_TRACE.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());

        // Trace 0: "v(out)", type REAL
        buf.extend_from_slice(&0u32.to_be_bytes()); // trace id
        // String "v(out)" -- length 6, padded to 8
        buf.extend_from_slice(&6u32.to_be_bytes());
        buf.extend_from_slice(b"v(out)");
        buf.extend_from_slice(&[0, 0]); // pad to 8
        buf.extend_from_slice(&PSF_TYPE_REAL.to_be_bytes());

        // Trace 1: "v(in)", type REAL
        buf.extend_from_slice(&1u32.to_be_bytes()); // trace id
        // String "v(in)" -- length 5, padded to 8
        buf.extend_from_slice(&5u32.to_be_bytes());
        buf.extend_from_slice(b"v(in)");
        buf.extend_from_slice(&[0, 0, 0]); // pad to 8
        buf.extend_from_slice(&PSF_TYPE_REAL.to_be_bytes());

        // VALUE section
        buf.extend_from_slice(&SECTION_VALUE.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());

        // Two real values
        buf.extend_from_slice(&3.3f64.to_be_bytes());   // v(out) = 3.3
        buf.extend_from_slice(&1.0f64.to_be_bytes());   // v(in) = 1.0

        let result = parse_psf(&buf).unwrap();
        assert_eq!(result.variables.len(), 2);
        assert_eq!(result.variables[0].name, "v(out)");
        assert_eq!(result.variables[1].name, "v(in)");
        assert!(!result.is_complex);
        assert_eq!(result.real_data.len(), 2);
        assert!((result.real_data[0][0] - 3.3).abs() < 1e-10);
        assert!((result.real_data[1][0] - 1.0).abs() < 1e-10);
    }

    /// Build a swept PSF with frequency sweep + 1 complex trace.
    #[test]
    fn test_parse_swept_complex_psf() {
        let mut buf: Vec<u8> = Vec::new();

        // Magic + version
        buf.extend_from_slice(PSF_MAGIC);
        buf.extend_from_slice(&1u32.to_be_bytes());

        // HEADER
        buf.extend_from_slice(&SECTION_HEADER.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());

        // SWEEP section
        buf.extend_from_slice(&SECTION_SWEEP.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());

        // Sweep var: "frequency", type REAL
        buf.extend_from_slice(&0u32.to_be_bytes()); // sweep id
        buf.extend_from_slice(&9u32.to_be_bytes()); // string length
        buf.extend_from_slice(b"frequency");
        buf.extend_from_slice(&[0, 0, 0]); // pad to 12
        buf.extend_from_slice(&PSF_TYPE_REAL.to_be_bytes());

        // TRACE section
        buf.extend_from_slice(&SECTION_TRACE.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());

        // Trace 0: "v(out)", type COMPLEX
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(&6u32.to_be_bytes());
        buf.extend_from_slice(b"v(out)");
        buf.extend_from_slice(&[0, 0]); // pad
        buf.extend_from_slice(&PSF_TYPE_COMPLEX.to_be_bytes());

        // VALUE section
        buf.extend_from_slice(&SECTION_VALUE.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());

        // Point 0: freq=1000.0, v(out) = (1.5 + 0.5j)
        buf.extend_from_slice(&1000.0f64.to_be_bytes());
        buf.extend_from_slice(&1.5f64.to_be_bytes());
        buf.extend_from_slice(&0.5f64.to_be_bytes());

        // Point 1: freq=2000.0, v(out) = (1.2 + 0.3j)
        buf.extend_from_slice(&2000.0f64.to_be_bytes());
        buf.extend_from_slice(&1.2f64.to_be_bytes());
        buf.extend_from_slice(&0.3f64.to_be_bytes());

        let result = parse_psf(&buf).unwrap();
        assert!(result.is_complex);
        assert_eq!(result.variables.len(), 2); // frequency + v(out)
        assert_eq!(result.variables[0].name, "frequency");
        assert_eq!(result.variables[0].var_type, "frequency");
        assert_eq!(result.variables[1].name, "v(out)");

        // Check data
        assert_eq!(result.real_data.len(), 2);
        assert_eq!(result.real_data[0].len(), 2); // 2 points
        assert!((result.real_data[0][0] - 1000.0).abs() < 1e-10);
        assert!((result.real_data[0][1] - 2000.0).abs() < 1e-10);

        // Complex data for v(out)
        assert_eq!(result.complex_data.len(), 2);
        assert!((result.complex_data[1][0].re - 1.5).abs() < 1e-10);
        assert!((result.complex_data[1][0].im - 0.5).abs() < 1e-10);
        assert!((result.complex_data[1][1].re - 1.2).abs() < 1e-10);
        assert!((result.complex_data[1][1].im - 0.3).abs() < 1e-10);
    }
}
