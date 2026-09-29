//! Decode only the selected Nia87 input notification collection's observed reports.
use byakko_core::contract::DeviceChange;

/// WebHID supplies `report_id` separately from the three payload bytes. Native
/// HID callers split the leading report ID from their four-byte input report.
pub fn decode(report_id: u8, payload: &[u8]) -> Option<DeviceChange> {
    if report_id != 5 || payload.len() != 3 {
        return None;
    }
    match payload {
        [4..=7, _, _] => Some(DeviceChange::Lighting),
        // Include the observed Windows-lock mask omitted by the vendor UI.
        [3, _, 0 | 1 | 2 | 4 | 9] | [2, _, 0] | [19, 0, 0] => Some(DeviceChange::Settings),
        [1, _, _] | [3, _, 8] | [13, 0, 0] => Some(DeviceChange::Configuration),
        // Start/stop (15), unknown commands and unrelated reports carry no edit.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_onboard_notifications_route_only_affected_features() {
        for payload in [[4, 3, 0], [5, 2, 0], [6, 3, 0], [7, 1, 0]] {
            assert_eq!(decode(5, &payload), Some(DeviceChange::Lighting));
        }
        for payload in [[3, 1, 1], [3, 0, 1], [3, 1, 9], [3, 2, 2]] {
            assert_eq!(decode(5, &payload), Some(DeviceChange::Settings));
        }
        for payload in [[13, 0, 0], [1, 2, 0], [3, 1, 8]] {
            assert_eq!(decode(5, &payload), Some(DeviceChange::Configuration));
        }
        assert_eq!(decode(5, &[15, 1, 0]), None);
        assert_eq!(decode(5, &[15, 0, 0]), None);
        assert_eq!(decode(5, &[255, 1, 0]), None);
        assert_eq!(decode(4, &[4, 1, 0]), None);
        assert_eq!(decode(5, &[4, 1]), None);
    }
}
