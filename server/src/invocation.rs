use serde::Serialize;

use cluster::ExperimentDescriptor;

use rocket::http::RawStr;
use rocket::request::FromParam;

use std::path::PathBuf;

use uuid::Uuid;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct InvocationId(Uuid);

#[allow(unused)]
pub struct Invocation {
    id: InvocationId,
    url: String,
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
