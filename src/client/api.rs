use cluster::host::{Host, HostId, HostState};
use cluster::invocation::{Invocation, InvocationId};

use reqwest::multipart;

use serde::{de::DeserializeOwned, Deserialize};

use std::error::Error;
use std::fmt;
use std::path::Path;

#[derive(Deserialize)]
enum Status {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "err")]
    Err,
}

#[derive(Deserialize)]
pub struct Response<T> {
    status: Status,
    payload: Option<T>,
    msg: Option<String>,
}

#[derive(Deserialize)]
pub struct EmptyResponse {
    status: Status,
    msg: Option<String>,
}

impl<T> Response<T> {
    pub fn into_result(self) -> Result<T, ResponseError> {
        match self.status {
            Status::Ok => Ok(self.payload.unwrap()),
            Status::Err => Err(ResponseErrorKind::BadResponse(self.msg).into()),
        }
    }
}

impl EmptyResponse {
    pub fn into_result(self) -> Result<(), ResponseError> {
        match self.status {
            Status::Ok => Ok(()),
            Status::Err => Err(ResponseErrorKind::BadResponse(self.msg).into()),
        }
    }
}

#[derive(Debug)]
pub struct ResponseError {
    cause: Option<Box<dyn Error>>,
    kind: ResponseErrorKind,
}

#[derive(Debug)]
enum ResponseErrorKind {
    BadResponse(Option<String>),
    RequestFailed,
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl fmt::Display for ResponseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ResponseErrorKind::BadResponse(Some(msg)) => write!(f, "{}", msg),
            ResponseErrorKind::BadResponse(None) => write!(f, "the API returned an error"),
            ResponseErrorKind::RequestFailed => write!(f, "could not reach the API"),
        }
    }
}

impl Error for ResponseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.cause {
            Some(ref cause) => Some(&**cause),
            _ => None,
        }
    }
}

impl From<ResponseErrorKind> for ResponseError {
    fn from(kind: ResponseErrorKind) -> ResponseError {
        ResponseError { cause: None, kind }
    }
}

pub struct Connector(String);

impl<'a> Connector {
    pub fn new(server: &str, port: u16) -> Connector {
        Connector(format!("http://{}:{}/api/", server, port))
    }

    pub fn status(&self, id: HostId, state: HostState) -> Result<(), ResponseError> {
        let state = match state {
            HostState::Idle => "idle".to_string(),
            HostState::Running { id } => format!("running/{}", id),
            HostState::Errored { id } => format!("errored/{}", id),
            HostState::Compressing { id } => format!("compressing/{}", id),
            HostState::Uploading { id } => format!("uploading/{}", id),
            HostState::Done { id } => format!("done/{}", id),
            _ => unreachable!(),
        };
        self.check(&format!("host/status/{}/{}", id, state))
    }

    pub fn register(&self, hostname: &str) -> Result<Host, ResponseError> {
        self.get::<Host>(&format!("host/register/{}", hostname))
    }

    pub fn current(&self) -> Result<InvocationId, ResponseError> {
        self.get::<InvocationId>("current")
    }

    pub fn invocation(&self, id: InvocationId) -> Result<Invocation, ResponseError> {
        self.get::<Invocation>(&format!("invocation/{}", id))
    }

    pub fn upload<P: AsRef<Path>>(
        &self,
        path: P,
        id: InvocationId,
        host: HostId,
    ) -> Result<(), ResponseError> {
        multipart::Form::new()
            .file("log", path)
            .map_err(|err| ResponseError {
                cause: Some(Box::new(err)),
                kind: ResponseErrorKind::RequestFailed,
            })
            .and_then(|form| {
                reqwest::Client::new()
                    .post(&format!("{}upload/{}/{}", &self.0, id, host))
                    .multipart(form)
                    .send()
                    .and_then(|mut response| response.json::<EmptyResponse>())
                    .map_err(|err| ResponseError {
                        cause: Some(Box::new(err)),
                        kind: ResponseErrorKind::RequestFailed,
                    })
                    .and_then(|response| response.into_result())
            })
    }

    fn get<T: DeserializeOwned>(&self, target: &str) -> Result<T, ResponseError> {
        reqwest::get(&format!("{}{}", self.0, target))
            .and_then(|mut response| response.json::<Response<T>>())
            .map_err(|err| ResponseError {
                cause: Some(Box::new(err)),
                kind: ResponseErrorKind::RequestFailed,
            })
            .and_then(|response| response.into_result())
    }

    fn check(&self, target: &str) -> Result<(), ResponseError> {
        reqwest::get(&format!("{}{}", self.0, target))
            .and_then(|mut response| response.json::<EmptyResponse>())
            .map_err(|err| ResponseError {
                cause: Some(Box::new(err)),
                kind: ResponseErrorKind::RequestFailed,
            })
            .and_then(|response| response.into_result())
    }
}
