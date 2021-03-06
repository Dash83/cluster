use cluster::host::*;
use cluster::invocation::*;

use git2::Repository;

use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{fmt, thread, time};

pub struct Instance {
    hosts: Arc<Mutex<HashMap<HostId, Host>>>,
    invocation: Mutex<Option<InvocationId>>,
    invocations: Mutex<HashMap<InvocationId, Invocation>>,
    path: PathBuf,
}

#[derive(Debug)]
pub struct InstanceError {
    cause: Option<Box<dyn Error>>,
    kind: InstanceErrorKind,
}

#[derive(Debug)]
pub enum InstanceErrorKind {
    /// The given hostname was already registered.
    HostRegistered,
    /// The repository supplied had a manifest that could not be parsed.
    BrokenManifest,
    /// There is no repository cloned.
    NothingCloned,
    /// There was a failure while attempting to clone the repository.
    CloningFailed,
    /// The cloned repository has commits missing (i.e. previously valid references are no longer
    /// present).
    MissingCommits,
    /// The supplied invocation or host ID was invalid.
    InvalidId,
}

impl fmt::Display for InstanceErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InstanceErrorKind::HostRegistered => {
                write!(f, "the given hostname was already registered")
            }
            InstanceErrorKind::BrokenManifest => write!(
                f,
                "the repository supplied had a manifest that could not be parsed"
            ),
            InstanceErrorKind::NothingCloned => write!(f, "there is no repository cloned"),
            InstanceErrorKind::CloningFailed => write!(
                f,
                "there was a failure while attempting to clone the repository"
            ),
            InstanceErrorKind::MissingCommits => {
                write!(f, "the cloned repository has commits missing")
            }
            InstanceErrorKind::InvalidId => {
                write!(f, "the supplied invocation or host ID was invalid")
            }
        }
    }
}

impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl Error for InstanceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.cause {
            Some(ref cause) => Some(&**cause),
            _ => None,
        }
    }
}

impl From<InstanceErrorKind> for InstanceError {
    fn from(kind: InstanceErrorKind) -> InstanceError {
        InstanceError { cause: None, kind }
    }
}

impl Instance {
    pub fn new<P: AsRef<Path>>(path: P) -> Instance {
        let hosts = Arc::new(Mutex::new(HashMap::new()));
        let instance = Instance {
            hosts: Arc::clone(&hosts),
            invocation: Mutex::new(None),
            invocations: Mutex::new(HashMap::new()),
            path: path.as_ref().to_path_buf(),
        };
        thread::spawn(move || loop {
            thread::sleep(time::Duration::from_millis(200));
            let mut hosts = hosts.lock().unwrap();
            for (_, host) in hosts.iter_mut() {
                if host.expired() {
                    host.set_state(HostState::Disconnected)
                }
            }
        });
        instance
    }

    #[inline]
    pub fn host<F, T>(&self, id: HostId, f: F) -> Option<T>
    where
        F: FnOnce(&mut Host) -> T,
    {
        self.hosts.lock().unwrap().get_mut(&id).map(f)
    }

    #[inline]
    pub fn hosts<F, T>(&self, f: F) -> T
    where
        F: Fn(&mut dyn Iterator<Item = &'_ mut Host>) -> T,
    {
        let mut hosts = self.hosts.lock().unwrap();
        let iter = hosts.iter_mut();
        f(&mut iter.map(|(_, host)| host))
    }

    #[inline]
    pub fn invocation<F, T>(&self, id: InvocationId, f: F) -> Option<T>
    where
        F: FnOnce(&mut Invocation) -> T,
    {
        match self.invocations.lock().unwrap().get_mut(&id) {
            Some(invocation) => Some(f(invocation)),
            _ => None,
        }
    }

    #[inline]
    pub fn invocations<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&mut dyn Iterator<Item = &'_ mut Invocation>) -> T,
    {
        let mut invocations = self.invocations.lock().unwrap();
        let iter = invocations.iter_mut();
        f(&mut iter.map(|(_, invocation)| invocation))
    }

    pub fn current_invocation(&self) -> Option<InvocationId> {
        match *self.invocation.lock().unwrap() {
            Some(ref invocation) => Some(*invocation),
            _ => None,
        }
    }

    pub fn register(&self, hostname: &str) -> Result<HostId, InstanceError> {
        let mut hosts = self.hosts.lock().unwrap();
        for (id, host) in hosts.iter_mut() {
            if hostname == host.hostname() {
                host.refresh();
                host.set_state(HostState::Idle);
                return Ok(*id);
            }
        }
        let host = Host::new(hostname);
        let id = host.id();
        hosts.insert(id, host);
        Ok(id)
    }

    pub fn invoke(&self, url: &str) -> Result<InvocationId, InstanceError> {
        let repo = self.clone(url)?;
        let commit = repo
            .head()
            .and_then(|head| head.resolve())
            .and_then(|resolved| resolved.peel_to_commit())
            .and_then(|commit| Ok(format!("{}", commit.id())))
            .map_err(|err| InstanceError {
                cause: Some(Box::new(err)),
                kind: InstanceErrorKind::MissingCommits,
            })?;
        self.build_invocation(url, &commit)
    }

    pub fn reinvoke(&self, id: InvocationId) -> Result<InvocationId, InstanceError> {
        let (url, commit) = match self.invocations.lock().unwrap().get(&id) {
            Some(old) => (old.url().to_string(), old.commit().to_string()),
            _ => return Err(InstanceErrorKind::InvalidId.into()),
        };
        let repo = self.clone(&url)?;
        cluster::rewind(&repo, &commit).map_err(|err| InstanceError {
            cause: Some(Box::new(err)),
            kind: InstanceErrorKind::CloningFailed,
        })?;
        self.build_invocation(&url, &commit)
    }

    pub fn cancel(&self) {
        *self.invocation.lock().unwrap() = None;
    }

    fn build_invocation(&self, url: &str, commit: &str) -> Result<InvocationId, InstanceError> {
        let (invocation, err) = Invocation::new(url, commit, &self.path);
        let id = invocation.id();
        self.invocations.lock().unwrap().insert(id, invocation);
        *self.invocation.lock().unwrap() = Some(id);
        if let Some(err) = err {
            return Err(InstanceError {
                cause: Some(Box::new(err)),
                kind: InstanceErrorKind::BrokenManifest,
            });
        }
        Ok(id)
    }

    fn clone(&self, url: &str) -> Result<Repository, InstanceError> {
        cluster::clone(url, &self.path).map_err(|err| InstanceError {
            cause: Some(Box::new(err)),
            kind: InstanceErrorKind::CloningFailed,
        })
    }
}
