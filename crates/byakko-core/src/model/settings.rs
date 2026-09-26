//! Device-neutral scalar settings and their advertised constraints.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
