//! Closed state/status vocabularies. `DeviceLifecycleState`, `SupportTier`,
//! and `JobState` mirror `packages/contracts/src/states.ts` (device
//! lifecycle, durable job) and `packages/contracts/src/connector.ts`
//! (support tier) — see each type's doc comment for its exact `Docs/`
//! citation. Every variant's wire string is the exact lowercase-hyphenated
//! value the cited document uses, transcribed, not invented.
//!
//! `OutboxStatus`/`InboxStatus` are this crate's own scaffold vocabulary
//! for the outbox/inbox table pair (not sourced from `Docs/SPEC.md` — see
//! their doc comments).

use crate::error::StorageError;

macro_rules! define_state_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $( $variant:ident => $wire:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $variant, )+
        }

        impl $name {
            /// Every variant, in declaration order — used by this module's
            /// own round-trip tests and available to callers that need to
            /// enumerate the closed set.
            pub const ALL: &'static [$name] = &[ $( $name::$variant, )+ ];

            /// The exact wire string this variant persists as.
            pub fn as_str(&self) -> &'static str {
                match self {
                    $( $name::$variant => $wire, )+
                }
            }

            /// Parses a stored/caller-supplied string into this closed
            /// enum, or a [`StorageError::InvalidState`] naming `field` if
            /// `value` is not one of the known wire strings — this is what
            /// makes a corrupted/hand-edited state value in the database a
            /// detected error rather than a silently accepted one.
            pub fn parse(field: &'static str, value: &str) -> Result<Self, StorageError> {
                match value {
                    $( $wire => Ok($name::$variant), )+
                    _ => Err(StorageError::InvalidState { field, value: value.to_string() }),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

define_state_enum!(
    /// `Docs/SPEC.md` Section 4.4 Device lifecycle (ASCII diagram node
    /// labels) — mirrors `packages/contracts/src/states.ts`
    /// `DEVICE_LIFECYCLE_STATES`.
    DeviceLifecycleState {
        Discovered => "discovered",
        Identifying => "identifying",
        Identified => "identified",
        Checking => "checking",
        EnrollmentRequired => "enrollment-required",
        Unsupported => "unsupported",
        Degraded => "degraded",
        Enrolled => "enrolled",
        Ready => "ready",
        Offline => "offline",
        Busy => "busy",
    }
);

define_state_enum!(
    /// `Docs/SPEC.md` 3.3 Device "support tier". The 4-value vocabulary
    /// itself is `Docs/CONNECTOR_SPEC.md` Section 2's "Support levels"
    /// table, as transcribed by `packages/contracts/src/connector.ts`
    /// (`SUPPORT_TIERS`) — this crate mirrors that existing transcription
    /// rather than independently re-deriving it from
    /// `Docs/CONNECTOR_SPEC.md`, which is outside this node's
    /// required-reading scope (`Docs/graph/tasks/DAT-001.md` §4).
    SupportTier {
        Certified => "certified",
        Supported => "supported",
        Custom => "custom",
        Unsupported => "unsupported",
    }
);

define_state_enum!(
    /// `Docs/SPEC.md` Section 4.7 Durable job. Mirrors
    /// `packages/contracts/src/states.ts` `JOB_STATES`.
    JobState {
        Queued => "queued",
        PolicyCheck => "policy-check",
        WaitingApproval => "waiting-approval",
        Preflight => "preflight",
        Executing => "executing",
        Restarting => "restarting",
        Testing => "testing",
        Observing => "observing",
        Succeeded => "succeeded",
        Failed => "failed",
        Cancelled => "cancelled",
        RollbackPending => "rollback-pending",
        RollingBack => "rolling-back",
        RolledBack => "rolled-back",
        OutcomeUnknown => "outcome-unknown",
    }
);

define_state_enum!(
    /// Not sourced from `Docs/SPEC.md` — `CLD-002` owns the actual cloud
    /// sync/delivery contract and may redefine this. This is `DAT-001`'s
    /// own minimal scaffold status vocabulary for the durable outbox table
    /// structure the task packet authorizes ("an outbox/inbox table pair
    /// for future cloud sync (CLD-002) ... just the durable table structure
    /// and repository read/write API" — `Docs/graph/tasks/DAT-001.md` §1).
    /// Deliberately does not encode delivery attempts/backoff policy
    /// (forbidden scope, §5): it tracks local persistence state only.
    OutboxStatus {
        Pending => "pending",
        Sending => "sending",
        Sent => "sent",
        Failed => "failed",
    }
);

define_state_enum!(
    /// Not sourced from `Docs/SPEC.md` — see [`OutboxStatus`]'s doc
    /// comment; the same provenance note applies to the inbox side of the
    /// pair.
    InboxStatus {
        Received => "received",
        Processed => "processed",
        Failed => "failed",
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_device_lifecycle_variant() {
        for state in DeviceLifecycleState::ALL {
            assert_eq!(
                DeviceLifecycleState::parse("lifecycle_state", state.as_str()).unwrap(),
                *state
            );
        }
    }

    #[test]
    fn round_trips_every_support_tier_variant() {
        for state in SupportTier::ALL {
            assert_eq!(
                SupportTier::parse("support_tier", state.as_str()).unwrap(),
                *state
            );
        }
    }

    #[test]
    fn round_trips_every_job_state_variant() {
        for state in JobState::ALL {
            assert_eq!(JobState::parse("state", state.as_str()).unwrap(), *state);
        }
    }

    #[test]
    fn round_trips_every_outbox_and_inbox_status_variant() {
        for state in OutboxStatus::ALL {
            assert_eq!(
                OutboxStatus::parse("status", state.as_str()).unwrap(),
                *state
            );
        }
        for state in InboxStatus::ALL {
            assert_eq!(
                InboxStatus::parse("status", state.as_str()).unwrap(),
                *state
            );
        }
    }

    #[test]
    fn rejects_unknown_value() {
        assert!(matches!(
            DeviceLifecycleState::parse("lifecycle_state", "not-a-real-state"),
            Err(StorageError::InvalidState { .. })
        ));
    }

    #[test]
    fn error_names_the_field_and_value() {
        match DeviceLifecycleState::parse("lifecycle_state", "bogus") {
            Err(StorageError::InvalidState { field, value }) => {
                assert_eq!(field, "lifecycle_state");
                assert_eq!(value, "bogus");
            }
            other => panic!("expected InvalidState, got {other:?}"),
        }
    }

    #[test]
    fn device_lifecycle_matches_spec_count() {
        // Docs/SPEC.md 4.4 lists 11 device lifecycle node labels.
        assert_eq!(DeviceLifecycleState::ALL.len(), 11);
    }

    #[test]
    fn job_state_matches_spec_count() {
        // Docs/SPEC.md 4.7 lists 15 durable job states.
        assert_eq!(JobState::ALL.len(), 15);
    }

    #[test]
    fn support_tier_matches_connector_ts_count() {
        assert_eq!(SupportTier::ALL.len(), 4);
    }
}
