use chrono::{DateTime, Utc};

use crate::descriptor::{ExperimentDescriptor, ExperimentParseError};
use crate::host::Host;

use rocket::http::RawStr;
use rocket::request::FromParam;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use uuid::Uuid;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvocationId(Uuid);

#[derive(Serialize, Deserialize)]
pub struct Invocation {
    id: InvocationId,
    url: String,
    commit: String,
    descriptor: Option<ExperimentDescriptor>,
    start: DateTime<Utc>,
    logs: HashMap<String, PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InvocationRecord {
    id: InvocationId,
    url: String,
    name: Option<String>,
    commit: String,
    start: DateTime<Utc>,
}

impl<'a> FromParam<'a> for InvocationId {
    type Error = &'a RawStr;

    fn from_param(param: &'a RawStr) -> Result<Self, Self::Error> {
        if let Ok(decoded) = param.url_decode() {
            if let Ok(uuid) = Uuid::parse_str(&decoded) {
                return Ok(InvocationId(uuid));
            }
        }
        Err(param)
    }
}

impl Invocation {
    pub fn new<P: AsRef<Path>>(
        url: &str,
        commit: &str,
        path: P,
    ) -> (Invocation, Option<ExperimentParseError>) {
        let descriptor = ExperimentDescriptor::load_from(path);
        let (descriptor, err) = match descriptor {
            Ok(descriptor) => (Some(descriptor), None),
            Err(err) => (None, Some(err)),
        };
        (
            Invocation {
                id: InvocationId(Uuid::new_v4()),
                url: url.to_string(),
                commit: commit.to_string(),
                descriptor,
                start: Utc::now(),
                logs: HashMap::new(),
            },
            err,
        )
    }

    pub fn id(&self) -> InvocationId {
        self.id
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn commit(&self) -> &str {
        &self.commit
    }

    pub fn host_has_logged(&self, hostname: &str) -> bool {
        self.logs.contains_key(hostname)
    }

    pub fn add_log<P: AsRef<Path>>(&mut self, host: &Host, path: P) {
        self.logs
            .insert(host.hostname().to_string(), path.as_ref().to_path_buf());
    }

    pub fn record(&self) -> InvocationRecord {
        InvocationRecord {
            id: self.id,
            url: self.url.to_string(),
            name: if let Some(ref descriptor) = self.descriptor {
                Some(descriptor.name().to_string())
            } else {
                None
            },
            commit: self.commit.to_string(),
            start: self.start,
        }
    }

    pub fn split(self) -> Option<(InvocationRecord, ExperimentDescriptor)> {
        let record = self.record();
        match self.descriptor {
            Some(descriptor) => Some((record, descriptor)),
            None => None,
        }
    }
}

impl InvocationRecord {
    pub fn id(&self) -> InvocationId {
        self.id
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn commit(&self) -> &str {
        &self.commit
    }
}

impl fmt::Display for InvocationId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
