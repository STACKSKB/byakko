//! Device-neutral projection and one-field apply for Nia87 scalar settings.
use super::{adapter, device, settings as native};
use byakko_core::{
    session::{ApplyFailure, Recovery},
    settings::{self, Capabilities, Content, Edit, Field, Kind, Snapshot, Value},
};
use std::path::Path;

pub fn capabilities() -> Capabilities {
    let number = |min, max| Kind::Number {
        min,
        max,
        step: 1,
        unit: "min".into(),
        disabled_zero: true,
    };
    Capabilities {
        backend_id: adapter::BACKEND_ID.into(),
        fields: vec![
            Field {
                id: "debounce".into(),
                label: "Debounce".into(),
                kind: Kind::Number {
                    min: 1,
                    max: 10,
                    step: 1,
                    unit: "ms".into(),
                    disabled_zero: false,
                },
            },
            Field {
                id: "auto_os".into(),
                label: "Automatic OS mode".into(),
                kind: Kind::Toggle,
            },
            Field {
                id: "bluetooth_sleep".into(),
                label: "Bluetooth sleep".into(),
                kind: number(1, 60),
            },
            Field {
                id: "radio_24_sleep".into(),
                label: "2.4 GHz sleep".into(),
                kind: number(1, 60),
            },
            Field {
                id: "bluetooth_deep_sleep".into(),
                label: "Bluetooth deep sleep".into(),
                kind: number(10, 60),
            },
            Field {
                id: "radio_24_deep_sleep".into(),
                label: "2.4 GHz deep sleep".into(),
                kind: number(10, 60),
            },
            Field {
                id: "backlight".into(),
                label: "Backlight".into(),
                kind: Kind::Toggle,
            },
        ],
    }
}

fn revision(native: &native::Settings) -> Vec<u8> {
    [
        native::DEBOUNCE_READ,
        native::AUTO_OS_READ,
        native::SLEEP_READ,
        native::OPTIONS_READ,
    ]
    .into_iter()
    .flat_map(|opcode| {
        native
            .raw_reply(opcode)
            .expect("native settings has every reply")
            .iter()
            .copied()
    })
    .collect()
}

fn project(native: &native::Settings) -> Snapshot {
    let [bluetooth, radio_24, bluetooth_deep, radio_24_deep] = native.sleep_seconds();
    let sleep_valid = [bluetooth, radio_24].into_iter().all(valid_normal)
        && [bluetooth_deep, radio_24_deep].into_iter().all(valid_deep);
    let editable = native.debounce() >= 1
        && native.debounce() <= 10
        && native
            .raw_reply(native::AUTO_OS_READ)
            .is_some_and(|raw| raw[1] <= 1)
        && sleep_valid;
    let content = if editable {
        Content::Editable(
            [
                ("debounce".into(), Value::Number(native.debounce() as u16)),
                ("auto_os".into(), Value::Toggle(native.auto_os())),
                ("bluetooth_sleep".into(), Value::Number(bluetooth / 60)),
                ("radio_24_sleep".into(), Value::Number(radio_24 / 60)),
                (
                    "bluetooth_deep_sleep".into(),
                    Value::Number(bluetooth_deep / 60),
                ),
                (
                    "radio_24_deep_sleep".into(),
                    Value::Number(radio_24_deep / 60),
                ),
                (
                    "backlight".into(),
                    Value::Toggle(native.backlight_enabled()),
                ),
            ]
            .into(),
        )
    } else {
        Content::Opaque {
            reason: "Nia87 settings contain values outside the editable catalog".into(),
        }
    };
    Snapshot {
        backend_id: adapter::BACKEND_ID.into(),
        revision: revision(native),
        content,
    }
}

fn valid_normal(seconds: u16) -> bool {
    seconds == 0 || (60..=3600).contains(&seconds) && seconds.is_multiple_of(60)
}
fn valid_deep(seconds: u16) -> bool {
    seconds == 0 || (600..=3600).contains(&seconds) && seconds.is_multiple_of(60)
}

fn from_revision(bytes: &[u8]) -> Result<native::Settings, String> {
    if bytes.len() != native::REPORT_LEN * 4 {
        return Err("Invalid Nia87 settings revision length".into());
    }
    native::Settings::decode(
        &bytes[0..64],
        &bytes[64..128],
        &bytes[128..192],
        &bytes[192..256],
    )
}

fn checked_native(snapshot: &Snapshot) -> Result<native::Settings, String> {
    if snapshot.backend_id != adapter::BACKEND_ID {
        return Err("Settings snapshot belongs to another backend".into());
    }
    let native = from_revision(&snapshot.revision)?;
    if project(&native) != *snapshot {
        return Err(
            "Nia87 settings snapshot differs from its revision; reload before editing".into(),
        );
    }
    Ok(native)
}

pub fn read() -> Result<Snapshot, String> {
    Ok(project(
        &device::read_settings().map_err(|error| error.to_string())?,
    ))
}

fn native_setting(expected: &native::Settings, edit: &Edit) -> Result<native::Setting, String> {
    let [bluetooth, radio_24, bluetooth_deep, radio_24_deep] = expected.sleep_seconds();
    match (edit.id.as_str(), &edit.value) {
        ("debounce", Value::Number(value)) => Ok(native::Setting::Debounce(
            u8::try_from(*value).map_err(|_| "Debounce exceeds Nia87 range")?,
        )),
        ("auto_os", Value::Toggle(value)) => Ok(native::Setting::AutoOs(*value)),
        ("bluetooth_sleep", Value::Number(value)) => Ok(native::Setting::Sleep([
            *value * 60,
            radio_24,
            bluetooth_deep,
            radio_24_deep,
        ])),
        ("radio_24_sleep", Value::Number(value)) => Ok(native::Setting::Sleep([
            bluetooth,
            *value * 60,
            bluetooth_deep,
            radio_24_deep,
        ])),
        ("bluetooth_deep_sleep", Value::Number(value)) => Ok(native::Setting::Sleep([
            bluetooth,
            radio_24,
            *value * 60,
            radio_24_deep,
        ])),
        ("radio_24_deep_sleep", Value::Number(value)) => Ok(native::Setting::Sleep([
            bluetooth,
            radio_24,
            bluetooth_deep,
            *value * 60,
        ])),
        ("backlight", Value::Toggle(value)) => Ok(native::Setting::Backlight(*value)),
        _ => Err("Unsupported Nia87 setting field/value".into()),
    }
}

pub fn apply(expected: &Snapshot, edit: &Edit, backup: &Path) -> Result<Snapshot, ApplyFailure> {
    let expected_native = checked_native(expected).map_err(not_attempted)?;
    if !matches!(expected.content, Content::Editable(_)) {
        return Err(not_attempted("Opaque Nia87 settings are read-only".into()));
    }
    settings::validate_value(&capabilities(), edit).map_err(not_attempted)?;
    let setting = native_setting(&expected_native, edit).map_err(not_attempted)?;
    let actual = device::apply_setting_detailed(&expected_native, setting, backup)?;
    let snapshot = project(&actual);
    if !matches!(&snapshot.content, Content::Editable(values) if values.get(&edit.id) == Some(&edit.value))
    {
        return Err(ApplyFailure {
            message: "Settings readback does not match the requested field".into(),
            recovery: Recovery::Unverified,
        });
    }
    Ok(snapshot)
}

fn not_attempted(message: String) -> ApplyFailure {
    ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured() -> native::Settings {
        let mut debounce = [0; 64];
        debounce[0] = native::DEBOUNCE_READ;
        debounce[2] = 4;
        let mut auto = [0; 64];
        auto[0] = native::AUTO_OS_READ;
        auto[1] = 1;
        let mut sleep = [0; 64];
        sleep[0] = native::SLEEP_READ;
        for (i, value) in [120u16, 180, 600, 900].into_iter().enumerate() {
            sleep[1 + i * 2..3 + i * 2].copy_from_slice(&value.to_le_bytes());
        }
        let mut options = [0; 64];
        options[0] = native::OPTIONS_READ;
        options[1] = 0;
        options[2] = 0x10;
        options[4] = 1;
        options[50] = 0xa5;
        native::Settings::decode(&debounce, &auto, &sleep, &options).unwrap()
    }

    #[test]
    fn snapshot_round_trips_all_four_full_native_replies() {
        let raw = captured();
        let snapshot = project(&raw);
        assert_eq!(snapshot.revision.len(), 256);
        assert_eq!(checked_native(&snapshot).unwrap(), raw);
        let Content::Editable(values) = snapshot.content else {
            unreachable!()
        };
        assert_eq!(values["debounce"], Value::Number(4));
        assert_eq!(values["bluetooth_deep_sleep"], Value::Number(10));
        assert_eq!(values["backlight"], Value::Toggle(false));
        assert_eq!(snapshot.revision[192 + 50], 0xa5);
    }

    #[test]
    fn forged_or_truncated_baseline_is_rejected() {
        let raw = captured();
        let mut snapshot = project(&raw);
        if let Content::Editable(values) = &mut snapshot.content {
            values.insert("debounce".into(), Value::Number(5));
        }
        assert!(checked_native(&snapshot).is_err());
        snapshot = project(&raw);
        snapshot.revision.pop();
        assert!(checked_native(&snapshot).is_err());
    }

    #[test]
    fn noncanonical_native_values_are_retained_as_opaque() {
        let mut debounce = [0; 64];
        debounce[0] = native::DEBOUNCE_READ;
        debounce[2] = 4;
        let mut auto = [0; 64];
        auto[0] = native::AUTO_OS_READ;
        let mut sleep = [0; 64];
        sleep[0] = native::SLEEP_READ;
        sleep[1] = 61;
        let mut options = [0; 64];
        options[0] = native::OPTIONS_READ;
        let raw = native::Settings::decode(&debounce, &auto, &sleep, &options).unwrap();
        let snapshot = project(&raw);
        assert!(matches!(snapshot.content, Content::Opaque { .. }));
        assert_eq!(checked_native(&snapshot).unwrap(), raw);
    }

    #[test]
    fn sleep_field_mapping_converts_minutes_and_preserves_other_timers() {
        let before = captured();
        let expected = project(&before);
        let edit = Edit {
            id: "radio_24_sleep".into(),
            value: Value::Number(4),
        };
        let raw = checked_native(&expected).unwrap();
        assert_eq!(
            native_setting(&raw, &edit).unwrap(),
            native::Setting::Sleep([120, 240, 600, 900])
        );
    }
}
