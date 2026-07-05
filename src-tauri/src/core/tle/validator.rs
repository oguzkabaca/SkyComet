use super::TleError;

pub const TLE_LINE_LENGTH: usize = 69;

pub fn validate_line(line: &str, expected_number: u8) -> Result<(), TleError> {
    if line.len() != TLE_LINE_LENGTH {
        return Err(TleError::InvalidLineLength(line.len()));
    }
    if !line.is_ascii() {
        return Err(TleError::InvalidField {
            field: format!("line{expected_number}"),
            message: "TLE lines must be ASCII".into(),
        });
    }
    let bytes = line.as_bytes();
    let first = char::from(bytes[0]);
    if first != char::from(b'0' + expected_number) {
        return Err(TleError::InvalidLineNumber {
            expected: expected_number,
            found: first,
        });
    }
    let computed = compute_checksum(&line[..TLE_LINE_LENGTH - 1]);
    let expected_digit = char::from(bytes[TLE_LINE_LENGTH - 1]);
    let expected = expected_digit
        .to_digit(10)
        .ok_or(TleError::ChecksumMismatch {
            line: expected_number,
            expected: 255,
            computed,
        })? as u8;
    if expected != computed {
        return Err(TleError::ChecksumMismatch {
            line: expected_number,
            expected,
            computed,
        });
    }
    Ok(())
}

pub fn compute_checksum(payload: &str) -> u8 {
    let mut sum: u32 = 0;
    for ch in payload.chars() {
        if let Some(d) = ch.to_digit(10) {
            sum += d;
        } else if ch == '-' {
            sum += 1;
        }
    }
    (sum % 10) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

    #[test]
    fn computes_checksum_correctly() {
        assert_eq!(compute_checksum(&ISS_L1[..68]), 7);
    }

    #[test]
    fn validates_correct_lines() {
        assert!(validate_line(ISS_L1, 1).is_ok());
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(matches!(
            validate_line("1 25544", 1),
            Err(TleError::InvalidLineLength(_))
        ));
    }

    #[test]
    fn rejects_wrong_line_number() {
        assert!(matches!(
            validate_line(ISS_L2, 1),
            Err(TleError::InvalidLineNumber { .. })
        ));
    }

    #[test]
    fn rejects_bad_checksum() {
        let mut bad = String::from(ISS_L1);
        bad.pop();
        bad.push('0');
        assert!(matches!(
            validate_line(&bad, 1),
            Err(TleError::ChecksumMismatch { .. })
        ));
    }
}
