#[macro_use]
extern crate clap;

use chrono::Utc;

use clap::{App, Arg};

use cluster::descriptor::ExperimentDescriptor;
use cluster::host::{Host, HostState};
use cluster::invocation::{Invocation, InvocationId, InvocationRecord};

use flate2::write::GzEncoder;
use flate2::Compression;

use git2::Repository;

use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet};
use nix::unistd::{fork, setpgid, ForkResult, Pid};

use reqwest::multipart;

use response::{EmptyResponse, Response};

use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{fmt, mem, process, thread, time};

mod response;

struct Client {
    path: PathBuf,
    server: String,
    host: Host,
    executor: Option<Executor>,
}

struct Executor {
    pid: Pid,
    descriptor: ExperimentDescriptor,
    invocation: InvocationRecord,
    repo: Repository,
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
    /// The subprocess for the invocation couldn't be forked.
    InvocationFailed,
    /// Couldn't compress the log directory for the current invocation.
    CompressionFailed,
    /// Couldn't upload the log archive for the current invocation.
    UploadFailed,
    /// There was a failure while attempting to clone the repository.
    CloningFailed,
    /// The cloned repository has commits missing (i.e. previously valid references are no longer
    /// present).
    MissingCommits,
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
            ClientErrorKind::InvocationFailed => {
                write!(f, "the subprocess for the invocation couldn't be forked")
            }
            ClientErrorKind::CompressionFailed => write!(
                f,
                "couldn't compress the log directory for the current invocation"
            ),
            ClientErrorKind::UploadFailed => write!(
                f,
                "couldn't upload the log archive for the current invocation"
            ),
            ClientErrorKind::CloningFailed => write!(
                f,
                "there was a failure while attempting to clone the repository"
            ),
            ClientErrorKind::MissingCommits => {
                write!(f, "the cloned repository has commits missing")
            }
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
    fn new<P: AsRef<Path>>(server: &str, port: u16, path: P) -> Result<Client, ClientError> {
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
        Ok(Client {
            path: path.as_ref().to_path_buf(),
            server,
            host,
            executor: None,
        })
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

    fn poll(&mut self) {
        match self.poll_raw() {
            Err(err) => {
                match err.kind {
                    ClientErrorKind::Disconnected => {}
                    _ => {
                        if let Some(id) = self.host.current_invocation() {
                            self.host.set_state(HostState::Errored { id });
                            self.push_state().unwrap_or(());
                        }
                    }
                }
                println!("{:?}", err);
            }
            _ => {}
        }
    }

    fn poll_raw(&mut self) -> Result<(), ClientError> {
        let response = reqwest::get(&format!("{}current", self.server))
            .and_then(|mut response| response.json::<Response<InvocationId>>())
            .map_err(|err| ClientError {
                cause: Some(Box::new(err)),
                kind: ClientErrorKind::Disconnected,
            })?;
        match response.into_result() {
            Ok(id) => match self.host.current_invocation() {
                Some(oid) if oid != id => {
                    self.executor = self.invoke(id)?;
                }
                None => {
                    self.executor = self.invoke(id)?;
                }
                _ => {
                    if let Some(ref executor) = self.executor {
                        if signal::killpg(executor.pid, None).is_err() {
                            self.kill()?;
                        }
                    }
                }
            },
            _ => {
                self.kill()?;
                self.host.set_state(HostState::Idle);
            }
        }
        self.push_state()
    }

    fn clone(&self, url: &str, commit: &str) -> Result<Repository, ClientError> {
        fs::remove_dir_all(&self.path).unwrap_or(());
        let repo = Repository::clone(url, &self.path).map_err(|err| ClientError {
            cause: Some(Box::new(err)),
            kind: ClientErrorKind::CloningFailed,
        })?;
        cluster::rewind(&repo, commit).map_err(|err| ClientError {
            cause: Some(Box::new(err)),
            kind: ClientErrorKind::MissingCommits,
        })?;
        Ok(repo)
    }

    fn invoke(&mut self, id: InvocationId) -> Result<Option<Executor>, ClientError> {
        let response = reqwest::get(&format!("{}invocation/{}", self.server, id))
            .and_then(|mut response| response.json::<Response<Invocation>>())
            .map_err(|err| ClientError {
                cause: Some(Box::new(err)),
                kind: ClientErrorKind::Disconnected,
            })?;
        match response.into_result() {
            Ok(invocation) => {
                if !invocation.host_has_logged(self.host.hostname()) {
                    self.invoke_local(invocation)
                } else {
                    self.kill()?;
                    self.host.set_state(HostState::Idle);
                    Ok(None)
                }
            }
            _ => Err(ClientErrorKind::BadResponse.into()),
        }
    }

    fn invoke_local(&mut self, invocation: Invocation) -> Result<Option<Executor>, ClientError> {
        match invocation.split() {
            Some((invocation, descriptor)) => {
                let mut repo = None;
                if let Some(old) = self.kill()? {
                    if old.invocation.url() == invocation.url() {
                        match cluster::rewind(&old.repo, invocation.commit()) {
                            Ok(_) => repo = Some(old.repo),
                            _ => {}
                        }
                    };
                }
                let repo = match repo {
                    Some(repo) => repo,
                    None => self.clone(invocation.url(), invocation.commit())?,
                };
                match fork() {
                    Ok(ForkResult::Parent { child, .. }) => {
                        self.host.set_state(HostState::Running {
                            id: invocation.id(),
                        });
                        Ok(Some(Executor {
                            pid: child,
                            descriptor,
                            invocation,
                            repo,
                        }))
                    }
                    Ok(ForkResult::Child) => {
                        setpgid(Pid::from_raw(0), Pid::from_raw(0)).unwrap();
                        unsafe {
                            signal::sigaction(
                                signal::SIGCHLD,
                                &SigAction::new(
                                    SigHandler::SigDfl,
                                    SaFlags::empty(),
                                    SigSet::empty(),
                                ),
                            )
                            .unwrap();
                        }
                        descriptor.execute_for(
                            self.host.hostname(),
                            &self.path,
                            &format!(
                                "{}@{}-{}",
                                self.host.hostname(),
                                descriptor.name(),
                                Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
                            ),
                        );
                        process::exit(1);
                    }
                    Err(err) => Err(ClientError {
                        cause: Some(Box::new(err)),
                        kind: ClientErrorKind::InvocationFailed,
                    }),
                }
            }
            None => Err(ClientErrorKind::BadResponse.into()),
        }
    }

    fn kill(&mut self) -> Result<Option<Executor>, ClientError> {
        let mut executor = None;
        mem::swap(&mut executor, &mut self.executor);
        if let Some(ref executor) = executor {
            println!("killing child process...");
            signal::killpg(executor.pid, signal::SIGTERM).unwrap_or(());
            signal::killpg(executor.pid, signal::SIGKILL).unwrap_or(());
            println!("done");
            self.upload(&executor)?;
            self.host.set_state(HostState::Done {
                id: executor.invocation.id(),
            });
            self.push_state()?;
        }
        Ok(executor)
    }

    fn upload(&mut self, executor: &Executor) -> Result<(), ClientError> {
        if let Some(path) = self.compress(executor)? {
            println!("uploading logs...");
            self.host.set_state(HostState::Uploading {
                id: executor.invocation.id(),
            });
            self.push_state()?;
            multipart::Form::new()
                .file("log", &path)
                .map_err(|err| ClientError {
                    cause: Some(Box::new(err)),
                    kind: ClientErrorKind::UploadFailed,
                })
                .and_then(|form| {
                    reqwest::Client::new()
                        .post(&format!(
                            "{}upload/{}/{}",
                            self.server,
                            executor.invocation.id(),
                            self.host.id()
                        ))
                        .multipart(form)
                        .send()
                        .and_then(|mut response| response.json::<EmptyResponse>())
                        .map_err(|err| ClientError {
                            cause: Some(Box::new(err)),
                            kind: ClientErrorKind::UploadFailed,
                        })
                        .and_then(|response| {
                            response.into_result().map_err(|err| ClientError {
                                cause: Some(Box::new(err)),
                                kind: ClientErrorKind::BadResponse,
                            })
                        })
                })?;
            fs::remove_file(path).unwrap_or(());
            println!("done");
        }
        Ok(())
    }

    fn compress(&mut self, executor: &Executor) -> Result<Option<PathBuf>, ClientError> {
        println!("compressing logs...");
        self.host.set_state(HostState::Compressing {
            id: executor.invocation.id(),
        });
        self.push_state()?;
        let path = self.path.join("archive.tar.gz");
        File::create(&path)
            .and_then(|tar_gz| {
                let enc = GzEncoder::new(tar_gz, Compression::default());
                let mut tar = tar::Builder::new(enc);
                tar.append_dir_all(".", self.path.join(executor.descriptor.log_dir()))
            })
            .map_err(|err| ClientError {
                cause: Some(Box::new(err)),
                kind: ClientErrorKind::CompressionFailed,
            })?;
        println!("done");
        Ok(Some(path))
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
        .arg(
            Arg::with_name("path")
                .long("path")
                .takes_value(true)
                .value_name("PATH")
                .help("the directory into which experiments will be cloned"),
        )
        .get_matches();
    unsafe {
        signal::sigaction(
            signal::SIGCHLD,
            &SigAction::new(SigHandler::SigIgn, SaFlags::SA_NOCLDWAIT, SigSet::empty()),
        )
        .unwrap();
    }
    let client = Client::new(
        matches.value_of("server").unwrap(),
        value_t!(matches, "port", u16).unwrap_or(8000),
        matches.value_of("path").unwrap_or("experiment/"),
    )
    .unwrap();
    let client = Arc::new(Mutex::new(client));
    {
        let client = Arc::clone(&client);
        ctrlc::set_handler(move || {
            client.lock().unwrap().kill().unwrap();
            process::exit(0);
        })
        .unwrap();
    }
    loop {
        client.lock().unwrap().poll();
        thread::sleep(time::Duration::from_millis(500));
    }
}
