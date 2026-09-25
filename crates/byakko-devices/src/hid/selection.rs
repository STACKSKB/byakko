//! Backend-neutral HID collection identity and exact selection rules.

use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identity {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
}

impl Identity {
    /// Stable discovery token for the same collection fields used by exact selection.
    /// Display strings are deliberately absent because they can vary by OS locale.
    pub fn discovery_id(&self) -> String {
        format!(
            "{}|{:04x}:{:04x}:{}:{:04x}:{:04x}",
            self.path, self.vendor_id, self.product_id, self.interface, self.usage_page, self.usage
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionError {
    Missing,
    Ambiguous(usize),
    Changed,
}

impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("The selected HID collection is no longer present"),
            Self::Ambiguous(count) => {
                write!(f, "Expected one HID collection; found {count}")
            }
            Self::Changed => {
                f.write_str("The selected HID collection identity changed; no device opened")
            }
        }
    }
}

impl std::error::Error for SelectionError {}

/// Pins one observed collection and only accepts a later unique exact match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target(Identity);

impl Target {
    pub fn from_identity(identity: Identity) -> Self {
        Self(identity)
    }

    pub fn identity(&self) -> &Identity {
        &self.0
    }

    pub fn select(&self, candidates: &[Identity]) -> Result<(), SelectionError> {
        let [candidate] = candidates else {
            return Err(if candidates.is_empty() {
                SelectionError::Missing
            } else {
                SelectionError::Ambiguous(candidates.len())
            });
        };
        if *candidate == self.0 {
            Ok(())
        } else {
            Err(SelectionError::Changed)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Availability<T> {
    Unavailable,
    Available(T),
    Ambiguous(Vec<T>),
    EnumerationFailed(String),
}

pub fn classify<T, E: fmt::Display>(result: Result<Vec<T>, E>) -> Availability<T> {
    match result {
        Ok(candidates) => {
            let mut candidates = candidates.into_iter();
            match (candidates.next(), candidates.next()) {
                (None, _) => Availability::Unavailable,
                (Some(candidate), None) => Availability::Available(candidate),
                (Some(first), Some(second)) => {
                    Availability::Ambiguous([first, second].into_iter().chain(candidates).collect())
                }
            }
        }
        Err(error) => Availability::EnumerationFailed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Availability, Identity, SelectionError, Target, classify};

    fn identity(path: &str) -> Identity {
        Identity {
            path: path.into(),
            vendor_id: 1,
            product_id: 2,
            interface: 3,
            usage_page: 4,
            usage: 5,
        }
    }

    #[test]
    fn classifies_empty_single_many_and_enumeration_failure() {
        assert_eq!(classify::<u8, &str>(Ok(vec![])), Availability::Unavailable);
        assert_eq!(
            classify::<u8, &str>(Ok(vec![7])),
            Availability::Available(7)
        );
        assert_eq!(
            classify::<u8, &str>(Ok(vec![1, 2])),
            Availability::Ambiguous(vec![1, 2])
        );
        assert_eq!(
            classify::<u8, &str>(Err("permission denied")),
            Availability::EnumerationFailed("permission denied".into())
        );
    }

    #[test]
    fn target_requires_one_exact_identity() {
        let expected = identity("path-a");
        let target = Target::from_identity(expected.clone());
        assert_eq!(target.select(std::slice::from_ref(&expected)), Ok(()));
        assert_eq!(target.select(&[]), Err(SelectionError::Missing));
        assert_eq!(
            target.select(&[expected.clone(), identity("path-b")]),
            Err(SelectionError::Ambiguous(2))
        );
        for replacement in [
            identity("path-b"),
            Identity {
                usage: 9,
                ..expected.clone()
            },
        ] {
            assert_eq!(
                target.select(std::slice::from_ref(&replacement)),
                Err(SelectionError::Changed)
            );
        }
    }

    #[test]
    fn discovery_id_changes_with_every_selected_identity_field() {
        let original = identity("path-a");
        let changes: [fn(&mut Identity); 6] = [
            |id| id.path = "path-b".into(),
            |id| id.vendor_id += 1,
            |id| id.product_id += 1,
            |id| id.interface += 1,
            |id| id.usage_page += 1,
            |id| id.usage += 1,
        ];
        for change in changes {
            let mut changed = original.clone();
            change(&mut changed);
            assert_ne!(original.discovery_id(), changed.discovery_id());
        }
    }
}
