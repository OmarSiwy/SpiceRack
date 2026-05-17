use std::process::Command;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_short, c_void};
use std::sync::Mutex;
use tempfile::NamedTempFile;
use std::io::Write;

use crate::result::{RawData, VarInfo};
use crate::rawfile;
use super::{Backend, BackendError};

// ── dlopen/dlsym FFI ──

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

const RTLD_LAZY: c_int = 1;

// ── libngspice C types ──

/// ngspice callback: receives output text lines (stdout/stderr from simulator)
type SendCharFn = extern "C" fn(*const c_char, c_int, *mut c_void) -> c_int;
/// ngspice callback: receives status/progress text
type SendStatFn = extern "C" fn(*const c_char, c_int, *mut c_void) -> c_int;
/// ngspice callback: called on controlled exit (error or quit)
type ControlledExitFn = extern "C" fn(c_int, bool, bool, c_int, *mut c_void) -> c_int;
/// ngspice callback: receives per-step simulation data (real-time streaming)
type SendDataFn = extern "C" fn(*mut VecValuesAll, c_int, c_int, *mut c_void) -> c_int;
/// ngspice callback: receives vector initialization info at simulation start
type SendInitDataFn = extern "C" fn(*mut VecInfoAll, c_int, *mut c_void) -> c_int;
/// ngspice callback: signals whether background thread is running
type BGThreadRunningFn = extern "C" fn(bool, c_int, *mut c_void) -> c_int;

// ngspice API function pointer types
type NgSpiceInitFn = unsafe extern "C" fn(
    SendCharFn, SendStatFn, ControlledExitFn,
    SendDataFn, SendInitDataFn, BGThreadRunningFn,
    *mut c_void,
) -> c_int;
type NgSpiceCmdFn = unsafe extern "C" fn(*const c_char) -> c_int;
type NgSpiceCircFn = unsafe extern "C" fn(*mut *mut c_char) -> c_int;
type NgGetVecInfoFn = unsafe extern "C" fn(*const c_char) -> *mut VecInfo;
type NgAllVecsFn = unsafe extern "C" fn(*const c_char) -> *mut *mut c_char;
type NgAllPlotsFn = unsafe extern "C" fn() -> *mut *mut c_char;

/// ngspice vector info returned by ngGet_Vec_Info (matches vector_info in sharedspice.h)
#[repr(C)]
pub struct VecInfo {
    pub v_name: *const c_char,
    pub v_type: c_int,
    pub v_flags: c_short,
    pub v_realdata: *mut f64,
    pub v_compdata: *mut NgComplex,
    pub v_length: c_int,
}

/// Complex number as used by ngspice
#[repr(C)]
pub struct NgComplex {
    pub cx_real: f64,
    pub cx_imag: f64,
}

/// Per-step data for all vectors (SendData callback)
#[repr(C)]
pub struct VecValuesAll {
    pub count: c_int,
    pub index: c_int,
    pub vecsa: *mut *mut VecValue,
}

/// Single vector value at one simulation step
#[repr(C)]
pub struct VecValue {
    pub name: *const c_char,
    pub creal: f64,
    pub cimag: f64,
    pub is_scale: bool,
    pub is_complex: bool,
}

/// Vector info sent at simulation start (SendInitData callback)
#[repr(C)]
pub struct VecInfoAll {
    pub name: *const c_char,
    pub title: *const c_char,
    pub date: *const c_char,
    pub type_: *const c_char,
    pub vec_count: c_int,
    pub vecs: *mut *mut VecInfoShort,
}

/// Short vector description (name + type index)
#[repr(C)]
pub struct VecInfoShort {
    pub number: c_int,
    pub vecname: *const c_char,
    pub is_real: bool,
    pub pdvec: *mut c_void,
    pub pdveccomp: *mut c_void,
}

/// A single streaming data point captured from the SendData callback
#[derive(Debug, Clone)]
pub struct StreamPoint {
    pub step: usize,
    pub values: Vec<(String, f64)>,
}

/// Internal shared state accessible from C callbacks via the userdata pointer.
/// This is heap-allocated and its pointer is passed as the `void* userdata`
/// argument to ngSpice_Init; each callback casts it back.
struct CallbackState {
    output: Mutex<Vec<String>>,
    stream_buffer: Mutex<Vec<StreamPoint>>,
}

// ── NgspiceSubprocess (existing) ──

/// NgSpice subprocess backend: write .cir, run `ngspice -b`, read .raw
pub struct NgspiceSubprocess;

impl Backend for NgspiceSubprocess {
    fn name(&self) -> &str {
        "ngspice-subprocess"
    }

    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        // Write netlist to temp file
        let mut cir_file = NamedTempFile::with_suffix(".cir")?;
        cir_file.write_all(netlist.as_bytes())?;
        cir_file.flush()?;

        let cir_path = cir_file.path();
        let raw_path = cir_path.with_extension("raw");

        // Run ngspice in batch mode
        let output = Command::new("ngspice")
            .arg("-b")
            .arg("-r")
            .arg(&raw_path)
            .arg(cir_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Extract error lines from ngspice output
            let errors: Vec<&str> = stdout
                .lines()
                .chain(stderr.lines())
                .filter(|l| l.contains("Error") || l.contains("error"))
                .collect();

            if !errors.is_empty() {
                return Err(BackendError::SimulationError(errors.join("\n")));
            }

            return Err(BackendError::SimulationError(format!(
                "ngspice exited with status {}\nstdout: {}\nstderr: {}",
                output.status,
                stdout.chars().take(500).collect::<String>(),
                stderr.chars().take(500).collect::<String>(),
            )));
        }

        // Capture stdout for .meas parsing
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();

        // Parse .raw output
        let raw_bytes = std::fs::read(&raw_path).map_err(|e| {
            BackendError::SimulationError(format!(
                "Failed to read raw file '{}': {}",
                raw_path.display(),
                e
            ))
        })?;

        let mut result = rawfile::parse_raw(&raw_bytes)?;
        result.stdout = stdout_str;

        // Cleanup
        let _ = std::fs::remove_file(&raw_path);

        Ok(result)
    }
}

// ── NgspiceShared (dlopen libngspice.so) ──

/// NgSpice shared library backend: loads libngspice.so at runtime via dlopen,
/// avoiding temp files and subprocess overhead.
///
/// # Safety
/// NgspiceShared is only used from one thread at a time (Backend::run takes &self
/// and ngspice itself is not thread-safe). The Send+Sync impls are required for
/// the Backend trait but callers must not run concurrent simulations on the same
/// instance.
pub struct NgspiceShared {
    lib: *mut c_void,
    // Function pointers resolved from libngspice.so
    #[allow(dead_code)]
    init: NgSpiceInitFn,
    circ: NgSpiceCircFn,
    command: NgSpiceCmdFn,
    get_vec_info: NgGetVecInfoFn,
    all_vecs: NgAllVecsFn,
    all_plots: NgAllPlotsFn,
    // Callback state (heap-allocated, pointer passed to ngspice as userdata)
    cb_state: *mut CallbackState,
}

unsafe impl Send for NgspiceShared {}
unsafe impl Sync for NgspiceShared {}

impl NgspiceShared {
    /// Known paths to search for libngspice.so
    const LIB_SEARCH_PATHS: &[&str] = &[
        "libngspice.so",
        "libngspice.so.0",
        "/usr/lib/libngspice.so",
        "/usr/lib/libngspice.so.0",
        "/usr/local/lib/libngspice.so",
        "/usr/lib/x86_64-linux-gnu/libngspice.so",
        "/usr/lib/x86_64-linux-gnu/libngspice.so.0",
    ];

    /// Check if libngspice.so can be found on the system.
    pub fn is_available() -> bool {
        for path in Self::LIB_SEARCH_PATHS {
            let c_path = match CString::new(*path) {
                Ok(p) => p,
                Err(_) => continue,
            };
            unsafe {
                // Clear previous errors
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

    /// Load libngspice.so, resolve all symbols, and call ngSpice_Init.
    pub fn new() -> Result<Self, BackendError> {
        let lib = Self::open_library()?;

        unsafe {
            let init: NgSpiceInitFn = Self::resolve_symbol(lib, "ngSpice_Init")?;
            let circ: NgSpiceCircFn = Self::resolve_symbol(lib, "ngSpice_Circ")?;
            let command: NgSpiceCmdFn = Self::resolve_symbol(lib, "ngSpice_Command")?;
            let get_vec_info: NgGetVecInfoFn = Self::resolve_symbol(lib, "ngGet_Vec_Info")?;
            let all_vecs: NgAllVecsFn = Self::resolve_symbol(lib, "ngSpice_AllVecs")?;
            let all_plots: NgAllPlotsFn = Self::resolve_symbol(lib, "ngSpice_AllPlots")?;

            // Allocate callback state on the heap
            let cb_state = Box::into_raw(Box::new(CallbackState {
                output: Mutex::new(Vec::new()),
                stream_buffer: Mutex::new(Vec::new()),
            }));

            // Initialize ngspice with our callbacks
            let ret = (init)(
                cb_send_char,
                cb_send_stat,
                cb_controlled_exit,
                cb_send_data,
                cb_send_init_data,
                cb_bg_thread_running,
                cb_state as *mut c_void,
            );

            if ret != 0 {
                // Cleanup on failure
                let _ = Box::from_raw(cb_state);
                dlclose(lib);
                return Err(BackendError::SimulationError(
                    format!("ngSpice_Init returned error code {}", ret),
                ));
            }

            Ok(Self {
                lib,
                init,
                circ,
                command,
                get_vec_info,
                all_vecs,
                all_plots,
                cb_state,
            })
        }
    }

    /// Try to dlopen libngspice.so from known paths.
    fn open_library() -> Result<*mut c_void, BackendError> {
        for path in Self::LIB_SEARCH_PATHS {
            let c_path = match CString::new(*path) {
                Ok(p) => p,
                Err(_) => continue,
            };
            unsafe {
                let _ = dlerror(); // Clear previous errors
                let handle = dlopen(c_path.as_ptr(), RTLD_LAZY);
                if !handle.is_null() {
                    return Ok(handle);
                }
            }
        }

        // Collect the last error for a helpful message
        let err_msg = unsafe {
            let err = dlerror();
            if err.is_null() {
                "libngspice.so not found".to_string()
            } else {
                CStr::from_ptr(err).to_string_lossy().into_owned()
            }
        };

        Err(BackendError::SimulationError(format!(
            "Failed to load libngspice.so: {}. \
            Install with: sudo apt install libngspice0-dev",
            err_msg
        )))
    }

    /// Resolve a symbol from the loaded library. Returns a transmuted function pointer.
    unsafe fn resolve_symbol<T>(lib: *mut c_void, name: &str) -> Result<T, BackendError> {
        let c_name = CString::new(name).map_err(|_| {
            BackendError::SimulationError(format!("Invalid symbol name: {}", name))
        })?;

        unsafe {
            let _ = dlerror(); // Clear
            let sym = dlsym(lib, c_name.as_ptr());
            let err = dlerror();

            if !err.is_null() {
                let err_str = CStr::from_ptr(err).to_string_lossy();
                return Err(BackendError::SimulationError(format!(
                    "Failed to resolve symbol '{}': {}", name, err_str
                )));
            }
            if sym.is_null() {
                return Err(BackendError::SimulationError(format!(
                    "Symbol '{}' resolved to null", name
                )));
            }

            Ok(std::mem::transmute_copy(&sym))
        }
    }

    /// Load a circuit netlist into ngspice. The netlist is split into lines
    /// and passed as a null-terminated array of C strings.
    fn load_circuit(&self, netlist: &str) -> Result<(), BackendError> {
        let lines: Vec<&str> = netlist.lines().collect();
        let mut c_lines: Vec<CString> = Vec::with_capacity(lines.len());
        for line in &lines {
            c_lines.push(CString::new(*line).map_err(|_| {
                BackendError::SimulationError(
                    "Netlist contains null bytes".to_string(),
                )
            })?);
        }

        // Build null-terminated array of pointers
        let mut ptrs: Vec<*mut c_char> = c_lines
            .iter()
            .map(|cs| cs.as_ptr() as *mut c_char)
            .collect();
        ptrs.push(std::ptr::null_mut()); // null terminator

        unsafe {
            let ret = (self.circ)(ptrs.as_mut_ptr());
            if ret != 0 {
                return Err(BackendError::SimulationError(format!(
                    "ngSpice_Circ returned error code {}", ret
                )));
            }
        }

        Ok(())
    }

    /// Execute an ngspice command string (e.g. "run", "op", "quit").
    fn exec_command(&self, cmd: &str) -> Result<(), BackendError> {
        let c_cmd = CString::new(cmd).map_err(|_| {
            BackendError::SimulationError(format!("Invalid command: {}", cmd))
        })?;

        unsafe {
            let ret = (self.command)(c_cmd.as_ptr());
            if ret != 0 {
                return Err(BackendError::SimulationError(format!(
                    "ngSpice_Command('{}') returned error code {}", cmd, ret
                )));
            }
        }

        Ok(())
    }

    /// Get all plot names currently in ngspice.
    fn get_all_plots(&self) -> Vec<String> {
        let mut plots = Vec::new();
        unsafe {
            let ptr = (self.all_plots)();
            if ptr.is_null() {
                return plots;
            }
            let mut i = 0;
            loop {
                let entry = *ptr.add(i);
                if entry.is_null() {
                    break;
                }
                if let Ok(s) = CStr::from_ptr(entry).to_str() {
                    plots.push(s.to_string());
                }
                i += 1;
            }
        }
        plots
    }

    /// Get all vector names for a given plot.
    fn get_all_vecs(&self, plot: &str) -> Vec<String> {
        let mut vecs = Vec::new();
        let c_plot = match CString::new(plot) {
            Ok(p) => p,
            Err(_) => return vecs,
        };
        unsafe {
            let ptr = (self.all_vecs)(c_plot.as_ptr());
            if ptr.is_null() {
                return vecs;
            }
            let mut i = 0;
            loop {
                let entry = *ptr.add(i);
                if entry.is_null() {
                    break;
                }
                if let Ok(s) = CStr::from_ptr(entry).to_str() {
                    vecs.push(s.to_string());
                }
                i += 1;
            }
        }
        vecs
    }

    /// Read a single vector's data by name. The name should be
    /// qualified with the plot name (e.g. "tran1.v(out)").
    fn read_vector(&self, vecname: &str) -> Option<(bool, Vec<f64>, Vec<(f64, f64)>)> {
        let c_name = CString::new(vecname).ok()?;
        unsafe {
            let info = (self.get_vec_info)(c_name.as_ptr());
            if info.is_null() {
                return None;
            }
            let vi = &*info;
            let len = vi.v_length as usize;
            if len == 0 {
                return None;
            }

            // Determine real vs complex from data pointer availability
            if !vi.v_realdata.is_null() {
                let data = std::slice::from_raw_parts(vi.v_realdata, len).to_vec();
                Some((true, data, Vec::new()))
            } else if !vi.v_compdata.is_null() {
                let comp_slice = std::slice::from_raw_parts(vi.v_compdata, len);
                let real_data: Vec<f64> = comp_slice.iter().map(|c| c.cx_real).collect();
                let comp_data: Vec<(f64, f64)> = comp_slice
                    .iter()
                    .map(|c| (c.cx_real, c.cx_imag))
                    .collect();
                Some((false, real_data, comp_data))
            } else {
                None
            }
        }
    }

    /// Extract results from ngspice after simulation. Reads the most recent plot,
    /// enumerates all vectors, and builds a RawData struct.
    fn extract_results(&self) -> Result<RawData, BackendError> {
        let plots = self.get_all_plots();
        if plots.is_empty() {
            return Err(BackendError::SimulationError(
                "No plots available after simulation".to_string(),
            ));
        }

        // Use the first (most recent) plot
        let plot = &plots[0];
        let vec_names = self.get_all_vecs(plot);

        if vec_names.is_empty() {
            return Err(BackendError::SimulationError(format!(
                "No vectors in plot '{}'", plot
            )));
        }

        let mut result = RawData::empty();
        result.plot_name = plot.clone();

        // Read captured stdout for .meas results etc.
        if let Ok(output) = unsafe { &*self.cb_state }.output.lock() {
            result.stdout = output.join("\n");
        }

        // Determine if results are complex by checking the first vector
        let mut is_complex = false;
        let mut var_infos = Vec::new();
        let mut real_vecs = Vec::new();
        let mut complex_vecs = Vec::new();

        let mut actual_idx = 0;
        for name in vec_names.iter() {
            // ngspice uses "plot.vecname" for qualified names
            let qualified = format!("{}.{}", plot, name);
            let read_result = self.read_vector(&qualified)
                .or_else(|| self.read_vector(name));

            // Skip vectors that ngspice lists but cannot provide data for
            // (e.g. #branch currents in ngspice 45 when not explicitly saved)
            let (is_real, real_data, comp_data) = match read_result {
                Some(data) => data,
                None => continue,
            };

            // Infer variable type from name
            let var_type = infer_var_type(name);

            var_infos.push(VarInfo {
                index: actual_idx,
                name: name.clone(),
                var_type,
            });
            actual_idx += 1;

            if is_real {
                real_vecs.push(real_data);
                complex_vecs.push(Vec::new());
            } else {
                is_complex = true;
                real_vecs.push(real_data);
                let cvec: Vec<num_complex::Complex64> = comp_data
                    .into_iter()
                    .map(|(re, im)| num_complex::Complex64::new(re, im))
                    .collect();
                complex_vecs.push(cvec);
            }
        }

        result.variables = var_infos;
        result.real_data = real_vecs;
        result.is_complex = is_complex;
        if is_complex {
            result.complex_data = complex_vecs;
        }

        Ok(result)
    }

    /// Drain any streaming data points that have been captured by the
    /// SendData callback since the last drain.
    pub fn drain_streaming_data(&self) -> Vec<StreamPoint> {
        if let Ok(mut buf) = unsafe { &*self.cb_state }.stream_buffer.lock() {
            std::mem::take(&mut *buf)
        } else {
            Vec::new()
        }
    }
}

impl Drop for NgspiceShared {
    fn drop(&mut self) {
        unsafe {
            // Tell ngspice to quit
            let quit = CString::new("quit").unwrap();
            let _ = (self.command)(quit.as_ptr());

            // Free callback state
            let _ = Box::from_raw(self.cb_state);

            // Close the shared library
            dlclose(self.lib);
        }
    }
}

impl Backend for NgspiceShared {
    fn name(&self) -> &str {
        "ngspice-shared"
    }

    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        // Clear previous output
        if let Ok(mut output) = unsafe { &*self.cb_state }.output.lock() {
            output.clear();
        }
        if let Ok(mut buf) = unsafe { &*self.cb_state }.stream_buffer.lock() {
            buf.clear();
        }

        // Load the circuit
        self.load_circuit(netlist)?;

        // Run the simulation
        self.exec_command("run")?;

        // Extract results from ngspice's internal data
        self.extract_results()
    }
}

// ── NgspiceSharedStreaming ──

/// Wrapper around NgspiceShared that provides a streaming data callback.
///
/// The user can register a callback that is called for each simulation step
/// (via the SendData ngspice callback). For batch use, drain_streaming_data()
/// returns all accumulated points.
pub struct NgspiceSharedStreaming {
    shared: NgspiceShared,
}

impl NgspiceSharedStreaming {
    /// Create a new streaming instance. Loads libngspice.so and initializes it.
    pub fn new() -> Result<Self, BackendError> {
        let shared = NgspiceShared::new()?;
        Ok(Self { shared })
    }

    /// Drain all streaming data points accumulated since the last drain.
    pub fn drain_streaming_data(&self) -> Vec<StreamPoint> {
        self.shared.drain_streaming_data()
    }

    /// Access the underlying shared backend for direct API calls.
    pub fn inner(&self) -> &NgspiceShared {
        &self.shared
    }
}

impl Backend for NgspiceSharedStreaming {
    fn name(&self) -> &str {
        "ngspice-shared-streaming"
    }

    fn run(&self, netlist: &str) -> Result<RawData, BackendError> {
        self.shared.run(netlist)
    }
}

// ── ngspice C callbacks ──

/// Callback: receives output text lines from ngspice (stdout/stderr).
/// Used to capture .meas results and error messages.
extern "C" fn cb_send_char(msg: *const c_char, _id: c_int, userdata: *mut c_void) -> c_int {
    if msg.is_null() || userdata.is_null() {
        return 0;
    }
    let state = unsafe { &*(userdata as *const CallbackState) };
    let s = unsafe { CStr::from_ptr(msg) };
    if let Ok(text) = s.to_str() {
        if let Ok(mut output) = state.output.lock() {
            output.push(text.to_string());
        }
    }
    0
}

/// Callback: receives status/progress text. Ignored for now.
extern "C" fn cb_send_stat(_msg: *const c_char, _id: c_int, _userdata: *mut c_void) -> c_int {
    0
}

/// Callback: called on controlled exit. Log errors but do not abort.
extern "C" fn cb_controlled_exit(
    exit_status: c_int,
    _immediate_unload: bool,
    _exit_on_quit: bool,
    _id: c_int,
    userdata: *mut c_void,
) -> c_int {
    if userdata.is_null() {
        return 0;
    }
    if exit_status != 0 {
        let state = unsafe { &*(userdata as *const CallbackState) };
        if let Ok(mut output) = state.output.lock() {
            output.push(format!("ngspice controlled exit with status {}", exit_status));
        }
    }
    0
}

/// Callback: receives per-step simulation data. Captures into stream_buffer.
extern "C" fn cb_send_data(data: *mut VecValuesAll, _count: c_int, _id: c_int, userdata: *mut c_void) -> c_int {
    if data.is_null() || userdata.is_null() {
        return 0;
    }

    let state = unsafe { &*(userdata as *const CallbackState) };
    let vva = unsafe { &*data };
    let num_vecs = vva.count as usize;

    if vva.vecsa.is_null() || num_vecs == 0 {
        return 0;
    }

    let mut values = Vec::with_capacity(num_vecs);
    unsafe {
        for i in 0..num_vecs {
            let vec_ptr = *vva.vecsa.add(i);
            if vec_ptr.is_null() {
                continue;
            }
            let vv = &*vec_ptr;
            let name = if vv.name.is_null() {
                String::from("?")
            } else {
                CStr::from_ptr(vv.name).to_string_lossy().into_owned()
            };
            values.push((name, vv.creal));
        }
    }

    let point = StreamPoint {
        step: vva.index as usize,
        values,
    };

    if let Ok(mut buf) = state.stream_buffer.lock() {
        buf.push(point);
    }

    0
}

/// Callback: receives vector info at simulation start. Ignored for now.
extern "C" fn cb_send_init_data(_data: *mut VecInfoAll, _id: c_int, _userdata: *mut c_void) -> c_int {
    0
}

/// Callback: signals background thread status. Ignored.
extern "C" fn cb_bg_thread_running(_is_running: bool, _id: c_int, _userdata: *mut c_void) -> c_int {
    0
}

// ── Helpers ──

/// Infer the variable type string from a vector name.
fn infer_var_type(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower == "time" {
        "time".to_string()
    } else if lower == "frequency" {
        "frequency".to_string()
    } else if lower.starts_with("i(") || lower.starts_with("@") || lower.contains("#branch") {
        "current".to_string()
    } else {
        "voltage".to_string()
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_var_type() {
        assert_eq!(infer_var_type("time"), "time");
        assert_eq!(infer_var_type("frequency"), "frequency");
        assert_eq!(infer_var_type("v(out)"), "voltage");
        assert_eq!(infer_var_type("i(Vin)"), "current");
        assert_eq!(infer_var_type("@m1[id]"), "current");
        assert_eq!(infer_var_type("V(net1)"), "voltage");
    }

    #[test]
    fn test_subprocess_name() {
        let backend = NgspiceSubprocess;
        assert_eq!(backend.name(), "ngspice-subprocess");
    }

    #[test]
    #[ignore] // Requires libngspice.so to be installed
    fn test_ngspice_shared_availability() {
        // This test checks if we can detect the library
        let available = NgspiceShared::is_available();
        println!("libngspice.so available: {}", available);
    }

    #[test]
    #[ignore] // Requires libngspice.so to be installed
    fn test_ngspice_shared_init() {
        let shared = NgspiceShared::new();
        assert!(shared.is_ok(), "Failed to init: {:?}", shared.err());
    }

    #[test]
    #[ignore] // Requires libngspice.so to be installed
    fn test_ngspice_shared_op() {
        let shared = NgspiceShared::new().expect("Failed to load libngspice.so");

        let netlist = "\
            test op\n\
            V1 vdd 0 3.3\n\
            R1 vdd out 1k\n\
            R2 out 0 2k\n\
            .op\n\
            .end\n";

        let result = shared.run(netlist);
        assert!(result.is_ok(), "Simulation failed: {:?}", result.err());

        let raw = result.unwrap();
        assert!(!raw.variables.is_empty(), "No variables in result");
    }

    #[test]
    #[ignore] // Requires libngspice.so to be installed
    fn test_ngspice_shared_tran() {
        let shared = NgspiceShared::new().expect("Failed to load libngspice.so");

        let netlist = "\
            test tran\n\
            V1 in 0 PULSE(0 1 0 1n 1n 5u 10u)\n\
            R1 in out 1k\n\
            C1 out 0 1n\n\
            .tran 10n 20u\n\
            .end\n";

        let result = shared.run(netlist);
        assert!(result.is_ok(), "Simulation failed: {:?}", result.err());

        let raw = result.unwrap();
        assert!(!raw.variables.is_empty());
        assert!(!raw.real_data.is_empty());
        assert!(raw.real_data[0].len() > 1, "Expected multiple data points");
    }

    #[test]
    #[ignore] // Requires libngspice.so to be installed
    fn test_ngspice_shared_streaming() {
        let streaming = NgspiceSharedStreaming::new()
            .expect("Failed to load libngspice.so");

        let netlist = "\
            test streaming\n\
            V1 in 0 PULSE(0 1 0 1n 1n 5u 10u)\n\
            R1 in out 1k\n\
            C1 out 0 1n\n\
            .tran 10n 20u\n\
            .end\n";

        let _ = streaming.run(netlist);

        let points = streaming.drain_streaming_data();
        // After simulation, stream buffer should have been populated
        // (exact count depends on ngspice step selection)
        println!("Streamed {} data points", points.len());
    }
}
