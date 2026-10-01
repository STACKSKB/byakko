#[derive(Clone, Copy, Debug)]
pub(super) struct DescriptorInfo {
    pub(super) notification_input_valid: bool,
    pub(super) target: bool,
    pub(super) target_feature_shape_valid: bool,
    pub(super) target_feature_count: usize,
    pub(super) application_usage: Option<(u16, u16)>,
}

#[derive(Clone, Copy)]
struct GlobalState {
    usage_page: u32,
    report_size: Option<u32>,
    report_count: Option<u32>,
    report_id: Option<u32>,
}

/// Parse HID items and inspect only top-level Application Collections. Usage
/// values inside nested collections or ordinary feature fields cannot qualify.
pub(super) fn parse_descriptor(bytes: &[u8]) -> Option<DescriptorInfo> {
    let mut position = 0;
    let mut usage_page = 0u32;
    let mut global_stack: Vec<GlobalState> = Vec::new();
    let mut local_usage = None;
    let mut local_minimum = None;
    let mut depth = 0usize;
    let mut target = false;
    let mut target_collection_depth = None;
    let mut notification_collection_depth = None;
    let mut notification_input_count = 0;
    let mut notification_input_valid = true;
    let mut report_size = None;
    let mut report_count = None;
    let mut report_id = None;
    let mut target_feature_shape_valid = true;
    let mut target_feature_count = 0;
    let mut application_usage = None;
    while position < bytes.len() {
        let prefix = bytes[position];
        position += 1;
        if prefix == 0xfe {
            if position + 2 > bytes.len() {
                return None;
            }
            let length = bytes[position] as usize;
            position += 2;
            position = position.checked_add(length)?;
            if position > bytes.len() {
                return None;
            }
            continue;
        }
        let size = match prefix & 3 {
            3 => 4,
            value => value as usize,
        };
        let end = position.checked_add(size)?;
        let data = bytes.get(position..end)?;
        position = end;
        let value = data.iter().enumerate().fold(0u32, |value, (shift, byte)| {
            value | (u32::from(*byte) << (shift * 8))
        });
        let item_type = (prefix >> 2) & 3;
        let tag = prefix >> 4;
        match (item_type, tag) {
            (1, 0) => usage_page = value,
            (1, 7) => report_size = Some(value),
            (1, 8) => report_id = Some(value),
            (1, 9) => report_count = Some(value),
            (1, 10) => global_stack.push(GlobalState {
                usage_page,
                report_size,
                report_count,
                report_id,
            }),
            (1, 11) => {
                let state = global_stack.pop()?;
                usage_page = state.usage_page;
                report_size = state.report_size;
                report_count = state.report_count;
                report_id = state.report_id;
            }
            (2, 0) => {
                local_usage.get_or_insert(full_usage(usage_page, value, size));
            }
            (2, 1) => local_minimum = Some(full_usage(usage_page, value, size)),
            (0, 10) => {
                let usage = local_usage.or(local_minimum);
                if depth == 0
                    && value == 1
                    && let Some(usage) = usage
                {
                    let page = (usage >> 16) as u16;
                    let usage = usage as u16;
                    if page == 0xffff && usage == 2 {
                        target = true;
                        target_collection_depth = Some(depth + 1);
                        application_usage = Some((page, usage));
                    } else if page == 0xffff && usage == 1 {
                        notification_collection_depth = Some(depth + 1);
                        if !target {
                            application_usage = Some((page, usage));
                        }
                    } else if application_usage.is_none() {
                        application_usage = Some((page, usage));
                    }
                }
                depth += 1;
                local_usage = None;
                local_minimum = None;
            }
            (0, 11)
                if target_collection_depth.is_some_and(|target_depth| depth >= target_depth) =>
            {
                target_feature_count += 1;
                target_feature_shape_valid &=
                    report_size == Some(8) && report_count == Some(64) && report_id.is_none();
                local_usage = None;
                local_minimum = None;
            }
            (0, 8)
                if notification_collection_depth
                    .is_some_and(|event_depth| depth >= event_depth) =>
            {
                notification_input_count += 1;
                notification_input_valid &=
                    report_size == Some(8) && report_count == Some(3) && report_id == Some(5);
                local_usage = None;
                local_minimum = None;
            }
            (0, 12) => {
                if depth == 0 {
                    return None;
                }
                if target_collection_depth == Some(depth) {
                    target_collection_depth = None;
                }
                if notification_collection_depth == Some(depth) {
                    notification_collection_depth = None;
                }
                depth -= 1;
                local_usage = None;
                local_minimum = None;
            }
            (0, _) => {
                local_usage = None;
                local_minimum = None;
            }
            _ => {}
        }
    }
    (depth == 0 && global_stack.is_empty()).then_some(DescriptorInfo {
        notification_input_valid: notification_input_valid && notification_input_count == 1,
        target,
        target_feature_shape_valid,
        target_feature_count,
        application_usage,
    })
}

fn full_usage(page: u32, value: u32, size: usize) -> u32 {
    if size == 4 {
        value
    } else {
        ((page & 0xffff) << 16) | value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_descriptor_requires_report_five_three_byte_payload() {
        let observed = [
            0x06, 0xff, 0xff, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x05, 0x75, 0x08, 0x95, 0x03, 0x81,
            0x02, 0xc0,
        ];
        let parsed = parse_descriptor(&observed).unwrap();
        assert!(parsed.notification_input_valid);
        assert_eq!(parsed.application_usage, Some((0xffff, 1)));
        for (index, value) in [(8, 4), (10, 7), (12, 64), (4, 2)] {
            let mut wrong = observed;
            wrong[index] = value;
            assert!(!parse_descriptor(&wrong).unwrap().notification_input_valid);
        }
        assert!(parse_descriptor(&observed[..observed.len() - 1]).is_none());
    }

    #[test]
    fn observed_nia87_descriptor_is_top_level_vendor_usage() {
        let descriptor = [
            0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01, 0x09, 0x02, 0x15, 0x80, 0x25, 0x7f, 0x75,
            0x08, 0x95, 0x40, 0xb1, 0x02, 0xc0,
        ];
        let mut linux_order = descriptor;
        linux_order[13..17].copy_from_slice(&[0x95, 0x40, 0x75, 0x08]);
        for bytes in [descriptor, linux_order] {
            let parsed = parse_descriptor(&bytes).unwrap();
            assert!(parsed.target);
            assert_eq!(parsed.target_feature_count, 1);
            assert!(parsed.target_feature_shape_valid);
        }
    }

    #[test]
    fn parser_rejects_incompatible_or_nested_collections() {
        let prefix = [0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01];
        let suffix = [0xb1, 0x02, 0xc0];
        for globals in [
            &[0x75, 0x07, 0x95, 0x40][..],
            &[0x75, 0x08, 0x95, 0x3f][..],
            &[0x85, 0x01, 0x75, 0x08, 0x95, 0x40][..],
        ] {
            let descriptor = [prefix.as_slice(), globals, suffix.as_slice()].concat();
            let parsed = parse_descriptor(&descriptor).unwrap();
            assert!(parsed.target);
            assert!(!parsed.target_feature_shape_valid);
        }
        let nested = [
            0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01, 0xc0,
            0xc0,
        ];
        assert!(!parse_descriptor(&nested).unwrap().target);
    }
}
