use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::{fmt, thread, time};

pub mod host;
pub mod invocation;

use self::host::*;
use self::invocation::*;

pub struct Instance {
    hosts: Arc<Mutex<HashMap<HostId, Host>>>,
    invocation: Mutex<Option<InvocationId>>,
    invocations: Mutex<HashMap<InvocationId, Invocation>>,
}

#[derive(Debug)]
pub struct InstanceError {
    cause: Option<Box<dyn Error>>,
    kind: InstanceErrorKind,
}

#[derive(Debug)]
pub enum InstanceErrorKind {
    /// The given hostname was already registered.
    HostRegisterd,
}

impl fmt::Display for InstanceErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InstanceErrorKind::HostRegisterd => {
                write!(f, "the given hostname was already registered")
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
    pub fn new() -> Instance {
        let hosts = Arc::new(Mutex::new(HashMap::new()));
        let instance = Instance {
            hosts: Arc::clone(&hosts),
            invocation: Mutex::new(None),
            invocations: Mutex::new(HashMap::new()),
        };
        thread::spawn(move || loop {
            thread::sleep(time::Duration::from_millis(200));
            let mut hosts = hosts.lock().unwrap();
            let mut expired = vec![];
            for (id, host) in hosts.iter() {
                if host.expired() {
                    expired.push(*id);
                }
            }
            for id in expired.into_iter() {
                hosts.remove(&id);
            }
        });
        instance
    }

    pub fn host<F, T>(&self, id: HostId, f: F) -> Option<T>
    where
        F: FnOnce(&mut Host) -> T,
    {
        self.hosts.lock().unwrap().get_mut(&id).map(f)
    }

    pub fn hosts<F, T>(&self, f: F) -> T
    where
        F: Fn(&mut dyn Iterator<Item = &'_ mut Host>) -> T,
    {
        let mut hosts = self.hosts.lock().unwrap();
        let iter = hosts.iter_mut();
        f(&mut iter.map(|(_, host)| host))
    }

    pub fn invocation<F, T>(&self, id: InvocationId, f: F) -> Option<T>
    where
        F: FnOnce(&mut Invocation) -> T,
    {
        match self.invocations.lock().unwrap().get_mut(&id) {
            Some(invocation) => Some(f(invocation)),
            _ => None,
        }
    }

    pub fn current_invocation(&self) -> Option<InvocationId> {
        match *self.invocation.lock().unwrap() {
            Some(ref invocation) => Some(*invocation),
            _ => None,
        }
    }

    pub fn register(&self, hostname: &str) -> Result<HostId, InstanceError> {
        let mut hosts = self.hosts.lock().unwrap();
        for (_, host) in hosts.iter() {
            if hostname == host.hostname() {
                return Err(InstanceErrorKind::HostRegisterd.into());
            }
        }
        let id = HostId::new();
        let host = Host::new(id, hostname);
        hosts.insert(id, host);
        Ok(id)
    }
}
