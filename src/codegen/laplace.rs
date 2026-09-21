//! `Laplace(V(..), H(s))` behavioural sources -> syntax a simulator accepts.
//!
//! The `B<n> out 0 V=Laplace(v(in), H(s))` text the circuit builder accepts is
//! not the syntax of any simulator this crate drives. Emitted verbatim,
//! ngspice dies with `Undefined parameter [s]` and `exit(1)`. This module
//! parses the expression into a rational function of `s` and re-emits it as
//! something real.
//!
//! # ngspice — the XSPICE `s_xfer` code model (verified, 44.2)
//!
//! ```text
//! AB1 %vd(in 0) %vd(out 0) B1_xfer
//! .model B1_xfer s_xfer(num_coeff=[1] den_coeff=[1.591549431e-4 1] int_ic=[0])
//! ```
//!
//! Facts established by running ngspice 44.2, not by reading the manual:
//!
//! * **Coefficients are in DESCENDING powers of `s`.** `den_coeff=[1e-6 1]`
//!   means `1e-6*s + 1`. Probe deck: `num=[1e-3 1] den=[1e-6 1]` at 1 kHz gave
//!   `vm=6.362140, vp=1.406682 rad`, which is `(1+j6.2832)/(1+j0.006283)`
//!   exactly. The ascending reading predicts `vm~1.0, vp~0` and is ruled out.
//! * **Improper transfer functions are rejected**: `S_XFER: Numerator
//!   coefficient array size greater than denominator coefficiant array size.`
//!   So `deg(num) <= deg(den)` is a hard constraint, and a bare `s` or `s^2`
//!   has no `s_xfer` spelling.
//! * **A degree-0 denominator segfaults ngspice** (`den_coeff=[1]`, exit 139).
//!   A constant `H` is therefore emitted as a plain VCVS/VCCS instead, which
//!   is both safe and what anyone would have written by hand.
//! * `int_ic` needs `len(den) - 1` entries (one per integrator stage);
//!   `int_ic=[]` is rejected with `Array parameter must have at least one
//!   value`.
//! * `%id(np nm)` output matches the SPICE `B np nm I=` and `G np nm nc+ nc-`
//!   sign convention exactly (all three gave phase pi on the same probe deck).
//! * `s_xfer` contributes nothing at the DC operating point: a DC gain of 3
//!   driven by 2 V reports `v(out) = 0` under `.op`. AC and transient are
//!   correct; `.op` through a Laplace block is not. That is the code model's
//!   behaviour, not this translation's.
//!
//! # LTspice — `Laplace=` on E/G (UNVERIFIED)
//!
//! LTspice is not installed on this machine and cannot be. The syntax emitted,
//! `Exxx n+ n- nc+ nc- Laplace=<func(s)>`, is what the LTspice help file
//! documents under "E. Voltage Dependent Voltage Source" / "Laplace"; every
//! claim about it here is documentation, not observation. The expression is
//! still routed through the parser below, so LTspice can only ever receive a
//! rational function we have fully understood — never an unparsed string.

/// Scale suffixes SPICE allows on a literal. `meg`/`mil` must be matched
/// before `m` or a megohm becomes a milliohm.
const SCALES: &[(&str, f64)] = &[
    ("meg", 1e6), ("mil", 25.4e-6),
    ("t", 1e12), ("g", 1e9), ("k", 1e3), ("m", 1e-3),
    ("u", 1e-6), ("n", 1e-9), ("p", 1e-12), ("f", 1e-15), ("a", 1e-18),
];

/// What a `Laplace(...)` source turns into.
#[derive(Debug, Clone, PartialEq)]
pub enum Laplace {
    /// `H(s)` is a constant — emit a plain VCVS/VCCS of this gain.
    Gain { input: (String, String), gain: f64 },
    /// A genuine transfer function. `num`/`den` are in **descending** powers
    /// of `s`, with `den.len() >= 2` and `num.len() <= den.len()`.
    Xfer { input: (String, String), num: Vec<f64>, den: Vec<f64> },
}

impl Laplace {
    pub fn input(&self) -> &(String, String) {
        match self {
            Self::Gain { input, .. } | Self::Xfer { input, .. } => input,
        }
    }
}

/// Does this behavioural-source expression claim to be a Laplace transform?
///
/// Case-insensitive on purpose: SPICE is, and the two predicates this replaces
/// disagreed — `circuit.rs` matched `"Laplace("` while `ir/mod.rs` matched
/// `"laplace"`, so a capital-L expression set the routing flag on one path and
/// not the other.
pub fn is_laplace(expr: &str) -> bool {
    let lower = expr.to_ascii_lowercase();
    match lower.find("laplace") {
        Some(i) => lower[i + 7..].trim_start().starts_with('('),
        None => false,
    }
}

/// Parse `Laplace(V(a[,b]), H(s))` into something emittable.
///
/// `Err` is a human-readable reason, meant to be handed straight to the user
/// inside a `CodeGenError`. Anything not fully understood is an error: a
/// wrong deck is worse than a refused one.
pub fn parse(expr: &str) -> Result<Laplace, String> {
    let t = expr.trim();
    let lower = t.to_ascii_lowercase();
    let head = lower
        .strip_prefix("laplace")
        .ok_or_else(|| format!("expected `Laplace(input, H(s))`, got `{t}`"))?;
    let rest = head.trim_start();
    if !rest.starts_with('(') {
        return Err(format!("expected `Laplace(input, H(s))`, got `{t}`"));
    }
    let open = t.len() - rest.len();
    let close = match_paren(t, open)
        .ok_or_else(|| format!("unbalanced parentheses in `{t}`"))?;
    if !t[close + 1..].trim().is_empty() {
        return Err(format!(
            "`Laplace(...)` must be the whole expression; `{}` trails it. \
             Arithmetic around a Laplace block has no equivalent in any \
             backend — fold it into H(s) instead.",
            t[close + 1..].trim()
        ));
    }

    let args = split_top_level(&t[open + 1..close]);
    if args.len() != 2 {
        return Err(format!(
            "`Laplace` takes exactly 2 arguments (input, H(s)), got {}",
            args.len()
        ));
    }

    let input = parse_input(args[0].trim())?;
    let h = Rational::parse(args[1].trim())?;
    let (num, den) = h.descending();

    if den.len() < 2 {
        // Constant denominator. s_xfer segfaults on it; a VCVS is the answer.
        if num.len() > 1 {
            return Err(improper(&num, &den));
        }
        return Ok(Laplace::Gain { input, gain: num[0] / den[0] });
    }
    if num.len() > den.len() {
        return Err(improper(&num, &den));
    }
    Ok(Laplace::Xfer { input, num, den })
}

fn improper(num: &[f64], den: &[f64]) -> String {
    format!(
        "improper transfer function: numerator degree {} exceeds denominator \
         degree {}. ngspice's s_xfer rejects this outright (\"Numerator \
         coefficient array size greater than denominator coefficiant array \
         size\"), so a pure differentiator such as `s` or `s^2` has no \
         translation. Add poles (e.g. `s/(1+s*tau)`).",
        num.len() - 1,
        den.len() - 1
    )
}

/// `V(a)` / `V(a,b)`. Anything else is refused: `I(Vx)` would need a current
/// input port, which is a different physical quantity from a branch current.
fn parse_input(arg: &str) -> Result<(String, String), String> {
    let lower = arg.to_ascii_lowercase();
    let inner = lower
        .strip_prefix('v')
        .map(str::trim_start)
        .and_then(|r| r.strip_prefix('('))
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(|| {
            format!(
                "Laplace input must be `V(node)` or `V(node1,node2)`, got `{arg}`. \
                 A branch current `I(Vx)` cannot drive a transfer block: the \
                 code model's current input port measures node current, not \
                 the current through a named source."
            )
        })?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    match parts.as_slice() {
        [a] if !a.is_empty() => Ok((a.to_string(), "0".to_string())),
        [a, b] if !a.is_empty() && !b.is_empty() => Ok((a.to_string(), b.to_string())),
        _ => Err(format!("cannot read node names out of `{arg}`")),
    }
}

/// Index of the `)` matching the `(` at `open`.
fn match_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

// ── rational functions of s ──

/// `num/den`, both **ascending** in powers of `s` (index == power) because
/// that is the order polynomial arithmetic is natural in. Flipped to the
/// descending order `s_xfer` wants only on the way out.
#[derive(Debug, Clone, PartialEq)]
struct Rational {
    num: Vec<f64>,
    den: Vec<f64>,
}

fn poly_mul(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    out
}

fn poly_add(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len().max(b.len())];
    for (i, x) in a.iter().enumerate() {
        out[i] += x;
    }
    for (i, y) in b.iter().enumerate() {
        out[i] += y;
    }
    out
}

/// Drop exactly-zero high-order terms. The comparison is exact, not a
/// tolerance: a legitimate coefficient can be 2.5e-8 (a 1 kHz two-pole
/// denominator) and must survive.
fn trim(mut v: Vec<f64>) -> Vec<f64> {
    while v.len() > 1 && v[v.len() - 1] == 0.0 {
        v.pop();
    }
    v
}

fn is_zero(p: &[f64]) -> bool {
    p.iter().all(|c| *c == 0.0)
}

impl Rational {
    fn constant(v: f64) -> Self {
        Self { num: vec![v], den: vec![1.0] }
    }

    fn s() -> Self {
        Self { num: vec![0.0, 1.0], den: vec![1.0] }
    }

    fn as_constant(&self) -> Option<f64> {
        (self.num.len() == 1 && self.den.len() == 1).then(|| self.num[0] / self.den[0])
    }

    fn mul(self, o: Self) -> Self {
        Self { num: trim(poly_mul(&self.num, &o.num)), den: trim(poly_mul(&self.den, &o.den)) }
    }

    fn div(self, o: Self) -> Result<Self, String> {
        if is_zero(&o.num) {
            return Err("division by zero in H(s)".into());
        }
        Ok(Self { num: trim(poly_mul(&self.num, &o.den)), den: trim(poly_mul(&self.den, &o.num)) })
    }

    fn add(self, o: Self) -> Self {
        let num = poly_add(&poly_mul(&self.num, &o.den), &poly_mul(&o.num, &self.den));
        Self { num: trim(num), den: trim(poly_mul(&self.den, &o.den)) }
    }

    fn neg(self) -> Self {
        Self { num: self.num.iter().map(|c| -c).collect(), den: self.den }
    }

    fn powi(self, n: i32) -> Result<Self, String> {
        let base = if n < 0 { Rational::constant(1.0).div(self)? } else { self };
        let mut acc = Rational::constant(1.0);
        for _ in 0..n.unsigned_abs() {
            acc = acc.mul(base.clone());
        }
        Ok(acc)
    }

    /// `(num, den)` in descending powers, normalized so `den` leads with a
    /// nonzero coefficient.
    fn descending(&self) -> (Vec<f64>, Vec<f64>) {
        let flip = |p: &Vec<f64>| {
            let mut v = trim(p.clone());
            v.reverse();
            v
        };
        (flip(&self.num), flip(&self.den))
    }

    fn parse(src: &str) -> Result<Self, String> {
        let toks = lex(src)?;
        let mut p = Parser { toks: &toks, pos: 0, src };
        let r = p.expr()?;
        if p.pos != p.toks.len() {
            return Err(format!("unexpected `{}` in H(s) `{src}`", p.toks[p.pos].show()));
        }
        if is_zero(&r.den) {
            return Err(format!("H(s) `{src}` has a zero denominator"));
        }
        Ok(r)
    }
}

// ── expression parsing ──

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    S,
    Op(char),
    Pow,
    LParen,
    RParen,
    Ident(String),
}

impl Tok {
    fn show(&self) -> String {
        match self {
            Tok::Num(v) => v.to_string(),
            Tok::S => "s".into(),
            Tok::Op(c) => c.to_string(),
            Tok::Pow => "^".into(),
            Tok::LParen => "(".into(),
            Tok::RParen => ")".into(),
            Tok::Ident(n) => n.clone(),
        }
    }
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '(' {
            out.push(Tok::LParen);
            i += 1;
        } else if c == ')' {
            out.push(Tok::RParen);
            i += 1;
        } else if c == '^' {
            out.push(Tok::Pow);
            i += 1;
        } else if c == '*' && i + 1 < b.len() && b[i + 1] == '*' {
            out.push(Tok::Pow);
            i += 2;
        } else if "+-*/".contains(c) {
            out.push(Tok::Op(c));
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let (tok, next) = lex_number(&b, i)?;
            out.push(tok);
            i = next;
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            let word: String = b[start..i].iter().collect();
            if word.eq_ignore_ascii_case("s") {
                out.push(Tok::S);
            } else {
                out.push(Tok::Ident(word));
            }
        } else {
            return Err(format!("unexpected character `{c}` in H(s) `{src}`"));
        }
    }
    Ok(out)
}

fn lex_number(b: &[char], start: usize) -> Result<(Tok, usize), String> {
    let mut i = start;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == '.') {
        i += 1;
    }
    // An exponent's `e` belongs to the number only when a digit or sign
    // follows; a bare `1e` is "1 exa" in SPICE, and `1e-4*s` must not swallow
    // the `*`.
    if i < b.len()
        && (b[i] == 'e' || b[i] == 'E')
        && i + 1 < b.len()
        && (b[i + 1].is_ascii_digit() || b[i + 1] == '+' || b[i + 1] == '-')
    {
        i += 2;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    let text: String = b[start..i].iter().collect();
    let mantissa: f64 = text
        .parse()
        .map_err(|_| format!("cannot read number `{text}` in H(s)"))?;
    // Scale suffix, longest match first.
    let tail: String = b[i..].iter().collect::<String>().to_ascii_lowercase();
    for (suffix, mult) in SCALES {
        if tail.starts_with(suffix) {
            return Ok((Tok::Num(mantissa * mult), i + suffix.len()));
        }
    }
    Ok((Tok::Num(mantissa), i))
}

struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
    src: &'a str,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn expr(&mut self) -> Result<Rational, String> {
        let mut lhs = self.term()?;
        while let Some(Tok::Op(c @ ('+' | '-'))) = self.peek() {
            let minus = *c == '-';
            self.pos += 1;
            let rhs = self.term()?;
            lhs = lhs.add(if minus { rhs.neg() } else { rhs });
        }
        Ok(lhs)
    }

    fn term(&mut self) -> Result<Rational, String> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Op(c @ ('*' | '/'))) = self.peek() {
            let divide = *c == '/';
            self.pos += 1;
            let rhs = self.unary()?;
            lhs = if divide { lhs.div(rhs)? } else { lhs.mul(rhs) };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Rational, String> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.pos += 1;
                Ok(self.unary()?.neg())
            }
            Some(Tok::Op('+')) => {
                self.pos += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Rational, String> {
        let base = self.atom()?;
        if let Some(Tok::Pow) = self.peek() {
            self.pos += 1;
            let exp = self.unary()?;
            let n = exp.as_constant().ok_or_else(|| {
                format!("exponent in H(s) `{}` must be a constant", self.src)
            })?;
            if n.fract() != 0.0 || n.abs() > 16.0 {
                return Err(format!(
                    "exponent {n} in H(s) `{}` must be a whole number in -16..16 \
                     (a fractional power of s is not a rational transfer function)",
                    self.src
                ));
            }
            return base.powi(n as i32);
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Rational, String> {
        match self.peek().cloned() {
            Some(Tok::Num(v)) => {
                self.pos += 1;
                Ok(Rational::constant(v))
            }
            Some(Tok::S) => {
                self.pos += 1;
                Ok(Rational::s())
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                let inner = self.expr()?;
                match self.peek() {
                    Some(Tok::RParen) => {
                        self.pos += 1;
                        Ok(inner)
                    }
                    _ => Err(format!("missing `)` in H(s) `{}`", self.src)),
                }
            }
            Some(Tok::Ident(name)) => Err(format!(
                "H(s) `{}` refers to `{name}`. Only numeric coefficients and \
                 `s` can be translated — a symbolic parameter would have to be \
                 guessed at, and the backends take coefficient arrays, not \
                 expressions. Substitute the value.",
                self.src
            )),
            Some(t) => Err(format!("unexpected `{}` in H(s) `{}`", t.show(), self.src)),
            None => Err(format!("H(s) `{}` ends early", self.src)),
        }
    }
}

// ── rendering ──

/// `{:e}` and nothing clever: `format_spice_number` would turn 1e6 into
/// `1meg`, which is fine in a netlist but noise inside a coefficient array.
fn coeff(v: f64) -> String {
    format!("{:e}", v)
}

/// Coefficient list for an `s_xfer` array parameter, descending.
pub fn coeff_array(c: &[f64]) -> String {
    c.iter().map(|v| coeff(*v)).collect::<Vec<_>>().join(" ")
}

/// Descending coefficients as a Horner-nested polynomial in `s`:
/// `[1e-3, 1] -> (1e-3*s+1e0)`. Horner avoids `**`/`^`, whose spelling differs
/// between expression dialects, and keeps the text short for high orders.
pub fn horner(c: &[f64]) -> String {
    let mut out = coeff(c[0]);
    for v in &c[1..] {
        out = format!("({out}*s+{})", coeff(*v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xfer(e: &str) -> (Vec<f64>, Vec<f64>) {
        match parse(e).unwrap() {
            Laplace::Xfer { num, den, .. } => (num, den),
            other => panic!("expected Xfer, got {other:?}"),
        }
    }

    fn approx(a: &[f64], b: &[f64]) {
        assert_eq!(a.len(), b.len(), "{a:?} vs {b:?}");
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() <= 1e-12 * y.abs().max(1e-12), "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn detects_any_case() {
        assert!(is_laplace("Laplace(v(in), 1/(1+s))"));
        assert!(is_laplace("LAPLACE (v(in), 1/(1+s))"));
        assert!(is_laplace("laplace(v(in), 1/(1+s))"));
        assert!(!is_laplace("v(in)*2"));
        // `laplace` without a call is a variable name, not a transform.
        assert!(!is_laplace("laplace_gain*v(in)"));
    }

    #[test]
    fn single_pole() {
        // 1/(1 + s*1.59e-4) -> descending den [1.59e-4, 1]
        let (num, den) = xfer("Laplace(v(in), 1/(1+s*1.59e-4))");
        approx(&num, &[1.0]);
        approx(&den, &[1.59e-4, 1.0]);
    }

    #[test]
    fn lead_lag() {
        let (num, den) = xfer("Laplace(V(in), (1+s*1e-3)/(1+s*1e-6))");
        approx(&num, &[1e-3, 1.0]);
        approx(&den, &[1e-6, 1.0]);
    }

    #[test]
    fn second_order() {
        let (num, den) = xfer("Laplace(v(in), 4/(1+s*2+s^2*3))");
        approx(&num, &[4.0]);
        approx(&den, &[3.0, 2.0, 1.0]);
    }

    #[test]
    fn power_forms_agree() {
        assert_eq!(xfer("Laplace(v(a), 1/(1+s^2))"), xfer("Laplace(v(a), 1/(1+s**2))"));
        assert_eq!(xfer("Laplace(v(a), 1/(1+s^2))"), xfer("Laplace(v(a), 1/(1+s*s))"));
    }

    #[test]
    fn differentiator_with_a_pole_is_fine() {
        let (num, den) = xfer("Laplace(v(in), s/(1+s*1e-3))");
        approx(&num, &[1.0, 0.0]);
        approx(&den, &[1e-3, 1.0]);
    }

    #[test]
    fn scale_suffixes() {
        let (_, den) = xfer("Laplace(v(in), 1/(1+s*159u))");
        approx(&den, &[159e-6, 1.0]);
        // meg before m: this is 1e6, not 1e-3. No common-factor reduction is
        // done, so 1/(1+s/1e6) comes out as 1e6/(s+1e6) — the same function.
        let (num, den) = xfer("Laplace(v(in), 1/(1+s/1meg))");
        approx(&num, &[1e6]);
        approx(&den, &[1.0, 1e6]);
        let (_, den) = xfer("Laplace(v(in), 1/(1+s*1m))");
        approx(&den, &[1e-3, 1.0]);
    }

    #[test]
    fn differential_input() {
        assert_eq!(
            parse("Laplace(V(a,b), 1/(1+s))").unwrap().input(),
            &("a".to_string(), "b".to_string())
        );
        assert_eq!(
            parse("Laplace(V(a), 1/(1+s))").unwrap().input(),
            &("a".to_string(), "0".to_string())
        );
    }

    #[test]
    fn constant_is_a_gain() {
        assert_eq!(
            parse("Laplace(v(in), 2.5)").unwrap(),
            Laplace::Gain { input: ("in".into(), "0".into()), gain: 2.5 }
        );
        // Ratios of constants collapse too.
        assert_eq!(
            parse("Laplace(v(in), 10/4)").unwrap(),
            Laplace::Gain { input: ("in".into(), "0".into()), gain: 2.5 }
        );
    }

    #[test]
    fn refuses_symbolic_parameters() {
        let e = parse("Laplace(v(in), 1/(1+s*tau))").unwrap_err();
        assert!(e.contains("tau"), "{e}");
        assert!(e.contains("Substitute the value"), "{e}");
    }

    #[test]
    fn refuses_improper_transfer_functions() {
        for h in ["s", "s^2", "s*2", "(1+s*1e-3)*s"] {
            let e = parse(&format!("Laplace(v(in), {h})")).unwrap_err();
            assert!(e.contains("improper"), "{h}: {e}");
        }
    }

    #[test]
    fn refuses_current_input() {
        let e = parse("Laplace(I(V1), 1/(1+s))").unwrap_err();
        assert!(e.contains("V(node)"), "{e}");
    }

    #[test]
    fn refuses_arithmetic_around_the_block() {
        let e = parse("2*Laplace(v(in), 1/(1+s))").unwrap_err();
        assert!(e.contains("expected `Laplace("), "{e}");
        let e = parse("Laplace(v(in), 1/(1+s)) + 1").unwrap_err();
        assert!(e.contains("whole expression"), "{e}");
    }

    #[test]
    fn refuses_zero_denominator() {
        assert!(parse("Laplace(v(in), 1/0)").unwrap_err().contains("zero"));
        assert!(parse("Laplace(v(in), 1/(s-s))").unwrap_err().contains("zero"));
    }

    #[test]
    fn refuses_junk() {
        assert!(parse("Laplace(v(in))").is_err());
        assert!(parse("Laplace(v(in), 1/(1+s)").is_err());
        assert!(parse("Laplace(v(in), )").is_err());
        assert!(parse("Laplace(v(in), 1 1)").is_err());
        assert!(parse("Laplace(v(in), s^0.5/(1+s))").is_err());
    }

    #[test]
    fn rendering_round_trips() {
        assert_eq!(coeff_array(&[1.59e-4, 1.0]), "1.59e-4 1e0");
        assert_eq!(horner(&[1.0]), "1e0");
        assert_eq!(horner(&[1e-3, 1.0]), "(1e-3*s+1e0)");
        assert_eq!(horner(&[3.0, 2.0, 1.0]), "((3e0*s+2e0)*s+1e0)");
    }
}
