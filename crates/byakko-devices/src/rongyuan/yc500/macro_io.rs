//! One complete yc500-shaped macro read over a caller-owned report exchange.
use super::macro_reports;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn read_complete(
    slot: u8,
    mut exchange: impl FnMut(u8, u8, u8) -> Result<[u8; 64]>,
) -> Result<Vec<u8>> {
    macro_reports::read_request(slot, 0)?;
    let mut bytes = Vec::with_capacity(256);
    for page in 0..4 {
        let data = exchange(0x8b, slot, page)?;
        bytes.extend_from_slice(&data);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_sequence_requests_one_complete_copy() {
        let mut calls = Vec::new();
        let data = read_complete(3, |opcode, index, page| {
            calls.push((opcode, index, page));
            let mut response = [page; 64];
            response[0] = opcode;
            Ok(response)
        })
        .unwrap();
        assert_eq!(data.len(), 256);
        assert_eq!(
            calls,
            (0..4).map(|page| (0x8b, 3, page)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn invalid_slot_is_rejected_before_exchange() {
        assert!(read_complete(50, |_, _, _| panic!("unexpected exchange")).is_err());
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

        let error = read_complete(1, |_, _, _| Err(Box::new(Disconnected))).unwrap_err();
        assert!(error.downcast_ref::<Disconnected>().is_some());
    }
}
