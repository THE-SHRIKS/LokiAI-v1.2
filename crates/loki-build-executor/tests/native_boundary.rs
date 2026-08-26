mod support;
use loki_build_executor::BuildError;
use loki_build_executor::broker::CapsuleBroker;
use loki_build_executor::containment::{CapsuleAclEntry, ContainmentPolicy};
use loki_build_executor::journal::MemoryJournal;
use loki_build_executor::protocol::{
    Authenticator, BoundedId, BrokerAction, DigestId, ResourceLimits, StagedInput,
};
use loki_build_executor::windows_boundary::{WindowsDiskApi, WindowsNativeBoundary};
use support::*;
#[derive(Default)]
struct DiskLedger {
    operations: Vec<&'static str>,
}
impl WindowsDiskApi for DiskLedger {
    type CapsuleHandle = BoundedId;
    fn create_from_sealed_base(
        &mut self,
        capsule: &BoundedId,
        _: &DigestId,
        _: &ResourceLimits,
    ) -> Result<(Self::CapsuleHandle, BoundedId), BuildError> {
        self.operations.push("create");
        Ok((capsule.clone(), id("volume-1")))
    }
    fn attach_private(&mut self, _: &Self::CapsuleHandle, _: &BoundedId) -> Result<(), BuildError> {
        self.operations.push("attach");
        Ok(())
    }
    fn mount_without_drive_letter(
        &mut self,
        _: &Self::CapsuleHandle,
        _: &BoundedId,
    ) -> Result<(), BuildError> {
        self.operations.push("mount");
        Ok(())
    }
    fn apply_capsule_acl(
        &mut self,
        _: &Self::CapsuleHandle,
        _: &BoundedId,
        _: &DigestId,
        plan: &[CapsuleAclEntry],
    ) -> Result<(), BuildError> {
        assert_eq!(plan.len(), 5);
        self.operations.push("acl");
        Ok(())
    }
    fn stage_verified_inputs(
        &mut self,
        _: &Self::CapsuleHandle,
        _: &BoundedId,
        _: &[StagedInput],
    ) -> Result<(), BuildError> {
        self.operations.push("stage");
        Ok(())
    }
    fn launch_attested_executor(
        &mut self,
        _: &Self::CapsuleHandle,
        _: &BoundedId,
        policy: &ContainmentPolicy,
    ) -> Result<(), BuildError> {
        policy.validate()?;
        self.operations.push("execute");
        Ok(())
    }
    fn seal_output_quarantine(
        &mut self,
        _: &Self::CapsuleHandle,
        _: &BoundedId,
    ) -> Result<(), BuildError> {
        self.operations.push("quarantine");
        Ok(())
    }
    fn detach(&mut self, _: &Self::CapsuleHandle, _: &BoundedId) -> Result<(), BuildError> {
        self.operations.push("detach");
        Ok(())
    }
    fn destroy(&mut self, _: &Self::CapsuleHandle, _: &BoundedId) -> Result<(), BuildError> {
        self.operations.push("destroy");
        Ok(())
    }
    fn reconcile_identity(
        &mut self,
        _: &BoundedId,
        _: Option<&BoundedId>,
    ) -> Result<(), BuildError> {
        self.operations.push("reconcile");
        Ok(())
    }
}
#[test]
fn typed_native_boundary_preserves_order_and_carries_no_host_paths() {
    let api = WindowsNativeBoundary::new(DiskLedger::default());
    let mut broker = CapsuleBroker::new(
        Authenticator::new(&KEY).unwrap(),
        MemoryJournal::default(),
        api,
    )
    .unwrap();
    for (index, action) in [
        BrokerAction::Create,
        BrokerAction::Attach,
        BrokerAction::Mount,
        BrokerAction::PrepareAcl,
        BrokerAction::AuthorizeExecution,
        BrokerAction::QuarantineOutput,
        BrokerAction::Detach,
        BrokerAction::Destroy,
    ]
    .into_iter()
    .enumerate()
    {
        broker
            .handle(
                &bytes(&signed(action, &format!("native-{index}"), 10, 20)),
                15,
            )
            .unwrap();
    }
    let (_, api) = broker.into_parts();
    assert_eq!(
        api.into_inner().operations,
        [
            "create",
            "attach",
            "mount",
            "acl",
            "stage",
            "execute",
            "quarantine",
            "detach",
            "destroy"
        ]
    );
}
