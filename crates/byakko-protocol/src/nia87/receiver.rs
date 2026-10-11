//! Nia87 3151:4011 receiver relay control reports.
//! These wrap ordinary keyboard commands; they do not change board settings.
use super::protocol;
use serde::Serialize;

pub const POLL_INTERVAL_MS: u64 = 100;
pub const STATUS_SETTLE_MS: u64 = 10;
pub const MAX_POLLS: usize = 5;

pub fn status_request() -> [u8; 64] {
    protocol::read_request(0xf7, 0, 0)
}
pub fn select_keyboard_request() -> [u8; 64] {
    protocol::read_request(0xf6, 10, 0)
}
pub fn release_reply_request() -> [u8; 64] {
    protocol::read_request(0xfc, 0, 0)
}
pub fn board_identity_request() -> [u8; 64] {
    protocol::read_request(0x8f, 0, 0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Status {
    pub reply_ready: bool,
    pub keyboard_online: bool,
    pub command_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatusError {
    Length(usize),
    Flag { offset: usize, value: u8 },
}
impl std::fmt::Display for StatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length(length) => write!(f, "Receiver status requires 64 bytes, got {length}"),
            Self::Flag { offset, value } => {
                write!(f, "Invalid receiver status flag at {offset}: {value}")
            }
        }
    }
}
impl std::error::Error for StatusError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayError {
    KeyboardOffline,
    PendingReply,
    CommandTimeout,
    ReplyTimeout,
}
impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::KeyboardOffline => "The receiver's keyboard is asleep or disconnected. Press a key, then choose Read / reconnect.",
            Self::PendingReply => "The receiver has an unread reply from an earlier operation. Unplug and reconnect the receiver, then choose Read / reconnect.",
            Self::CommandTimeout => "The receiver did not become ready for a keyboard command. Choose Read / reconnect.",
            Self::ReplyTimeout => "The keyboard did not return a reply through the receiver. Press a key, then choose Read / reconnect.",
        })
    }
}
impl std::error::Error for RelayError {}

impl Status {
    pub fn decode(payload: &[u8]) -> Result<Self, StatusError> {
        if payload.len() != 64 {
            return Err(StatusError::Length(payload.len()));
        }
        for offset in [0, 3, 5] {
            if payload[offset] > 1 {
                return Err(StatusError::Flag {
                    offset,
                    value: payload[offset],
                });
            }
        }
        // Other bytes include battery/mouse state and retained reply bytes.
        // They are not keyboard flags, response opcodes or checksums.
        Ok(Self {
            reply_ready: payload[0] == 1,
            keyboard_online: payload[3] == 0,
            command_ready: payload[5] == 1,
        })
    }

    /// An old pending reply cannot establish correlation for a new raw page.
    pub fn before_command(self) -> Result<(), RelayError> {
        if !self.keyboard_online {
            return Err(RelayError::KeyboardOffline);
        }
        if self.reply_ready {
            return Err(RelayError::PendingReply);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityError {
    Length(usize),
    Opcode(u8),
    Board(u16),
}
impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length(length) => write!(
                f,
                "Receiver keyboard identity requires 64 bytes, got {length}"
            ),
            Self::Opcode(opcode) => write!(
                f,
                "Unrelated receiver keyboard identity response: {opcode:02x}"
            ),
            Self::Board(board) => write!(
                f,
                "Receiver is paired with an unsupported keyboard: {board:04x}"
            ),
        }
    }
}
impl std::error::Error for IdentityError {}

pub fn validate_board_identity(payload: &[u8]) -> Result<(), IdentityError> {
    if payload.len() != 64 {
        return Err(IdentityError::Length(payload.len()));
    }
    if payload[0] != 0x8f {
        return Err(IdentityError::Opcode(payload[0]));
    }
    let board = u16::from_le_bytes([payload[1], payload[2]]);
    if board != 0x095f {
        return Err(IdentityError::Board(board));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_controls_match_independently_observed_headers() {
        for (report, header) in [
            (status_request(), [0xf7, 0, 0, 0, 0, 0, 0, 0x08]),
            (select_keyboard_request(), [0xf6, 10, 0, 0, 0, 0, 0, 0xff]),
            (release_reply_request(), [0xfc, 0, 0, 0, 0, 0, 0, 0x03]),
        ] {
            assert_eq!(&report[..8], &header);
            assert!(report[8..].iter().all(|byte| *byte == 0));
        }
    }

    #[test]
    fn status_decodes_awake_sleep_and_pending_without_interpreting_retained_bytes() {
        let mut payload = [0; 64];
        payload[..9].copy_from_slice(&[0, 100, 0, 0, 1, 1, 1, 0x7f, 0]);
        let awake = Status::decode(&payload).unwrap();
        assert!(awake.command_ready);
        assert_eq!(awake.before_command(), Ok(()));
        payload[3] = 1;
        assert_eq!(
            Status::decode(&payload).unwrap().before_command(),
            Err(RelayError::KeyboardOffline)
        );
        payload[3] = 0;
        payload[0] = 1;
        assert_eq!(
            Status::decode(&payload).unwrap().before_command(),
            Err(RelayError::PendingReply)
        );
        for offset in [0, 3, 5] {
            let mut invalid = payload;
            invalid[offset] = 2;
            assert_eq!(
                Status::decode(&invalid),
                Err(StatusError::Flag { offset, value: 2 })
            );
        }
        assert_eq!(Status::decode(&[]), Err(StatusError::Length(0)));
        assert_eq!(Status::decode(&[0; 65]), Err(StatusError::Length(65)));
    }

    #[test]
    fn receiver_identity_does_not_grant_support_to_another_board() {
        let mut reply = [0; 64];
        reply[..3].copy_from_slice(&[0x8f, 0x5f, 0x09]);
        assert_eq!(validate_board_identity(&reply), Ok(()));
        reply[1] = 0x60;
        assert_eq!(
            validate_board_identity(&reply),
            Err(IdentityError::Board(0x0960))
        );
        reply[0] = 0;
        assert_eq!(
            validate_board_identity(&reply),
            Err(IdentityError::Opcode(0))
        );
        assert_eq!(
            validate_board_identity(&[0; 63]),
            Err(IdentityError::Length(63))
        );
    }
}
