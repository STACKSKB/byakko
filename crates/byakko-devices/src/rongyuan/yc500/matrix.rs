//! Parameterized yc500-shaped four-byte matrix records and write reports.

use crate::rongyuan::report::{REPORT_LEN, set_bit7_checksum};

const SLOT_LEN: usize = 4;
const HEADER_LEN: usize = 8;
const READ_SLOTS_PER_PAGE: usize = REPORT_LEN / SLOT_LEN;
const WRITE_SLOTS_PER_PAGE: usize = (REPORT_LEN - HEADER_LEN) / SLOT_LEN;
const FULL_OPCODES: [u8; 2] = [0x09, 0x10];
const SINGLE_OPCODES: [u8; 2] = [0x13, 0x15];
const FULL_MARKER: [u8; 2] = [0xf8, 0x01];

/// Board geometry for the yc500 matrix report shape. Commands are family-fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Yc500MatrixGeometry {
    pub read_pages: u8,
    pub writable_slots: u8,
}

impl Yc500MatrixGeometry {
    fn sizes(self) -> Result<(usize, usize), String> {
        let matrix_slots = usize::from(self.read_pages) * READ_SLOTS_PER_PAGE;
        let writable = usize::from(self.writable_slots);
        if matrix_slots == 0
            || writable == 0
            || writable > matrix_slots
            || !writable.is_multiple_of(WRITE_SLOTS_PER_PAGE)
        {
            return Err("invalid yc500 matrix dimensions".into());
        }
        Ok((matrix_slots, writable / WRITE_SLOTS_PER_PAGE))
    }

    pub fn matrix_from_pages(self, pages: &[[u8; REPORT_LEN]]) -> Result<Vec<[u8; 4]>, String> {
        let (matrix_slots, _) = self.sizes()?;
        if pages.len() != usize::from(self.read_pages) {
            return Err(format!(
                "expected {} matrix pages, got {}",
                self.read_pages,
                pages.len()
            ));
        }
        let mut matrix = Vec::with_capacity(matrix_slots);
        for page in pages {
            for slot in page.as_chunks::<SLOT_LEN>().0 {
                matrix.push(*slot);
            }
        }
        Ok(matrix)
    }

    pub fn full_matrix_reports(
        self,
        fn_layer: bool,
        index: u8,
        matrix: &[[u8; 4]],
    ) -> Result<Vec<[u8; REPORT_LEN]>, String> {
        let (matrix_slots, write_pages) = self.sizes()?;
        if matrix.len() != matrix_slots {
            return Err(format!(
                "expected {matrix_slots} matrix slots, got {}",
                matrix.len()
            ));
        }
        if matrix[usize::from(self.writable_slots)..]
            .iter()
            .any(|slot| *slot != [0; 4])
        {
            return Err(format!(
                "matrix slots from {} onward cannot be written",
                self.writable_slots
            ));
        }

        let mut reports = Vec::with_capacity(write_pages);
        for page in 0..write_pages {
            let mut report = [0; REPORT_LEN];
            report[0] = FULL_OPCODES[usize::from(fn_layer)];
            report[1] = index;
            report[2..4].copy_from_slice(&FULL_MARKER);
            report[4] = page as u8;
            set_bit7_checksum(&mut report);
            for (slot, binding) in matrix
                [page * WRITE_SLOTS_PER_PAGE..(page + 1) * WRITE_SLOTS_PER_PAGE]
                .iter()
                .enumerate()
            {
                let offset = HEADER_LEN + slot * SLOT_LEN;
                report[offset..offset + SLOT_LEN].copy_from_slice(binding);
            }
            reports.push(report);
        }
        Ok(reports)
    }

    pub fn single_key_report(
        self,
        fn_layer: bool,
        index: u8,
        slot: usize,
        binding: [u8; 4],
    ) -> Result<[u8; REPORT_LEN], String> {
        self.sizes()?;
        if slot >= usize::from(self.writable_slots) {
            return Err(format!(
                "slot {slot} is outside the {} writable matrix slots",
                self.writable_slots
            ));
        }
        let mut report = [0; REPORT_LEN];
        report[0] = SINGLE_OPCODES[usize::from(fn_layer)];
        report[1] = index;
        report[2] = slot as u8;
        report[8..12].copy_from_slice(&binding);
        set_bit7_checksum(&mut report);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_board_uses_its_own_safe_geometry_with_yc500_commands() {
        let spec = Yc500MatrixGeometry {
            read_pages: 2,
            writable_slots: 28,
        };
        let mut pages = [[0u8; REPORT_LEN]; 2];
        pages[1][44..48].copy_from_slice(&[1, 2, 3, 4]);
        let matrix = spec.matrix_from_pages(&pages).unwrap();
        assert_eq!(matrix.len(), 32);
        assert_eq!(matrix[27], [1, 2, 3, 4]);
        let reports = spec.full_matrix_reports(true, 7, &matrix).unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(&reports[1][..5], &[0x10, 7, 0xf8, 0x01, 1]);
        assert_eq!(&reports[1][60..64], &[1, 2, 3, 4]);
        let single = spec.single_key_report(false, 7, 27, [9, 8, 7, 6]).unwrap();
        assert_eq!(&single[..3], &[0x13, 7, 27]);
        assert_eq!(&single[8..12], &[9, 8, 7, 6]);
        assert!(spec.single_key_report(false, 7, 28, [0; 4]).is_err());
    }
}
