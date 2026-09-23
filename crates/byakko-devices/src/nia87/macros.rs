//! Nia87 compatibility facade for the observed yc500-shaped simple-macro codec.
pub use crate::rongyuan::yc500::macro_program::{Macro, MacroEvent, decode, encode};
pub use crate::rongyuan::yc500::macro_reports::{read_request, write_reports};

#[cfg(test)]
const BUFFER_LEN: usize = 256;
#[cfg(test)]
const PAGE_DATA_LEN: usize = 56;
#[cfg(test)]
const WRITE_PAGES: usize = 5;

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
        .unwrap(); // 58 bytes used; remaining pages must still be cleared.
        let reports = write_reports(49, &data).unwrap();
        assert_eq!(reports.len(), 5);
        assert_eq!(&reports[0][..8], &[0x16, 49, 0, 56, 0, 0, 0, 0x80]);
        assert_eq!(&reports[1][..8], &[0x16, 49, 1, 56, 0, 0, 0, 0x7f]);
        assert_eq!(&reports[4][..8], &[0x16, 49, 4, 56, 1, 0, 0, 0x7b]);
        assert_eq!(&reports[0][8..64], &data[..56]);
        assert_eq!(&reports[1][8..10], &data[56..58]);
        assert!(reports[1][10..].iter().all(|byte| *byte == 0));
        assert!(
            reports[2..]
                .iter()
                .all(|report| report[8..].iter().all(|byte| *byte == 0))
        );
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
        let clear = write_reports(0, &empty).unwrap();
        assert_eq!(clear.len(), 5);
        assert!(
            clear
                .iter()
                .all(|report| report[8..].iter().all(|byte| *byte == 0))
        );
        assert!(clear[..4].iter().all(|report| report[4] == 0));
        assert_eq!(&clear[4][..8], &[0x16, 0, 4, 56, 1, 0, 0, 0xac]);
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
        assert_eq!(reports.len(), 5);
        assert_eq!(reports[1][4], 0);
        assert_eq!(reports[4][4], 1);
    }

    #[test]
    fn shorter_macro_replaces_all_pages_of_longer_macro() {
        let long = encode(&Macro {
            repeat_count: 1,
            events: vec![
                MacroEvent::Key {
                    usage: 4,
                    down: true,
                    delay_ms: 1
                };
                120
            ],
        })
        .unwrap(); // 242 encoded bytes, reaching page 4.
        let short = encode(&Macro {
            repeat_count: 2,
            events: vec![MacroEvent::Key {
                usage: 5,
                down: false,
                delay_ms: 50,
            }],
        })
        .unwrap();
        let mut storage = [0u8; BUFFER_LEN];
        for data in [&long, &short] {
            let reports = write_reports(49, data).unwrap();
            assert_eq!(reports.len(), WRITE_PAGES);
            for (page, report) in reports.iter().enumerate() {
                let start = page * PAGE_DATA_LEN;
                let len = (BUFFER_LEN - start).min(PAGE_DATA_LEN);
                storage[start..start + len].copy_from_slice(&report[8..8 + len]);
            }
        }
        assert_eq!(storage.as_slice(), short.as_slice());
    }
}
