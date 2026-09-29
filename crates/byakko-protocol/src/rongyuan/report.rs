//! Shared 64-byte BIT7 report framing. HID report IDs are separate.

pub const REPORT_LEN: usize = 64;
const CHECKSUM_OFFSET: usize = 7;

pub fn set_bit7_checksum(report: &mut [u8; REPORT_LEN]) {
    let sum = report[..CHECKSUM_OFFSET]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    report[CHECKSUM_OFFSET] = 0xffu8.wrapping_sub(sum);
}
