#[macro_use]
extern crate clap;

use clap::{App, Arg};

use cluster::host::{Host, HostState};

use response::{EmptyResponse, Response};

use std::error::Error;
use std::fmt;

mod response;

struct Client {
    server: String,
    host: Host,
}

#[derive(Debug)]
struct ClientError {
    cause: Option<Box<dyn Error>>,
    kind: ClientErrorKind,
}

#[derive(Debug)]
enum ClientErrorKind {
    /// Couldn't get the client hostname from the system.
    NoHostname,
    /// Couldn't perform initial registration with the server.
    RegistrationFailed,
    /// Registered, but currently disconnected from the server.
    Disconnected,
    /// Requests successfully reaching the server, but responses have returned errors.
    BadResponse,
}

impl fmt::Display for ClientErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientErrorKind::NoHostname => {
                write!(f, "couldn't get the client hostname from the system")
            }
            ClientErrorKind::RegistrationFailed => {
                write!(f, "couldn't perform initial registration with the server")
            }
            ClientErrorKind::Disconnected => {
                write!(f, "registered, but currently disconnected from the server")
            }
            ClientErrorKind::BadResponse => write!(
                f,
                "requests successfully reaching the server, but responses have returned errors"
            ),
        }
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl Error for ClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.cause {
            Some(ref cause) => Some(&**cause),
            _ => None,
        }
    }
}

impl From<ClientErrorKind> for ClientError {
    fn from(kind: ClientErrorKind) -> ClientError {
        ClientError { cause: None, kind }
    }
}

impl Client {
    fn new(server: &str, port: u16) -> Result<Client, ClientError> {
        let server = format!("http://{}:{}/api/", server, port);
        let hostname = gethostname::gethostname()
            .into_string()
            .map_err(|_| ClientError::from(ClientErrorKind::NoHostname))?;
        let host = reqwest::get(&format!("{}host/register/{}", server, hostname))
            .and_then(|mut response| response.json::<Response<Host>>())
            .map_err(|err| ClientError {
                cause: Some(Box::new(err)),
                kind: ClientErrorKind::RegistrationFailed,
            })
            .and_then(|response| {
                response.into_result().map_err(|err| ClientError {
                    cause: Some(Box::new(err)),
                    kind: ClientErrorKind::RegistrationFailed,
                })
            })?;
        Ok(Client { server, host })
    }

    fn push_state(&self) -> Result<(), ClientError> {
        let state = match self.host.state() {
            HostState::Idle => "idle".to_string(),
            HostState::Running { id } => format!("running/{}", id),
            HostState::Errored { id } => format!("errored/{}", id),
            HostState::Compressing { id } => format!("compressing/{}", id),
            HostState::Uploading { id } => format!("uploading/{}", id),
            HostState::Done { id } => format!("done/{}", id),
            _ => unreachable!(),
        };
        reqwest::get(&format!(
            "{}host/status/{}/{}",
            self.server,
            self.host.id(),
            state
        ))
        .and_then(|mut response| response.json::<EmptyResponse>())
        .map_err(|err| ClientError {
            cause: Some(Box::new(err)),
            kind: ClientErrorKind::Disconnected,
        })
        .and_then(|response| {
            response.into_result().map_err(|err| ClientError {
                cause: Some(Box::new(err)),
                kind: ClientErrorKind::BadResponse,
            })
        })
    }

    fn poll(&mut self) -> Result<(), ClientError> {
        self.push_state()?;
        Ok(())
    }
}

fn main() {
    let matches = App::new("clusterc")
        .version("0.2.0")
        .author("Nathan Corbyn <me@nathancorbyn.com>")
        .arg(
            Arg::with_name("server")
                .short("s")
                .long("server")
                .takes_value(true)
                .value_name("SERVER")
                .required(true)
                .help("the address for the cluster server"),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .takes_value(true)
                .value_name("PORT")
                .help("the port for the server"),
        )
        .get_matches();
    let mut client = match Client::new(
        matches.value_of("server").unwrap(),
        value_t!(matches, "port", u16).unwrap_or(8000),
    ) {
        Ok(client) => client,
        Err(err) => {
            println!("{}", err);
            println!("{}", err.source().unwrap());
            return;
        }
    };
    loop {
        match client.poll() {
            Err(err) => {
                println!("{}", err);
                println!("{}", err.source().unwrap());
                return;
            }
            _ => continue,
        }
    }
}
