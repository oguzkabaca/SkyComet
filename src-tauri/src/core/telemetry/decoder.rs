use super::TelemetryError;

const AX25_CALLSIGN_LEN: usize = 6;
const AX25_ADDRESS_LEN: usize = 7;
const AX25_MIN_DEST_SOURCE_LEN: usize = AX25_ADDRESS_LEN * 2;

pub fn decode_ax25_callsigns_from_hex(
    frame_hex: &str,
) -> Result<Option<(String, String)>, TelemetryError> {
    let bytes = hex_to_bytes(frame_hex)?;
    if bytes.len() < AX25_MIN_DEST_SOURCE_LEN {
        return Ok(None);
    }

    let destination = decode_callsign(&bytes[0..AX25_ADDRESS_LEN]);
    let source = decode_callsign(&bytes[AX25_ADDRESS_LEN..AX25_MIN_DEST_SOURCE_LEN]);
    Ok(Some((destination, source)))
}

fn hex_to_bytes(frame_hex: &str) -> Result<Vec<u8>, TelemetryError> {
    let compact = frame_hex
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    if compact.len() % 2 != 0 {
        return Err(TelemetryError::Parse(
            "hex frame has odd length".to_string(),
        ));
    }

    let mut bytes = Vec::with_capacity(compact.len() / 2);
    let mut chars = compact.chars();
    while let (Some(high), Some(low)) = (chars.next(), chars.next()) {
        let high = high
            .to_digit(16)
            .ok_or_else(|| TelemetryError::Parse(format!("invalid hex digit '{high}'")))?;
        let low = low
            .to_digit(16)
            .ok_or_else(|| TelemetryError::Parse(format!("invalid hex digit '{low}'")))?;
        bytes.push(((high << 4) | low) as u8);
    }
    Ok(bytes)
}

fn decode_callsign(address: &[u8]) -> String {
    address
        .iter()
        .take(AX25_CALLSIGN_LEN)
        .map(|byte| char::from(byte >> 1))
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_callsign(callsign: &str, ssid: u8) -> Vec<u8> {
        let mut padded = callsign.as_bytes().to_vec();
        padded.resize(AX25_CALLSIGN_LEN, b' ');
        let mut out = padded.into_iter().map(|byte| byte << 1).collect::<Vec<_>>();
        out.push(ssid);
        out
    }

    #[test]
    fn decodes_synthetic_destination_and_source_callsigns() {
        let mut bytes = encoded_callsign("APRS", 0x60);
        bytes.extend(encoded_callsign("N0CALL", 0x61));
        bytes.extend([0x03, 0xF0]);
        let frame_hex = bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();

        let decoded = decode_ax25_callsigns_from_hex(&frame_hex).unwrap().unwrap();
        assert_eq!(decoded, ("APRS".to_string(), "N0CALL".to_string()));
    }

    #[test]
    fn short_frame_returns_none() {
        let decoded = decode_ax25_callsigns_from_hex("82A0").unwrap();
        assert!(decoded.is_none());
    }

    #[test]
    fn bad_hex_returns_parse_error() {
        let err = decode_ax25_callsigns_from_hex("82ZZ").unwrap_err();
        assert!(matches!(err, TelemetryError::Parse(_)));
    }
}
