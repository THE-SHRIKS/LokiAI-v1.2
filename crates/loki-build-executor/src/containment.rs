use serde::{Deserialize, Serialize};

use crate::{BuildError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainmentPolicy {
    pub appcontainer: bool,
    pub low_integrity: bool,
    pub network_capabilities: Vec<String>,
    pub win32k_disabled_before_create: bool,
    pub create_suspended: bool,
    pub attest_child_before_resume: bool,
    pub explicit_handle_list_only: bool,
    pub job_kill_on_close: bool,
    pub job_breakaway_disabled: bool,
    pub max_processes: u32,
    pub max_memory_bytes: u64,
    pub max_cpu_time_ms: u64,
    pub max_output_bytes: u64,
}

impl ContainmentPolicy {
    pub fn strict(
        max_processes: u32,
        max_memory_bytes: u64,
        max_cpu_time_ms: u64,
        max_output_bytes: u64,
    ) -> Self {
        Self {
            appcontainer: true,
            low_integrity: true,
            network_capabilities: Vec::new(),
            win32k_disabled_before_create: true,
            create_suspended: true,
            attest_child_before_resume: true,
            explicit_handle_list_only: true,
            job_kill_on_close: true,
            job_breakaway_disabled: true,
            max_processes,
            max_memory_bytes,
            max_cpu_time_ms,
            max_output_bytes,
        }
    }

    pub fn validate(&self) -> Result<(), BuildError> {
        let retained = self.appcontainer
            && self.low_integrity
            && self.network_capabilities.is_empty()
            && self.win32k_disabled_before_create
            && self.create_suspended
            && self.attest_child_before_resume
            && self.explicit_handle_list_only
            && self.job_kill_on_close
            && self.job_breakaway_disabled
            && self.max_processes > 0
            && self.max_memory_bytes > 0
            && self.max_cpu_time_ms > 0
            && self.max_output_bytes > 0;
        retained.then_some(()).ok_or_else(|| {
            BuildError::new(
                ErrorCode::Validation,
                "required containment control is absent or weakened",
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapsulePath {
    Executor,
    Source,
    Toolchain,
    Output,
    Scratch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapsuleRight {
    ReadExecute,
    ReadOnly,
    WriteCreate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapsuleAclEntry {
    pub path: CapsulePath,
    pub right: CapsuleRight,
}

pub fn capsule_acl_plan() -> Vec<CapsuleAclEntry> {
    vec![
        CapsuleAclEntry {
            path: CapsulePath::Executor,
            right: CapsuleRight::ReadExecute,
        },
        CapsuleAclEntry {
            path: CapsulePath::Source,
            right: CapsuleRight::ReadOnly,
        },
        CapsuleAclEntry {
            path: CapsulePath::Toolchain,
            right: CapsuleRight::ReadExecute,
        },
        CapsuleAclEntry {
            path: CapsulePath::Output,
            right: CapsuleRight::WriteCreate,
        },
        CapsuleAclEntry {
            path: CapsulePath::Scratch,
            right: CapsuleRight::WriteCreate,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weakened_policy_is_rejected() {
        let mut p = ContainmentPolicy::strict(8, 1, 1, 1);
        p.network_capabilities.push("internetClient".into());
        assert!(p.validate().is_err());
    }
    #[test]
    fn acl_plan_names_only_capsule_internal_roles() {
        assert_eq!(capsule_acl_plan().len(), 5);
    }
}
