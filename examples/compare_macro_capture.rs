//! Offline comparison of WinDbg HID dumps with Byakko's macro writer.
//! Unsent logical bytes remain unknown unless --assume-zero-unobserved is explicit.
use std::{env, fs, path::Path};

const HID_LEN: usize = 67;
const PAYLOAD_LEN: usize = 64;
const PAGE_LEN: usize = 56;

#[derive(Debug)]
struct CapturedPage {
    slot: u8,
    page: u8,
    final_page: bool,
    payload: [u8; PAYLOAD_LEN],
}

fn dump_row(line: &str) -> Option<Result<Vec<u8>, String>> {
    let (address, rest) = line.split_once("  ")?;
    if address.len() != 8 || !address.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let hex = rest
        .trim_start()
        .split("  ")
        .next()
        .unwrap_or("")
        .replace('-', " ");
    Some(
        hex.split_whitespace()
            .map(|word| {
                if word.len() != 2 {
                    return Err(format!("invalid hex byte {word}"));
                }
                u8::from_str_radix(word, 16).map_err(|_| format!("invalid hex byte {word}"))
            })
            .collect(),
    )
}

fn parse_frames(log: &str) -> Result<Vec<[u8; HID_LEN]>, String> {
    let mut frames = Vec::new();
    let mut current: Option<(usize, Vec<u8>)> = None;
    for (index, line) in log.lines().enumerate() {
        let line_no = index + 1;
        if let Some(raw) = line.strip_prefix("HID len=") {
            if let Some((start, bytes)) = current.take() {
                return Err(format!(
                    "incomplete HID dump at line {start}: {} bytes",
                    bytes.len()
                ));
            }
            let length: usize = raw
                .trim()
                .parse()
                .map_err(|_| format!("invalid HID length at line {line_no}"))?;
            if length != HID_LEN {
                return Err(format!(
                    "HID length {length} at line {line_no}; expected {HID_LEN}"
                ));
            }
            current = Some((line_no, Vec::new()));
            continue;
        }
        if let Some((start, bytes)) = current.as_mut() {
            if let Some(row) = dump_row(line) {
                bytes.extend(row.map_err(|e| format!("line {line_no}: {e}"))?);
                if bytes.len() > HID_LEN {
                    return Err(format!("HID dump at line {start} exceeds {HID_LEN} bytes"));
                }
                if bytes.len() == HID_LEN {
                    frames.push(bytes.as_slice().try_into().expect("length checked"));
                    current = None;
                }
            } else if !line.trim().is_empty() {
                return Err(format!(
                    "incomplete HID dump at line {start} before line {line_no}"
                ));
            }
        }
    }
    if let Some((start, bytes)) = current {
        return Err(format!(
            "incomplete HID dump at line {start}: {} bytes",
            bytes.len()
        ));
    }
    Ok(frames)
}

fn macro_page(frame: &[u8; HID_LEN]) -> Result<Option<CapturedPage>, String> {
    if frame[0] != 0 {
        return Err("nonzero HID report ID".into());
    }
    let payload: [u8; PAYLOAD_LEN] = frame[1..65].try_into().expect("fixed frame");
    if payload[0] != 0x16 {
        return Ok(None);
    }
    if payload[1] > 49 || payload[2] > 4 || payload[3] != 56 || payload[4] > 1 {
        return Err(format!("invalid macro header: {:02X?}", &payload[..8]));
    }
    let sum = payload[..8]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    if sum != 0xff {
        return Err(format!(
            "bad macro checksum for slot {} page {}",
            payload[1], payload[2]
        ));
    }
    Ok(Some(CapturedPage {
        slot: payload[1],
        page: payload[2],
        final_page: payload[4] == 1,
        payload,
    }))
}

fn groups(frames: &[[u8; HID_LEN]]) -> Result<Vec<Vec<CapturedPage>>, String> {
    let mut result = Vec::new();
    let mut active: Vec<CapturedPage> = Vec::new();
    for frame in frames {
        let Some(page) = macro_page(frame)? else {
            continue;
        };
        let wanted = active.len() as u8;
        if page.page != wanted || (!active.is_empty() && page.slot != active[0].slot) {
            return Err(format!(
                "out-of-order macro page: slot {} page {}, expected page {wanted}",
                page.slot, page.page
            ));
        }
        let done = page.final_page;
        active.push(page);
        if done {
            result.push(std::mem::take(&mut active));
        }
    }
    if !active.is_empty() {
        return Err(format!(
            "incomplete macro group for slot {}",
            active[0].slot
        ));
    }
    Ok(result)
}

fn compare(group: &[CapturedPage], assume_zero: bool) -> String {
    let observed_len = (group.len() * PAGE_LEN).min(256);
    let mut lines = vec![format!(
        "slot {}: {} captured page(s), {observed_len} of 256 logical bytes observed; {} unsent bytes unknown",
        group[0].slot,
        group.len(),
        256 - observed_len
    )];
    lines.push(format!(
        "captured final flag: page {}; native final flag: page 4",
        group.last().unwrap().page
    ));
    if !assume_zero {
        lines.push("native payload comparison skipped: pass --assume-zero-unobserved to state the zero-padding assumption".into());
        return lines.join("\n");
    }
    let mut logical = vec![0u8; 256];
    for page in group {
        let start = page.page as usize * PAGE_LEN;
        let len = PAGE_LEN.min(256 - start);
        logical[start..start + len].copy_from_slice(&page.payload[8..8 + len]);
    }
    match byakko::macros::decode(&logical) {
        Ok(decoded) => lines.push(format!(
            "decoded under zero-padding assumption: {} event(s), repeat {}",
            decoded.events.len(),
            decoded.repeat_count
        )),
        Err(error) => {
            lines.push(format!(
                "cannot decode under zero-padding assumption: {error}"
            ));
            return lines.join("\n");
        }
    }
    match byakko::macros::write_reports(group[0].slot, &logical) {
        Ok(native) => {
            let match_data = group
                .iter()
                .all(|page| page.payload[8..] == native[page.page as usize][8..]);
            let match_header_prefix = group
                .iter()
                .all(|page| page.payload[..4] == native[page.page as usize][..4]);
            lines.push(format!("observed page data matches native: {match_data}"));
            lines.push(format!("observed header prefix (opcode/slot/page/length) matches native: {match_header_prefix}"));
            lines.push(format!("native page count: {}", native.len()));
        }
        Err(error) => lines.push(format!("native report construction failed: {error}")),
    }
    lines.join("\n")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut path = "Research/captures/macro-official-headers-1.log".to_owned();
    let mut assume_zero = false;
    for argument in env::args().skip(1) {
        if argument == "--assume-zero-unobserved" {
            assume_zero = true;
        } else if path == "Research/captures/macro-official-headers-1.log" {
            path = argument;
        } else {
            return Err(format!("unexpected argument: {argument}").into());
        }
    }
    let frames = parse_frames(&fs::read_to_string(Path::new(&path))?)?;
    let groups = groups(&frames)?;
    println!(
        "{} HID frame(s), {} complete macro group(s)",
        frames.len(),
        groups.len()
    );
    for group in groups {
        println!("{}", compare(&group, assume_zero));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(page: u8, final_page: bool) -> String {
        let mut payload = [0u8; 64];
        payload[..5].copy_from_slice(&[0x16, 0, page, 56, u8::from(final_page)]);
        payload[8..14].copy_from_slice(&[1, 0, 0xf0, 0x81, 0xf0, 0x32]);
        payload[7] = 0xffu8.wrapping_sub(
            payload[..7]
                .iter()
                .fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
        );
        let mut bytes = vec![0];
        bytes.extend(payload);
        bytes.extend([0, 0]);
        let mut log = "HID len=67\n".to_owned();
        for (row, chunk) in bytes.chunks(16).enumerate() {
            log.push_str(&format!(
                "{:08x}  {}  .\n",
                row * 16,
                chunk
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        log
    }
    #[test]
    fn valid_67_byte_capture() {
        let groups = groups(&parse_frames(&fixture(0, true)).unwrap()).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            &groups[0][0].payload[8..14],
            &[1, 0, 0xf0, 0x81, 0xf0, 0x32]
        );
        assert!(compare(&groups[0], true).contains("observed page data matches native: true"));
    }
    #[test]
    fn rejects_truncated_dump() {
        let mut dump = fixture(0, true);
        dump.truncate(dump.rfind('\n').unwrap());
        dump.truncate(dump.rfind('\n').unwrap() + 1);
        assert!(parse_frames(&dump).is_err());
    }
    #[test]
    fn rejects_out_of_order_pages() {
        assert!(groups(&parse_frames(&fixture(1, true)).unwrap()).is_err());
    }
    #[test]
    fn rejects_bad_checksum() {
        let dump = fixture(0, true).replacen("b0", "b1", 1);
        assert!(groups(&parse_frames(&dump).unwrap()).is_err());
    }
}
