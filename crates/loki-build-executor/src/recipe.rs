use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::protocol::{BoundedId, DigestId, ResourceLimits};
use crate::{BuildError, ErrorCode};

const MAX_ARGS: usize = 128;
const MAX_ENV: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArgumentToken {
    Literal(BoundedId),
    SourceRoot,
    ToolchainRoot,
    OutputRoot,
    ScratchRoot,
    TargetId,
    Parameter(BoundedId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolvedArgument {
    Literal(BoundedId),
    CapsuleSourceRoot,
    CapsuleToolchainRoot,
    CapsuleOutputRoot,
    CapsuleScratchRoot,
    TargetId(BoundedId),
    ParameterValue { name: BoundedId, value: BoundedId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedInvocation {
    pub executable_id: BoundedId,
    pub arguments: Vec<ResolvedArgument>,
    pub environment: Vec<EnvironmentRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecretClass {
    Public,
    SensitiveReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentRule {
    pub name: BoundedId,
    pub value: BoundedId,
    pub secret_class: SecretClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputRule {
    pub relative_id: BoundedId,
    pub format: BoundedId,
    pub max_bytes: u64,
    pub target_id: BoundedId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRecipe {
    pub recipe_id: BoundedId,
    pub recipe_version: BoundedId,
    pub connector_id: BoundedId,
    pub connector_version: BoundedId,
    pub executable_id: BoundedId,
    pub toolchain_digest: DigestId,
    pub argument_template: Vec<ArgumentToken>,
    pub parameters: BTreeMap<BoundedId, BTreeSet<BoundedId>>,
    pub environment: Vec<EnvironmentRule>,
    pub outputs: Vec<OutputRule>,
    pub limits: ResourceLimits,
    pub network_denied: bool,
}

impl Ord for BoundedId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}
impl PartialOrd for BoundedId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl BuildRecipe {
    pub fn validate(&self) -> Result<(), BuildError> {
        for id in [
            &self.recipe_id,
            &self.recipe_version,
            &self.connector_id,
            &self.connector_version,
            &self.executable_id,
        ] {
            id.validate()?;
        }
        self.toolchain_digest.validate()?;
        self.limits.validate()?;
        if !self.network_denied {
            return Err(BuildError::new(
                ErrorCode::Validation,
                "build recipes must deny network",
            ));
        }
        if self.argument_template.is_empty() || self.argument_template.len() > MAX_ARGS {
            return Err(BuildError::new(
                ErrorCode::Validation,
                "argument template count is outside bounds",
            ));
        }
        if self.environment.len() > MAX_ENV || self.outputs.is_empty() {
            return Err(BuildError::new(
                ErrorCode::Validation,
                "environment or output count is outside bounds",
            ));
        }
        let mut names = BTreeSet::new();
        for rule in &self.environment {
            rule.name.validate()?;
            rule.value.validate()?;
            if !names.insert(rule.name.clone()) {
                return Err(BuildError::new(
                    ErrorCode::Validation,
                    "duplicate environment name",
                ));
            }
            if matches!(rule.secret_class, SecretClass::SensitiveReference) {
                return Err(BuildError::new(
                    ErrorCode::Validation,
                    "build executor receives no secret references",
                ));
            }
        }
        for token in &self.argument_template {
            match token {
                ArgumentToken::Literal(value) | ArgumentToken::Parameter(value) => {
                    value.validate()?
                }
                ArgumentToken::SourceRoot
                | ArgumentToken::ToolchainRoot
                | ArgumentToken::OutputRoot
                | ArgumentToken::ScratchRoot
                | ArgumentToken::TargetId => {}
            }
            if let ArgumentToken::Parameter(name) = token
                && !self.parameters.contains_key(name)
            {
                return Err(BuildError::new(
                    ErrorCode::Validation,
                    "argument references undeclared parameter",
                ));
            }
        }
        for (name, values) in &self.parameters {
            name.validate()?;
            if values.is_empty() {
                return Err(BuildError::new(
                    ErrorCode::Validation,
                    "parameter allowlist must not be empty",
                ));
            }
            for value in values {
                value.validate()?;
            }
        }
        for output in &self.outputs {
            output.relative_id.validate()?;
            output.format.validate()?;
            output.target_id.validate()?;
        }
        if self
            .outputs
            .iter()
            .any(|o| o.max_bytes == 0 || o.max_bytes > self.limits.output_bytes)
        {
            return Err(BuildError::new(
                ErrorCode::Validation,
                "output exceeds recipe resource bound",
            ));
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<DigestId, BuildError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| BuildError::new(ErrorCode::Validation, "recipe serialization failed"))?;
        DigestId::parse(
            Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
    }

    pub fn resolve(
        &self,
        target_id: BoundedId,
        selected: &BTreeMap<BoundedId, BoundedId>,
    ) -> Result<ResolvedInvocation, BuildError> {
        self.validate()?;
        target_id.validate()?;
        if selected.len() != self.parameters.len() {
            return Err(BuildError::new(
                ErrorCode::Validation,
                "parameter selection must exactly match the recipe declaration",
            ));
        }
        for (name, value) in selected {
            let allowed = self.parameters.get(name).ok_or_else(|| {
                BuildError::new(ErrorCode::Validation, "undeclared parameter rejected")
            })?;
            if !allowed.contains(value) {
                return Err(BuildError::new(
                    ErrorCode::Validation,
                    "parameter value is not allowlisted",
                ));
            }
        }
        let arguments = self
            .argument_template
            .iter()
            .map(|token| match token {
                ArgumentToken::Literal(value) => Ok(ResolvedArgument::Literal(value.clone())),
                ArgumentToken::SourceRoot => Ok(ResolvedArgument::CapsuleSourceRoot),
                ArgumentToken::ToolchainRoot => Ok(ResolvedArgument::CapsuleToolchainRoot),
                ArgumentToken::OutputRoot => Ok(ResolvedArgument::CapsuleOutputRoot),
                ArgumentToken::ScratchRoot => Ok(ResolvedArgument::CapsuleScratchRoot),
                ArgumentToken::TargetId => Ok(ResolvedArgument::TargetId(target_id.clone())),
                ArgumentToken::Parameter(name) => selected
                    .get(name)
                    .cloned()
                    .map(|value| ResolvedArgument::ParameterValue {
                        name: name.clone(),
                        value,
                    })
                    .ok_or_else(|| {
                        BuildError::new(ErrorCode::Validation, "required parameter is missing")
                    }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ResolvedInvocation {
            executable_id: self.executable_id.clone(),
            arguments,
            environment: self.environment.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildProvenance {
    pub job_id: BoundedId,
    pub correlation_id: BoundedId,
    pub source_digest: DigestId,
    pub recipe_digest: DigestId,
    pub toolchain_digest: DigestId,
    pub executor_digest: DigestId,
    pub dependency_lock_digests: Vec<DigestId>,
    pub normalized_parameters_digest: DigestId,
    pub output_digest: DigestId,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub externally_validated: bool,
}

impl BuildProvenance {
    pub fn validate(&self) -> Result<(), BuildError> {
        if !self.externally_validated || self.completed_at_ms < self.started_at_ms {
            return Err(BuildError::new(
                ErrorCode::Validation,
                "provenance requires external validation and ordered timestamps",
            ));
        }
        self.job_id.validate()?;
        self.correlation_id.validate()?;
        for digest in [
            &self.source_digest,
            &self.recipe_digest,
            &self.toolchain_digest,
            &self.executor_digest,
            &self.normalized_parameters_digest,
            &self.output_digest,
        ] {
            digest.validate()?;
        }
        for digest in &self.dependency_lock_digests {
            digest.validate()?;
        }
        Ok(())
    }

    pub fn cache_key(&self) -> Result<DigestId, BuildError> {
        self.validate()?;
        #[derive(Serialize)]
        struct Inputs<'a> {
            source: &'a DigestId,
            recipe: &'a DigestId,
            toolchain: &'a DigestId,
            executor: &'a DigestId,
            locks: &'a [DigestId],
            parameters: &'a DigestId,
        }
        let bytes = serde_json::to_vec(&Inputs {
            source: &self.source_digest,
            recipe: &self.recipe_digest,
            toolchain: &self.toolchain_digest,
            executor: &self.executor_digest,
            locks: &self.dependency_lock_digests,
            parameters: &self.normalized_parameters_digest,
        })
        .map_err(|_| BuildError::new(ErrorCode::Validation, "cache key serialization failed"))?;
        DigestId::parse(
            Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputCandidate<'a> {
    pub relative_id: &'a BoundedId,
    pub format: &'a BoundedId,
    pub target_id: &'a BoundedId,
    pub bytes: u64,
    pub digest: &'a DigestId,
}

pub fn validate_external_output(
    recipe: &BuildRecipe,
    candidate: &OutputCandidate<'_>,
    independently_measured_digest: &DigestId,
) -> Result<(), BuildError> {
    recipe.validate()?;
    candidate.relative_id.validate()?;
    candidate.format.validate()?;
    candidate.target_id.validate()?;
    candidate.digest.validate()?;
    independently_measured_digest.validate()?;
    let rule = recipe
        .outputs
        .iter()
        .find(|rule| &rule.relative_id == candidate.relative_id)
        .ok_or_else(|| {
            BuildError::new(ErrorCode::Validation, "output identity is not allowlisted")
        })?;
    if &rule.format != candidate.format
        || &rule.target_id != candidate.target_id
        || candidate.bytes > rule.max_bytes
        || candidate.digest != independently_measured_digest
    {
        return Err(BuildError::new(
            ErrorCode::Validation,
            "external output format, target, size, or digest validation failed",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> BoundedId {
        BoundedId::parse(value).unwrap()
    }
    fn digest(ch: char) -> DigestId {
        DigestId::parse(ch.to_string().repeat(64)).unwrap()
    }
    fn limits() -> ResourceLimits {
        ResourceLimits {
            memory_bytes: 128 * 1024 * 1024,
            disk_bytes: 512 * 1024 * 1024,
            cpu_time_ms: 10_000,
            wall_time_ms: 20_000,
            process_count: 8,
            output_bytes: 16 * 1024 * 1024,
        }
    }
    fn recipe() -> BuildRecipe {
        BuildRecipe {
            recipe_id: id("recipe-1"),
            recipe_version: id("1.0.0"),
            connector_id: id("connector-1"),
            connector_version: id("1.0.0"),
            executable_id: id("toolchain-1"),
            toolchain_digest: digest('a'),
            argument_template: vec![
                ArgumentToken::SourceRoot,
                ArgumentToken::OutputRoot,
                ArgumentToken::Parameter(id("profile")),
            ],
            parameters: BTreeMap::from([(
                id("profile"),
                BTreeSet::from([id("release"), id("debug")]),
            )]),
            environment: vec![EnvironmentRule {
                name: id("LANG"),
                value: id("C"),
                secret_class: SecretClass::Public,
            }],
            outputs: vec![OutputRule {
                relative_id: id("firmware-bin"),
                format: id("esp-image"),
                max_bytes: 1024,
                target_id: id("board-1"),
            }],
            limits: limits(),
            network_denied: true,
        }
    }

    #[test]
    fn recipe_resolves_only_allowlisted_typed_values() {
        let resolved = recipe()
            .resolve(
                id("board-1"),
                &BTreeMap::from([(id("profile"), id("release"))]),
            )
            .unwrap();
        assert_eq!(resolved.executable_id, id("toolchain-1"));
        assert!(
            recipe()
                .resolve(
                    id("board-1"),
                    &BTreeMap::from([(id("profile"), id("unreviewed"))]),
                )
                .is_err()
        );
    }

    #[test]
    fn output_is_not_accepted_before_external_identity_validation() {
        let output_digest = digest('b');
        let candidate = OutputCandidate {
            relative_id: &id("firmware-bin"),
            format: &id("esp-image"),
            target_id: &id("board-1"),
            bytes: 100,
            digest: &output_digest,
        };
        assert!(validate_external_output(&recipe(), &candidate, &output_digest).is_ok());
        assert!(validate_external_output(&recipe(), &candidate, &digest('c')).is_err());
    }

    #[test]
    fn source_recipe_toolchain_or_parameters_change_the_cache_key() {
        let base = BuildProvenance {
            job_id: id("job-1"),
            correlation_id: id("corr-1"),
            source_digest: digest('1'),
            recipe_digest: recipe().fingerprint().unwrap(),
            toolchain_digest: digest('2'),
            executor_digest: digest('3'),
            dependency_lock_digests: vec![digest('4')],
            normalized_parameters_digest: digest('5'),
            output_digest: digest('6'),
            started_at_ms: 1,
            completed_at_ms: 2,
            externally_validated: true,
        };
        let key = base.cache_key().unwrap();
        for changed in [
            BuildProvenance {
                source_digest: digest('7'),
                ..base.clone()
            },
            BuildProvenance {
                recipe_digest: digest('8'),
                ..base.clone()
            },
            BuildProvenance {
                toolchain_digest: digest('9'),
                ..base.clone()
            },
            BuildProvenance {
                normalized_parameters_digest: digest('a'),
                ..base.clone()
            },
        ] {
            assert_ne!(key, changed.cache_key().unwrap());
        }
    }
}
