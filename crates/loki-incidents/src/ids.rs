//! Stable identifier newtypes for this crate's durable entities
//! (`AGENTS.md` rule 5: "use stable IDs ... for operations that may be
//! retried"). Each is a validated, opaque string wrapper generated from a
//! random v4 UUID (`crate::ids::new_uuid_string`) — the same `uuid` crate
//! already pinned at this exact version in `crates/loki-runtime` and
//! `crates/loki-ipc`, not a new dependency decision.

use loki_connectors_sdk::error::ValidationError;

use crate::validate::non_empty_bounded;

/// A fresh, randomly generated stable identifier string (lowercase hyphenated
/// UUID form, e.g. `"a1b2c3d4-...-...-...-..."`).
pub(crate) fn new_uuid_string() -> String {
    uuid::Uuid::new_v4().to_string()
}

macro_rules! stable_id {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
                let value = value.into();
                non_empty_bounded($field, &value, 128)?;
                Ok(Self(value))
            }

            /// Generates a fresh, random id. Never returns a value that
            /// would fail `Self::new` — infallible by construction.
            pub fn generate() -> Self {
                Self(new_uuid_string())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

stable_id!(IncidentId, "incident_id");
stable_id!(EvidenceId, "evidence_id");
stable_id!(WorkIntentId, "work_intent_id");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_non_empty_and_distinct() {
        let a = IncidentId::generate();
        let b = IncidentId::generate();
        assert_ne!(a, b);
        assert!(!a.as_str().is_empty());
    }

    #[test]
    fn new_rejects_empty() {
        assert!(IncidentId::new("").is_err());
        assert!(EvidenceId::new("evidence-1").is_ok());
        assert!(WorkIntentId::new("wi-1").is_ok());
    }
}
