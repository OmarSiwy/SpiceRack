//! Verilog-A sources on their way to a simulator.
//!
//! `veriloga()` records the `.va` source in a subcircuit's load list; each
//! backend's codegen decides what that becomes. ngspice and VACASK load OSDI,
//! so their codegen compiles the source with OpenVAF ([`compile_osdi`]);
//! Spectre includes it with `ahdl_include`; ESPice compiles it itself through
//! VerA (`.hdl`), so no OpenVAF run happens on that path.

use std::path::{Path, PathBuf};
use std::process::Command;

/// True for a load-list entry that is Verilog-A source rather than a binary.
pub fn is_source(path: &str) -> bool {
    let p = Path::new(path);
    matches!(p.extension().and_then(|e| e.to_str()), Some(e) if e.eq_ignore_ascii_case("va") || e.eq_ignore_ascii_case("vams"))
}

/// The `.va` file for a path or inline source. Inline source is written to a
/// temp file named by its hash, so the same text always maps to the same file.
pub fn resolve(source_or_path: &str) -> Result<PathBuf, String> {
    let trimmed = source_or_path.trim();
    if trimmed.ends_with(".va") && !trimmed.contains('\n') {
        let p = Path::new(trimmed);
        if !p.exists() {
            return Err(format!("Verilog-A file not found: {}", trimmed));
        }
        // Absolute, so a deck written to a temp dir still finds it.
        return p.canonicalize().map_err(|e| format!("{}: {}", trimmed, e));
    }
    let dir = std::env::temp_dir().join("spicerack_va");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        trimmed.hash(&mut h);
        h.finish()
    };
    let va_file = dir.join(format!("inline_{:016x}.va", hash));
    std::fs::write(&va_file, trimmed).map_err(|e| format!("Failed to write temp .va file: {}", e))?;
    Ok(va_file)
}

/// Compile `va` to an `.osdi` beside it with OpenVAF, skipping the run when
/// the `.osdi` is newer than the source.
pub fn compile_osdi(va: &Path) -> Result<String, String> {
    let osdi_path = va.with_extension("osdi");
    if let (Ok(v), Ok(o)) = (va.metadata().and_then(|m| m.modified()), osdi_path.metadata().and_then(|m| m.modified()))
        && o > v
    {
        return Ok(osdi_path.to_string_lossy().to_string());
    }
    let output = Command::new("openvaf").arg(va).arg("-o").arg(&osdi_path).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "openvaf not found on $PATH. Install OpenVAF to compile Verilog-A models.\n\
             See: https://openvaf.semimod.de"
                .to_string()
        } else {
            format!("Failed to run openvaf: {}", e)
        }
    })?;
    if !output.status.success() {
        return Err(format!(
            "openvaf compilation failed (exit {}):\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    Ok(osdi_path.to_string_lossy().to_string())
}

/// A load list with every Verilog-A source replaced by its compiled `.osdi`,
/// for the backends that load OSDI.
pub fn osdi_loads(loads: &[String]) -> Result<Vec<String>, String> {
    loads.iter().map(|l| if is_source(l) { compile_osdi(Path::new(l)) } else { Ok(l.clone()) }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_vs_binary() {
        assert!(is_source("/m/a.va"));
        assert!(is_source("A.VA"));
        assert!(!is_source("/m/a.osdi"));
        assert!(!is_source("/m/va"));
    }

    #[test]
    fn binaries_pass_through_without_openvaf() {
        assert_eq!(osdi_loads(&["/m/a.osdi".into()]).unwrap(), vec!["/m/a.osdi".to_string()]);
    }

    #[test]
    fn inline_source_resolves_to_a_stable_file() {
        let src = "module t(a); inout a; electrical a; endmodule";
        let a = resolve(src).unwrap();
        assert_eq!(a, resolve(src).unwrap());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), src);
    }
}
