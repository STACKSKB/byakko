//! Stable yc500-shaped macro reads over a caller-owned report exchange.
use super::macro_reports;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn read_stable(
    slot: u8,
    mut exchange: impl FnMut(u8, u8, u8) -> Result<[u8; 64]>,
) -> Result<Vec<u8>> {
    stable_reads(|| read_complete(slot, &mut exchange))
}

fn read_complete(
    slot: u8,
    exchange: &mut impl FnMut(u8, u8, u8) -> Result<[u8; 64]>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(256);
    for page in 0..4 {
        macro_reports::read_request(slot, page)?;
        let barrier = exchange(0x80, 0, 0)?;
        if barrier[0] != 0x80 {
            return Err("Macro read identity barrier failed".into());
        }
        let data = exchange(0x8b, slot, page)?;
        if data == barrier {
            return Err("Macro read returned stale identity data".into());
        }
        bytes.extend_from_slice(&data);
    }
    Ok(bytes)
}

pub fn stable_reads(mut read: impl FnMut() -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    // A post-write first page can mix new and old bytes. Require two
    // consecutive complete copies, permitting one transitional first copy.
    let mut previous = None;
    for _ in 0..3 {
        let bytes = read()?;
        if bytes.len() != 256 {
            return Err("Incomplete macro snapshot".into());
        }
        if previous.as_ref() == Some(&bytes) {
            return Ok(bytes);
        }
        previous = Some(bytes);
    }
    Err("Macro did not stabilize across three complete reads".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_sequence_checks_each_barrier_and_two_full_copies() {
        let mut calls = Vec::new();
        let data = read_stable(3, |opcode, index, page| {
            calls.push((opcode, index, page));
            let mut response = [page; 64];
            response[0] = opcode;
            Ok(response)
        })
        .unwrap();
        assert_eq!(data.len(), 256);
        assert_eq!(calls.len(), 16);
        for pair in calls.as_chunks::<2>().0 {
            assert_eq!(pair[0], (0x80, 0, 0));
            assert_eq!(pair[1].0, 0x8b);
            assert_eq!(pair[1].1, 3);
        }
        assert_eq!(calls[1].2, 0);
        assert_eq!(calls[7].2, 3);
        assert_eq!(calls[9].2, 0);
    }

    #[test]
    fn invalid_slot_is_rejected_before_exchange() {
        assert!(read_stable(50, |_, _, _| panic!("unexpected exchange")).is_err());
    }

    #[test]
    fn barrier_and_stale_reply_fail_closed() {
        let invalid = read_stable(0, |_, _, _| Ok([0; 64])).unwrap_err();
        assert_eq!(invalid.to_string(), "Macro read identity barrier failed");

        let stale = read_stable(0, |_, _, _| {
            let mut reply = [0; 64];
            reply[0] = 0x80;
            Ok(reply)
        })
        .unwrap_err();
        assert_eq!(stale.to_string(), "Macro read returned stale identity data");
    }

    #[test]
    fn exchange_error_is_preserved() {
        #[derive(Debug)]
        struct Disconnected;
        impl std::fmt::Display for Disconnected {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("disconnected")
            }
        }
        impl std::error::Error for Disconnected {}

        let error = read_stable(1, |_, _, _| Err(Box::new(Disconnected))).unwrap_err();
        assert!(error.downcast_ref::<Disconnected>().is_some());
    }
}
