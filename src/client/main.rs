#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;

use api::Connector;

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

use rand::Rng;

use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::{cmp, fmt, mem, process, thread, time};

mod api;

struct Client {
    path: PathBuf,
    connector: Arc<Connector>,
    host: Arc<RwLock<Host>>,
    executor: Option<Executor>,
    history: Option<Executor>,
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
        let connector = Arc::new(Connector::new(server, port));
        let hostname = gethostname::gethostname()
            .into_string()
            .map_err(|_| ClientError::from(ClientErrorKind::NoHostname))?;
        info!("registering with server...");
        let host = Arc::new(RwLock::new(loop {
            match connector.register(&hostname) {
                Ok(host) => {
                    info!("registered");
                    break host;
                }
                _ => {
                    info!("retrying registration...");
                    thread::sleep(time::Duration::from_millis(500));
                }
            }
        }));
        {
            let connector = Arc::clone(&connector);
            let host = Arc::clone(&host);
            thread::spawn(move || {
                let mut rng = rand::thread_rng();
                loop {
                    thread::sleep(time::Duration::from_millis(500));
                    let (id, state) = {
                        let host = host.read().unwrap();
                        (host.id(), host.state())
                    };
                    let mut retries = 0;
                    debug!("pushing client status");
                    while let Err(ref err) = connector.status(id, state) {
                        if err.is_bad_response() {
                            warn!("failed to push status, retrying registration...");
                            match connector.register(&hostname) {
                                Ok(registered) => {
                                    info!("registered");
                                    *host.write().unwrap() = registered;
                                }
                                _ => warn!("registration failed"),
                            }
                            break;
                        }
                        retries = cmp::min(retries + 1, 3);
                        let backoff = rng.gen_range(0, 1 << retries);
                        thread::sleep(backoff * time::Duration::from_millis(500))
                    }
                }
            });
        }
        Ok(Client {
            path: path.as_ref().to_path_buf(),
            host,
            connector,
            executor: None,
            history: None,
        })
    }

    fn poll(&mut self) {
        debug!("polling server status");
        if let Err(err) = self.poll_raw() {
            let invocation = { self.host.read().unwrap().current_invocation() };
            if let Some(id) = invocation {
                self.set_state(HostState::Errored { id });
            }
            error!("{}", err);
        }
    }

    fn poll_raw(&mut self) -> Result<(), ClientError> {
        for retries in 0..128 {
            match self.connector.current() {
                Ok(id) => {
                    let invocation = { self.host.read().unwrap().current_invocation() };
                    match invocation {
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
                    }
                    return Ok(());
                }
                _ => {
                    warn!("failed to get current invocation ID, retrying...");
                    let backoff = rand::thread_rng().gen_range(0, 1 << cmp::min(retries, 3));
                    thread::sleep(backoff * time::Duration::from_millis(500))
                }
            }
        }
        self.kill()?;
        self.set_state(HostState::Idle);
        Ok(())
    }

    fn clone(&self, url: &str, commit: &str) -> Result<Repository, ClientError> {
        let repo = cluster::clone(url, &self.path).map_err(|err| ClientError {
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
        match self.connector.invocation(id) {
            Ok(invocation) => {
                if !invocation.host_has_logged(self.host.read().unwrap().hostname()) {
                    self.invoke_local(invocation)
                } else {
                    self.kill()?;
                    self.set_state(HostState::Done { id });
                    Ok(None)
                }
            }
            _ => Err(ClientErrorKind::BadResponse.into()),
        }
    }

    fn invoke_local(&mut self, invocation: Invocation) -> Result<Option<Executor>, ClientError> {
        match invocation.split() {
            Some((invocation, descriptor)) => {
                self.kill()?;
                let mut repo = None;
                if let Some(ref old) = self.history {
                    if old.invocation.url() == invocation.url() {
                        debug!("attempting to use existing repository");
                        match cluster::rewind(&old.repo, invocation.commit()) {
                            Ok(_) => match Repository::open(&self.path) {
                                Ok(opened) => {
                                    repo = Some(opened);
                                }
                                _ => debug!("failed to reopen cloned repository"),
                            },
                            _ => debug!("failed to jump to commit {}", invocation.commit()),
                        }
                    }
                }
                let repo = match repo {
                    Some(repo) => repo,
                    None => self.clone(invocation.url(), invocation.commit())?,
                };
                info!("forking child process...");
                match fork() {
                    Ok(ForkResult::Parent { child, .. }) => {
                        ignore_children();
                        info!("forked child process");
                        self.set_state(HostState::Running {
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
                        let host = self.host.read().unwrap();
                        descriptor.execute_for(
                            host.hostname(),
                            &self.path,
                            &format!(
                                "{}@{}-{}",
                                host.hostname(),
                                descriptor.name(),
                                Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
                            ),
                        );
                        process::exit(0);
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

    fn kill(&mut self) -> Result<(), ClientError> {
        restore_children();
        if self.executor.is_some() {
            self.history = None;
            mem::swap(&mut self.history, &mut self.executor);
            if let Some(ref executor) = self.history {
                info!("killing child process...");
                signal::killpg(executor.pid, signal::SIGTERM).unwrap_or(());
                signal::killpg(executor.pid, signal::SIGKILL).unwrap_or(());
                info!("killed child process");
                self.upload(executor)?;
                self.set_state(HostState::Done {
                    id: executor.invocation.id(),
                });
            }
        }
        Ok(())
    }

    fn upload(&self, executor: &Executor) -> Result<(), ClientError> {
        if let Some(path) = self.compress(executor)? {
            info!("uploading logs...");
            self.set_state(HostState::Uploading {
                id: executor.invocation.id(),
            });
            self.connector
                .upload(
                    &path,
                    executor.invocation.id(),
                    self.host.read().unwrap().id(),
                )
                .map_err(|err| ClientError {
                    cause: Some(Box::new(err)),
                    kind: ClientErrorKind::UploadFailed,
                })?;
            fs::remove_file(path).unwrap_or(());
            info!("uploaded logs");
        }
        Ok(())
    }

    fn compress(&self, executor: &Executor) -> Result<Option<PathBuf>, ClientError> {
        let log_dir = self.path.join(executor.descriptor.log_dir());
        if log_dir.exists() {
            info!("compressing logs...");
            self.set_state(HostState::Compressing {
                id: executor.invocation.id(),
            });
            let path = self.path.join("archive.tar.gz");
            File::create(&path)
                .and_then(|tar_gz| {
                    let enc = GzEncoder::new(tar_gz, Compression::default());
                    let mut tar = tar::Builder::new(enc);
                    tar.append_dir_all(".", log_dir)
                })
                .map_err(|err| ClientError {
                    cause: Some(Box::new(err)),
                    kind: ClientErrorKind::CompressionFailed,
                })?;
            info!("compressed logs");
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    fn set_state(&self, state: HostState) {
        self.host.write().unwrap().set_state(state);
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
    env_logger::init();
    info!("starting client...");
    let client = Arc::new(Mutex::new(
        Client::new(
            matches.value_of("server").unwrap(),
            value_t!(matches, "port", u16).unwrap_or(8000),
            matches.value_of("path").unwrap_or("experiment/"),
        )
        .unwrap(),
    ));
    {
        let client = Arc::clone(&client);
        thread::spawn(move || loop {
            client.lock().unwrap().poll();
            thread::sleep(time::Duration::from_millis(2000));
        });
    }
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::SIGTERM, Arc::clone(&term)).unwrap();
    signal_hook::flag::register(signal_hook::SIGHUP, Arc::clone(&term)).unwrap();
    signal_hook::flag::register(signal_hook::SIGINT, Arc::clone(&term)).unwrap();
    signal_hook::flag::register(signal_hook::SIGQUIT, Arc::clone(&term)).unwrap();
    while !term.load(Ordering::Relaxed) {
        if term.swap(false, Ordering::Relaxed) {
            info!("exiting...");
            match client.lock().unwrap().kill() {
                Ok(_) => process::exit(0),
                _ => process::exit(1),
            }
        }
    }
}

fn ignore_children() {
    unsafe {
        signal::sigaction(
            signal::SIGCHLD,
            &SigAction::new(SigHandler::SigIgn, SaFlags::SA_NOCLDWAIT, SigSet::empty()),
        )
        .unwrap();
    }
}

fn restore_children() {
    unsafe {
        signal::sigaction(
            signal::SIGCHLD,
            &SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty()),
        )
        .unwrap();
    }
}
