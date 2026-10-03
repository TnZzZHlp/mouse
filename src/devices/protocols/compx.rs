//! Validated payloads for the 17-byte Compx Report 8 format.
pub fn payload(report: &[u8], command: u8, minimum_len: u8) -> Option<&[u8]> {
    if report.len() != 17
        || report[0] != 8
        || report[1] != command
        || report[2] != 0
        || report[5] < minimum_len
        || report[5] > 10
        || report.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) != 0x55
    {
        return None;
    }
    Some(&report[6..6 + usize::from(report[5])])
}

pub fn model(report: &[u8]) -> Option<(u8, u8)> {
    let data = payload(report, 0x10, 2)?;
    Some((data[0], data[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_identity_requires_a_successful_model_response() {
        let capture = [
            8, 0x10, 0, 0, 0, 3, 2, 0x26, 0x2d, 0, 0, 0, 0, 0, 0, 0, 0xe5,
        ];
        assert_eq!(model(&capture), Some((2, 38)));
        let mut bad = capture;
        bad[7] = 39;
        assert_eq!(
            model(&bad),
            None,
            "Corrupted model report must not be accepted"
        );
        bad[16] = bad[16].wrapping_sub(1);
        assert_eq!(
            model(&bad),
            Some((2, 39)),
            "Another valid model stays distinguishable"
        );
        bad[1] = 4;
        bad[16] = bad[16].wrapping_add(12);
        assert_eq!(
            model(&bad),
            None,
            "Battery response must not authenticate a receiver"
        );
    }
}
