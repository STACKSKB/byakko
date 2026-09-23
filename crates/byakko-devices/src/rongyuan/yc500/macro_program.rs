//! Logical simple-macro program encoding for the observed yc500-shaped store.
use serde::{Deserialize, Serialize};

pub(super) const BUFFER_LEN: usize = 256;
const SAFE_END: usize = 248;

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

/// Encode a macro into the zero-padded 256-byte logical device buffer.
pub fn encode(macro_data: &Macro) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(BUFFER_LEN);
    bytes.extend_from_slice(&macro_data.repeat_count.to_le_bytes());

    for event in &macro_data.events {
        let delay_ms = match *event {
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
                delay_ms
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
                delay_ms
            }
            MacroEvent::Move { dx, dy, delay_ms } => {
                bytes.extend_from_slice(&[249, short_delay(delay_ms), dx as u8, dy as u8]);
                delay_ms
            }
        };
        if delay_ms == 0 || delay_ms > 127 {
            bytes.extend_from_slice(&delay_ms.to_le_bytes());
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
