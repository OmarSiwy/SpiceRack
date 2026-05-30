use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::process::Command;
use tempfile::TempDir;
use crate::result::RawData;
use crate::rawfile;
use super::{Backend, BackendCapabilities, BackendError};

/// Vacask subprocess backend: translate SPICE→Vacask, run `vacask`, read .raw
pub struct VacaskSubprocess;

pub const VACASK_CAPS: BackendCapabilities = BackendCapabilities {
    xspice: false,
    osdi: true,
    measures: false,
    step_params: false,
    control_blocks: false,
    laplace_sources: false,
    verilog_cosim: false,
};

impl Backend for VacaskSubprocess {
    fn name(&self) -> &str {
        "vacask"
    }

    fn capabilities(&self) -> BackendCapabilities { VACASK_CAPS }

    fn codegen(&self) -> Box<dyn crate::codegen::CodeGen> {
        Box::new(crate::codegen::vacask::VacaskCodeGen)
    }

    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        let vacask_netlist = spice_to_vacask(netlist);
        self.run_vacask_netlist(&vacask_netlist)
    }

    fn run_netlist(&self, netlist: &str) -> Result<RawData, BackendError> {
        self.run_vacask_netlist(netlist)
    }
}

impl VacaskSubprocess {
    fn run_vacask_netlist(&self, netlist: &str) -> Result<RawData, BackendError> {
        let tmp_dir = TempDir::new()?;
        let sim_path = tmp_dir.path().join("circuit.sim");

        std::fs::write(&sim_path, netlist.as_bytes())?;

        let output = Command::new("vacask")
            .arg(&sim_path)
            .current_dir(tmp_dir.path())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(BackendError::SimulationError(format!(
                "vacask exited with status {}\nstdout: {}\nstderr: {}",
                output.status,
                stdout.chars().take(500).collect::<String>(),
                stderr.chars().take(500).collect::<String>(),
            )));
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();

        // Vacask names output files by analysis name (e.g., "op1.raw", "ac1.raw")
        // Find the first .raw file in the output directory
        let raw_path = find_raw_file(tmp_dir.path())?;
        let raw_bytes = std::fs::read(&raw_path).map_err(|e| {
            BackendError::SimulationError(format!(
                "Failed to read raw file '{}': {}",
                raw_path.display(), e
            ))
        })?;

        let mut result = rawfile::parse_raw(&raw_bytes)?;
        result.stdout = stdout_str;
        Ok(result)
    }
}

fn find_raw_file(dir: &std::path::Path) -> Result<std::path::PathBuf, BackendError> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        BackendError::SimulationError(format!("Failed to read output dir: {}", e))
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "raw") {
            return Ok(path);
        }
    }

    Err(BackendError::SimulationError(
        "No .raw output file produced by vacask".to_string(),
    ))
}

/// Translate a SPICE netlist to Vacask (Spectre-like) format.
///
/// Handles the most common elements. Users needing advanced Vacask features
/// should write Vacask netlists directly.
pub fn spice_to_vacask(spice: &str) -> String {
    let mut out = String::with_capacity(spice.len() * 2);
    let mut analysis_counter: u32 = 0;
    let mut in_subckt = false;

    for line in spice.lines() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        // pre_osdi -> load (Vacask uses OSDI too)
        if trimmed.starts_with("pre_osdi") || trimmed.starts_with("PRE_OSDI") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                out.push_str(&format!("load \"{}\"\n", parts[1]));
            }
            continue;
        }

        // Title line (first non-empty line in SPICE is title)
        if out.is_empty() {
            out.push_str(&format!("// {}\n", trimmed));
            continue;
        }

        // Comments
        if trimmed.starts_with('*') {
            out.push_str(&format!("//{}\n", &trimmed[1..]));
            continue;
        }

        // Continuation lines
        if trimmed.starts_with('+') {
            out.push_str(&format!("+ {}\n", &trimmed[1..].trim()));
            continue;
        }

        // Dot commands
        if trimmed.starts_with('.') {
            let upper = trimmed.to_uppercase();
            if upper.starts_with(".TITLE") {
                out.push_str(&format!("// {}\n", trimmed));
            } else if upper.starts_with(".END") && !upper.starts_with(".ENDS") {
                // .end — skip, vacask doesn't need it
            } else if upper.starts_with(".ENDS") {
                out.push_str(&format!("ends {}\n",
                    if in_subckt { "" } else { "" }
                ));
                in_subckt = false;
            } else if upper.starts_with(".SUBCKT") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3 {
                    let name = parts[1];
                    let pins: Vec<&str> = parts[2..].iter()
                        .take_while(|p| !p.contains('='))
                        .copied()
                        .collect();
                    out.push_str(&format!("subckt {} ({})\n", name, pins.join(" ")));
                    in_subckt = true;
                }
            } else if upper.starts_with(".MODEL") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3 {
                    let name = parts[1];
                    let kind = parts[2];
                    out.push_str(&format!("model {} {}", name, kind));
                    // Convert parenthesized params to key=value
                    let rest = parts[3..].join(" ");
                    let rest = rest.replace('(', " ").replace(')', " ");
                    for param in rest.split_whitespace() {
                        out.push_str(&format!(" {}", param));
                    }
                    out.push('\n');
                }
            } else if upper.starts_with(".INCLUDE") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    out.push_str(&format!("include {}\n", parts[1]));
                }
            } else if upper.starts_with(".LIB") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3 {
                    out.push_str(&format!("include {} section={}\n", parts[1], parts[2]));
                }
            } else if upper.starts_with(".PARAM") {
                let rest = &trimmed[6..].trim();
                out.push_str(&format!("parameters {}\n", rest));
            } else if upper.starts_with(".OPTION") {
                let rest = &trimmed[8..].trim();
                out.push_str(&format!("myopt options {}\n", rest));
            } else if upper.starts_with(".TEMP") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    out.push_str(&format!("myopt options temp={}\n", parts[1]));
                }
            } else if upper.starts_with(".IC") {
                let rest = &trimmed[3..].trim();
                out.push_str(&format!("ic {}\n", rest));
            } else if upper.starts_with(".NODESET") {
                let rest = &trimmed[8..].trim();
                out.push_str(&format!("nodeset {}\n", rest));
            } else if upper.starts_with(".SAVE") {
                let rest = &trimmed[5..].trim();
                out.push_str(&format!("save {}\n", rest));
            } else {
                // Analysis statements
                analysis_counter += 1;
                let aname = format!("an{}", analysis_counter);
                if let Some(vacask_analysis) = translate_analysis(trimmed, &aname) {
                    out.push_str(&vacask_analysis);
                    out.push('\n');
                } else {
                    out.push_str(&format!("// UNTRANSLATED: {}\n", trimmed));
                }
            }
            continue;
        }

        // Component instances
        if let Some(translated) = translate_component(trimmed) {
            out.push_str(&translated);
            out.push('\n');
        } else {
            out.push_str(&format!("// UNTRANSLATED: {}\n", trimmed));
        }
    }

    out
}

fn translate_analysis(line: &str, name: &str) -> Option<String> {
    let upper = line.to_uppercase();
    let parts: Vec<&str> = line.split_whitespace().collect();

    if upper.starts_with(".OP") {
        Some(format!("{} () dc", name))
    } else if upper.starts_with(".AC") {
        // .ac dec|oct|lin N fstart fstop
        if parts.len() >= 5 {
            let sweep_type = parts[1].to_lowercase();
            let n = parts[2];
            let fstart = parts[3];
            let fstop = parts[4];
            Some(format!("{} ac start={} stop={} {}={}", name, fstart, fstop, sweep_type, n))
        } else {
            None
        }
    } else if upper.starts_with(".TRAN") {
        // .tran tstep tstop [tstart [tmax]] [uic]
        if parts.len() >= 3 {
            let tstop = parts[2];
            let mut result = format!("{} tran stop={}", name, tstop);
            if parts.len() >= 4 && !parts[3].eq_ignore_ascii_case("uic") {
                // tstart specified — vacask doesn't have tstart, skip
            }
            if parts.iter().any(|p| p.eq_ignore_ascii_case("uic")) {
                result.push_str(" ic=all");
            }
            Some(result)
        } else {
            None
        }
    } else if upper.starts_with(".NOISE") {
        // .noise V(out,ref) src dec N fstart fstop
        if parts.len() >= 7 {
            let output = parts[1];
            let src = parts[2];
            let sweep = parts[3].to_lowercase();
            let n = parts[4];
            let fstart = parts[5];
            let fstop = parts[6];
            Some(format!("{} noise {} iprobe={} start={} stop={} {}={}",
                name, output, src, fstart, fstop, sweep, n))
        } else {
            None
        }
    } else if upper.starts_with(".DC") {
        // .dc src start stop step
        if parts.len() >= 5 {
            let dev = parts[1];
            let start = parts[2];
            let stop = parts[3];
            let step = parts[4];
            Some(format!("{} dc dev={} param=dc start={} stop={} step={}",
                name, dev, start, stop, step))
        } else {
            Some(format!("{} dc", name))
        }
    } else if upper.starts_with(".TF") {
        // .tf outvar insrc
        if parts.len() >= 3 {
            let outvar = parts[1];
            let insrc = parts[2];
            Some(format!("{} dcxf {} probe={}", name, outvar, insrc))
        } else {
            None
        }
    } else if upper.starts_with(".SENS") {
        // .sens outvar [ac ...]
        if parts.len() >= 2 {
            Some(format!("// UNTRANSLATED: {}", line))
        } else {
            None
        }
    } else {
        None
    }
}

fn translate_component(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let name = parts[0];
    let prefix = name.chars().next()?.to_ascii_uppercase();

    match prefix {
        'R' => {
            // R1 n1 n2 value [params...]
            if parts.len() >= 4 {
                let (n1, n2) = (parts[1], parts[2]);
                let val = parts[3];
                Some(format!("{} ({} {}) resistor r={}", name.to_lowercase(), n1, n2, val))
            } else {
                None
            }
        }
        'C' => {
            if parts.len() >= 4 {
                let (n1, n2) = (parts[1], parts[2]);
                let val = parts[3];
                Some(format!("{} ({} {}) capacitor c={}", name.to_lowercase(), n1, n2, val))
            } else {
                None
            }
        }
        'L' => {
            if parts.len() >= 4 {
                let (n1, n2) = (parts[1], parts[2]);
                let val = parts[3];
                Some(format!("{} ({} {}) inductor l={}", name.to_lowercase(), n1, n2, val))
            } else {
                None
            }
        }
        'V' => {
            // V1 np nm [DC val] [AC mag [phase]] [SIN(...)] [PULSE(...)]
            if parts.len() >= 3 {
                let (np, nm) = (parts[1], parts[2]);
                let rest = parts[3..].join(" ");
                let dc_val = extract_dc_value(&rest);
                let ac_spec = extract_ac_spec(&rest);
                let tran_spec = extract_tran_spec(&rest);

                let mut result = format!("{} ({} {}) vsource", name.to_lowercase(), np, nm);
                if let Some(dc) = dc_val {
                    result.push_str(&format!(" dc={}", dc));
                }
                if let Some(ac) = ac_spec {
                    result.push_str(&format!(" mag={}", ac));
                }
                if let Some(tran) = tran_spec {
                    result.push_str(&format!(" {}", tran));
                }
                Some(result)
            } else {
                None
            }
        }
        'I' => {
            if parts.len() >= 3 {
                let (np, nm) = (parts[1], parts[2]);
                let rest = parts[3..].join(" ");
                let dc_val = extract_dc_value(&rest);
                let mut result = format!("{} ({} {}) isource", name.to_lowercase(), np, nm);
                if let Some(dc) = dc_val {
                    result.push_str(&format!(" dc={}", dc));
                }
                Some(result)
            } else {
                None
            }
        }
        'D' => {
            // D1 np nm model
            if parts.len() >= 4 {
                let (np, nm, model) = (parts[1], parts[2], parts[3]);
                Some(format!("{} ({} {}) {}", name.to_lowercase(), np, nm, model))
            } else {
                None
            }
        }
        'Q' => {
            // Q1 nc nb ne model
            if parts.len() >= 5 {
                let (nc, nb, ne, model) = (parts[1], parts[2], parts[3], parts[4]);
                Some(format!("{} ({} {} {}) {}", name.to_lowercase(), nc, nb, ne, model))
            } else {
                None
            }
        }
        'M' => {
            // M1 nd ng ns nb model [W=... L=... ...]
            if parts.len() >= 6 {
                let (nd, ng, ns, nb, model) = (parts[1], parts[2], parts[3], parts[4], parts[5]);
                let mut result = format!("{} ({} {} {} {}) {}",
                    name.to_lowercase(), nd, ng, ns, nb, model);
                // Copy remaining key=value params
                for p in &parts[6..] {
                    if p.contains('=') {
                        result.push_str(&format!(" {}", p.to_lowercase()));
                    }
                }
                Some(result)
            } else {
                None
            }
        }
        'X' => {
            // X1 node1 node2 ... subckt_name [params]
            if parts.len() >= 3 {
                // In SPICE, subcircuit name is last non-param token
                let mut nodes = Vec::new();
                let mut subckt_name = "";
                for p in &parts[1..] {
                    if p.contains('=') {
                        break;
                    }
                    if !subckt_name.is_empty() {
                        nodes.push(subckt_name);
                    }
                    subckt_name = p;
                }
                let mut result = format!("{} ({}) {}",
                    name.to_lowercase(), nodes.join(" "), subckt_name);
                // Copy params
                for p in &parts[1..] {
                    if p.contains('=') {
                        result.push_str(&format!(" {}", p));
                    }
                }
                Some(result)
            } else {
                None
            }
        }
        'E' => {
            // E1 np nm ncp ncm gain (VCVS)
            if parts.len() >= 6 {
                let (np, nm, ncp, ncm, gain) = (parts[1], parts[2], parts[3], parts[4], parts[5]);
                Some(format!("{} ({} {} {} {}) vcvs gain={}",
                    name.to_lowercase(), np, nm, ncp, ncm, gain))
            } else {
                None
            }
        }
        'G' => {
            // G1 np nm ncp ncm gm (VCCS)
            if parts.len() >= 6 {
                let (np, nm, ncp, ncm, gm) = (parts[1], parts[2], parts[3], parts[4], parts[5]);
                Some(format!("{} ({} {} {} {}) vccs gain={}",
                    name.to_lowercase(), np, nm, ncp, ncm, gm))
            } else {
                None
            }
        }
        'F' => {
            // F1 np nm vsense gain (CCCS)
            if parts.len() >= 5 {
                let (np, nm, vsense, gain) = (parts[1], parts[2], parts[3], parts[4]);
                Some(format!("{} ({} {}) cccs probe={} gain={}",
                    name.to_lowercase(), np, nm, vsense, gain))
            } else {
                None
            }
        }
        'H' => {
            // H1 np nm vsense transresistance (CCVS)
            if parts.len() >= 5 {
                let (np, nm, vsense, tr) = (parts[1], parts[2], parts[3], parts[4]);
                Some(format!("{} ({} {}) ccvs probe={} gain={}",
                    name.to_lowercase(), np, nm, vsense, tr))
            } else {
                None
            }
        }
        'K' => {
            // K1 L1 L2 coupling
            if parts.len() >= 4 {
                let (l1, l2, coupling) = (parts[1], parts[2], parts[3]);
                Some(format!("{} ({} {}) mutual coupling={}",
                    name.to_lowercase(), l1.to_lowercase(), l2.to_lowercase(), coupling))
            } else {
                None
            }
        }
        'A' => {
            // XSPICE A-element: not supported in Vacask, pass as comment
            Some(format!("// XSPICE (unsupported in vacask): {}", line))
        }
        _ => None,
    }
}

fn extract_dc_value(rest: &str) -> Option<&str> {
    let upper = rest.to_uppercase();
    if let Some(pos) = upper.find("DC") {
        let after = &rest[pos + 2..].trim_start();
        after.split_whitespace().next()
    } else {
        // If it's just a number (no DC keyword)
        let first = rest.split_whitespace().next()?;
        if first.parse::<f64>().is_ok() || first.ends_with(|c: char| "fpnumkMGT".contains(c)) {
            Some(first)
        } else {
            None
        }
    }
}

fn extract_ac_spec(rest: &str) -> Option<&str> {
    let upper = rest.to_uppercase();
    if let Some(pos) = upper.find("AC") {
        let after = &rest[pos + 2..].trim_start();
        after.split_whitespace().next()
    } else {
        None
    }
}

fn extract_tran_spec(rest: &str) -> Option<String> {
    let upper = rest.to_uppercase();

    if let Some(pos) = upper.find("SIN(") {
        let start = pos;
        if let Some(end) = rest[start..].find(')') {
            let inner = &rest[start + 4..start + end];
            let vals: Vec<&str> = inner.split_whitespace().collect();
            if vals.len() >= 4 {
                return Some(format!(
                    "type=\"sine\" sinedc={} ampl={} freq={}",
                    vals[0], vals[1], vals[2]
                ));
            }
        }
    }

    if let Some(pos) = upper.find("PULSE(") {
        let start = pos;
        if let Some(end) = rest[start..].find(')') {
            let inner = &rest[start + 6..start + end];
            let vals: Vec<&str> = inner.split_whitespace().collect();
            if vals.len() >= 7 {
                return Some(format!(
                    "type=\"pulse\" val0={} val1={} delay=0 rise={} fall={} width={} period={}",
                    vals[0], vals[1], vals[4], vals[5], vals[2], vals[3]
                ));
            }
        }
    }

    None
}

// ── VacaskLibrary (dlopen libvacask.so) ──

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

const RTLD_LAZY: c_int = 1;

// Assumed Vacask C API function pointer types.
// These are best-effort assumptions based on typical simulator library APIs.
// If the actual Vacask C API has different signatures, symbol resolution will
// fail at runtime and the backend will fall back to the subprocess path.
type VacaskInitFn = unsafe extern "C" fn() -> c_int;
type VacaskLoadNetlistFn = unsafe extern "C" fn(*const c_char) -> c_int;
type VacaskRunFn = unsafe extern "C" fn() -> c_int;
type VacaskGetResultFn = unsafe extern "C" fn() -> *mut VacaskResult;
type VacaskCleanupFn = unsafe extern "C" fn();

/// Opaque result handle returned by vacask_get_result().
/// The actual layout depends on the Vacask version; we treat it as opaque
/// and only read through accessor fields documented in the assumed API.
#[repr(C)]
pub struct VacaskResult {
    /// Pointer to raw file data (Nutmeg format) in memory
    pub raw_data: *const u8,
    /// Length of raw file data in bytes
    pub raw_data_len: usize,
    /// Status code: 0 = success
    pub status: c_int,
    /// Error message (null if no error)
    pub error_msg: *const c_char,
}

/// Vacask shared library backend: loads libvacask.so at runtime via dlopen.
///
/// This avoids the subprocess + temp file overhead of VacaskSubprocess.
/// Falls back to subprocess if the library cannot be loaded or if symbols
/// do not match the assumed API.
///
/// # Assumed C API
/// ```c
/// int vacask_init();
/// int vacask_load_netlist(const char* netlist);
/// int vacask_run();
/// vacask_result* vacask_get_result();
/// void vacask_cleanup();
/// ```
///
/// Since there is no official public Vacask C API specification, these
/// assumptions may need adjustment for specific Vacask versions.
pub struct VacaskLibrary {
    lib: *mut c_void,
    #[allow(dead_code)]
    init: VacaskInitFn,
    load_netlist: VacaskLoadNetlistFn,
    run: VacaskRunFn,
    get_result: VacaskGetResultFn,
    cleanup: VacaskCleanupFn,
}

unsafe impl Send for VacaskLibrary {}
unsafe impl Sync for VacaskLibrary {}

impl VacaskLibrary {
    /// Known paths to search for libvacask.so
    const LIB_SEARCH_PATHS: &[&str] = &[
        "libvacask.so",
        "libvacask.so.0",
        "/usr/lib/libvacask.so",
        "/usr/local/lib/libvacask.so",
        "/opt/vacask/lib/libvacask.so",
    ];

    /// Check if libvacask.so can be found on the system.
    pub fn is_available() -> bool {
        for path in Self::LIB_SEARCH_PATHS {
            let c_path = match CString::new(*path) {
                Ok(p) => p,
                Err(_) => continue,
            };
            unsafe {
                let _ = dlerror();
                let handle = dlopen(c_path.as_ptr(), RTLD_LAZY);
                if !handle.is_null() {
                    dlclose(handle);
                    return true;
                }
            }
        }
        false
    }

    /// Load libvacask.so and resolve all symbols.
    pub fn new() -> Result<Self, BackendError> {
        let lib = Self::open_library()?;

        unsafe {
            let init: VacaskInitFn = Self::resolve_symbol(lib, "vacask_init")?;
            let load_netlist: VacaskLoadNetlistFn = Self::resolve_symbol(lib, "vacask_load_netlist")?;
            let run: VacaskRunFn = Self::resolve_symbol(lib, "vacask_run")?;
            let get_result: VacaskGetResultFn = Self::resolve_symbol(lib, "vacask_get_result")?;
            let cleanup: VacaskCleanupFn = Self::resolve_symbol(lib, "vacask_cleanup")?;

            let ret = (init)();
            if ret != 0 {
                dlclose(lib);
                return Err(BackendError::SimulationError(format!(
                    "vacask_init returned error code {}. \
                    The Vacask C API may have changed; falling back to subprocess is recommended.",
                    ret
                )));
            }

            Ok(Self { lib, init, load_netlist, run, get_result, cleanup })
        }
    }

    fn open_library() -> Result<*mut c_void, BackendError> {
        for path in Self::LIB_SEARCH_PATHS {
            let c_path = match CString::new(*path) {
                Ok(p) => p,
                Err(_) => continue,
            };
            unsafe {
                let _ = dlerror();
                let handle = dlopen(c_path.as_ptr(), RTLD_LAZY);
                if !handle.is_null() {
                    return Ok(handle);
                }
            }
        }

        let err_msg = unsafe {
            let err = dlerror();
            if err.is_null() {
                "libvacask.so not found".to_string()
            } else {
                CStr::from_ptr(err).to_string_lossy().into_owned()
            }
        };

        Err(BackendError::SimulationError(format!(
            "Failed to load libvacask.so: {}. \
            Falling back to vacask subprocess backend.",
            err_msg
        )))
    }

    /// Resolve a symbol from the loaded library.
    unsafe fn resolve_symbol<T>(lib: *mut c_void, name: &str) -> Result<T, BackendError> {
        let c_name = CString::new(name).map_err(|_| {
            BackendError::SimulationError(format!("Invalid symbol name: {}", name))
        })?;

        unsafe {
            let _ = dlerror();
            let sym = dlsym(lib, c_name.as_ptr());
            let err = dlerror();

            if !err.is_null() {
                let err_str = CStr::from_ptr(err).to_string_lossy();
                return Err(BackendError::SimulationError(format!(
                    "Failed to resolve vacask symbol '{}': {}. \
                    The Vacask C API may differ from our assumptions.",
                    name, err_str
                )));
            }
            if sym.is_null() {
                return Err(BackendError::SimulationError(format!(
                    "Vacask symbol '{}' resolved to null", name
                )));
            }

            Ok(std::mem::transmute_copy(&sym))
        }
    }
}

impl Drop for VacaskLibrary {
    fn drop(&mut self) {
        unsafe {
            (self.cleanup)();
            dlclose(self.lib);
        }
    }
}

impl Backend for VacaskLibrary {
    fn name(&self) -> &str {
        "vacask-shared"
    }

    fn capabilities(&self) -> BackendCapabilities { VACASK_CAPS }

    fn codegen(&self) -> Box<dyn crate::codegen::CodeGen> {
        Box::new(crate::codegen::vacask::VacaskCodeGen)
    }

    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        let vacask_netlist = spice_to_vacask(netlist);
        self.run_vacask_netlist(&vacask_netlist)
    }

    fn run_netlist(&self, netlist: &str) -> Result<RawData, BackendError> {
        self.run_vacask_netlist(netlist)
    }
}

impl VacaskLibrary {
    fn run_vacask_netlist(&self, netlist: &str) -> Result<RawData, BackendError> {
        let c_netlist = CString::new(netlist).map_err(|_| {
            BackendError::SimulationError("Netlist contains null bytes".to_string())
        })?;

        unsafe {
            let ret = (self.load_netlist)(c_netlist.as_ptr());
            if ret != 0 {
                return Err(BackendError::SimulationError(format!(
                    "vacask_load_netlist returned error code {}", ret
                )));
            }

            let ret = (self.run)();
            if ret != 0 {
                return Err(BackendError::SimulationError(format!(
                    "vacask_run returned error code {}", ret
                )));
            }

            let result_ptr = (self.get_result)();
            if result_ptr.is_null() {
                return Err(BackendError::SimulationError(
                    "vacask_get_result returned null".to_string()
                ));
            }

            let result = &*result_ptr;

            if result.status != 0 {
                let err_msg = if result.error_msg.is_null() {
                    format!("Vacask simulation failed with status {}", result.status)
                } else {
                    CStr::from_ptr(result.error_msg).to_string_lossy().into_owned()
                };
                return Err(BackendError::SimulationError(err_msg));
            }

            if result.raw_data.is_null() || result.raw_data_len == 0 {
                return Err(BackendError::SimulationError(
                    "Vacask produced no output data".to_string()
                ));
            }

            let raw_bytes = std::slice::from_raw_parts(result.raw_data, result.raw_data_len);
            rawfile::parse_raw(raw_bytes).map_err(Into::into)
        }
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires libvacask.so to be installed
    fn test_vacask_library_availability() {
        let available = VacaskLibrary::is_available();
        println!("libvacask.so available: {}", available);
    }

    #[test]
    #[ignore] // Requires libvacask.so to be installed
    fn test_vacask_library_init() {
        let lib = VacaskLibrary::new();
        assert!(lib.is_ok(), "Failed to init: {:?}", lib.err());
    }

    #[test]
    #[ignore] // Requires libvacask.so to be installed
    fn test_vacask_library_op() {
        let lib = VacaskLibrary::new().expect("Failed to load libvacask.so");

        let netlist = "\
            test op\n\
            V1 vdd 0 3.3\n\
            R1 vdd out 1k\n\
            R2 out 0 2k\n\
            .op\n\
            .end\n";

        let result = lib.run(netlist);
        assert!(result.is_ok(), "Simulation failed: {:?}", result.err());
    }
}
