extern crate chrono;
extern crate cluster;
extern crate gethostname;
extern crate git2;
extern crate reqwest;
extern crate serde;
extern crate shared_child;
extern crate toml;

use chrono::Utc;

use cluster::Experiment;

use git2::Repository;

use shared_child::SharedChild;

use std::collections::HashMap;
use std::fs::File;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::{fs, thread, time};

const SERVER: &'static str = "http://localhost:8000";

struct Client {
    hostname: String,
    experiment: Arc<Mutex<Experiment>>,
    to_kill: Arc<Mutex<Option<Arc<SharedChild>>>>,
}

impl Client {
    fn new() -> Client {
        Client {
            hostname: gethostname::gethostname().into_string().unwrap(),
            experiment: Arc::new(Mutex::new(Default::default())),
            to_kill: Arc::new(Mutex::new(None)),
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
        if let Some(ref child) = *self.to_kill.lock().unwrap() {
            child.kill().unwrap();
        }
    }

    fn invoke(&self) {
        let now = Utc::now();
        let hostname = self.hostname.clone();
        let log_name = format!(
            "{}@{}-{:?}",
            &hostname,
            { self.experiment.lock().unwrap().name().to_string() },
            now
        );
        let experiment = Arc::clone(&self.experiment);
        let to_kill = Arc::clone(&self.to_kill);
        thread::spawn(move || {
            let command = { experiment.lock().unwrap().gen_command() };
            if let Some(mut command) = command {
                let (out, err) = create_log_files(&log_name);
                command.stdout(Stdio::from(out));
                command.stderr(Stdio::from(err));
                if let Ok(child) = SharedChild::spawn(&mut command) {
                    let child = Arc::new(child);
                    {
                        *to_kill.lock().unwrap() = Some(Arc::clone(&child))
                    }
                    if let Err(_) = child.wait() {
                        return;
                    }
                }
            }
            let command = {
                experiment
                    .lock()
                    .unwrap()
                    .get(&hostname)
                    .unwrap()
                    .gen_command()
            };
            if let Some(mut command) = command {
                let (out, err) = create_log_files(&log_name);
                command.stdout(Stdio::from(out));
                command.stderr(Stdio::from(err));
                if let Ok(child) = SharedChild::spawn(&mut command) {
                    let child = Arc::new(child);
                    {
                        *to_kill.lock().unwrap() = Some(Arc::clone(&child))
                    }
                    if let Err(_) = child.wait() {
                        return;
                    }
                }
            }
        });
    }
}

fn create_log_files(name: &str) -> (File, File) {
    let out = File::create(format!("{}.stdout", name)).unwrap();
    let err = File::create(format!("{}.stderr", name)).unwrap();
    (out, err)
}

fn main() {
    let mut client = Client::new();
    loop {
        client.poll();
        thread::sleep(time::Duration::from_millis(500));
    }
}
