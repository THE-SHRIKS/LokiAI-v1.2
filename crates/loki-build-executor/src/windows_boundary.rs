//! Compile-time Windows broker boundary. The privileged HOST-VERIFY node owns
//! the concrete signed Win32 implementation; this module makes its authority
//! explicit and injectable without exposing caller-controlled host paths.

use crate::containment::{CapsuleAclEntry, ContainmentPolicy};
use crate::native::{CreatedCapsule, NativeCapsuleApi};
use crate::protocol::{BoundedId, DigestId, ResourceLimits, StagedInput};
use crate::{BuildError, ErrorCode};

pub trait WindowsDiskApi: Send {
    type CapsuleHandle: Send;
    fn create_from_sealed_base(
        &mut self,
        capsule: &BoundedId,
        base: &DigestId,
        limits: &ResourceLimits,
    ) -> Result<(Self::CapsuleHandle, BoundedId), BuildError>;
    fn attach_private(
        &mut self,
        handle: &Self::CapsuleHandle,
        volume: &BoundedId,
    ) -> Result<(), BuildError>;
    fn mount_without_drive_letter(
        &mut self,
        handle: &Self::CapsuleHandle,
        volume: &BoundedId,
    ) -> Result<(), BuildError>;
    fn apply_capsule_acl(
        &mut self,
        handle: &Self::CapsuleHandle,
        volume: &BoundedId,
        sid_digest: &DigestId,
        plan: &[CapsuleAclEntry],
    ) -> Result<(), BuildError>;
    fn stage_verified_inputs(
        &mut self,
        handle: &Self::CapsuleHandle,
        volume: &BoundedId,
        inputs: &[StagedInput],
    ) -> Result<(), BuildError>;
    fn launch_attested_executor(
        &mut self,
        handle: &Self::CapsuleHandle,
        volume: &BoundedId,
        policy: &ContainmentPolicy,
    ) -> Result<(), BuildError>;
    fn seal_output_quarantine(
        &mut self,
        handle: &Self::CapsuleHandle,
        volume: &BoundedId,
    ) -> Result<(), BuildError>;
    fn detach(
        &mut self,
        handle: &Self::CapsuleHandle,
        volume: &BoundedId,
    ) -> Result<(), BuildError>;
    fn destroy(
        &mut self,
        handle: &Self::CapsuleHandle,
        volume: &BoundedId,
    ) -> Result<(), BuildError>;
    fn reconcile_identity(
        &mut self,
        capsule: &BoundedId,
        volume: Option<&BoundedId>,
    ) -> Result<(), BuildError>;
}

pub struct WindowsNativeBoundary<D: WindowsDiskApi> {
    disk: D,
    active: Option<(BoundedId, D::CapsuleHandle, BoundedId)>,
}

impl<D: WindowsDiskApi> WindowsNativeBoundary<D> {
    pub fn new(disk: D) -> Self {
        Self { disk, active: None }
    }

    pub fn into_inner(self) -> D {
        self.disk
    }

    fn with_active<F>(
        &mut self,
        capsule: &BoundedId,
        volume: &BoundedId,
        operation: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut D, &D::CapsuleHandle) -> Result<(), BuildError>,
    {
        let active = self.active.take().ok_or_else(|| {
            BuildError::new(
                ErrorCode::IdentityMismatch,
                "native capsule handle is absent",
            )
        })?;
        let (active_capsule, handle, active_volume) = active;
        if &active_capsule != capsule || &active_volume != volume {
            self.active = Some((active_capsule, handle, active_volume));
            return Err(BuildError::new(
                ErrorCode::IdentityMismatch,
                "native capsule handle is bound to another identity",
            ));
        }
        let result = operation(&mut self.disk, &handle);
        self.active = Some((active_capsule, handle, active_volume));
        result
    }
}

impl<D: WindowsDiskApi> NativeCapsuleApi for WindowsNativeBoundary<D> {
    fn create(
        &mut self,
        capsule: &BoundedId,
        base: &DigestId,
        limits: &ResourceLimits,
    ) -> Result<CreatedCapsule, BuildError> {
        if self.active.is_some() {
            return Err(BuildError::new(
                ErrorCode::IdentityMismatch,
                "broker already owns an active capsule handle",
            ));
        }
        let (handle, volume_id) = self.disk.create_from_sealed_base(capsule, base, limits)?;
        self.active = Some((capsule.clone(), handle, volume_id.clone()));
        Ok(CreatedCapsule { volume_id })
    }
    fn attach(&mut self, capsule: &BoundedId, volume: &BoundedId) -> Result<(), BuildError> {
        self.with_active(capsule, volume, |disk, handle| {
            disk.attach_private(handle, volume)
        })
    }
    fn mount_private(&mut self, capsule: &BoundedId, volume: &BoundedId) -> Result<(), BuildError> {
        self.with_active(capsule, volume, |disk, handle| {
            disk.mount_without_drive_letter(handle, volume)
        })
    }
    fn prepare_internal_acl(
        &mut self,
        capsule: &BoundedId,
        volume: &BoundedId,
        sid: &DigestId,
        plan: &[CapsuleAclEntry],
    ) -> Result<(), BuildError> {
        self.with_active(capsule, volume, |disk, handle| {
            disk.apply_capsule_acl(handle, volume, sid, plan)
        })
    }
    fn stage_inputs(
        &mut self,
        capsule: &BoundedId,
        volume: &BoundedId,
        inputs: &[StagedInput],
    ) -> Result<(), BuildError> {
        self.with_active(capsule, volume, |disk, handle| {
            disk.stage_verified_inputs(handle, volume, inputs)
        })
    }
    fn attest_and_execute(
        &mut self,
        capsule: &BoundedId,
        volume: &BoundedId,
        policy: &ContainmentPolicy,
    ) -> Result<(), BuildError> {
        self.with_active(capsule, volume, |disk, handle| {
            disk.launch_attested_executor(handle, volume, policy)
        })
    }
    fn quarantine_output(
        &mut self,
        capsule: &BoundedId,
        volume: &BoundedId,
    ) -> Result<(), BuildError> {
        self.with_active(capsule, volume, |disk, handle| {
            disk.seal_output_quarantine(handle, volume)
        })
    }
    fn detach(&mut self, capsule: &BoundedId, volume: &BoundedId) -> Result<(), BuildError> {
        self.with_active(capsule, volume, |disk, handle| disk.detach(handle, volume))
    }
    fn destroy(&mut self, capsule: &BoundedId, volume: &BoundedId) -> Result<(), BuildError> {
        self.with_active(capsule, volume, |disk, handle| disk.destroy(handle, volume))?;
        self.active = None;
        Ok(())
    }
    fn reconcile(
        &mut self,
        capsule: &BoundedId,
        volume: Option<&BoundedId>,
    ) -> Result<(), BuildError> {
        self.disk.reconcile_identity(capsule, volume)
    }
}
