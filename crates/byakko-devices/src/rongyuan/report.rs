//! Shared 64-byte BIT7 report framing. HID report IDs are separate.

pub const REPORT_LEN: usize = 64;
const CHECKSUM_OFFSET: usize = 7;

pub fn set_bit7_checksum(report: &mut [u8; REPORT_LEN]) {
    let sum = report[..CHECKSUM_OFFSET]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    report[CHECKSUM_OFFSET] = 0xffu8.wrapping_sub(sum);
}

pub fn read_request(opcode: u8, index: u8, page: u8) -> [u8; REPORT_LEN] {
    let mut report = [0; REPORT_LEN];
    report[0] = opcode;
    report[1] = index;
    report[2] = page;
    set_bit7_checksum(&mut report);
    report
}
