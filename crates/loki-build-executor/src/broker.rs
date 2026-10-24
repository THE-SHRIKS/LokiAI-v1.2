use crate::containment::{ContainmentPolicy, capsule_acl_plan};
use crate::journal::{Journal, JournalEntry, JournalEntryDraft};
use crate::lifecycle::CapsuleState;
use crate::native::NativeCapsuleApi;
use crate::protocol::{Authenticator, BoundedId, BrokerAction, BrokerRequest, DigestId};
use crate::{BuildError, ErrorCode};

pub struct CapsuleBroker<J: Journal, A: NativeCapsuleApi> {
    auth: Authenticator,
    journal: J,
    api: A,
    state: CapsuleState,
    job_id: Option<BoundedId>,
    capsule_id: Option<BoundedId>,
    base_digest: Option<DigestId>,
    appcontainer_sid_digest: Option<DigestId>,
    volume_id: Option<BoundedId>,
}

impl<J: Journal, A: NativeCapsuleApi> CapsuleBroker<J, A> {
    pub fn new(auth: Authenticator, journal: J, api: A) -> Result<Self, BuildError> {
        let entries = journal.entries()?;
        let (state, job_id, capsule_id, base_digest, appcontainer_sid_digest, volume_id) =
            if let Some(last) = entries.last() {
                // A last entry with `intent == true` records that a mutation was
                // journaled as started but never journaled as complete or
                // quarantined. The process may have crashed between the native
                // call and its completion record, so the real native result is
                // unknown. For actions whose intent and complete states are the
                // same `CapsuleState` (see `target_states`), reconstructing
                // `state == last.state` would make a retry's short-circuit in
                // `handle` report success without ever invoking `apply`/the
                // native operation again. Treat any dangling intent as
                // uncertain and quarantine on reconstruction, matching the
                // "uncertain result -> quarantine, never assume success" rule
                // applied to the synchronous native-failure path below.
                let state = if last.intent {
                    CapsuleState::Quarantined
                } else {
                    last.state
                };
                (
                    state,
                    Some(last.job_id.clone()),
                    Some(last.capsule_id.clone()),
                    Some(last.sealed_base_digest.clone()),
                    Some(last.appcontainer_sid_digest.clone()),
                    last.volume_id.clone(),
                )
            } else {
                (CapsuleState::Requested, None, None, None, None, None)
            };
        Ok(Self {
            auth,
            journal,
            api,
            state,
            job_id,
            capsule_id,
            base_digest,
            appcontainer_sid_digest,
            volume_id,
        })
    }

    pub fn handle(&mut self, raw: &[u8], now_ms: u64) -> Result<CapsuleState, BuildError> {
        let request = self.auth.verify_and_consume(raw, now_ms)?;
        if self.job_id.is_none() && !matches!(request.action, BrokerAction::Create) {
            return Err(BuildError::new(
                ErrorCode::InvalidTransition,
                "an unbound capsule must begin with create",
            ));
        }
        self.bind_or_verify(&request)?;
        if self.state.is_terminal() {
            return Err(BuildError::new(
                ErrorCode::Quarantined,
                "terminal capsule cannot be reused",
            ));
        }
        if let Some(expected) = &request.expected_volume_id
            && self
                .volume_id
                .as_ref()
                .is_some_and(|actual| actual != expected)
        {
            return Err(BuildError::new(
                ErrorCode::IdentityMismatch,
                "volume identity does not match journal",
            ));
        }

        let (intent, complete) = target_states(request.action);
        if self.state == complete {
            return Ok(self.state);
        }
        self.state.transition(intent)?;
        self.append(intent, true, now_ms, "mutation intent persisted")?;

        let result = self.apply(&request);
        match result {
            Ok(()) => {
                self.state = complete;
                self.append(complete, false, now_ms, "mutation completed and attested")?;
                Ok(complete)
            }
            Err(operation_error) => {
                self.state = CapsuleState::Quarantined;
                self.append(
                    CapsuleState::Quarantined,
                    false,
                    now_ms,
                    "native result uncertain or failed; capsule quarantined",
                )?;
                Err(BuildError::new(
                    operation_error.code,
                    format!("{}; capsule quarantined", operation_error.detail),
                ))
            }
        }
    }

    fn bind_or_verify(&mut self, request: &BrokerRequest) -> Result<(), BuildError> {
        if let (Some(job), Some(capsule), Some(base), Some(sid)) = (
            &self.job_id,
            &self.capsule_id,
            &self.base_digest,
            &self.appcontainer_sid_digest,
        ) {
            if job != &request.job_id
                || capsule != &request.capsule_id
                || base != &request.sealed_base_digest
                || sid != &request.appcontainer_sid_digest
            {
                return Err(BuildError::new(
                    ErrorCode::IdentityMismatch,
                    "cross-job, capsule, sealed-base, or AppContainer SID substitution rejected",
                ));
            }
        } else {
            self.job_id = Some(request.job_id.clone());
            self.capsule_id = Some(request.capsule_id.clone());
            self.base_digest = Some(request.sealed_base_digest.clone());
            self.appcontainer_sid_digest = Some(request.appcontainer_sid_digest.clone());
        }
        Ok(())
    }

    fn apply(&mut self, request: &BrokerRequest) -> Result<(), BuildError> {
        let capsule = self
            .capsule_id
            .as_ref()
            .expect("identity bound before apply");
        match request.action {
            BrokerAction::Create => {
                let created =
                    self.api
                        .create(capsule, &request.sealed_base_digest, &request.limits)?;
                if request
                    .expected_volume_id
                    .as_ref()
                    .is_some_and(|expected| expected != &created.volume_id)
                {
                    return Err(BuildError::new(
                        ErrorCode::IdentityMismatch,
                        "created volume identity differs from expectation",
                    ));
                }
                self.volume_id = Some(created.volume_id);
                Ok(())
            }
            BrokerAction::Reconcile => self.api.reconcile(capsule, self.volume_id.as_ref()),
            action => {
                let volume = self.volume_id.as_ref().ok_or_else(|| {
                    BuildError::new(
                        ErrorCode::IdentityMismatch,
                        "journal has no created volume identity",
                    )
                })?;
                match action {
                    BrokerAction::Attach => self.api.attach(capsule, volume),
                    BrokerAction::Mount => self.api.mount_private(capsule, volume),
                    BrokerAction::PrepareAcl => {
                        self.api.prepare_internal_acl(
                            capsule,
                            volume,
                            &request.appcontainer_sid_digest,
                            &capsule_acl_plan(),
                        )?;
                        self.api
                            .stage_inputs(capsule, volume, &request.staged_inputs)
                    }
                    BrokerAction::AuthorizeExecution => {
                        let policy = ContainmentPolicy::strict(
                            request.limits.process_count,
                            request.limits.memory_bytes,
                            request.limits.cpu_time_ms,
                            request.limits.output_bytes,
                        );
                        policy.validate()?;
                        self.api.attest_and_execute(capsule, volume, &policy)
                    }
                    BrokerAction::QuarantineOutput => self.api.quarantine_output(capsule, volume),
                    BrokerAction::Detach => self.api.detach(capsule, volume),
                    BrokerAction::Destroy => self.api.destroy(capsule, volume),
                    BrokerAction::Create | BrokerAction::Reconcile => unreachable!(),
                }
            }
        }
    }

    fn append(
        &mut self,
        state: CapsuleState,
        intent: bool,
        timestamp_ms: u64,
        detail: &str,
    ) -> Result<(), BuildError> {
        let entries = self.journal.entries()?;
        let entry = JournalEntry::new(JournalEntryDraft {
            sequence: entries.len() as u64,
            job_id: self.job_id.clone().expect("bound"),
            capsule_id: self.capsule_id.clone().expect("bound"),
            sealed_base_digest: self.base_digest.clone().expect("bound"),
            appcontainer_sid_digest: self.appcontainer_sid_digest.clone().expect("bound"),
            volume_id: self.volume_id.clone(),
            state,
            intent,
            timestamp_ms,
            previous_hash: entries
                .last()
                .map_or_else(|| "0".repeat(64), |last| last.entry_hash.clone()),
            detail: detail.to_string(),
        })?;
        self.journal.append_durable(entry)
    }

    pub fn into_parts(self) -> (J, A) {
        (self.journal, self.api)
    }
}

fn target_states(action: BrokerAction) -> (CapsuleState, CapsuleState) {
    match action {
        BrokerAction::Create => (CapsuleState::Creating, CapsuleState::Created),
        BrokerAction::Attach => (CapsuleState::Attaching, CapsuleState::Attached),
        BrokerAction::Mount => (CapsuleState::Mounting, CapsuleState::Mounted),
        BrokerAction::PrepareAcl => (CapsuleState::PreparingAcl, CapsuleState::Prepared),
        BrokerAction::AuthorizeExecution => (CapsuleState::Executing, CapsuleState::Executing),
        BrokerAction::QuarantineOutput => (
            CapsuleState::OutputQuarantined,
            CapsuleState::OutputQuarantined,
        ),
        BrokerAction::Detach => (CapsuleState::Detaching, CapsuleState::Detached),
        BrokerAction::Destroy => (CapsuleState::Destroying, CapsuleState::Destroyed),
        BrokerAction::Reconcile => (CapsuleState::Quarantined, CapsuleState::Quarantined),
    }
}
