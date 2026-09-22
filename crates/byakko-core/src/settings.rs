//! Device-neutral scalar settings and their advertised constraints.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod editor;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend_id: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub kind: Kind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Toggle,
    Number {
        min: u16,
        max: u16,
        step: u16,
        unit: String,
        disabled_zero: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Toggle(bool),
    Number(u16),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Content {
    Editable(BTreeMap<String, Value>),
    Opaque { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub backend_id: String,
    pub revision: Vec<u8>,
    pub content: Content,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Edit {
    pub id: String,
    pub value: Value,
}

pub fn validate_capabilities(caps: &Capabilities) -> Result<(), String> {
    if caps.backend_id.is_empty() || caps.fields.is_empty() {
        return Err("Empty settings catalog".into());
    }
    let mut seen = BTreeSet::new();
    for field in &caps.fields {
        if field.id.is_empty() || field.label.is_empty() || !seen.insert(&field.id) {
            return Err("Invalid settings field ID or label".into());
        }
        if let Kind::Number {
            min,
            max,
            step,
            disabled_zero,
            ..
        } = &field.kind
            && (*step == 0
                || *min > *max
                || (*disabled_zero && *min == 0)
                || (max - min) % step != 0)
        {
            return Err("Invalid settings numeric range".into());
        }
    }
    Ok(())
}

pub fn validate_value(caps: &Capabilities, edit: &Edit) -> Result<(), String> {
    let field = caps
        .fields
        .iter()
        .find(|field| field.id == edit.id)
        .ok_or("Unknown settings field")?;
    match (&field.kind, &edit.value) {
        (Kind::Toggle, Value::Toggle(_)) => Ok(()),
        (
            Kind::Number {
                min,
                max,
                step,
                disabled_zero,
                ..
            },
            Value::Number(value),
        ) if (*disabled_zero && *value == 0)
            || (*min <= *value && *value <= *max && (*value - *min) % *step == 0) =>
        {
            Ok(())
        }
        _ => Err(format!("Invalid value for settings field {}", edit.id)),
    }
}

pub fn validate_snapshot(caps: &Capabilities, snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.backend_id != caps.backend_id {
        return Err("Settings result belongs to a different backend".into());
    }
    if let Content::Editable(values) = &snapshot.content {
        if values.len() != caps.fields.len() {
            return Err("Settings snapshot has missing fields".into());
        }
        for (id, value) in values {
            validate_value(
                caps,
                &Edit {
                    id: id.clone(),
                    value: value.clone(),
                },
            )?;
        }
    }
    Ok(())
}
