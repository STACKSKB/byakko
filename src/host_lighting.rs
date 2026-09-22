//! Original USB host-frame encoders from the selected Nia87 protocol trace.
//! These encode frames only; they do not sample audio or capture a screen.
//! Wireless framing is deliberately separate and is not implemented here.

/// A screen sample is one RGBA pixel. No image or user content is retained.
pub fn screen_report(rgba: [u8; 4]) -> [u8; 64] {
    let mut report = [0; 64];
    report[0] = 0x0f;
    report[1..5].copy_from_slice(&rgba);
    bit7(&mut report);
    report
}

/// Ordinary USB music frame: exactly 32 already-scaled intensity values.
/// This is not the nibble-packed wireless format or an audio transform.
pub fn music_report(intensities: [u8; 32]) -> [u8; 64] {
    let mut report = [0; 64];
    report[0] = 0x0e;
    report[8..40].copy_from_slice(&intensities);
    bit7(&mut report);
    report
}

fn bit7(report: &mut [u8; 64]) {
    report[7] = !report[..7]
        .iter()
        .fold(0u8, |sum, value| sum.wrapping_add(*value));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_header_checksum_wraps_without_consuming_pixel_fields() {
        let report = screen_report([8, 16, 24, 255]);
        assert_eq!(&report[..8], &[0x0f, 8, 16, 24, 255, 0, 0, 0xc1]);
        assert!(report[8..].iter().all(|byte| *byte == 0));
        assert_eq!(
            report[..8]
                .iter()
                .fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
            255
        );
    }

    #[test]
    fn music_preserves_all_32_bands_and_zero_pads_tail() {
        let bands = std::array::from_fn(|index| index as u8 * 8);
        let report = music_report(bands);
        assert_eq!(&report[..8], &[0x0e, 0, 0, 0, 0, 0, 0, 0xf1]);
        assert_eq!(&report[8..40], &bands);
        assert!(report[40..].iter().all(|byte| *byte == 0));
    }
}
