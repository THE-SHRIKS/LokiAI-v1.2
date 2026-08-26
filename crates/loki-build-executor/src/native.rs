use crate::containment::{CapsuleAclEntry, ContainmentPolicy};
use crate::protocol::{BoundedId, DigestId, ResourceLimits, StagedInput};
use crate::{BuildError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedCapsule {
    pub volume_id: BoundedId,
}

/// Narrow low-level boundary implemented only by the separately reviewed,
/// signed host broker. It carries identities and sealed descriptors, never a
/// caller-controlled path, command, executable, URL, environment, or handle.
pub trait NativeCapsuleApi: Send {
    fn create(
        &mut self,
        capsule_id: &BoundedId,
        sealed_base_digest: &DigestId,
        limits: &ResourceLimits,
    ) -> Result<CreatedCapsule, BuildError>;
    fn attach(&mut self, capsule_id: &BoundedId, volume_id: &BoundedId) -> Result<(), BuildError>;
    fn mount_private(
        &mut self,
        capsule_id: &BoundedId,
        volume_id: &BoundedId,
    ) -> Result<(), BuildError>;
    fn prepare_internal_acl(
        &mut self,
        capsule_id: &BoundedId,
        volume_id: &BoundedId,
        sid_digest: &DigestId,
        plan: &[CapsuleAclEntry],
    ) -> Result<(), BuildError>;
    fn stage_inputs(
        &mut self,
        capsule_id: &BoundedId,
        volume_id: &BoundedId,
        inputs: &[StagedInput],
    ) -> Result<(), BuildError>;
    fn attest_and_execute(
        &mut self,
        capsule_id: &BoundedId,
        volume_id: &BoundedId,
        policy: &ContainmentPolicy,
    ) -> Result<(), BuildError>;
    fn quarantine_output(
        &mut self,
        capsule_id: &BoundedId,
        volume_id: &BoundedId,
    ) -> Result<(), BuildError>;
    fn detach(&mut self, capsule_id: &BoundedId, volume_id: &BoundedId) -> Result<(), BuildError>;
    fn destroy(&mut self, capsule_id: &BoundedId, volume_id: &BoundedId) -> Result<(), BuildError>;
    fn reconcile(
        &mut self,
        capsule_id: &BoundedId,
        volume_id: Option<&BoundedId>,
    ) -> Result<(), BuildError>;
}

/// CODE-node production placeholder: always unavailable. It prevents an
/// accidental host operation before HOST-VERIFY supplies a reviewed adapter.
#[derive(Debug, Default)]
pub struct HostOperationsDisabled;

impl NativeCapsuleApi for HostOperationsDisabled {
    fn create(
        &mut self,
        _: &BoundedId,
        _: &DigestId,
        _: &ResourceLimits,
    ) -> Result<CreatedCapsule, BuildError> {
        disabled()
    }
    fn attach(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        disabled()
    }
    fn mount_private(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        disabled()
    }
    fn prepare_internal_acl(
        &mut self,
        _: &BoundedId,
        _: &BoundedId,
        _: &DigestId,
        _: &[CapsuleAclEntry],
    ) -> Result<(), BuildError> {
        disabled()
    }
    fn stage_inputs(
        &mut self,
        _: &BoundedId,
        _: &BoundedId,
        _: &[StagedInput],
    ) -> Result<(), BuildError> {
        disabled()
    }
    fn attest_and_execute(
        &mut self,
        _: &BoundedId,
        _: &BoundedId,
        _: &ContainmentPolicy,
    ) -> Result<(), BuildError> {
        disabled()
    }
    fn quarantine_output(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        disabled()
    }
    fn detach(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        disabled()
    }
    fn destroy(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        disabled()
    }
    fn reconcile(&mut self, _: &BoundedId, _: Option<&BoundedId>) -> Result<(), BuildError> {
        disabled()
    }
}

fn disabled<T>() -> Result<T, BuildError> {
    Err(BuildError::new(
        ErrorCode::NativeUnavailable,
        "host capsule operations are disabled until BLD-001-R2-HOST-VERIFY",
    ))
}
