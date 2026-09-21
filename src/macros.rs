//! Pure codec for the Nia87 simple macro store.
//!
//! Reports are 64-byte device payloads; the host HID transport may add a
//! separate report-ID byte. This module does not perform device I/O.

use serde::{Deserialize, Serialize};

const BUFFER_LEN: usize = 256;
const SAFE_END: usize = 248;
const REPORT_LEN: usize = 64;
const PAGE_DATA_LEN: usize = 56;
const MAX_SLOT: u8 = 49;

fn set_bit7_checksum(report: &mut [u8; REPORT_LEN]) {
    let sum = report[..7]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    report[7] = 0xffu8.wrapping_sub(sum);
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub repeat_count: u16,
    pub events: Vec<MacroEvent>,
}

/// `button` is the stored mouse action byte, 240 through 248.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroEvent {
    Key {
        usage: u8,
        down: bool,
        delay_ms: u16,
    },
    MouseButton {
        button: u8,
        down: bool,
        delay_ms: u16,
    },
    Move {
        dx: i8,
        dy: i8,
        delay_ms: u16,
    },
}

fn check_slot(slot: u8) -> Result<(), String> {
    if slot > MAX_SLOT {
        return Err(format!("macro slot {slot} is outside 0..={MAX_SLOT}"));
    }
    Ok(())
}

/// Encode a macro into the zero-padded 256-byte logical device buffer.
pub fn encode(macro_data: &Macro) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(BUFFER_LEN);
    bytes.extend_from_slice(&macro_data.repeat_count.to_le_bytes());

    for event in &macro_data.events {
        match *event {
            MacroEvent::Key {
                usage,
                down,
                delay_ms,
            } => {
                if !(4..=239).contains(&usage) {
                    return Err(format!("keyboard usage {usage} is outside 4..=239"));
                }
                bytes.push(usage);
                bytes.push((if down { 0x80 } else { 0 }) | short_delay(delay_ms));
                if delay_ms == 0 || delay_ms > 127 {
                    bytes.extend_from_slice(&delay_ms.to_le_bytes());
                }
            }
            MacroEvent::MouseButton {
                button,
                down,
                delay_ms,
            } => {
                if !(240..=248).contains(&button) {
                    return Err(format!("mouse action byte {button} is outside 240..=248"));
                }
                bytes.push(button);
                bytes.push((if down { 0x80 } else { 0 }) | short_delay(delay_ms));
                if delay_ms == 0 || delay_ms > 127 {
                    bytes.extend_from_slice(&delay_ms.to_le_bytes());
                }
            }
            MacroEvent::Move { dx, dy, delay_ms } => {
                bytes.extend_from_slice(&[249, short_delay(delay_ms), dx as u8, dy as u8]);
                if delay_ms == 0 || delay_ms > 127 {
                    bytes.extend_from_slice(&delay_ms.to_le_bytes());
                }
            }
        }
        if bytes.len() > SAFE_END {
            return Err(format!("macro exceeds {SAFE_END}-byte safe encoded limit"));
        }
    }

    bytes.resize(BUFFER_LEN, 0);
    Ok(bytes)
}

fn short_delay(delay_ms: u16) -> u8 {
    if (1..=127).contains(&delay_ms) {
        delay_ms as u8
    } else {
        0
    }
}

/// Decode a complete logical device buffer, retaining explicit zero delays.
pub fn decode(data: &[u8]) -> Result<Macro, String> {
    if data.len() != BUFFER_LEN {
        return Err(format!(
            "expected {BUFFER_LEN} macro bytes, got {}",
            data.len()
        ));
    }
    if data[SAFE_END..].iter().any(|byte| *byte != 0) {
        return Err("nonzero bytes beyond safe macro limit".to_owned());
    }

    let repeat_count = u16::from_le_bytes([data[0], data[1]]);
    let mut events = Vec::new();
    let mut at = 2;
    while at < SAFE_END && data[at] != 0 {
        let tag = data[at];
        let (event, consumed) = match tag {
            4..=248 => {
                if at + 2 > SAFE_END {
                    return Err("truncated macro action".to_owned());
                }
                let flags = data[at + 1];
                let short = flags & 0x7f;
                let (delay_ms, len) = if short != 0 {
                    (u16::from(short), 2)
                } else {
                    if at + 4 > SAFE_END {
                        return Err("truncated macro long delay".to_owned());
                    }
                    (u16::from_le_bytes([data[at + 2], data[at + 3]]), 4)
                };
                let down = flags & 0x80 != 0;
                let event = if tag <= 239 {
                    MacroEvent::Key {
                        usage: tag,
                        down,
                        delay_ms,
                    }
                } else {
                    MacroEvent::MouseButton {
                        button: tag,
                        down,
                        delay_ms,
                    }
                };
                (event, len)
            }
            249 => {
                if at + 4 > SAFE_END {
                    return Err("truncated mouse movement".to_owned());
                }
                let short = data[at + 1];
                if short > 127 {
                    return Err("invalid mouse movement delay byte".to_owned());
                }
                let (delay_ms, len) = if short != 0 {
                    (u16::from(short), 4)
                } else {
                    if at + 6 > SAFE_END {
                        return Err("truncated mouse movement long delay".to_owned());
                    }
                    (u16::from_le_bytes([data[at + 4], data[at + 5]]), 6)
                };
                (
                    MacroEvent::Move {
                        dx: data[at + 2] as i8,
                        dy: data[at + 3] as i8,
                        delay_ms,
                    },
                    len,
                )
            }
            _ => return Err(format!("unknown macro action byte {tag} at offset {at}")),
        };
        at += consumed;
        events.push(event);
    }
    if data[at..].iter().any(|byte| *byte != 0) {
        return Err(format!(
            "nonzero bytes after macro terminator at offset {at}"
        ));
    }
    Ok(Macro {
        repeat_count,
        events,
    })
}

/// Build the Nia87 inherited macro read request for one of four raw pages.
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

/// Frame a complete logical buffer into 56-byte simple-macro write pages.
pub fn write_reports(slot: u8, data: &[u8]) -> Result<Vec<[u8; REPORT_LEN]>, String> {
    check_slot(slot)?;
    let macro_data = decode(data)?;

    // Count zero-valued long-delay tails too; the final nonzero byte can be
    // before a page boundary even when the encoded event crosses it.
    let used_end = 2 + macro_data
        .events
        .iter()
        .map(|event| match event {
            MacroEvent::Key { delay_ms, .. } | MacroEvent::MouseButton { delay_ms, .. } => {
                if (1..=127).contains(delay_ms) { 2 } else { 4 }
            }
            MacroEvent::Move { delay_ms, .. } => {
                if (1..=127).contains(delay_ms) {
                    4
                } else {
                    6
                }
            }
        })
        .sum::<usize>();
    // One all-zero page is needed to clear an existing macro in a slot.
    let page_count = used_end.div_ceil(PAGE_DATA_LEN).max(1);
    let mut reports = Vec::with_capacity(page_count);
    for page in 0..page_count {
        let mut report = [0u8; REPORT_LEN];
        report[0] = 0x16;
        report[1] = slot;
        report[2] = page as u8;
        report[3] = PAGE_DATA_LEN as u8;
        report[4] = u8::from(page + 1 == page_count);
        set_bit7_checksum(&mut report);
        let start = page * PAGE_DATA_LEN;
        let end = (start + PAGE_DATA_LEN).min(BUFFER_LEN);
        report[8..8 + end - start].copy_from_slice(&data[start..end]);
        reports.push(report);
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_vector_covers_modifiers_buttons_movement_and_delays() {
        let value = Macro {
            repeat_count: 0x1234,
            events: vec![
                MacroEvent::Key {
                    usage: 224,
                    down: true,
                    delay_ms: 1,
                },
                MacroEvent::Key {
                    usage: 4,
                    down: false,
                    delay_ms: 127,
                },
                MacroEvent::MouseButton {
                    button: 240,
                    down: true,
                    delay_ms: 128,
                },
                MacroEvent::Move {
                    dx: -5,
                    dy: 7,
                    delay_ms: 0,
                },
            ],
        };
        let data = encode(&value).unwrap();
        assert_eq!(
            &data[..18],
            &[
                0x34, 0x12, 224, 0x81, 4, 0x7f, 240, 0x80, 0x80, 0, 249, 0, 251, 7, 0, 0, 0, 0
            ]
        );
        assert_eq!(decode(&data).unwrap(), value);
    }

    #[test]
    fn zero_and_max_delay_round_trip() {
        let value = Macro {
            repeat_count: u16::MAX,
            events: vec![
                MacroEvent::Key {
                    usage: 231,
                    down: false,
                    delay_ms: 0,
                },
                MacroEvent::Move {
                    dx: -128,
                    dy: 127,
                    delay_ms: 127,
                },
                MacroEvent::Move {
                    dx: 1,
                    dy: -1,
                    delay_ms: 128,
                },
                MacroEvent::MouseButton {
                    button: 248,
                    down: false,
                    delay_ms: u16::MAX,
                },
            ],
        };
        assert_eq!(decode(&encode(&value).unwrap()).unwrap(), value);
    }

    #[test]
    fn safe_boundary_and_overflow() {
        let one = MacroEvent::Key {
            usage: 4,
            down: true,
            delay_ms: 0,
        };
        let fits = Macro {
            repeat_count: 1,
            events: vec![one.clone(); 61],
        }; // 2 + 61 * 4 = 246
        assert_eq!(decode(&encode(&fits).unwrap()).unwrap(), fits);
        let full = Macro {
            repeat_count: 1,
            events: vec![one.clone(); 61]
                .into_iter()
                .chain([MacroEvent::Key {
                    usage: 5,
                    down: false,
                    delay_ms: 1,
                }])
                .collect(),
        };
        assert_eq!(decode(&encode(&full).unwrap()).unwrap(), full); // 248
        let reports = write_reports(0, &encode(&full).unwrap()).unwrap();
        assert_eq!(reports.len(), 5);
        assert_eq!(&reports[4][..8], &[0x16, 0, 4, 56, 1, 0, 0, 0xac]);
        let too_long = Macro {
            repeat_count: 1,
            events: vec![one; 62],
        }; // 250
        assert!(encode(&too_long).is_err());
    }

    #[test]
    fn reports_have_exact_headers_and_final_page() {
        let data = encode(&Macro {
            repeat_count: 1,
            events: vec![
                MacroEvent::Key {
                    usage: 4,
                    down: true,
                    delay_ms: 1
                };
                28
            ],
        })
        .unwrap(); // 58 bytes used, so two pages
        let reports = write_reports(49, &data).unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(&reports[0][..8], &[0x16, 49, 0, 56, 0, 0, 0, 0x80]);
        assert_eq!(&reports[1][..8], &[0x16, 49, 1, 56, 1, 0, 0, 0x7e]);
        assert_eq!(&reports[0][8..64], &data[..56]);
        assert_eq!(&reports[1][8..10], &data[56..58]);
        assert!(reports[1][10..].iter().all(|byte| *byte == 0));
        let read = read_request(49, 3).unwrap();
        assert_eq!(&read[..8], &[0x8b, 49, 3, 0, 0, 0, 0, 0x40]);
    }

    #[test]
    fn empty_clear_and_malformed_inputs() {
        let empty = encode(&Macro {
            repeat_count: 0,
            events: vec![],
        })
        .unwrap();
        assert_eq!(write_reports(0, &empty).unwrap().len(), 1);
        assert!(read_request(50, 0).is_err());
        assert!(read_request(0, 4).is_err());
        assert!(write_reports(0, &empty[..255]).is_err());
        assert!(
            encode(&Macro {
                repeat_count: 0,
                events: vec![MacroEvent::Key {
                    usage: 3,
                    down: true,
                    delay_ms: 1
                }]
            })
            .is_err()
        );
        let mut malformed = empty.clone();
        malformed[2] = 250;
        assert!(decode(&malformed).is_err());
        malformed[2] = 4;
        malformed[3] = 0;
        malformed[4] = 0;
        malformed[5] = 0;
        malformed[6] = 4; // valid next event follows, but its long delay is zero
        assert_eq!(decode(&malformed).unwrap().events.len(), 2);
        malformed[2] = 0; // nonzero bytes now follow a terminator
        assert!(decode(&malformed).is_err());
        malformed = empty;
        malformed[248] = 1;
        assert!(decode(&malformed).is_err());
    }

    #[test]
    fn zero_delay_tail_across_page_boundary_is_sent() {
        let mut events = vec![
            MacroEvent::Key {
                usage: 4,
                down: true,
                delay_ms: 1
            };
            26
        ];
        events.push(MacroEvent::Key {
            usage: 5,
            down: false,
            delay_ms: 0,
        });
        let data = encode(&Macro {
            repeat_count: 0,
            events,
        })
        .unwrap();
        assert_eq!(data[54], 5);
        assert_eq!(&data[55..58], &[0, 0, 0]);
        let reports = write_reports(0, &data).unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[1][4], 1);
    }
}
