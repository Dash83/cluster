use cluster::{ExperimentDescriptor, ExperimentParseError};

use rocket::http::RawStr;
use rocket::request::FromParam;

use serde::Serialize;

use std::path::{Path, PathBuf};

use uuid::Uuid;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct InvocationId(Uuid);

#[derive(Serialize)]
pub struct Invocation {
    id: InvocationId,
    url: String,
    commit: String,
    descriptor: Option<ExperimentDescriptor>,
    logs: Vec<PathBuf>,
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
    pub(crate) fn new<P: AsRef<Path>>(
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
                descriptor: descriptor,
                logs: vec![],
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
}
