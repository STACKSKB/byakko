//! Page-report construction for the observed yc500-shaped simple-macro store.
use super::macro_program::decode;
use crate::rongyuan::report::{REPORT_LEN, set_bit7_checksum};

const PAGE_DATA_LEN: usize = 56;
/// Observed writable extent: a 26-byte final page preserved neighboring picture
/// data in the 2026-09-26 Nia87 slot-49 test. Getters still return 256 bytes.
pub(crate) const WRITE_LEN: usize = 250;
const WRITE_PAGES: usize = 5;
const MAX_SLOT: u8 = 49;

fn check_slot(slot: u8) -> Result<(), String> {
    if slot > MAX_SLOT {
        return Err(format!("macro slot {slot} is outside 0..={MAX_SLOT}"));
    }
    Ok(())
}

/// Build a yc500-shaped macro read request for one of four raw pages.
pub fn read_request(slot: u8, page: u8) -> Result<[u8; REPORT_LEN], String> {
    check_slot(slot)?;
    if page >= 4 {
        return Err(format!("macro read page {page} is outside 0..=3"));
    }
    let mut report = [0u8; REPORT_LEN];
    report[0] = 0x8b;
    report[1] = slot;
    report[2] = page;
    set_bit7_checksum(&mut report);
    Ok(report)
}

/// Replace all 250 writable bytes with four full pages and one 26-byte final page.
/// The logical read buffer is 256 bytes; its final six bytes are zero padding.
pub fn write_reports(slot: u8, data: &[u8]) -> Result<Vec<[u8; REPORT_LEN]>, String> {
    check_slot(slot)?;
    decode(data)?;

    // The device retains untouched pages from an earlier longer macro. Send
    // every page, including zero-filled trailing pages, to clear the whole store.
    let mut reports = Vec::with_capacity(WRITE_PAGES);
    for page in 0..WRITE_PAGES {
        let start = page * PAGE_DATA_LEN;
        let end = (start + PAGE_DATA_LEN).min(WRITE_LEN);
        let mut report = [0u8; REPORT_LEN];
        report[0] = 0x16;
        report[1] = slot;
        report[2] = page as u8;
        report[3] = (end - start) as u8;
        report[4] = u8::from(page + 1 == WRITE_PAGES);
        set_bit7_checksum(&mut report);
        report[8..8 + end - start].copy_from_slice(&data[start..end]);
        reports.push(report);
    }
    Ok(reports)
}
