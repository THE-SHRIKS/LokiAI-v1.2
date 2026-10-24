#![allow(dead_code)]
use loki_build_executor::BuildError;
use loki_build_executor::containment::{CapsuleAclEntry, ContainmentPolicy};
use loki_build_executor::native::{CreatedCapsule, NativeCapsuleApi};
use loki_build_executor::protocol::{
    Authenticator, BoundedId, BrokerAction, BrokerRequest, DigestId, InputKind, ResourceLimits,
    SignedEnvelope, StagedInput,
};
pub const KEY: [u8; 32] = [0x5a; 32];
pub fn id(value: &str) -> BoundedId {
    BoundedId::parse(value).unwrap()
}
pub fn digest(ch: char) -> DigestId {
    DigestId::parse(ch.to_string().repeat(64)).unwrap()
}
pub fn limits() -> ResourceLimits {
    ResourceLimits {
        memory_bytes: 128 * 1024 * 1024,
        disk_bytes: 512 * 1024 * 1024,
        cpu_time_ms: 10_000,
        wall_time_ms: 20_000,
        process_count: 8,
        output_bytes: 16 * 1024 * 1024,
    }
}
pub fn request(action: BrokerAction) -> BrokerRequest {
    BrokerRequest {
        job_id: id("job-1"),
        capsule_id: id("capsule-1"),
        sealed_base_digest: digest('a'),
        expected_volume_id: (!matches!(action, BrokerAction::Create)).then(|| id("volume-1")),
        appcontainer_sid_digest: digest('b'),
        staged_inputs: vec![StagedInput {
            id: id("source-1"),
            kind: InputKind::SourceSnapshot,
            digest: digest('c'),
            bytes: 1024,
        }],
        limits: limits(),
        action,
    }
}
pub fn signed(action: BrokerAction, nonce: &str, issued: u64, expires: u64) -> SignedEnvelope {
    Authenticator::new(&KEY)
        .unwrap()
        .sign(id(nonce), issued, expires, request(action))
        .unwrap()
}
pub fn bytes(envelope: &SignedEnvelope) -> Vec<u8> {
    serde_json::to_vec(envelope).unwrap()
}
#[derive(Debug, Default)]
pub struct RecordingApi {
    pub operations: Vec<&'static str>,
    pub fail: Option<&'static str>,
}
impl RecordingApi {
    fn record(&mut self, op: &'static str) -> Result<(), BuildError> {
        self.operations.push(op);
        if self.fail == Some(op) {
            Err(BuildError::new(
                loki_build_executor::ErrorCode::NativeFailure,
                format!("injected {op} failure"),
            ))
        } else {
            Ok(())
        }
    }
}
impl NativeCapsuleApi for RecordingApi {
    fn create(
        &mut self,
        _: &BoundedId,
        _: &DigestId,
        _: &ResourceLimits,
    ) -> Result<CreatedCapsule, BuildError> {
        self.record("create")?;
        Ok(CreatedCapsule {
            volume_id: id("volume-1"),
        })
    }
    fn attach(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        self.record("attach")
    }
    fn mount_private(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        self.record("mount")
    }
    fn prepare_internal_acl(
        &mut self,
        _: &BoundedId,
        _: &BoundedId,
        _: &DigestId,
        plan: &[CapsuleAclEntry],
    ) -> Result<(), BuildError> {
        assert_eq!(plan.len(), 5);
        self.record("acl")
    }
    fn stage_inputs(
        &mut self,
        _: &BoundedId,
        _: &BoundedId,
        _: &[StagedInput],
    ) -> Result<(), BuildError> {
        self.record("stage")
    }
    fn attest_and_execute(
        &mut self,
        _: &BoundedId,
        _: &BoundedId,
        policy: &ContainmentPolicy,
    ) -> Result<(), BuildError> {
        policy.validate()?;
        self.record("execute")
    }
    fn quarantine_output(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        self.record("quarantine-output")
    }
    fn detach(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        self.record("detach")
    }
    fn destroy(&mut self, _: &BoundedId, _: &BoundedId) -> Result<(), BuildError> {
        self.record("destroy")
    }
    fn reconcile(&mut self, _: &BoundedId, _: Option<&BoundedId>) -> Result<(), BuildError> {
        self.record("reconcile")
    }
}
