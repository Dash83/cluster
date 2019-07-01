extern crate cluster;
extern crate gethostname;
extern crate git2;
extern crate reqwest;
extern crate serde;
extern crate toml;

use cluster::Experiment;

use git2::Repository;

use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::{fs, thread, time};

const SERVER: &'static str = "http://localhost:8000";

struct Client {
    hostname: String,
    child: Arc<Mutex<Option<Child>>>,
    experiment: Arc<Mutex<Experiment>>,
}

impl Client {
    fn new() -> Client {
        Client {
            hostname: gethostname::gethostname().into_string().unwrap(),
            child: Arc::new(Mutex::new(None)),
            experiment: Arc::new(Mutex::new(Default::default())),
        }
    }

    fn poll(&mut self) {
        if let Ok(mut response) = reqwest::get(&format!("{}/api/status/{}", SERVER, &self.hostname))
        {
            let response = response.json::<HashMap<String, String>>().unwrap();
            let running = response
                .get("running")
                .map(|x| x.parse::<bool>().unwrap_or(true))
                .unwrap_or(true);
            if !running {
                self.kill();
                let url = response.get("url").unwrap();
                fs::remove_dir_all(cluster::PATH).unwrap_or(());
                if let Ok(_) = Repository::clone(url, cluster::PATH) {
                    if let Ok(_) = reqwest::get(&format!("{}/api/ready/{}", SERVER, &self.hostname))
                    {
                        {
                            *self.experiment.lock().unwrap() =
                                Experiment::load(cluster::experiment_path(), url).unwrap();
                        }
                        self.invoke();
                    }
                }
            }
        }
    }

    fn kill(&self) {
        if let Some(ref mut child) = *self.child.lock().unwrap() {
            match child.kill() {
                Ok(_) => {}
                Err(_) => panic!("couldn't kill child"),
            }
        }
    }

    fn invoke(&self) {
        let hostname = self.hostname.clone();
        let child = Arc::clone(&self.child);
        let experiment = Arc::clone(&self.experiment);
        thread::spawn(move || {
            let handle = { experiment.lock().unwrap().run() };
            if let Some(handle) = handle {
                {
                    *child.lock().unwrap() = Some(handle);
                }
                if let Some(ref mut handle) = *child.lock().unwrap() {
                    handle.wait().unwrap();
                }
                *child.lock().unwrap() =
                    { experiment.lock().unwrap().get(&hostname).unwrap().run() }
            }
        });
    }
}

fn main() {
    let mut client = Client::new();
    loop {
        client.poll();
        thread::sleep(time::Duration::from_millis(500));
    }
}
