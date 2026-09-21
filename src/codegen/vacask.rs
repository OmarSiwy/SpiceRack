//! VACASK netlist generation.
//!
//! VACASK reads its own netlist language, not a SPICE dialect and not Spectre.
//! Everything below was probed against the binary
//! (`share/doc/vacask/{demo,test}/*.sim` are the authoritative examples); where
//! VACASK has no faithful spelling for something the IR can express, this file
//! returns a `CodeGenError` rather than emitting a deck that dies at parse time.
//!
//! Shape of a generated deck:
//!
//! ```text
//! <title>
//! ground 0
//! load "spice/resistor.osdi"  // one per device module actually used
//! model resistor sp_resistor  // VACASK has no anonymous instances
//! r1 (in out) resistor r=1e3
//! control
//!   options rawfile="binary"
//!   analysis op1 op
//! endc
//! ```
//!
//! Two properties make the mechanical translation trustworthy:
//!
//! 1. `spice/*.osdi` is VACASK's own build of the SPICE device set, under the
//!    same parameter names `.model` cards use, so model parameters pass through
//!    unchanged instead of through a whitelist that would go stale.
//! 2. VACASK hard-errors on an unknown parameter ("Parameter 'x' not found")
//!    rather than defaulting it, so a name that gets past this file and does not
//!    exist over there is a loud failure, never a silent substitution.
//!
//! Everything is lowercased: SPICE is case-insensitive, VACASK is not.

use std::collections::BTreeSet;
use crate::ir::*;
use super::{CodeGen, CodeGenError};

pub struct VacaskCodeGen;

/// A device module the deck needs before it can be elaborated.
///
/// SPICE carries R/C/L values on the instance and never writes a `.model` for
/// them; VACASK has no anonymous instances, so every device needs a model. The
/// synthesised ones are named after the device, which is what upstream's own
/// decks do (`model resistor resistor` in `test/test_op.sim`); a user `.model`
/// wearing one of these names is a collision and is refused.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Need {
    /// SPICE device set: `load "spice/<m>.osdi"` + `model <m> sp_<m>`.
    Sp(&'static str),
    /// Built into the simulator: no load, but still a `model` line.
    Builtin(&'static str),
}

impl Need {
    fn auto_model(&self) -> &'static str {
        match self {
            Need::Sp(m) | Need::Builtin(m) => m,
        }
    }
}

fn unsupported_comp(what: impl Into<String>) -> CodeGenError {
    CodeGenError::UnsupportedComponent { backend: "vacask".into(), component: what.into() }
}

fn unsupported_analysis(what: impl Into<String>) -> CodeGenError {
    CodeGenError::UnsupportedAnalysis { backend: "vacask".into(), analysis: what.into() }
}

// ── Numbers ──

/// A plain number VACASK cannot misread. SPICE scale suffixes are
/// case-INSENSITIVE and VACASK's are not (`M` is milli in SPICE and MEGA in
/// VACASK: a factor of 1e9 with nothing to notice it), so nothing computed here
/// is ever re-spelled with a suffix.
fn num(v: f64) -> String {
    if v == 0.0 { "0".to_string() } else { format!("{:e}", v) }
}

/// SPICE scale suffix -> VACASK's spelling. `meg`/`mil` must be tried before
/// `m`: taking the shorter match first turns a megohm into a milliohm.
const SCALES: &[(&str, &str, f64)] = &[
    ("meg", "meg", 1e6), ("mil", "mil", 25.4e-6),
    ("t", "T", 1e12), ("g", "G", 1e9), ("k", "k", 1e3), ("m", "m", 1e-3),
    ("u", "u", 1e-6), ("n", "n", 1e-9), ("p", "p", 1e-12), ("f", "f", 1e-15),
    ("a", "a", 1e-18),
];

/// Parse a SPICE number-with-suffix. Returns `(value, vacask_spelling)`.
fn parse_spice_num(t: &str) -> Option<(f64, String)> {
    let b = t.as_bytes();
    let mut end = 0;
    while end < b.len() {
        let c = b[end] as char;
        if c.is_ascii_digit() || c == '.' || c == '+' || c == '-' {
            end += 1;
            continue;
        }
        // An exponent's `e` belongs to the number, but only when a sign or a
        // digit follows it: a bare `1e` is still "1 exa".
        if (c == 'e' || c == 'E')
            && end + 1 < b.len()
            && ((b[end + 1] as char).is_ascii_digit() || b[end + 1] == b'+' || b[end + 1] == b'-')
        {
            end += 1;
            continue;
        }
        break;
    }
    let mantissa: f64 = t[..end].parse().ok()?;
    let suffix = t[end..].to_ascii_lowercase();
    for (spice, vacask, mult) in SCALES {
        if suffix.starts_with(spice) {
            return Some((mantissa * mult, format!("{}{}", &t[..end], vacask)));
        }
    }
    // A trailing unit with no scale letter (`5V`, `1ohm`) is ignored by SPICE.
    if suffix.chars().all(|c| c.is_ascii_alphabetic()) {
        return Some((mantissa, t[..end].to_string()));
    }
    None
}

/// A value written as text: a number gets re-spelled in VACASK's suffixes,
/// anything else (a parameter name, an arithmetic expression — VACASK has
/// those) passes through minus the ngspice `{}` wrapper it has no use for.
fn respell(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix('{').and_then(|r| r.strip_suffix('}')).unwrap_or(t).trim();
    match parse_spice_num(t) {
        Some((_, spelled)) => spelled,
        None => t.to_ascii_lowercase(),
    }
}

/// Numeric value of a text field, when one is needed to compute with.
fn value_of(s: &str) -> Option<f64> {
    parse_spice_num(s.trim().trim_start_matches('{').trim_end_matches('}').trim()).map(|(v, _)| v)
}

// ── Model kind mapping ──

/// SPICE MOSFET LEVEL -> VACASK module. A level that is not here is refused:
/// substituting "the nearest model" compares two different sets of equations.
fn mos_module(level: u32) -> Option<&'static str> {
    Some(match level {
        1 => "mos1",
        2 => "mos2",
        3 => "mos3",
        6 => "mos6",
        9 => "mos9",
        49 | 8 => "bsim3v3",
        54 | 14 => "bsim4v8",
        _ => return None,
    })
}

/// `.model` TYPE -> (module, polarity), for the types whose parameter sets
/// transcribe name-for-name AT LEVEL 1.
///
/// `nmos`/`pmos` are absent on purpose: their LEVEL selects between seven
/// modules. R/C/L are absent too, and they are the one place the "same
/// parameter names" premise fails — `sp_resistor` renames the model card's
/// `r`/`tc1`/`tc2` to `model_r`/`model_tc1`/`model_tc2` to keep them apart from
/// the instance parameters of the same name, so a name-for-name transcription
/// would set the instance parameter and quietly change the device.
fn model_type(kind: &str) -> Option<(&'static str, i8)> {
    Some(match kind {
        "d" => ("diode", 0),
        "npn" => ("bjt", 1),
        "pnp" => ("bjt", -1),
        "njf" => ("jfet1", 1),
        "pjf" => ("jfet1", -1),
        "nmf" => ("mes1", 1),
        "pmf" => ("mes1", -1),
        "vdmos" => ("vdmos", 1),
        _ => return None,
    })
}

// ── Options ──

/// `.options` keys that only steer a simulator's own bookkeeping or printing.
/// Dropping one cannot change a number in the raw file, which is the only
/// reason a key is on this list.
const COSMETIC_OPTIONS: &[&str] = &[
    "noacct", "acct", "list", "node", "post", "trans", "nopage", "nomod",
    "lvlcod", "klu", "sparse", "savecurrents", "numdgt",
];

/// `.options KEY=VALUE` whose VACASK spelling is the same word and whose
/// meaning is the same quantity. Probed against the binary; `itl1` and `maxord`
/// are absent because VACASK rejects them.
const NUMERIC_OPTIONS: &[&str] = &[
    "reltol", "abstol", "vntol", "chgtol", "gmin", "temp", "tnom",
];

// ── Codegen ──

impl VacaskCodeGen {
    /// The module a component needs loaded, if any.
    fn need(comp: &Component) -> Option<Need> {
        Some(match comp {
            Component::Resistor { .. } => Need::Sp("resistor"),
            Component::Capacitor { .. } => Need::Sp("capacitor"),
            Component::Inductor { .. } => Need::Sp("inductor"),
            Component::MutualInductor { .. } => Need::Builtin("mutual"),
            Component::VoltageSource { .. } => Need::Builtin("vsource"),
            Component::CurrentSource { .. } => Need::Builtin("isource"),
            Component::Vcvs { .. } => Need::Builtin("vcvs"),
            Component::Vccs { .. } => Need::Builtin("vccs"),
            Component::Cccs { .. } => Need::Builtin("cccs"),
            Component::Ccvs { .. } => Need::Builtin("ccvs"),
            _ => return None,
        })
    }

    fn value(&self, v: &IrValue) -> String {
        match v {
            IrValue::Numeric { value } => num(*value),
            IrValue::Expression { expr } => respell(expr),
            IrValue::Raw { text } => respell(text),
        }
    }

    /// Instance parameters. `m` is SPICE's device multiplier, which VACASK
    /// spells with the Verilog-A builtin.
    fn params(&self, params: &[(String, String)]) -> String {
        let mut s = String::new();
        for (k, v) in params {
            let k = k.to_ascii_lowercase();
            let k = if k == "m" { "$mfactor".to_string() } else { k };
            s.push_str(&format!(" {}={}", k, respell(v)));
        }
        s
    }

    /// `mag=`/`phase=` on an independent source.
    fn ac_spec(&self, mag: &Option<f64>, phase: &Option<f64>) -> String {
        let mut s = String::new();
        if let Some(m) = mag {
            s.push_str(&format!(" mag={}", num(*m)));
        }
        if let Some(p) = phase {
            s.push_str(&format!(" phase={}", num(*p)));
        }
        s
    }

    fn waveform(&self, name: &str, wf: &IrWaveform) -> Result<String, CodeGenError> {
        Ok(match wf {
            IrWaveform::Sin { offset, amplitude, frequency, delay, damping, phase } => {
                // VACASK accepts `sinephase` and does not apply it: the waveform
                // is byte-identical for 0, 45 and 90 degrees. `delay` and
                // `theta` beside it ARE honoured, which is what makes dropping
                // this one plausible and wrong — a 90-degree SIN would run and
                // sit 1.414 V away from what the deck asked for.
                if *phase != 0.0 {
                    return Err(unsupported_comp(format!(
                        "{}: SIN phase ({} deg) is accepted but ignored by VACASK",
                        name, phase
                    )));
                }
                format!(
                    "type=\"sine\" sinedc={} ampl={} freq={} delay={} theta={}",
                    num(*offset), num(*amplitude), num(*frequency), num(*delay), num(*damping),
                )
            }
            IrWaveform::Pulse { initial, pulsed, delay, rise_time, fall_time, pulse_width, period } => {
                // VACASK rejects a period that is not strictly greater than
                // rise+fall+width, while SPICE's own PER=PW default violates
                // exactly that. A zero period means "single pulse" to VACASK,
                // which is what SPICE means by PER >= TSTOP anyway.
                let sum = rise_time + fall_time + pulse_width;
                let period = if *period > 0.0 && *period <= sum { 0.0 } else { *period };
                format!(
                    "type=\"pulse\" val0={} val1={} delay={} rise={} fall={} width={} period={}",
                    num(*initial), num(*pulsed), num(*delay),
                    num(*rise_time), num(*fall_time), num(*pulse_width), num(period),
                )
            }
            IrWaveform::Exp { initial, pulsed, rise_delay, rise_tau, fall_delay, fall_tau } => {
                // SPICE's TD2 is an ABSOLUTE time ("start falling at t = TD2");
                // VACASK measures its `td2` from `delay`. Passed straight
                // through the fall starts TD1 late, which reads as an
                // integration disagreement rather than a different waveform.
                if fall_delay < rise_delay {
                    return Err(unsupported_comp(format!(
                        "{}: EXP fall delay ({}) precedes rise delay ({})",
                        name, fall_delay, rise_delay
                    )));
                }
                format!(
                    "type=\"exp\" val0={} val1={} delay={} tau1={} td2={} tau2={}",
                    num(*initial), num(*pulsed), num(*rise_delay),
                    num(*rise_tau), num(fall_delay - rise_delay), num(*fall_tau),
                )
            }
            IrWaveform::Sffm { offset, amplitude, carrier_freq, modulation_index, signal_freq } => {
                // The FM pedestal is `sinedc`, same as the sine's. `offset` is
                // also a real vsource parameter and is silently ignored on an
                // FM source, which is what makes the wrong spelling dangerous.
                format!(
                    "type=\"fm\" sinedc={} ampl={} freq={} modindex={} modfreq={}",
                    num(*offset), num(*amplitude), num(*carrier_freq),
                    num(*modulation_index), num(*signal_freq),
                )
            }
            IrWaveform::Am { amplitude, offset, modulating_freq, carrier_freq, delay } => {
                format!(
                    "type=\"am\" sinedc={} ampl={} freq={} modfreq={} delay={}",
                    num(*offset), num(*amplitude), num(*carrier_freq),
                    num(*modulating_freq), num(*delay),
                )
            }
            // `type="pwl" wave=[t0, v0, ...]` is the documented spelling and
            // VACASK does parse the list (`print instance` reads it back
            // correctly), but the transient it then runs is identically zero
            // with uninitialised memory at t=0 — measured 6.9e-310 V on a deck
            // whose source should have ramped to 1 V. A named refusal beats a
            // plausible wrong answer.
            IrWaveform::Pwl { .. } => {
                return Err(unsupported_comp(format!(
                    "{}: PWL sources are broken in VACASK (the transient comes back identically zero)",
                    name
                )));
            }
        })
    }

    /// `ic=[...]` / `nodeset=[...]`: a flat list, semicolon-separated, node
    /// names quoted and values bare.
    fn node_list(key: &str, items: &[(String, f64)]) -> String {
        let body: Vec<String> = items
            .iter()
            .map(|(n, v)| format!("\"{}\"; {}", n.to_ascii_lowercase(), num(*v)))
            .collect();
        format!(" {}=[{}]", key, body.join("; "))
    }

    /// One `sweep` line. `points` counts INTERVALS, not points, and the
    /// endpoint is RECOMPUTED rather than copied: SPICE walks from START by
    /// STEP and stops before passing STOP, while VACASK divides `from`..`to`
    /// into equal intervals and always lands ON `to`. Copying STOP across puts
    /// every sample on a different grid.
    fn sweep_line(scale: &str, target: &str, name: &str, param: &str, start: f64, stop: f64, step: f64)
        -> Result<String, CodeGenError>
    {
        if step == 0.0 {
            return Err(unsupported_analysis("sweep with a zero step"));
        }
        let span = ((stop - start) / step).abs();
        let n = (span + 1e-9 * span.max(1.0)).floor();
        if n < 1.0 || n.is_nan() || n > 1e7 {
            return Err(unsupported_analysis(format!(
                "sweep of '{}' covers {} intervals", name, n
            )));
        }
        let last = start + (stop - start).signum() * n * step.abs();
        Ok(format!(
            "sweep {} {}=\"{}\" parameter=\"{}\" from={} to={} mode=\"lin\" points={}",
            scale, target, name.to_ascii_lowercase(), param, num(start), num(last), n as u64,
        ))
    }

    /// `.step param` -> a control variable plus a sweep over it. A `var`
    /// declared in the control block IS visible to the netlist body, verified
    /// against the binary, so `r1 (a b) resistor r=rval` tracks the sweep.
    fn step_sweep(sp: &StepParam) -> Result<String, CodeGenError> {
        let name = sp.param.to_ascii_lowercase();
        let scale = format!("step_{}", sanitize(&name));
        match sp.sweep_type.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("dec") | Some("oct") => {
                let mode = sp.sweep_type.as_deref().unwrap().to_ascii_lowercase();
                Ok(format!(
                    "sweep {} variable=\"{}\" from={} to={} mode=\"{}\" points={}",
                    scale, name, num(sp.start), num(sp.stop), mode, sp.step as u64,
                ))
            }
            _ => Ok(format!(
                "sweep {} variable=\"{}\" from={} to={} step={}",
                scale, name, num(sp.start), num(sp.stop), num(sp.step),
            )),
        }
    }

    /// Frequency-swept analyses. For `dec`/`oct` SPICE's N is points per
    /// decade/octave and so is VACASK's; for `lin` SPICE's N is the TOTAL point
    /// count while VACASK counts intervals, hence the `- 1`.
    fn freq_sweep(variation: &str, points: u32, start: f64, stop: f64)
        -> Result<String, CodeGenError>
    {
        let mode = variation.to_ascii_lowercase();
        if !matches!(mode.as_str(), "dec" | "oct" | "lin") {
            return Err(unsupported_analysis(format!("frequency sweep type '{}'", variation)));
        }
        let n = if mode == "lin" { points.saturating_sub(1) } else { points };
        if n == 0 {
            return Err(unsupported_analysis(format!("frequency sweep with {} points", points)));
        }
        Ok(format!(
            "from={} to={} mode=\"{}\" points={}",
            num(start), num(stop), mode, n
        ))
    }

    /// The `analysis` statement plus any `sweep` lines that must precede it.
    fn analysis_block(&self, idx: usize, a: &Analysis) -> Result<Vec<String>, CodeGenError> {
        let name = format!("{}{}", a.kind_str(), idx + 1);
        let mut out = Vec::new();
        match a {
            Analysis::Op => out.push(format!("analysis {} op", name)),

            // VACASK has no `.dc`: a DC sweep is a `sweep` block wrapped around
            // an operating point. SPICE's FIRST source is the fast one and
            // VACASK's LAST `sweep` is, so the blocks emit in the opposite
            // order.
            Analysis::Dc { sweeps } => {
                if sweeps.is_empty() {
                    return Err(unsupported_analysis("dc with no swept source"));
                }
                for (i, sw) in sweeps.iter().enumerate().rev() {
                    let scale = if i == 0 { "vsweep".to_string() } else { format!("sweep{}", i) };
                    out.push(Self::sweep_line(
                        &scale, "instance", &sw.source, "dc", sw.start, sw.stop, sw.step,
                    )?);
                }
                out.push(format!("  analysis {} op", name));
            }

            Analysis::Ac { variation, points, start, stop } => {
                out.push(format!(
                    "analysis {} ac {}",
                    name, Self::freq_sweep(variation, *points, *start, *stop)?
                ));
            }

            // `step` is only a STARTING step in VACASK; `maxstep` is the
            // separate bound. SPICE's TSTEP also caps the internal step because
            // TMAX defaults to min(TSTEP, (TSTOP-TSTART)/50). Emitting `step`
            // alone lets VACASK take strides SPICE never would.
            Analysis::Transient { step, stop, start, max_step, uic } => {
                if start.unwrap_or(0.0) != 0.0 {
                    return Err(unsupported_analysis(
                        "transient with a nonzero start time (VACASK always saves from t=0)",
                    ));
                }
                let tmax = max_step.unwrap_or_else(|| step.min(stop / 50.0));
                let mut s = format!(
                    "analysis {} tran step={} stop={} maxstep={}",
                    name, num(*step), num(*stop), num(tmax)
                );
                s.push_str(if *uic { " icmode=\"uic\"" } else { " icmode=\"op\"" });
                out.push(s);
            }

            Analysis::Noise { output, reference, source, variation, points, start, stop, .. } => {
                if !reference.is_empty() && reference != "0" {
                    return Err(unsupported_analysis(
                        "noise with a non-ground reference node (VACASK's `out=` names one node)",
                    ));
                }
                out.push(format!(
                    "analysis {} noise out=\"{}\" in=\"{}\" {}",
                    name,
                    strip_wrapper(output),
                    source.to_ascii_lowercase(),
                    Self::freq_sweep(variation, *points, *start, *stop)?,
                ));
            }

            // VACASK's `dcxf` reports the transfer function from EVERY source
            // to `out` in one plot, so the input source is implicit.
            Analysis::Tf { output, .. } => {
                out.push(format!("analysis {} dcxf out=\"{}\"", name, strip_wrapper(output)));
            }

            other => {
                return Err(unsupported_analysis(other.kind_str().to_string()));
            }
        }
        Ok(out)
    }

    fn emit_model(&self, m: &ModelDef, loads: &mut BTreeSet<String>) -> Result<String, CodeGenError> {
        let kind = m.kind.to_ascii_lowercase();
        let name = m.name.to_ascii_lowercase();

        // LEVEL is the module SELECTOR, not a parameter, and defaults to 1.
        let mut level = 1u32;
        for (k, v) in &m.parameters {
            if k.eq_ignore_ascii_case("level") {
                level = value_of(v).map(|f| f as u32).ok_or_else(|| {
                    unsupported_comp(format!("model {}: LEVEL={}", name, v))
                })?;
            }
        }

        let (module, sign) = if kind == "nmos" || kind == "pmos" {
            let module = mos_module(level).ok_or_else(|| {
                unsupported_comp(format!("model {}: {} LEVEL={} has no VACASK module", name, kind, level))
            })?;
            (module, if kind.starts_with('n') { 1i8 } else { -1 })
        } else if let Some((module, sign)) = model_type(&kind) {
            // Every other type here is the LEVEL 1 device. `.model x NPN
            // (LEVEL=4)` is VBIC and `LEVEL=8` is HICUM: different equations
            // wearing the same keyword, and mapping on the keyword alone would
            // run them as Gummel-Poon.
            if level != 1 {
                return Err(unsupported_comp(format!(
                    "model {}: {} LEVEL={} has no VACASK module", name, kind, level
                )));
            }
            (module, sign)
        } else if matches!(kind.as_str(), "r" | "c" | "l" | "res" | "cap" | "ind" | "sw" | "csw") {
            // See `model_type`: the passive modules rename their model-card
            // parameters, so a name-for-name transcription would silently set
            // the instance parameter instead. Switches have no VACASK module.
            return Err(unsupported_comp(format!(
                "model {}: '{}' model cards have no faithful VACASK spelling", name, kind
            )));
        } else {
            // A module the user loaded themselves (`osdi_loads`). Pass it
            // through: VACASK errors loudly with "Model not found" if it is not
            // there, which beats guessing at a substitute.
            return Ok(format!("model {} {}{}", name, kind, self.model_params(&m.parameters, vec![])));
        };

        loads.insert(format!("spice/{}.osdi", module));
        // `type=` is the polarity selector, not a `.model` parameter, and it
        // also keeps an otherwise-empty list non-empty: `model x sp_diode ( )`
        // is a VACASK syntax error.
        let lead = if sign == 0 { vec![] } else { vec![format!("type={}", sign)] };
        Ok(format!("model {} sp_{}{}", name, module, self.model_params(&m.parameters, lead)))
    }

    fn model_params(&self, params: &[(String, String)], lead: Vec<String>) -> String {
        let body: Vec<String> = lead
            .into_iter()
            .chain(
                params
                    .iter()
                    .filter(|(k, _)| !k.eq_ignore_ascii_case("level"))
                    .map(|(k, v)| format!("{}={}", k.to_ascii_lowercase(), respell(v))),
            )
            .collect();
        if body.is_empty() { String::new() } else { format!(" ({})", body.join(" ")) }
    }
}

/// `V(out)` / `v(out)` / `i(v1)` -> `out` / `v1`. VACASK names a node by itself.
fn strip_wrapper(s: &str) -> String {
    let t = s.trim().to_ascii_lowercase();
    for p in ["v(", "i("] {
        if let Some(inner) = t.strip_prefix(p).and_then(|r| r.strip_suffix(')')) {
            return inner.trim().to_string();
        }
    }
    t
}

/// VACASK identifiers cannot carry punctuation.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

impl CodeGen for VacaskCodeGen {
    fn backend_name(&self) -> &str {
        "vacask"
    }

    fn emit_netlist(&self, ir: &CircuitIR) -> Result<String, CodeGenError> {
        let mut loads: BTreeSet<String> = BTreeSet::new();
        let mut needs: BTreeSet<Need> = BTreeSet::new();

        let subs: Vec<&Subcircuit> = std::iter::once(&ir.top).chain(ir.subcircuit_defs.iter()).collect();
        for sub in &subs {
            for comp in &sub.components {
                if let Some(n) = Self::need(comp) {
                    needs.insert(n);
                }
            }
        }
        if let Some(tb) = &ir.testbench {
            for comp in &tb.stimulus {
                if let Some(n) = Self::need(comp) {
                    needs.insert(n);
                }
            }
        }

        // Models first: they contribute loads too.
        let mut model_lines = Vec::new();
        for sub in &subs {
            for m in &sub.models {
                model_lines.push(self.emit_model(m, &mut loads)?);
            }
        }

        for n in &needs {
            if let Need::Sp(m) = n {
                loads.insert(format!("spice/{}.osdi", m));
            }
        }
        for path in &ir.top.osdi_loads {
            loads.insert(path.clone());
        }

        let title = ir.top.name.replace(['\n', '\r'], " ");
        let title = if title.trim().is_empty() { "circuit".to_string() } else { title };
        let mut lines = vec![title, String::new(), "ground 0".into(), String::new()];

        for path in &loads {
            lines.push(format!("load \"{}\"", path));
        }

        for inc in &ir.top.includes {
            lines.push(format!("include \"{}\"", inc));
        }
        for (path, section) in &ir.top.libs {
            lines.push(format!("include \"{}\" section={}", path, section));
        }
        for lib in &ir.model_libraries {
            for setup in &lib.setup_includes {
                lines.push(format!("include \"{}\"", setup));
            }
            let path = lib.backend_paths.get("vacask").unwrap_or(&lib.path);
            match &lib.corner {
                Some(corner) => lines.push(format!("include \"{}\" section={}", path, corner)),
                None => lines.push(format!("include \"{}\"", path)),
            }
        }
        lines.push(String::new());

        for n in &needs {
            let module = match n {
                Need::Sp(m) => format!("sp_{}", m),
                Need::Builtin(m) => m.to_string(),
            };
            // A user model wearing a synthesised name would be a duplicate
            // definition; VACASK takes the first and the deck then runs the
            // wrong device, so refuse instead.
            if subs.iter().any(|s| s.models.iter().any(|m| m.name.eq_ignore_ascii_case(n.auto_model()))) {
                return Err(CodeGenError::Other(format!(
                    "model '{}' collides with the model this backend synthesises for that device; rename it",
                    n.auto_model()
                )));
            }
            lines.push(format!("model {} {}", n.auto_model(), module));
        }
        // VACASK scopes a model to the block it is written in. The IR keeps
        // models on their subcircuit; hoisting them all to the top means a
        // model shared between definitions still resolves.
        lines.extend(model_lines);
        lines.push(String::new());

        // `.step param` names become control variables, so they must not also
        // be declared as netlist parameters.
        let stepped: BTreeSet<String> = ir
            .testbench
            .iter()
            .flat_map(|tb| tb.step_params.iter())
            .map(|sp| sp.param.to_ascii_lowercase())
            .collect();

        let top_params: Vec<&ParamDef> = ir
            .top
            .parameters
            .iter()
            .filter(|p| !stepped.contains(&p.name.to_ascii_lowercase()))
            .collect();
        if !top_params.is_empty() {
            let body: Vec<String> = top_params
                .iter()
                .map(|p| match &p.default {
                    Some(d) => format!("{}={}", p.name.to_ascii_lowercase(), respell(d)),
                    None => format!("{}=0", p.name.to_ascii_lowercase()),
                })
                .collect();
            lines.push(format!("parameters {}", body.join(" ")));
            lines.push(String::new());
        }

        for sc in &ir.subcircuit_defs {
            lines.push(self.emit_subcircuit(sc)?);
            lines.push(String::new());
        }

        for comp in &ir.top.components {
            lines.push(self.emit_component(comp)?);
        }
        for inst in &ir.top.instances {
            lines.push(self.emit_instance(inst));
        }
        for raw in &ir.top.raw_spice {
            lines.push(raw.clone());
        }

        if let Some(tb) = &ir.testbench {
            for comp in &tb.stimulus {
                lines.push(self.emit_component(comp)?);
            }
            lines.push(String::new());
            lines.push("control".into());

            let mut opt_line = String::from("  options rawfile=\"binary\"");
            let extra = self.emit_options(&tb.options)?;
            opt_line.push_str(&extra);
            if let Some(t) = tb.temperature {
                opt_line.push_str(&format!(" temp={}", num(t)));
            }
            if let Some(t) = tb.nominal_temperature {
                opt_line.push_str(&format!(" tnom={}", num(t)));
            }
            lines.push(opt_line);

            if !tb.measures.is_empty() {
                // "Command not found" — VACASK has no measure statement at all.
                return Err(unsupported_analysis(
                    ".measure (VACASK has no measurement statement; post-process the raw file)",
                ));
            }

            for sp in &tb.step_params {
                lines.push(format!("  var {}={}", sp.param.to_ascii_lowercase(), num(sp.start)));
            }

            let step_sweeps: Vec<String> = tb
                .step_params
                .iter()
                .map(Self::step_sweep)
                .collect::<Result<_, _>>()?;

            // IC/nodeset are analysis settings in VACASK, not standalone cards.
            let mut ic = String::new();
            if !tb.initial_conditions.is_empty() {
                ic.push_str(&Self::node_list("ic", &tb.initial_conditions));
            }
            if !tb.node_sets.is_empty() {
                ic.push_str(&Self::node_list("nodeset", &tb.node_sets));
            }

            // ponytail: `tb.saves` is deliberately dropped. VACASK saves every
            // node voltage and every source branch flow unless `strictsave=1`
            // is set, so a `save` line restricts nothing and only risks a parse
            // error on a SPICE-shaped output expression. Emit `save <expr>`
            // here if internal device outputs (`p(r1,i)`) are ever needed.
            for line in &tb.extra_lines {
                lines.push(format!("  {}", line));
            }

            for (idx, a) in tb.analyses.iter().enumerate() {
                // A `sweep` in VACASK binds to the next `analysis`, so the
                // stepped sweeps are repeated ahead of each one.
                for s in &step_sweeps {
                    lines.push(format!("  {}", s));
                }
                for l in self.analysis_block(idx, a)? {
                    let l = if l.trim_start().starts_with("analysis") {
                        format!("{}{}", l, ic)
                    } else {
                        l
                    };
                    lines.push(format!("  {}", l));
                }
            }
            lines.push("endc".into());
        }

        let mut out = lines.join("\n");
        out.push('\n');
        Ok(out)
    }

    fn emit_subcircuit(&self, sc: &Subcircuit) -> Result<String, CodeGenError> {
        let ports: Vec<String> = sc.ports.iter().map(|p| p.name.to_ascii_lowercase()).collect();
        let mut lines = vec![format!("subckt {} ({})", sc.name.to_ascii_lowercase(), ports.join(" "))];
        if !sc.parameters.is_empty() {
            let body: Vec<String> = sc
                .parameters
                .iter()
                .map(|p| match &p.default {
                    Some(d) => format!("{}={}", p.name.to_ascii_lowercase(), respell(d)),
                    None => format!("{}=0", p.name.to_ascii_lowercase()),
                })
                .collect();
            lines.push(format!("parameters {}", body.join(" ")));
        }
        for comp in &sc.components {
            lines.push(format!("  {}", self.emit_component(comp)?));
        }
        for inst in &sc.instances {
            lines.push(format!("  {}", self.emit_instance(inst)));
        }
        lines.push("ends".into());
        Ok(lines.join("\n"))
    }

    fn emit_component(&self, comp: &Component) -> Result<String, CodeGenError> {
        let auto = |n: Need| n.auto_model();
        let s = match comp {
            Component::Resistor { name, n1, n2, value, params } => format!(
                "r{} ({} {}) {} r={}{}",
                name.to_ascii_lowercase(), n1.to_ascii_lowercase(), n2.to_ascii_lowercase(),
                auto(Need::Sp("resistor")), self.value(value), self.params(params),
            ),
            Component::Capacitor { name, n1, n2, value, params } => format!(
                "c{} ({} {}) {} c={}{}",
                name.to_ascii_lowercase(), n1.to_ascii_lowercase(), n2.to_ascii_lowercase(),
                auto(Need::Sp("capacitor")), self.value(value), self.params(params),
            ),
            Component::Inductor { name, n1, n2, value, params } => format!(
                "l{} ({} {}) {} l={}{}",
                name.to_ascii_lowercase(), n1.to_ascii_lowercase(), n2.to_ascii_lowercase(),
                auto(Need::Sp("inductor")), self.value(value), self.params(params),
            ),
            // VACASK's mutual inductance is a device with NO nodes that names
            // the two inductors it couples.
            Component::MutualInductor { name, inductor1, inductor2, coupling } => format!(
                "k{} () {} k={} ind1=\"l{}\" ind2=\"l{}\"",
                name.to_ascii_lowercase(), auto(Need::Builtin("mutual")), num(*coupling),
                inductor1.to_ascii_lowercase(), inductor2.to_ascii_lowercase(),
            ),
            Component::VoltageSource { name, np, nm, value, ac_magnitude, ac_phase, waveform } => {
                let n = format!("v{}", name.to_ascii_lowercase());
                let mut s = format!(
                    "{} ({} {}) {} dc={}",
                    n, np.to_ascii_lowercase(), nm.to_ascii_lowercase(),
                    auto(Need::Builtin("vsource")), self.value(value),
                );
                s.push_str(&self.ac_spec(ac_magnitude, ac_phase));
                if let Some(wf) = waveform {
                    s.push_str(&format!(" {}", self.waveform(&n, wf)?));
                }
                s
            }
            Component::CurrentSource { name, np, nm, value, ac_magnitude, ac_phase, waveform } => {
                let n = format!("i{}", name.to_ascii_lowercase());
                let mut s = format!(
                    "{} ({} {}) {} dc={}",
                    n, np.to_ascii_lowercase(), nm.to_ascii_lowercase(),
                    auto(Need::Builtin("isource")), self.value(value),
                );
                s.push_str(&self.ac_spec(ac_magnitude, ac_phase));
                if let Some(wf) = waveform {
                    s.push_str(&format!(" {}", self.waveform(&n, wf)?));
                }
                s
            }
            Component::Vcvs { name, np, nm, ncp, ncm, gain } => format!(
                "e{} ({} {} {} {}) {} gain={}",
                name.to_ascii_lowercase(), np.to_ascii_lowercase(), nm.to_ascii_lowercase(),
                ncp.to_ascii_lowercase(), ncm.to_ascii_lowercase(),
                auto(Need::Builtin("vcvs")), num(*gain),
            ),
            Component::Vccs { name, np, nm, ncp, ncm, transconductance } => format!(
                "g{} ({} {} {} {}) {} gain={}",
                name.to_ascii_lowercase(), np.to_ascii_lowercase(), nm.to_ascii_lowercase(),
                ncp.to_ascii_lowercase(), ncm.to_ascii_lowercase(),
                auto(Need::Builtin("vccs")), num(*transconductance),
            ),
            // The current-controlled pair names its sensing device with
            // `ctlinst` instead of taking it as a node.
            Component::Cccs { name, np, nm, vsense, gain } => format!(
                "f{} ({} {}) {} ctlinst=\"{}\" gain={}",
                name.to_ascii_lowercase(), np.to_ascii_lowercase(), nm.to_ascii_lowercase(),
                auto(Need::Builtin("cccs")), vsense.to_ascii_lowercase(), num(*gain),
            ),
            Component::Ccvs { name, np, nm, vsense, transresistance } => format!(
                "h{} ({} {}) {} ctlinst=\"{}\" gain={}",
                name.to_ascii_lowercase(), np.to_ascii_lowercase(), nm.to_ascii_lowercase(),
                auto(Need::Builtin("ccvs")), vsense.to_ascii_lowercase(), num(*transresistance),
            ),
            Component::Diode { name, np, nm, model, params } => format!(
                "d{} ({} {}) {}{}",
                name.to_ascii_lowercase(), np.to_ascii_lowercase(), nm.to_ascii_lowercase(),
                model.to_ascii_lowercase(), self.params(params),
            ),
            Component::Bjt { name, nc, nb, ne, model, params } => format!(
                "q{} ({} {} {}) {}{}",
                name.to_ascii_lowercase(), nc.to_ascii_lowercase(), nb.to_ascii_lowercase(),
                ne.to_ascii_lowercase(), model.to_ascii_lowercase(), self.params(params),
            ),
            Component::Mosfet { name, nd, ng, ns, nb, model, params } => format!(
                "m{} ({} {} {} {}) {}{}",
                name.to_ascii_lowercase(), nd.to_ascii_lowercase(), ng.to_ascii_lowercase(),
                ns.to_ascii_lowercase(), nb.to_ascii_lowercase(),
                model.to_ascii_lowercase(), self.params(params),
            ),
            Component::Jfet { name, nd, ng, ns, model, params } => format!(
                "j{} ({} {} {}) {}{}",
                name.to_ascii_lowercase(), nd.to_ascii_lowercase(), ng.to_ascii_lowercase(),
                ns.to_ascii_lowercase(), model.to_ascii_lowercase(), self.params(params),
            ),
            Component::Mesfet { name, nd, ng, ns, model, params } => format!(
                "z{} ({} {} {}) {}{}",
                name.to_ascii_lowercase(), nd.to_ascii_lowercase(), ng.to_ascii_lowercase(),
                ns.to_ascii_lowercase(), model.to_ascii_lowercase(), self.params(params),
            ),
            // Raw text is passed through verbatim: it is the escape hatch for
            // native VACASK statements, and VACASK rejects anything else loudly.
            Component::RawSpice { line } => line.clone(),

            Component::BehavioralVoltage { name, .. } | Component::BehavioralCurrent { name, .. } => {
                return Err(unsupported_comp(format!(
                    "b{}: behavioural sources are an expression language VACASK does not share",
                    name.to_ascii_lowercase()
                )));
            }
            Component::VSwitch { name, .. } => {
                return Err(unsupported_comp(format!("s{}: VACASK has no voltage-controlled switch", name)));
            }
            Component::ISwitch { name, .. } => {
                return Err(unsupported_comp(format!("w{}: VACASK has no current-controlled switch", name)));
            }
            Component::TLine { name, .. } => {
                return Err(unsupported_comp(format!("t{}: VACASK has no lossless transmission line", name)));
            }
            Component::Xspice { name, .. } => {
                return Err(unsupported_comp(format!("a{}: XSPICE A-elements are ngspice-only", name)));
            }
        };
        Ok(s)
    }

    fn emit_analysis(&self, analysis: &Analysis) -> Result<String, CodeGenError> {
        Ok(self.analysis_block(0, analysis)?.join("\n"))
    }

    fn emit_options(&self, opts: &SimOptions) -> Result<String, CodeGenError> {
        let mut s = String::new();
        for (key, val) in &opts.portable {
            let k = key.to_ascii_lowercase();
            if NUMERIC_OPTIONS.contains(&k.as_str()) {
                s.push_str(&format!(" {}={}", k, respell(val)));
            } else if k == "method" {
                let m = match val.to_ascii_lowercase().as_str() {
                    "trap" | "trapezoidal" => "trap",
                    "gear" | "gear2" => "gear2",
                    other => {
                        return Err(unsupported_analysis(format!("options method={}", other)));
                    }
                };
                s.push_str(&format!(" tran_method=\"{}\"", m));
            } else if COSMETIC_OPTIONS.contains(&k.as_str()) {
                continue;
            } else {
                return Err(unsupported_analysis(format!(
                    "option '{}' has no known VACASK spelling (pass it through \
                     backend-specific options if you know VACASK's name for it)",
                    key
                )));
            }
        }
        if let Some(specific) = opts.backend_specific.get("vacask") {
            for (key, val) in specific {
                s.push_str(&format!(" {}={}", key, val));
            }
        }
        Ok(s)
    }
}

impl VacaskCodeGen {
    fn emit_instance(&self, inst: &Instance) -> String {
        let nodes: Vec<String> = inst.port_mapping.iter().map(|n| n.to_ascii_lowercase()).collect();
        format!(
            "x{} ({}) {}{}",
            inst.name.to_ascii_lowercase(),
            nodes.join(" "),
            inst.subcircuit.to_ascii_lowercase(),
            self.params(&inst.parameters),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sub(name: &str) -> Subcircuit {
        Subcircuit {
            name: name.into(), ports: vec![], parameters: vec![], components: vec![],
            instances: vec![], models: vec![], raw_spice: vec![], includes: vec![],
            libs: vec![], osdi_loads: vec![], verilog_blocks: vec![],
        }
    }

    fn tb(analyses: Vec<Analysis>) -> Testbench {
        Testbench {
            dut: "t".into(), stimulus: vec![], analyses, options: SimOptions::default(),
            saves: vec![], measures: vec![], temperature: None, nominal_temperature: None,
            initial_conditions: vec![], node_sets: vec![], step_params: vec![], extra_lines: vec![],
        }
    }

    #[test]
    fn spice_suffixes_are_respelled_for_vacask() {
        // SPICE `M` is milli, VACASK `M` is MEGA: a factor of 1e9 with nothing
        // to notice it. `1G` folded to lowercase is a VACASK parse error.
        assert_eq!(respell("1M"), "1m");
        assert_eq!(respell("1meg"), "1meg");
        assert_eq!(respell("1G"), "1G");
        assert_eq!(respell("10U"), "10u");
        assert_eq!(respell("{1k}"), "1k");
        assert_eq!(respell("myparam"), "myparam");
    }

    #[test]
    fn tran_always_carries_maxstep() {
        // `step` is only a starting step in VACASK; without `maxstep` it takes
        // strides SPICE never would.
        let cg = VacaskCodeGen;
        let a = Analysis::Transient { step: 1e-6, stop: 1e-3, start: None, max_step: None, uic: false };
        let s = cg.emit_analysis(&a).unwrap();
        assert!(s.contains("maxstep="), "{}", s);
        // min(TSTEP, TSTOP/50) = min(1e-6, 2e-5) = 1e-6
        assert!(s.contains("maxstep=1e-6"), "{}", s);
    }

    #[test]
    fn dc_sweep_counts_intervals_and_recomputes_the_endpoint() {
        let cg = VacaskCodeGen;
        // 0..10 by 0.5 is 21 SPICE samples => 20 intervals.
        let a = Analysis::Dc { sweeps: vec![DcSweep { source: "V1".into(), start: 0.0, stop: 10.0, step: 0.5 }] };
        let s = cg.emit_analysis(&a).unwrap();
        assert!(s.contains("points=20"), "{}", s);
        assert!(s.contains("instance=\"v1\" parameter=\"dc\""), "{}", s);

        // 0..1.1 by 0.2: SPICE stops at 1.0 without ever reaching 1.1.
        let a = Analysis::Dc { sweeps: vec![DcSweep { source: "Vgs".into(), start: 0.0, stop: 1.1, step: 0.2 }] };
        let s = cg.emit_analysis(&a).unwrap();
        assert!(s.contains("points=5"), "{}", s);
        assert!(s.contains("to=1e0"), "{}", s);
    }

    #[test]
    fn two_source_dc_emits_blocks_in_reverse() {
        let cg = VacaskCodeGen;
        let a = Analysis::Dc {
            sweeps: vec![
                DcSweep { source: "Vds".into(), start: 0.0, stop: 1.0, step: 0.1 },
                DcSweep { source: "Vgs".into(), start: 0.0, stop: 1.0, step: 0.5 },
            ],
        };
        let s = cg.emit_analysis(&a).unwrap();
        let gs = s.find("\"vgs\"").unwrap();
        let ds = s.find("\"vds\"").unwrap();
        // SPICE's first source is the fast one; VACASK's last sweep is.
        assert!(gs < ds, "outer sweep must come first:\n{}", s);
    }

    #[test]
    fn ac_lin_counts_intervals_but_dec_does_not() {
        let cg = VacaskCodeGen;
        let lin = cg.emit_analysis(&Analysis::Ac { variation: "lin".into(), points: 101, start: 1.0, stop: 1e3 }).unwrap();
        assert!(lin.contains("points=100"), "{}", lin);
        let dec = cg.emit_analysis(&Analysis::Ac { variation: "dec".into(), points: 10, start: 1.0, stop: 1e3 }).unwrap();
        assert!(dec.contains("points=10"), "{}", dec);
    }

    #[test]
    fn pwl_is_refused_rather_than_emitted() {
        let cg = VacaskCodeGen;
        let c = Component::VoltageSource {
            name: "1".into(), np: "a".into(), nm: "0".into(),
            value: IrValue::Numeric { value: 0.0 }, ac_magnitude: None, ac_phase: None,
            waveform: Some(IrWaveform::Pwl { values: vec![(0.0, 0.0), (1e-3, 1.0)] }),
        };
        let err = cg.emit_component(&c).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("pwl"), "{}", err);
    }

    #[test]
    fn measures_are_refused() {
        let mut t = tb(vec![Analysis::Op]);
        t.measures = vec!["tran vmax MAX V(out)".into()];
        let ir = CircuitIR { top: sub("m"), testbench: Some(t), subcircuit_defs: vec![], model_libraries: vec![] };
        assert!(VacaskCodeGen.emit_netlist(&ir).is_err());
    }

    #[test]
    fn step_params_become_a_control_variable_and_a_sweep() {
        let mut t = tb(vec![Analysis::Op]);
        t.step_params = vec![StepParam { param: "rval".into(), start: 1e3, stop: 3e3, step: 1e3, sweep_type: None }];
        let mut top = sub("stepped");
        top.parameters = vec![ParamDef { name: "rval".into(), default: Some("1k".into()) }];
        let ir = CircuitIR { top, testbench: Some(t), subcircuit_defs: vec![], model_libraries: vec![] };
        let n = VacaskCodeGen.emit_netlist(&ir).unwrap();
        assert!(n.contains("var rval=1e3"), "{}", n);
        assert!(n.contains("sweep step_rval variable=\"rval\""), "{}", n);
        // A stepped name must not also be a netlist parameter.
        assert!(!n.contains("parameters rval"), "{}", n);
    }

    #[test]
    fn model_library_uses_the_vacask_path() {
        let mut backend_paths = HashMap::new();
        backend_paths.insert("ngspice".into(), "/pdk/ngspice/sky130.lib.spice".into());
        backend_paths.insert("vacask".into(), "/pdk/vacask/sky130.sim".into());
        let ir = CircuitIR {
            top: sub("pdk_test"), testbench: None, subcircuit_defs: vec![],
            model_libraries: vec![ModelLibrary {
                name: "sky130".into(), path: "/pdk/default.lib".into(), corner: Some("tt".into()),
                backend_paths, setup_includes: vec![],
            }],
        };
        let n = VacaskCodeGen.emit_netlist(&ir).unwrap();
        assert!(n.contains("include \"/pdk/vacask/sky130.sim\" section=tt"), "{}", n);
        assert!(!n.contains("ngspice"), "{}", n);
    }

    #[test]
    fn model_library_falls_back_to_the_default_path() {
        let ir = CircuitIR {
            top: sub("fallback"), testbench: None, subcircuit_defs: vec![],
            model_libraries: vec![ModelLibrary {
                name: "custom".into(), path: "/models/custom.sim".into(), corner: None,
                backend_paths: HashMap::new(), setup_includes: vec![],
            }],
        };
        let n = VacaskCodeGen.emit_netlist(&ir).unwrap();
        assert!(n.contains("include \"/models/custom.sim\""), "{}", n);
    }

    #[test]
    fn bjt_model_carries_its_polarity_and_loads_its_module() {
        let mut top = sub("bjt");
        top.models = vec![ModelDef {
            name: "QN".into(), kind: "NPN".into(),
            parameters: vec![("IS".into(), "1e-15".into()), ("BF".into(), "150".into())],
        }];
        let ir = CircuitIR { top, testbench: None, subcircuit_defs: vec![], model_libraries: vec![] };
        let n = VacaskCodeGen.emit_netlist(&ir).unwrap();
        assert!(n.contains("load \"spice/bjt.osdi\""), "{}", n);
        assert!(n.contains("model qn sp_bjt (type=1 is=1e-15 bf=150)"), "{}", n);
    }

    #[test]
    fn a_mosfet_level_without_a_module_is_refused() {
        let mut top = sub("m");
        top.models = vec![ModelDef {
            name: "nm".into(), kind: "NMOS".into(), parameters: vec![("LEVEL".into(), "77".into())],
        }];
        let ir = CircuitIR { top, testbench: None, subcircuit_defs: vec![], model_libraries: vec![] };
        assert!(VacaskCodeGen.emit_netlist(&ir).is_err());
    }

    #[test]
    fn passive_model_cards_are_refused() {
        // sp_resistor renames the model card's parameters; transcribing them
        // name-for-name would set the instance parameters instead.
        let mut top = sub("r");
        top.models = vec![ModelDef { name: "rm".into(), kind: "R".into(), parameters: vec![("tc1".into(), "1e-3".into())] }];
        let ir = CircuitIR { top, testbench: None, subcircuit_defs: vec![], model_libraries: vec![] };
        assert!(VacaskCodeGen.emit_netlist(&ir).is_err());
    }

    #[test]
    fn unknown_options_refuse_instead_of_being_dropped() {
        let mut t = tb(vec![Analysis::Op]);
        t.options.portable = vec![("itl1".into(), "100".into())];
        let ir = CircuitIR { top: sub("o"), testbench: Some(t), subcircuit_defs: vec![], model_libraries: vec![] };
        assert!(VacaskCodeGen.emit_netlist(&ir).is_err());

        let mut t = tb(vec![Analysis::Op]);
        t.options.portable = vec![("reltol".into(), "1e-4".into()), ("noacct".into(), "".into())];
        let ir = CircuitIR { top: sub("o"), testbench: Some(t), subcircuit_defs: vec![], model_libraries: vec![] };
        let n = VacaskCodeGen.emit_netlist(&ir).unwrap();
        assert!(n.contains("reltol=1e-4"), "{}", n);
        assert!(!n.contains("noacct"), "{}", n);
    }
}
