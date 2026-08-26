use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::lifecycle::CapsuleState;
use crate::protocol::{BoundedId, DigestId};
use crate::{BuildError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    pub sequence: u64,
    pub job_id: BoundedId,
    pub capsule_id: BoundedId,
    pub sealed_base_digest: DigestId,
    pub appcontainer_sid_digest: DigestId,
    pub volume_id: Option<BoundedId>,
    pub state: CapsuleState,
    pub intent: bool,
    pub timestamp_ms: u64,
    pub previous_hash: String,
    pub entry_hash: String,
    pub detail: String,
}

impl JournalEntry {
    pub fn new(draft: JournalEntryDraft) -> Result<Self, BuildError> {
        let JournalEntryDraft {
            sequence,
            job_id,
            capsule_id,
            sealed_base_digest,
            appcontainer_sid_digest,
            volume_id,
            state,
            intent,
            timestamp_ms,
            previous_hash,
            detail,
        } = draft;
        if previous_hash.len() != 64
            || !previous_hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(BuildError::new(
                ErrorCode::Journal,
                "previous journal hash is invalid",
            ));
        }
        let mut entry = Self {
            sequence,
            job_id,
            capsule_id,
            sealed_base_digest,
            appcontainer_sid_digest,
            volume_id,
            state,
            intent,
            timestamp_ms,
            previous_hash,
            entry_hash: String::new(),
            detail,
        };
        if entry.detail.len() > 512 {
            return Err(BuildError::new(
                ErrorCode::Journal,
                "journal detail exceeds 512 bytes",
            ));
        }
        entry.entry_hash = entry.calculate_hash()?;
        Ok(entry)
    }

    pub fn verify(&self) -> Result<(), BuildError> {
        (self.entry_hash == self.calculate_hash()?)
            .then_some(())
            .ok_or_else(|| BuildError::new(ErrorCode::Journal, "journal entry hash mismatch"))
    }

    fn calculate_hash(&self) -> Result<String, BuildError> {
        #[derive(Serialize)]
        struct Hashable<'a> {
            sequence: u64,
            job_id: &'a BoundedId,
            capsule_id: &'a BoundedId,
            sealed_base_digest: &'a DigestId,
            appcontainer_sid_digest: &'a DigestId,
            volume_id: &'a Option<BoundedId>,
            state: CapsuleState,
            intent: bool,
            timestamp_ms: u64,
            previous_hash: &'a str,
            detail: &'a str,
        }
        let bytes = serde_json::to_vec(&Hashable {
            sequence: self.sequence,
            job_id: &self.job_id,
            capsule_id: &self.capsule_id,
            sealed_base_digest: &self.sealed_base_digest,
            appcontainer_sid_digest: &self.appcontainer_sid_digest,
            volume_id: &self.volume_id,
            state: self.state,
            intent: self.intent,
            timestamp_ms: self.timestamp_ms,
            previous_hash: &self.previous_hash,
            detail: &self.detail,
        })
        .map_err(|_| BuildError::new(ErrorCode::Journal, "journal entry serialization failed"))?;
        Ok(hex(&Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone)]
pub struct JournalEntryDraft {
    pub sequence: u64,
    pub job_id: BoundedId,
    pub capsule_id: BoundedId,
    pub sealed_base_digest: DigestId,
    pub appcontainer_sid_digest: DigestId,
    pub volume_id: Option<BoundedId>,
    pub state: CapsuleState,
    pub intent: bool,
    pub timestamp_ms: u64,
    pub previous_hash: String,
    pub detail: String,
}

pub trait Journal: Send {
    fn append_durable(&mut self, entry: JournalEntry) -> Result<(), BuildError>;
    fn entries(&self) -> Result<Vec<JournalEntry>, BuildError>;
}

#[derive(Debug, Default)]
pub struct MemoryJournal {
    entries: Vec<JournalEntry>,
}

impl Journal for MemoryJournal {
    fn append_durable(&mut self, entry: JournalEntry) -> Result<(), BuildError> {
        validate_next(&self.entries, &entry)?;
        self.entries.push(entry);
        Ok(())
    }
    fn entries(&self) -> Result<Vec<JournalEntry>, BuildError> {
        Ok(self.entries.clone())
    }
}

#[derive(Debug)]
pub struct FileJournal {
    path: PathBuf,
}

impl FileJournal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, BuildError> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(BuildError::new(
                ErrorCode::Journal,
                "journal path must be absolute and broker-owned",
            ));
        }
        let journal = Self {
            path: path.to_owned(),
        };
        if journal.lock_path().exists() {
            return Err(BuildError::new(
                ErrorCode::Journal,
                "journal ownership lock exists; reconciliation is required",
            ));
        }
        let _ = journal.entries()?;
        Ok(journal)
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }

    fn acquire_lock(&self) -> Result<JournalLock, BuildError> {
        let path = self.lock_path();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                BuildError::new(
                    ErrorCode::Journal,
                    format!("journal single-writer lock unavailable: {error}"),
                )
            })?;
        let guard = JournalLock { path };
        file.write_all(b"loki-build-capsule-journal-v1\n")
            .map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        Ok(guard)
    }
}

struct JournalLock {
    path: PathBuf,
}

impl Drop for JournalLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Journal for FileJournal {
    fn append_durable(&mut self, entry: JournalEntry) -> Result<(), BuildError> {
        let _lock = self.acquire_lock()?;
        let current = self.entries()?;
        validate_next(&current, &entry)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(io_error)?;
        serde_json::to_writer(&mut file, &entry)
            .map_err(|_| BuildError::new(ErrorCode::Journal, "journal serialization failed"))?;
        file.write_all(b"\n").map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        Ok(())
    }

    fn entries(&self) -> Result<Vec<JournalEntry>, BuildError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&self.path).map_err(io_error)?;
        let mut entries = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line.map_err(io_error)?;
            let entry: JournalEntry = serde_json::from_str(&line).map_err(|_| {
                BuildError::new(ErrorCode::Journal, "journal contains malformed JSON")
            })?;
            validate_next(&entries, &entry)?;
            entries.push(entry);
        }
        Ok(entries)
    }
}

fn validate_next(entries: &[JournalEntry], entry: &JournalEntry) -> Result<(), BuildError> {
    entry.job_id.validate()?;
    entry.capsule_id.validate()?;
    entry.sealed_base_digest.validate()?;
    entry.appcontainer_sid_digest.validate()?;
    if let Some(volume_id) = &entry.volume_id {
        volume_id.validate()?;
    }
    entry.verify()?;
    let expected_sequence = entries.len() as u64;
    if entry.sequence != expected_sequence {
        return Err(BuildError::new(
            ErrorCode::Journal,
            "journal sequence is not contiguous",
        ));
    }
    let expected_hash = entries
        .last()
        .map_or_else(|| "0".repeat(64), |last| last.entry_hash.clone());
    if entry.previous_hash != expected_hash {
        return Err(BuildError::new(
            ErrorCode::Journal,
            "journal hash chain is broken",
        ));
    }
    if let Some(first) = entries.first()
        && (entry.job_id != first.job_id
            || entry.capsule_id != first.capsule_id
            || entry.sealed_base_digest != first.sealed_base_digest
            || entry.appcontainer_sid_digest != first.appcontainer_sid_digest)
    {
        return Err(BuildError::new(
            ErrorCode::IdentityMismatch,
            "journal identity changed across entries",
        ));
    }
    let semantic_transition = if let Some(previous) = entries.last() {
        previous.state.can_transition(entry.state)
            || (previous.state == entry.state && previous.intent && !entry.intent)
    } else {
        CapsuleState::Requested.can_transition(entry.state)
            || entry.state == CapsuleState::Requested
    };
    if !semantic_transition {
        return Err(BuildError::new(
            ErrorCode::Journal,
            "journal state sequence is not a valid lifecycle transition",
        ));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn io_error(error: std::io::Error) -> BuildError {
    BuildError::new(ErrorCode::Journal, format!("journal I/O failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_journal_round_trips_and_detects_tamper() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        let mut journal = FileJournal::open(&path).unwrap();
        let entry = JournalEntry::new(JournalEntryDraft {
            sequence: 0,
            job_id: BoundedId::parse("job-1").unwrap(),
            capsule_id: BoundedId::parse("capsule-1").unwrap(),
            sealed_base_digest: DigestId::parse("a".repeat(64)).unwrap(),
            appcontainer_sid_digest: DigestId::parse("b".repeat(64)).unwrap(),
            volume_id: None,
            state: CapsuleState::Requested,
            intent: false,
            timestamp_ms: 1,
            previous_hash: "0".repeat(64),
            detail: "accepted".to_string(),
        })
        .unwrap();
        journal.append_durable(entry).unwrap();
        assert_eq!(journal.entries().unwrap().len(), 1);
        std::fs::write(&path, b"{}\n").unwrap();
        assert!(journal.entries().is_err());
    }

    #[test]
    fn stale_single_writer_lock_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        std::fs::write(path.with_extension("lock"), b"stale").unwrap();
        assert!(matches!(
            FileJournal::open(path),
            Err(BuildError {
                code: ErrorCode::Journal,
                ..
            })
        ));
    }

    #[test]
    fn invalid_semantic_state_sequence_is_rejected() {
        let identity = || JournalEntryDraft {
            sequence: 0,
            job_id: BoundedId::parse("job-1").unwrap(),
            capsule_id: BoundedId::parse("capsule-1").unwrap(),
            sealed_base_digest: DigestId::parse("a".repeat(64)).unwrap(),
            appcontainer_sid_digest: DigestId::parse("b".repeat(64)).unwrap(),
            volume_id: None,
            state: CapsuleState::Mounted,
            intent: true,
            timestamp_ms: 1,
            previous_hash: "0".repeat(64),
            detail: "invalid skip".to_string(),
        };
        let mut journal = MemoryJournal::default();
        assert!(
            journal
                .append_durable(JournalEntry::new(identity()).unwrap())
                .is_err()
        );
    }
}
