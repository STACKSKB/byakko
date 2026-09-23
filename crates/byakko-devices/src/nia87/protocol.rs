//! Pure Nia87 feature-report framing and matrix conversion.
//!
//! Reports here are 64-byte device payloads. A host HID API may require a
//! separate leading report-ID byte when sending or receiving them.

use crate::rongyuan::{report, yc500::matrix::Yc500MatrixGeometry};

pub type Matrix = Vec<[u8; 4]>;

const REPORT_LEN: usize = report::REPORT_LEN;
const READ_PAGES: usize = 8;
#[cfg(test)]
const WRITE_PAGES: usize = 9;
#[cfg(test)]
const MATRIX_SLOTS: usize = 128;
const WRITABLE_SLOTS: usize = 126;

const NIA87_MATRIX: Yc500MatrixGeometry = Yc500MatrixGeometry {
    read_pages: READ_PAGES as u8,
    writable_slots: WRITABLE_SLOTS as u8,
};

/// Build a page or identity read request with the Nia87 BIT7 header checksum.
///
/// Callers select the opcode; this function only supplies the common framing.
pub fn read_request(opcode: u8, index: u8, page: u8) -> [u8; REPORT_LEN] {
    report::read_request(opcode, index, page)
}

/// Convert exactly eight raw 64-byte page responses into 128 four-byte slots.
///
/// The stock read method treats every response byte as matrix data, including
/// all-zero and special-key entries.
pub fn matrix_from_pages(pages: &[[u8; REPORT_LEN]]) -> Result<Matrix, String> {
    NIA87_MATRIX.matrix_from_pages(pages)
}

/// Build the nine full-matrix write reports used by the stock Nia87 method.
///
/// The write format carries 126 slots; the two trailing matrix slots must be
/// empty so a caller cannot silently lose device data.
pub fn full_matrix_reports(
    fn_layer: bool,
    index: u8,
    matrix: &[[u8; 4]],
) -> Result<Vec<[u8; REPORT_LEN]>, String> {
    NIA87_MATRIX.full_matrix_reports(fn_layer, index, matrix)
}

/// Build the stock single-key write for a physical matrix slot.
pub fn single_key_report(
    fn_layer: bool,
    index: u8,
    slot: usize,
    binding: [u8; 4],
) -> Result<[u8; REPORT_LEN], String> {
    NIA87_MATRIX.single_key_report(fn_layer, index, slot, binding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_requests_match_confirmed_identity_headers() {
        let version = read_request(0x80, 0, 0);
        assert_eq!(&version[..8], &[0x80, 0, 0, 0, 0, 0, 0, 0x7f]);
        assert!(version[8..].iter().all(|byte| *byte == 0));

        let profile = read_request(0x85, 0, 0);
        assert_eq!(&profile[..8], &[0x85, 0, 0, 0, 0, 0, 0, 0x7a]);

        let last_page = read_request(0x89, 3, 7);
        assert_eq!(&last_page[..8], &[0x89, 3, 7, 0, 0, 0, 0, 0x6c]);
    }

    #[test]
    fn matrix_pages_preserve_every_four_byte_slot() {
        let mut pages = [[0u8; REPORT_LEN]; READ_PAGES];
        pages[0][60..64].copy_from_slice(&[0xfe, 0x80, 0xff, 0x01]);
        pages[1][..4].copy_from_slice(&[10, 1, 0, 0]);
        pages[7][60..64].copy_from_slice(&[0x55, 0xaa, 0x77, 0x99]);

        let matrix = matrix_from_pages(&pages).unwrap();
        assert_eq!(matrix.len(), MATRIX_SLOTS);
        assert_eq!(matrix[15], [0xfe, 0x80, 0xff, 0x01]);
        assert_eq!(matrix[16], [10, 1, 0, 0]);
        assert_eq!(matrix[127], [0x55, 0xaa, 0x77, 0x99]);
        assert!(matrix_from_pages(&pages[..7]).is_err());
        let nine_pages = [[0u8; REPORT_LEN]; 9];
        assert!(matrix_from_pages(&nine_pages).is_err());
    }

    #[test]
    fn full_reports_preserve_page_boundaries_and_header_checksums() {
        let mut matrix = vec![[0u8; 4]; MATRIX_SLOTS];
        matrix[13] = [0xfe, 0x80, 0xff, 0x01];
        matrix[14] = [10, 1, 0, 0];
        matrix[125] = [0x55, 0xaa, 0x77, 0x99];

        let base = full_matrix_reports(false, 0, &matrix).unwrap();
        assert_eq!(base.len(), WRITE_PAGES);
        assert_eq!(&base[0][..8], &[0x09, 0, 0xf8, 0x01, 0, 0, 0, 0xfd]);
        assert_eq!(&base[0][60..64], &matrix[13]);
        assert_eq!(&base[1][8..12], &matrix[14]);
        assert_eq!(&base[8][60..64], &matrix[125]);

        let function = full_matrix_reports(true, 1, &matrix).unwrap();
        assert_eq!(&function[8][..8], &[0x10, 1, 0xf8, 0x01, 8, 0, 0, 0xed]);
        assert_eq!(&function[0][60..64], &matrix[13]);
    }

    #[test]
    fn full_reports_reject_unwritable_slots_and_wrong_lengths() {
        assert!(full_matrix_reports(false, 0, &[[0; 4]; 127]).is_err());
        assert!(full_matrix_reports(false, 0, &[[0; 4]; 129]).is_err());
        for trailing_slot in [126, 127] {
            let mut matrix = vec![[0u8; 4]; MATRIX_SLOTS];
            matrix[trailing_slot] = [1, 2, 3, 4];
            assert!(full_matrix_reports(false, 0, &matrix).is_err());
        }
    }

    #[test]
    fn single_key_reports_keep_binding_bytes_and_check_slot_bounds() {
        let binding = [0xfe, 0x80, 0xff, 0x01];
        let base = single_key_report(false, 2, 125, binding).unwrap();
        assert_eq!(&base[..8], &[0x13, 2, 125, 0, 0, 0, 0, 0x6d]);
        assert_eq!(&base[8..12], &binding);
        assert!(base[12..].iter().all(|byte| *byte == 0));

        let function = single_key_report(true, 0, 0, binding).unwrap();
        assert_eq!(&function[..8], &[0x15, 0, 0, 0, 0, 0, 0, 0xea]);
        assert_eq!(&function[8..12], &binding);
        assert!(single_key_report(false, 0, 126, binding).is_err());
        assert!(single_key_report(false, 0, usize::MAX, binding).is_err());
    }

    #[test]
    fn complete_reports_match_independent_nia87_vectors() {
        let mut matrix = vec![[0; 4]; MATRIX_SLOTS];
        matrix[0] = [1, 2, 3, 4];
        matrix[125] = [5, 6, 7, 8];
        let actual = full_matrix_reports(false, 0, &matrix).unwrap();
        let mut expected = Vec::new();
        for page in 0..9 {
            let mut report = [0; REPORT_LEN];
            report[..8].copy_from_slice(&[0x09, 0, 0xf8, 1, page, 0, 0, 0xfd - page]);
            if page == 0 {
                report[8..12].copy_from_slice(&[1, 2, 3, 4]);
            }
            if page == 8 {
                report[60..64].copy_from_slice(&[5, 6, 7, 8]);
            }
            expected.push(report);
        }
        assert_eq!(actual, expected);

        let mut single = [0; REPORT_LEN];
        single[..8].copy_from_slice(&[0x15, 0, 0, 0, 0, 0, 0, 0xea]);
        single[8..12].copy_from_slice(&[9, 8, 7, 6]);
        assert_eq!(single_key_report(true, 0, 0, [9, 8, 7, 6]).unwrap(), single);
    }
}
