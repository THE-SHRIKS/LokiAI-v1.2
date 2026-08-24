//! Durable local-only setup workflow (`LONB-001`).
//!
//! This state machine stores references validated against the actual local agent
//! and device registries. It never creates cloud account/workspace/enrollment
//! state and cannot perform device or agent operations.

use std::{fmt, path::Path, sync::Mutex};

use loki_agents::AgentRegistry;
use loki_local_authority::{AuthorityError, LocalAuthority, LocalCapability, LocalContext};
use loki_local_devices::LocalDeviceBindings;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

const MAX_ID: usize = 256;
const MAX_ERROR: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupState {
    Owner,
    Agent,
    Choice,
    Device,
    Project,
    Complete,
    Failed,
}
impl SetupState {
    fn wire(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Agent => "agent",
            Self::Choice => "choice",
            Self::Device => "device",
            Self::Project => "project",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
    fn parse(v: &str) -> Option<Self> {
        match v {
            "owner" => Some(Self::Owner),
            "agent" => Some(Self::Agent),
            "choice" => Some(Self::Choice),
            "device" => Some(Self::Device),
            "project" => Some(Self::Project),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupBranch {
    ExistingDevice,
    ProjectOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupRecord {
    pub setup_id: String,
    pub installation_id: String,
    pub state: SetupState,
    pub agent_connection_id: Option<String>,
    pub branch: Option<SetupBranch>,
    pub device_binding_id: Option<String>,
    pub project_ref: Option<String>,
    pub failed_from: Option<SetupState>,
    pub last_error: Option<String>,
    pub revision: i64,
    pub correlation_id: String,
    pub updated_at_ms: i64,
}

#[derive(Debug)]
pub enum SetupError {
    Authority(AuthorityError),
    Sqlite(rusqlite::Error),
    Agent(loki_agents::AgentError),
    Device(loki_local_devices::DeviceBindingError),
    InvalidInput { field: &'static str, reason: String },
    NotFound,
    StaleState,
    InvalidTransition,
    Conflict(String),
}
impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Authority(_) => "local authority denied setup action",
                Self::Sqlite(_) => "setup database error",
                Self::Agent(_) => "agent reference is not active",
                Self::Device(_) => "device binding reference is not active",
                Self::InvalidInput { .. } => "invalid setup input",
                Self::NotFound => "setup not found",
                Self::StaleState => "setup state/revision changed",
                Self::InvalidTransition => "invalid setup transition",
                Self::Conflict(_) => "setup conflict",
            }
        )
    }
}
impl std::error::Error for SetupError {}
impl From<AuthorityError> for SetupError {
    fn from(v: AuthorityError) -> Self {
        Self::Authority(v)
    }
}
impl From<rusqlite::Error> for SetupError {
    fn from(v: rusqlite::Error) -> Self {
        Self::Sqlite(v)
    }
}
impl From<loki_agents::AgentError> for SetupError {
    fn from(v: loki_agents::AgentError) -> Self {
        Self::Agent(v)
    }
}
impl From<loki_local_devices::DeviceBindingError> for SetupError {
    fn from(v: loki_local_devices::DeviceBindingError) -> Self {
        Self::Device(v)
    }
}

pub struct LocalSetupStore {
    connection: Mutex<Connection>,
}
#[allow(clippy::too_many_arguments)]
impl LocalSetupStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SetupError> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn open_in_memory() -> Result<Self, SetupError> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(c: Connection) -> Result<Self, SetupError> {
        c.busy_timeout(std::time::Duration::from_secs(5))?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS local_setup(setup_id TEXT PRIMARY KEY,installation_id TEXT NOT NULL UNIQUE,state TEXT NOT NULL,agent_connection_id TEXT,branch TEXT,device_binding_id TEXT,project_ref TEXT,failed_from TEXT,last_error TEXT,revision INTEGER NOT NULL,correlation_id TEXT NOT NULL,updated_at_ms INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS local_setup_events(setup_id TEXT NOT NULL,revision INTEGER NOT NULL,from_state TEXT,to_state TEXT NOT NULL,correlation_id TEXT NOT NULL,at_ms INTEGER NOT NULL,PRIMARY KEY(setup_id,revision));")?;
        Ok(Self {
            connection: Mutex::new(c),
        })
    }

    pub fn start(
        &self,
        authority: &LocalAuthority,
        ctx: &LocalContext,
        session: &str,
        installation_id: &str,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        self.auth(authority, ctx, session, now)?;
        validate("installation_id", installation_id, MAX_ID)?;
        let mut c = self.lock()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = load_by_installation(&tx, installation_id)? {
            return Ok(r);
        }
        let r = SetupRecord {
            setup_id: format!("setup_{}", Uuid::new_v4().simple()),
            installation_id: installation_id.into(),
            state: SetupState::Owner,
            agent_connection_id: None,
            branch: None,
            device_binding_id: None,
            project_ref: None,
            failed_from: None,
            last_error: None,
            revision: 1,
            correlation_id: format!("corr_{}", Uuid::new_v4().simple()),
            updated_at_ms: now,
        };
        insert_record(&tx, &r)?;
        insert_event(&tx, &r, None)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn confirm_owner(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        id: &str,
        rev: i64,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        self.transition(
            a,
            c,
            s,
            id,
            rev,
            SetupState::Owner,
            SetupState::Agent,
            now,
            |_| Ok(()),
        )
    }
    pub fn set_agent(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        agents: &AgentRegistry,
        id: &str,
        rev: i64,
        agent_id: &str,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        agents.require_active_connection(agent_id)?;
        validate("agent_connection_id", agent_id, MAX_ID)?;
        self.transition(
            a,
            c,
            s,
            id,
            rev,
            SetupState::Agent,
            SetupState::Choice,
            now,
            |r| {
                r.agent_connection_id = Some(agent_id.into());
                Ok(())
            },
        )
    }
    pub fn choose(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        id: &str,
        rev: i64,
        branch: SetupBranch,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        let to = match branch {
            SetupBranch::ExistingDevice => SetupState::Device,
            SetupBranch::ProjectOnly => SetupState::Project,
        };
        self.transition(a, c, s, id, rev, SetupState::Choice, to, now, |r| {
            r.branch = Some(branch);
            Ok(())
        })
    }
    pub fn set_device(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        devices: &LocalDeviceBindings,
        id: &str,
        rev: i64,
        binding_id: &str,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        devices.require_active_binding(binding_id)?;
        validate("device_binding_id", binding_id, MAX_ID)?;
        self.transition(
            a,
            c,
            s,
            id,
            rev,
            SetupState::Device,
            SetupState::Complete,
            now,
            |r| {
                r.device_binding_id = Some(binding_id.into());
                Ok(())
            },
        )
    }
    pub fn set_project(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        id: &str,
        rev: i64,
        project_ref: &str,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        validate("project_ref", project_ref, MAX_ID)?;
        self.transition(
            a,
            c,
            s,
            id,
            rev,
            SetupState::Project,
            SetupState::Complete,
            now,
            |r| {
                r.project_ref = Some(project_ref.into());
                Ok(())
            },
        )
    }
    pub fn fail(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        id: &str,
        rev: i64,
        error: &str,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        validate("error", error, MAX_ERROR)?;
        let current = self.get(id)?;
        if current.state == SetupState::Complete || current.state == SetupState::Failed {
            return Err(SetupError::InvalidTransition);
        }
        self.transition(
            a,
            c,
            s,
            id,
            rev,
            current.state,
            SetupState::Failed,
            now,
            |r| {
                r.failed_from = Some(current.state);
                r.last_error = Some(error.into());
                Ok(())
            },
        )
    }
    pub fn retry(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        id: &str,
        rev: i64,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        let current = self.get(id)?;
        let target = current.failed_from.ok_or(SetupError::InvalidTransition)?;
        self.transition(a, c, s, id, rev, SetupState::Failed, target, now, |r| {
            r.failed_from = None;
            r.last_error = None;
            Ok(())
        })
    }
    pub fn back(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        id: &str,
        rev: i64,
        now: i64,
    ) -> Result<SetupRecord, SetupError> {
        let current = self.get(id)?;
        let to = match current.state {
            SetupState::Agent => SetupState::Owner,
            SetupState::Choice => SetupState::Agent,
            SetupState::Device | SetupState::Project => SetupState::Choice,
            SetupState::Complete => match current.branch {
                Some(SetupBranch::ExistingDevice) => SetupState::Device,
                Some(SetupBranch::ProjectOnly) => SetupState::Project,
                None => return Err(SetupError::InvalidTransition),
            },
            _ => return Err(SetupError::InvalidTransition),
        };
        self.transition(a, c, s, id, rev, current.state, to, now, |r| {
            match to {
                SetupState::Owner => {
                    r.agent_connection_id = None;
                    r.branch = None;
                    r.device_binding_id = None;
                    r.project_ref = None
                }
                SetupState::Agent => {
                    r.branch = None;
                    r.device_binding_id = None;
                    r.project_ref = None
                }
                SetupState::Choice => {
                    r.branch = None;
                    r.device_binding_id = None;
                    r.project_ref = None
                }
                SetupState::Device => r.device_binding_id = None,
                SetupState::Project => r.project_ref = None,
                _ => {}
            }
            Ok(())
        })
    }
    pub fn get(&self, id: &str) -> Result<SetupRecord, SetupError> {
        let connection = self.lock()?;
        load(&connection, id)?.ok_or(SetupError::NotFound)
    }
    fn transition<F>(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        id: &str,
        rev: i64,
        from: SetupState,
        to: SetupState,
        now: i64,
        mutate: F,
    ) -> Result<SetupRecord, SetupError>
    where
        F: FnOnce(&mut SetupRecord) -> Result<(), SetupError>,
    {
        self.auth(a, c, s, now)?;
        let mut conn = self.lock()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut r = load(&tx, id)?.ok_or(SetupError::NotFound)?;
        if r.revision != rev || r.state != from {
            return Err(SetupError::StaleState);
        }
        mutate(&mut r)?;
        r.state = to;
        r.revision += 1;
        r.correlation_id = format!("corr_{}", Uuid::new_v4().simple());
        r.updated_at_ms = now;
        update_record(&tx, &r)?;
        insert_event(&tx, &r, Some(from))?;
        tx.commit()?;
        Ok(r)
    }
    fn auth(
        &self,
        a: &LocalAuthority,
        c: &LocalContext,
        s: &str,
        now: i64,
    ) -> Result<(), SetupError> {
        a.authorize_local(c, s, LocalCapability::ReadLocalState, now)?;
        Ok(())
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, SetupError> {
        self.connection
            .lock()
            .map_err(|_| SetupError::Conflict("setup database lock poisoned".into()))
    }
}

fn branch_wire(v: Option<SetupBranch>) -> Option<&'static str> {
    v.map(|b| match b {
        SetupBranch::ExistingDevice => "existing_device",
        SetupBranch::ProjectOnly => "project_only",
    })
}
fn branch_parse(v: Option<String>) -> Result<Option<SetupBranch>, SetupError> {
    match v.as_deref() {
        None => Ok(None),
        Some("existing_device") => Ok(Some(SetupBranch::ExistingDevice)),
        Some("project_only") => Ok(Some(SetupBranch::ProjectOnly)),
        _ => Err(SetupError::Conflict("stored branch invalid".into())),
    }
}
fn insert_record(c: &Connection, r: &SetupRecord) -> Result<(), SetupError> {
    c.execute(
        "INSERT INTO local_setup VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            r.setup_id,
            r.installation_id,
            r.state.wire(),
            r.agent_connection_id,
            branch_wire(r.branch),
            r.device_binding_id,
            r.project_ref,
            r.failed_from.map(SetupState::wire),
            r.last_error,
            r.revision,
            r.correlation_id,
            r.updated_at_ms
        ],
    )?;
    Ok(())
}
fn update_record(c: &Connection, r: &SetupRecord) -> Result<(), SetupError> {
    c.execute("UPDATE local_setup SET state=?1,agent_connection_id=?2,branch=?3,device_binding_id=?4,project_ref=?5,failed_from=?6,last_error=?7,revision=?8,correlation_id=?9,updated_at_ms=?10 WHERE setup_id=?11",params![r.state.wire(),r.agent_connection_id,branch_wire(r.branch),r.device_binding_id,r.project_ref,r.failed_from.map(SetupState::wire),r.last_error,r.revision,r.correlation_id,r.updated_at_ms,r.setup_id])?;
    Ok(())
}
fn insert_event(
    c: &Connection,
    r: &SetupRecord,
    from: Option<SetupState>,
) -> Result<(), SetupError> {
    c.execute(
        "INSERT INTO local_setup_events VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            r.setup_id,
            r.revision,
            from.map(SetupState::wire),
            r.state.wire(),
            r.correlation_id,
            r.updated_at_ms
        ],
    )?;
    Ok(())
}
fn load(c: &Connection, id: &str) -> Result<Option<SetupRecord>, SetupError> {
    load_q(
        c,
        "SELECT setup_id,installation_id,state,agent_connection_id,branch,device_binding_id,project_ref,failed_from,last_error,revision,correlation_id,updated_at_ms FROM local_setup WHERE setup_id=?1",
        id,
    )
}
fn load_by_installation(c: &Connection, id: &str) -> Result<Option<SetupRecord>, SetupError> {
    load_q(
        c,
        "SELECT setup_id,installation_id,state,agent_connection_id,branch,device_binding_id,project_ref,failed_from,last_error,revision,correlation_id,updated_at_ms FROM local_setup WHERE installation_id=?1",
        id,
    )
}
fn load_q(c: &Connection, q: &str, v: &str) -> Result<Option<SetupRecord>, SetupError> {
    type RawSetup = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        String,
        i64,
    );
    let raw: Option<RawSetup> = c
        .query_row(q, [v], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
            ))
        })
        .optional()?;
    raw.map(|x| {
        Ok(SetupRecord {
            setup_id: x.0,
            installation_id: x.1,
            state: SetupState::parse(&x.2)
                .ok_or_else(|| SetupError::Conflict("stored state invalid".into()))?,
            agent_connection_id: x.3,
            branch: branch_parse(x.4)?,
            device_binding_id: x.5,
            project_ref: x.6,
            failed_from: x
                .7
                .map(|s| {
                    SetupState::parse(&s)
                        .ok_or_else(|| SetupError::Conflict("stored failed_from invalid".into()))
                })
                .transpose()?,
            last_error: x.8,
            revision: x.9,
            correlation_id: x.10,
            updated_at_ms: x.11,
        })
    })
    .transpose()
}
fn validate(field: &'static str, v: &str, max: usize) -> Result<(), SetupError> {
    if v.is_empty() || v.len() > max || v.chars().any(char::is_control) {
        Err(SetupError::InvalidInput {
            field,
            reason: format!("must contain 1..={max} non-control bytes"),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loki_agents::AgentScope;
    use loki_local_devices::{BindOutcome, BindRequest};
    struct Fixture {
        a: LocalAuthority,
        c: LocalContext,
        s: String,
        agents: AgentRegistry,
        agent: String,
        devices: LocalDeviceBindings,
        device: String,
        store: LocalSetupStore,
    }
    fn fixture() -> Fixture {
        let a = LocalAuthority::open_in_memory().unwrap();
        let c = LocalContext::current("ipc-setup").unwrap();
        let installation = a.initialize_current(&c, 1000).unwrap();
        let s = a.issue_session(&c, 2000, 100000).unwrap();
        let agents = AgentRegistry::open_in_memory().unwrap();
        let agent = agents
            .register(
                &a,
                &c,
                &s.session_id,
                "agent",
                &[AgentScope::ReadDiagnostics],
                3000,
            )
            .unwrap()
            .connection_id;
        let devices = LocalDeviceBindings::open_in_memory().unwrap();
        let req = BindRequest {
            connector_id: "con".into(),
            hardware_identity: "hw".into(),
            runtime_id: "run".into(),
            evidence_ref: "ev".into(),
            request_nonce: "n".into(),
        };
        let device = match devices.bind(&a, &c, &s.session_id, &req, 3100).unwrap() {
            BindOutcome::Created(b) => b.binding_id,
            _ => unreachable!(),
        };
        let store = LocalSetupStore::open_in_memory().unwrap();
        let _ = installation;
        Fixture {
            a,
            c,
            s: s.session_id,
            agents,
            agent,
            devices,
            device,
            store,
        }
    }
    fn to_choice(f: &Fixture) -> SetupRecord {
        let r = f.store.start(&f.a, &f.c, &f.s, "install", 4000).unwrap();
        let r = f
            .store
            .confirm_owner(&f.a, &f.c, &f.s, &r.setup_id, r.revision, 4100)
            .unwrap();
        f.store
            .set_agent(
                &f.a,
                &f.c,
                &f.s,
                &f.agents,
                &r.setup_id,
                r.revision,
                &f.agent,
                4200,
            )
            .unwrap()
    }
    #[test]
    fn device_branch_requires_real_active_binding() {
        let f = fixture();
        let r = to_choice(&f);
        let r = f
            .store
            .choose(
                &f.a,
                &f.c,
                &f.s,
                &r.setup_id,
                r.revision,
                SetupBranch::ExistingDevice,
                4300,
            )
            .unwrap();
        assert!(
            f.store
                .set_device(
                    &f.a,
                    &f.c,
                    &f.s,
                    &f.devices,
                    &r.setup_id,
                    r.revision,
                    "missing",
                    4400
                )
                .is_err()
        );
        let done = f
            .store
            .set_device(
                &f.a,
                &f.c,
                &f.s,
                &f.devices,
                &r.setup_id,
                r.revision,
                &f.device,
                4400,
            )
            .unwrap();
        assert_eq!(done.state, SetupState::Complete);
    }
    #[test]
    fn project_branch_completes_without_fake_device() {
        let f = fixture();
        let r = to_choice(&f);
        let r = f
            .store
            .choose(
                &f.a,
                &f.c,
                &f.s,
                &r.setup_id,
                r.revision,
                SetupBranch::ProjectOnly,
                4300,
            )
            .unwrap();
        let done = f
            .store
            .set_project(
                &f.a,
                &f.c,
                &f.s,
                &r.setup_id,
                r.revision,
                "project-real-ref",
                4400,
            )
            .unwrap();
        assert_eq!(done.state, SetupState::Complete);
        assert!(done.device_binding_id.is_none());
    }
    #[test]
    fn stale_revision_and_illegal_completion_fail() {
        let f = fixture();
        let r = f.store.start(&f.a, &f.c, &f.s, "install", 4000).unwrap();
        let next = f
            .store
            .confirm_owner(&f.a, &f.c, &f.s, &r.setup_id, r.revision, 4100)
            .unwrap();
        assert!(matches!(
            f.store
                .confirm_owner(&f.a, &f.c, &f.s, &r.setup_id, r.revision, 4200),
            Err(SetupError::StaleState)
        ));
        assert_eq!(next.state, SetupState::Agent);
    }
    #[test]
    fn failure_retry_and_back_clear_descendants() {
        let f = fixture();
        let r = to_choice(&f);
        let failed = f
            .store
            .fail(&f.a, &f.c, &f.s, &r.setup_id, r.revision, "temporary", 4300)
            .unwrap();
        let retried = f
            .store
            .retry(&f.a, &f.c, &f.s, &r.setup_id, failed.revision, 4400)
            .unwrap();
        assert_eq!(retried.state, SetupState::Choice);
        let back = f
            .store
            .back(&f.a, &f.c, &f.s, &r.setup_id, retried.revision, 4500)
            .unwrap();
        assert_eq!(back.state, SetupState::Agent);
        assert!(back.branch.is_none());
    }
    #[test]
    fn restart_resumes_exact_state() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("setup.db");
        let f = fixture();
        let id;
        {
            let s = LocalSetupStore::open(&p).unwrap();
            let r = s.start(&f.a, &f.c, &f.s, "install", 4000).unwrap();
            id = r.setup_id;
            s.confirm_owner(&f.a, &f.c, &f.s, &id, r.revision, 4100)
                .unwrap();
        }
        let s = LocalSetupStore::open(&p).unwrap();
        assert_eq!(s.get(&id).unwrap().state, SetupState::Agent);
    }
}
