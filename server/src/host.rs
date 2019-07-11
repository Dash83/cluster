use serde::Serialize;

use std::time;

use rocket::http::RawStr;
use rocket::request::FromParam;

use uuid::Uuid;

use crate::invocation::*;

const TIMEOUT: time::Duration = time::Duration::from_secs(5);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct HostId(Uuid);

#[derive(Debug, Serialize)]
pub struct Host {
    id: HostId,
    #[serde(skip)]
    timestamp: time::Instant,
    hostname: String,
    state: HostState,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "desc")]
pub enum HostState {
    /// Host is registered, but not actively part of an invocation.
    #[serde(rename = "idle")]
    Idle,
    /// Host is actively running an invocation.
    #[serde(rename = "running")]
    Running(#[serde(rename = "id")] InvocationId),
    /// Host attempted to execute an invocation, but could not do so successfully due to an error
    /// external to te invocation itself, or host successfully executed invocation, but entered a
    /// failure state while compressing or uploading logs. (Functionally equivalent to idle, but
    /// important for diagnostics.)
    #[serde(rename = "running")]
    Errored(#[serde(rename = "id")] InvocationId),
    /// Host successfully executed an invocation (either to completion or to an error internal to the
    /// invocation) and is now compressing log files for the invocation.
    #[serde(rename = "running")]
    Compressing(#[serde(rename = "id")] InvocationId),
    /// Host successfully compressed log files an invocation and is now uploading them.
    #[serde(rename = "running")]
    Uploading(#[serde(rename = "id")] InvocationId),
    /// Host successfully executed an invocation to completion. (Functionally equivalent to idle,
    /// but important for diagnostics.)
    #[serde(rename = "running")]
    Done(#[serde(rename = "id")] InvocationId),
}

impl<'a> FromParam<'a> for HostId {
    type Error = &'a RawStr;

    fn from_param(param: &'a RawStr) -> Result<Self, Self::Error> {
        if let Ok(decoded) = param.url_decode() {
            if let Ok(uuid) = Uuid::parse_str(&decoded) {
                return Ok(HostId(uuid));
            }
        }
        Err(param)
    }
}

impl Host {
    pub(crate) fn new(hostname: &str) -> Host {
        Host {
            id: HostId(Uuid::new_v4()),
            hostname: hostname.to_string(),
            timestamp: time::Instant::now(),
            state: HostState::Idle,
        }
    }

    pub fn id(&self) -> HostId {
        self.id
    }

    pub fn refresh(&mut self) {
        self.timestamp = time::Instant::now()
    }

    pub fn set_state(&mut self, state: HostState) {
        self.state = state
    }

    pub fn state(&self) -> HostState {
        self.state
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn current_invocation(&self) -> Option<InvocationId> {
        match self.state {
            HostState::Running(id)
            | HostState::Errored(id)
            | HostState::Compressing(id)
            | HostState::Uploading(id)
            | HostState::Done(id) => Some(id),
            _ => None,
        }
    }

    pub fn expired(&self) -> bool {
        time::Instant::now() > self.timestamp + TIMEOUT
    }
}
