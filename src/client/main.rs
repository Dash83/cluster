extern crate chrono;
extern crate cluster;
extern crate gethostname;
extern crate git2;
extern crate libc;
extern crate reqwest;
extern crate serde;
extern crate toml;

use chrono::Utc;

use cluster::{Experiment, Host};

use git2::Repository;

use nix::sys::signal;
use nix::unistd::{fork, setpgid, ForkResult, Pid};

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::process::{self, Command, Stdio};
use std::{env, fs, mem, thread, time};

struct Client {
    headless: bool,
    hostname: String,
    server: String,
    experiment: Option<Experiment>,
    executor: Option<Pid>,
}

impl Client {
    fn new(server: &str) -> Client {
        Client {
            headless: false,
            hostname: gethostname::gethostname().into_string().unwrap(),
            server: server.to_string(),
            experiment: None,
            executor: None,
        }
    }

    fn poll(&mut self) {
        if let Ok(mut response) = reqwest::get(&format!(
            "http://{}/api/status/{}",
            self.server, self.hostname
        )) {
            let response = response.json::<HashMap<String, String>>().unwrap();
            let running = response
                .get("running")
                .map(|x| x.parse::<bool>().unwrap_or(true))
                .unwrap_or(true);
            if !running {
                let restarted = response.get("restarted").unwrap().parse::<bool>().unwrap();
                self.kill();
                if !restarted || self.experiment.is_none() {
                    let url = response.get("url").unwrap();
                    fs::remove_dir_all(cluster::PATH).unwrap_or(());
                    if let Ok(_) = Repository::clone(url, cluster::PATH) {
                        if let Ok(_) = reqwest::get(&format!(
                            "http://{}/api/ready/{}",
                            self.server, self.hostname
                        )) {
                            self.experiment =
                                Some(Experiment::load(cluster::experiment_path(), url).unwrap());
                            self.invoke();
                        }
                    }
                } else {
                    self.kill();
                    if let Ok(_) = reqwest::get(&format!(
                        "http://{}/api/ready/{}",
                        self.server, self.hostname
                    )) {
                        self.invoke();
                    }
                }
            }
        }
    }

    fn kill(&mut self) {
        let mut child = None;
        mem::swap(&mut self.executor, &mut child);
        if let Some(child) = child {
            let child = Pid::from_raw(-child.as_raw());
            match signal::kill(child, signal::SIGTERM) {
                _ => {}
            }
            match signal::kill(child, signal::SIGKILL) {
                _ => {}
            }
        }
    }

    fn invoke(&mut self) {
        if let Some(ref experiment) = self.experiment {
            let name = format!(
                "{}@{}-{}",
                self.hostname,
                experiment.name(),
                Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
            );
            match fork() {
                Ok(ForkResult::Parent { child, .. }) => self.executor = Some(child),
                Ok(ForkResult::Child) => {
                    setpgid(Pid::from_raw(0), Pid::from_raw(0)).unwrap();
                    if let Some(ref mut experiment) = self.experiment {
                        if !self.headless {
                            if let Ok(_) = experiment.invoke::<File, File>((None, None)) {
                                if let Ok(_) = experiment
                                    .get_mut(&self.hostname)
                                    .unwrap()
                                    .invoke::<File, File>((None, None))
                                {
                                    process::exit(0);
                                }
                            }
                        } else {
                            if let Ok(_) = experiment.invoke_as(&name) {
                                if let Ok(_) =
                                    experiment.get_mut(&self.hostname).unwrap().invoke_as(&name)
                                {
                                    process::exit(0);
                                }
                            }
                        }
                    }
                    process::exit(1);
                }
                Err(_) => {}
            }
        }
    }
}

trait Invokable {
    fn invoke<T, U>(&mut self, pipe: (Option<T>, Option<U>)) -> Result<(), ()>
    where
        T: Into<Stdio>,
        U: Into<Stdio>;

    fn invoke_as<P: AsRef<Path>>(&mut self, log: P) -> Result<(), ()> {
        let out = File::create(log.as_ref().with_extension("stdout")).ok();
        let err = File::create(log.as_ref().with_extension("stderr")).ok();
        self.invoke((out, err))
    }
}

impl Invokable for Command {
    fn invoke<T, U>(&mut self, (out, err): (Option<T>, Option<U>)) -> Result<(), ()>
    where
        T: Into<Stdio>,
        U: Into<Stdio>,
    {
        if let Some(out) = out {
            self.stdout(out.into());
        }
        if let Some(err) = err {
            self.stderr(err.into());
        }
        match self.status() {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }
}

impl Invokable for Host {
    fn invoke<T, U>(&mut self, (out, err): (Option<T>, Option<U>)) -> Result<(), ()>
    where
        T: Into<Stdio>,
        U: Into<Stdio>,
    {
        if let Some(mut command) = self.gen_command() {
            command.invoke((out, err))
        } else {
            Ok(())
        }
    }
}

impl Invokable for Experiment {
    fn invoke<T, U>(&mut self, (out, err): (Option<T>, Option<U>)) -> Result<(), ()>
    where
        T: Into<Stdio>,
        U: Into<Stdio>,
    {
        if let Some(mut command) = self.gen_command() {
            command.invoke((out, err))
        } else {
            Ok(())
        }
    }
}

fn main() {
    let mut headless = false;
    let mut server = String::from("192.168.100.1:8000");
    for arg in env::args() {
        if arg == "--headless" {
            headless = true;
        } else {
            server = arg.to_string();
        }
    }
    let mut client = Client::new(&server);
    client.headless = headless;
    loop {
        client.poll();
        thread::sleep(time::Duration::from_millis(500));
    }
}
