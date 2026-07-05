//! Protocol engine — a single concrete, transport-independent codec driven by
//! [`ProtocolSpec`] data (ADR 0010 K3). Encodes a position into command bytes
//! and decodes a readout back into a position. **Pure**: no serial I/O, fully
//! fixture-testable without hardware (the F8 key insight).
//!
//! Not a trait — one struct parameterized by data (AGENTS §1.4: a trait needs
//! 2+ real impls; the rotor backend trait is `RotorBackend` in F8.3).
//!
//! Numeric mapping follows `docs/calculations.md` §8.2:
//! encode `raw = round((value − offset) / scale)`, decode `value = raw·scale + offset`.

use thiserror::Error;

use super::spec::{Operation, ProtocolSpec};

/// A rotor pointing position in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotorPosition {
    pub az_deg: f64,
    pub el_deg: f64,
}

#[derive(Debug, Error, PartialEq)]
pub enum ProtocolError {
    #[error("unknown template field: {0}")]
    UnknownField(String),
    #[error("bad format spec: {0}")]
    BadFormatSpec(String),
    #[error("unterminated token in template")]
    UnterminatedToken,
    #[error("operation requires a position")]
    MissingPosition,
    #[error("response parse failed: {0}")]
    ParseFailed(String),
    #[error("response bytes are not valid UTF-8")]
    NotUtf8,
}

/// One piece of a parsed template.
#[derive(Debug, Clone, PartialEq)]
enum Segment {
    Literal(String),
    Token { field: Field, fmt: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Field {
    Az,
    El,
}

impl Field {
    fn parse(name: &str) -> Result<Field, ProtocolError> {
        match name {
            "az" => Ok(Field::Az),
            "el" => Ok(Field::El),
            other => Err(ProtocolError::UnknownField(other.to_string())),
        }
    }
}

pub struct ProtocolEngine {
    spec: ProtocolSpec,
}

impl ProtocolEngine {
    pub fn new(spec: ProtocolSpec) -> Self {
        Self { spec }
    }

    pub fn spec(&self) -> &ProtocolSpec {
        &self.spec
    }

    /// Validate the spec's templates without any I/O: all four templates must
    /// parse, and the set/response templates must each carry both an az and an
    /// el token (a position command/readout needs both axes).
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for tmpl in [
            &self.spec.set_template,
            &self.spec.query_template,
            &self.spec.stop_template,
            &self.spec.response_pattern,
        ] {
            parse_template(tmpl)?;
        }
        for (label, tmpl) in [
            ("set_template", &self.spec.set_template),
            ("response_pattern", &self.spec.response_pattern),
        ] {
            let segs = parse_template(tmpl)?;
            let has_az = segs.iter().any(|s| {
                matches!(
                    s,
                    Segment::Token {
                        field: Field::Az,
                        ..
                    }
                )
            });
            let has_el = segs.iter().any(|s| {
                matches!(
                    s,
                    Segment::Token {
                        field: Field::El,
                        ..
                    }
                )
            });
            if !has_az || !has_el {
                return Err(ProtocolError::ParseFailed(format!(
                    "{label} must contain both az and el tokens"
                )));
            }
        }
        if !self.spec.scale.is_finite() || self.spec.scale == 0.0 || !self.spec.offset.is_finite() {
            return Err(ProtocolError::BadFormatSpec(
                "scale must be finite non-zero and offset finite".to_string(),
            ));
        }
        Ok(())
    }

    /// Encode an operation into command bytes. `pos` is required for
    /// `SetPosition` (and any template carrying tokens).
    pub fn encode(
        &self,
        op: Operation,
        pos: Option<RotorPosition>,
    ) -> Result<Vec<u8>, ProtocolError> {
        let template = match op {
            Operation::SetPosition => &self.spec.set_template,
            Operation::QueryPosition => &self.spec.query_template,
            Operation::Stop => &self.spec.stop_template,
        };
        self.render(template, pos)
    }

    /// Render the position into the **device readout** shape (`response_pattern`).
    /// Used to simulate the reply a device would send — the encode→decode
    /// loopback (ADR 0010 K4) that proves the codec without hardware.
    pub fn encode_readout(&self, pos: RotorPosition) -> Result<Vec<u8>, ProtocolError> {
        self.render(&self.spec.response_pattern, Some(pos))
    }

    /// Render a token template with an optional position into bytes.
    fn render(&self, template: &str, pos: Option<RotorPosition>) -> Result<Vec<u8>, ProtocolError> {
        let segments = parse_template(template)?;
        let mut out = String::new();
        for seg in &segments {
            match seg {
                Segment::Literal(text) => out.push_str(text),
                Segment::Token { field, fmt } => {
                    let position = pos.ok_or(ProtocolError::MissingPosition)?;
                    let value = match field {
                        Field::Az => position.az_deg,
                        Field::El => position.el_deg,
                    };
                    // calc §8.2: raw = (value − offset) / scale. Quantization to
                    // the protocol's numeric precision is done by the token
                    // format spec (e.g. %03.0f → 1°, %.1f → 0.1°), not a fixed
                    // round-to-integer — otherwise sub-degree protocols lose
                    // their fractional part.
                    let raw = (value - self.spec.offset) / self.spec.scale;
                    out.push_str(&format_value(raw, fmt.as_deref())?);
                }
            }
        }
        Ok(out.into_bytes())
    }

    /// Decode a position readout. Trailing bytes after the pattern are ignored
    /// (line terminators are not part of `response_pattern`).
    pub fn decode(&self, bytes: &[u8]) -> Result<RotorPosition, ProtocolError> {
        let text = std::str::from_utf8(bytes).map_err(|_| ProtocolError::NotUtf8)?;
        let input: Vec<char> = text.chars().collect();
        let segments = parse_template(&self.spec.response_pattern)?;

        let mut pos = 0usize;
        let mut az: Option<f64> = None;
        let mut el: Option<f64> = None;

        for seg in &segments {
            match seg {
                Segment::Literal(text) => {
                    let lit: Vec<char> = text.chars().collect();
                    if pos + lit.len() > input.len() || input[pos..pos + lit.len()] != lit[..] {
                        return Err(ProtocolError::ParseFailed(format!(
                            "expected literal {text:?} at offset {pos}"
                        )));
                    }
                    pos += lit.len();
                }
                Segment::Token { field, .. } => {
                    let (raw, next) = scan_number(&input, pos).ok_or_else(|| {
                        ProtocolError::ParseFailed(format!("no number at offset {pos}"))
                    })?;
                    // calc §8.2: value = raw·scale + offset
                    let value = raw * self.spec.scale + self.spec.offset;
                    match field {
                        Field::Az => az = Some(value),
                        Field::El => el = Some(value),
                    }
                    pos = next;
                }
            }
        }

        match (az, el) {
            (Some(az_deg), Some(el_deg)) => Ok(RotorPosition { az_deg, el_deg }),
            _ => Err(ProtocolError::ParseFailed(
                "response did not yield both az and el".to_string(),
            )),
        }
    }
}

/// Split a template into literal and `{field|fmt}` token segments.
fn parse_template(template: &str) -> Result<Vec<Segment>, ProtocolError> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }
            let mut inner = String::new();
            let mut closed = false;
            for ic in chars.by_ref() {
                if ic == '}' {
                    closed = true;
                    break;
                }
                inner.push(ic);
            }
            if !closed {
                return Err(ProtocolError::UnterminatedToken);
            }
            let (field_name, fmt) = match inner.split_once('|') {
                Some((f, spec)) => (f.to_string(), Some(spec.to_string())),
                None => (inner, None),
            };
            let field = Field::parse(&field_name)?;
            segments.push(Segment::Token { field, fmt });
        } else {
            literal.push(c);
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    Ok(segments)
}

/// Format a numeric value per a printf subset `%[0][+][width][.prec]f`.
/// `None` → plain integer (prec 0, no pad, no sign).
fn format_value(value: f64, spec: Option<&str>) -> Result<String, ProtocolError> {
    let (zero, plus, width, prec) = match spec {
        None => (false, false, 0usize, 0usize),
        Some(s) => parse_fmt(s)?,
    };

    let neg = value.is_sign_negative() && value != 0.0;
    let abs_str = format!("{:.*}", prec, value.abs());
    let sign = if neg {
        "-"
    } else if plus {
        "+"
    } else {
        ""
    };

    let total = sign.len() + abs_str.len();
    if width > total {
        let pad = width - total;
        if zero {
            Ok(format!("{sign}{}{abs_str}", "0".repeat(pad)))
        } else {
            Ok(format!("{}{sign}{abs_str}", " ".repeat(pad)))
        }
    } else {
        Ok(format!("{sign}{abs_str}"))
    }
}

/// Parse a `%[0][+][width][.prec]f` spec → (zero_pad, force_sign, width, prec).
fn parse_fmt(spec: &str) -> Result<(bool, bool, usize, usize), ProtocolError> {
    let bad = || ProtocolError::BadFormatSpec(spec.to_string());
    let bytes = spec.as_bytes();
    if bytes.first() != Some(&b'%') || bytes.last() != Some(&b'f') {
        return Err(bad());
    }
    let body = &spec[1..spec.len() - 1];
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;

    let mut zero = false;
    let mut plus = false;
    while i < chars.len() && (chars[i] == '0' || chars[i] == '+') {
        if chars[i] == '0' {
            zero = true;
        } else {
            plus = true;
        }
        i += 1;
    }

    let mut width = 0usize;
    let mut saw_width = false;
    while i < chars.len() && chars[i].is_ascii_digit() {
        width = width * 10 + (chars[i] as usize - '0' as usize);
        saw_width = true;
        i += 1;
    }
    let _ = saw_width;

    let mut prec = 0usize;
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let mut saw = false;
        while i < chars.len() && chars[i].is_ascii_digit() {
            prec = prec * 10 + (chars[i] as usize - '0' as usize);
            saw = true;
            i += 1;
        }
        if !saw {
            return Err(bad());
        }
    }

    if i != chars.len() {
        return Err(bad());
    }
    Ok((zero, plus, width, prec))
}

/// Scan a leading number (optional sign, digits, one dot) from `input[pos..]`.
/// Returns the parsed value and the index just past the number.
fn scan_number(input: &[char], pos: usize) -> Option<(f64, usize)> {
    let mut i = pos;
    if i < input.len() && (input[i] == '+' || input[i] == '-') {
        i += 1;
    }
    let mut seen_digit = false;
    let mut seen_dot = false;
    while i < input.len() {
        let c = input[i];
        if c.is_ascii_digit() {
            seen_digit = true;
            i += 1;
        } else if c == '.' && !seen_dot {
            seen_dot = true;
            i += 1;
        } else {
            break;
        }
    }
    if !seen_digit {
        return None;
    }
    let num: String = input[pos..i].iter().collect();
    num.parse::<f64>().ok().map(|v| (v, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- format_value -----------------------------------------------------

    #[test]
    fn format_zero_pad_width() {
        assert_eq!(format_value(180.0, Some("%03.0f")).unwrap(), "180");
        assert_eq!(format_value(5.0, Some("%03.0f")).unwrap(), "005");
    }

    #[test]
    fn format_force_sign_zero_pad() {
        assert_eq!(format_value(180.0, Some("%+04.0f")).unwrap(), "+180");
        assert_eq!(format_value(5.0, Some("%+04.0f")).unwrap(), "+005");
        assert_eq!(format_value(-5.0, Some("%+04.0f")).unwrap(), "-005");
    }

    #[test]
    fn format_precision() {
        assert_eq!(format_value(45.5, Some("%.1f")).unwrap(), "45.5");
        assert_eq!(format_value(180.0, Some("%.1f")).unwrap(), "180.0");
    }

    #[test]
    fn format_rejects_bad_spec() {
        assert!(matches!(
            format_value(1.0, Some("%03d")),
            Err(ProtocolError::BadFormatSpec(_))
        ));
        assert!(matches!(
            format_value(1.0, Some("03.0f")),
            Err(ProtocolError::BadFormatSpec(_))
        ));
    }

    // --- parse_template ---------------------------------------------------

    #[test]
    fn parse_template_literal_and_tokens() {
        let segs = parse_template("AZ={az|%03.0f} EL={el|%03.0f}").unwrap();
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0], Segment::Literal("AZ=".to_string()));
        assert!(matches!(
            segs[1],
            Segment::Token {
                field: Field::Az,
                ..
            }
        ));
        assert_eq!(segs[2], Segment::Literal(" EL=".to_string()));
        assert!(matches!(
            segs[3],
            Segment::Token {
                field: Field::El,
                ..
            }
        ));
    }

    #[test]
    fn parse_template_rejects_unterminated() {
        assert!(matches!(
            parse_template("AZ={az"),
            Err(ProtocolError::UnterminatedToken)
        ));
    }

    #[test]
    fn parse_template_rejects_unknown_field() {
        assert!(matches!(
            parse_template("{foo}"),
            Err(ProtocolError::UnknownField(_))
        ));
    }

    // --- encode (per preset) ---------------------------------------------

    fn pos(az: f64, el: f64) -> RotorPosition {
        RotorPosition {
            az_deg: az,
            el_deg: el,
        }
    }

    #[test]
    fn encode_gs232_set() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232b());
        let bytes = eng
            .encode(Operation::SetPosition, Some(pos(180.0, 45.0)))
            .unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "W180 045\r");
    }

    #[test]
    fn encode_query_and_stop_need_no_position() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232b());
        assert_eq!(
            String::from_utf8(eng.encode(Operation::QueryPosition, None).unwrap()).unwrap(),
            "C2\r"
        );
        assert_eq!(
            String::from_utf8(eng.encode(Operation::Stop, None).unwrap()).unwrap(),
            "S\r"
        );
    }

    #[test]
    fn encode_set_without_position_errors() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232b());
        assert!(matches!(
            eng.encode(Operation::SetPosition, None),
            Err(ProtocolError::MissingPosition)
        ));
    }

    #[test]
    fn encode_easycomm2_set() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_easycomm2());
        let bytes = eng
            .encode(Operation::SetPosition, Some(pos(180.0, 45.5)))
            .unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "AZ180.0 EL45.5\n");
    }

    #[test]
    fn encode_gs232a_response_shape_uses_signed_pad() {
        // GS-232A response_pattern is also a valid encode template.
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232a());
        let segs = parse_template(&eng.spec().response_pattern).unwrap();
        // Two adjacent signed tokens, no literal between.
        assert_eq!(segs.len(), 2);
    }

    // --- decode (per preset) ---------------------------------------------

    #[test]
    fn decode_gs232b() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232b());
        let p = eng.decode(b"AZ=180 EL=045").unwrap();
        assert_eq!(p, pos(180.0, 45.0));
    }

    #[test]
    fn decode_gs232b_ignores_trailing() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232b());
        let p = eng.decode(b"AZ=180 EL=045\r\n").unwrap();
        assert_eq!(p, pos(180.0, 45.0));
    }

    #[test]
    fn decode_gs232a_adjacent_signed() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232a());
        let p = eng.decode(b"+0180+0090").unwrap();
        assert_eq!(p, pos(180.0, 90.0));
    }

    #[test]
    fn decode_easycomm2() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_easycomm2());
        let p = eng.decode(b"AZ180.5 EL045.2").unwrap();
        assert_eq!(p, pos(180.5, 45.2));
    }

    #[test]
    fn decode_rejects_literal_mismatch() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232b());
        assert!(matches!(
            eng.decode(b"XY=180 EL=045"),
            Err(ProtocolError::ParseFailed(_))
        ));
    }

    #[test]
    fn decode_rejects_missing_number() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232b());
        assert!(matches!(
            eng.decode(b"AZ= EL=045"),
            Err(ProtocolError::ParseFailed(_))
        ));
    }

    // --- encode → decode round-trip (loopback; ADR K4 pre-work) ----------

    #[test]
    fn roundtrip_gs232b_loopback() {
        let eng = ProtocolEngine::new(ProtocolSpec::preset_gs232b());
        let original = pos(123.0, 67.0);
        // Set command and the position readout share the same numeric shape;
        // build the readout the device would echo and decode it back.
        let set =
            String::from_utf8(eng.encode(Operation::SetPosition, Some(original)).unwrap()).unwrap();
        assert_eq!(set, "W123 067\r");
        let readout = format!("AZ={:03} EL={:03}", 123, 67);
        assert_eq!(eng.decode(readout.as_bytes()).unwrap(), original);
    }

    #[test]
    fn roundtrip_all_presets_via_response_pattern() {
        // Encode using the response_pattern as template, then decode it back —
        // proves codec symmetry without hardware for every ASCII preset.
        for spec in ProtocolSpec::ascii_presets() {
            let response_tmpl = spec.response_pattern.clone();
            let eng = ProtocolEngine::new(spec);
            let original = pos(200.0, 12.0);
            // Reuse the engine's formatter by encoding through a temp spec whose
            // set_template equals the response pattern.
            let mut temp = eng.spec().clone();
            temp.set_template = response_tmpl;
            let temp_eng = ProtocolEngine::new(temp);
            let wire = temp_eng
                .encode(Operation::SetPosition, Some(original))
                .unwrap();
            let decoded = eng.decode(&wire).unwrap();
            assert_eq!(decoded, original);
        }
    }
}
