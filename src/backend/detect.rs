use std::process::Command;
use std::sync::OnceLock;

use super::BackendKind;
use super::ltspice;
use super::ngspice::NgspiceShared;
use super::vacask::VacaskLibrary;

static CACHED_BACKENDS: OnceLock<Vec<BackendKind>> = OnceLock::new();

/// Detect available backends by scanning $PATH, shared libraries, and
/// platform-specific locations. Cached after first call.
pub fn detect_backends() -> &'static Vec<BackendKind> {
    CACHED_BACKENDS.get_or_init(|| {
        let mut backends = Vec::new();

        // NGSpice shared library (preferred over subprocess — no temp files)
        if NgspiceShared::is_available() {
            backends.push(BackendKind::NgspiceShared);
        }

        // NGSpice subprocess
        if is_on_path("ngspice") {
            backends.push(BackendKind::NgspiceSubprocess);
        }


        // LTspice (platform-specific detection)
        if let Some((executable, use_wine)) = ltspice::detect_ltspice() {
            backends.push(BackendKind::Ltspice { executable, use_wine });
        }

        // Vacask shared library (preferred over subprocess)
        if VacaskLibrary::is_available() {
            backends.push(BackendKind::VacaskShared);
        }

        // Vacask subprocess
        if is_on_path("vacask") {
            backends.push(BackendKind::Vacask);
        }

        // Spectre
        if is_on_path("spectre") {
            backends.push(BackendKind::Spectre);
        }

        backends
    })
}

fn is_on_path(program: &str) -> bool {
    Command::new("which")
        .arg(program)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
