use serde::Deserialize;

use std::error::Error;
use std::fmt;

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
            Status::Err => Err(ResponseError(self.msg)),
        }
    }
}

impl EmptyResponse {
    pub fn into_result(self) -> Result<(), ResponseError> {
        match self.status {
            Status::Ok => Ok(()),
            Status::Err => Err(ResponseError(self.msg)),
        }
    }
}

#[derive(Debug)]
pub struct ResponseError(Option<String>);

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Some(ref msg) => write!(f, "{}", msg),
            _ => write!(f, "an error occured"),
        }
    }
}

impl Error for ResponseError {}
