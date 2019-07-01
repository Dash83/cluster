extern crate serde;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::prelude::*;
use std::path::Path;
use std::process::{Child, Command};

pub const PATH: &'static str = "experiment/";
pub const DEFAULT: &'static str = "https://github.com/doctorn/cluster_example.git";
pub const DEPLOYMENT: &'static str = "deployment.toml";

#[derive(Serialize, Deserialize, Default)]
pub struct Experiment {
    name: String,
    #[serde(skip_deserializing)]
    url: String,
    command: Option<String>,
    args: Option<Vec<String>>,
    hosts: HashMap<String, Host>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Host {
    command: Option<String>,
    args: Option<Vec<String>>,
    #[serde(skip_deserializing)]
    running: bool,
}

impl Experiment {
    pub fn load<P: AsRef<Path>>(path: P, url: &str) -> Result<Experiment, Box<dyn Error>> {
        let mut file = fs::File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let mut experiment = toml::from_str::<Experiment>(&contents)?;
        experiment.url = url.to_string();
        Ok(experiment)
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn get(&self, host: &str) -> Option<&Host> {
        self.hosts.get(host)
    }

    pub fn set_running(&mut self, host: &str, running: bool) {
        if let Some(mut host) = self.hosts.get_mut(host) {
            host.running = running
        }
    }

    pub fn running_hosts(&self) -> Vec<String> {
        let mut status = vec![];
        for (name, host) in self.hosts.iter() {
            if host.running() {
                status.push(name.clone());
            }
        }
        status
    }

    pub fn run(&self) -> Option<Child> {
        run(&self.command, &self.args)
    }
}

impl Host {
    pub fn running(&self) -> bool {
        self.running
    }

    pub fn run(&self) -> Option<Child> {
        run(&self.command, &self.args)
    }
}

pub fn experiment_path() -> String {
    format!("{}{}", PATH, DEPLOYMENT)
}

fn run(command: &Option<String>, args: &Option<Vec<String>>) -> Option<Child> {
    if let Some(ref command) = command {
        let mut command = &mut Command::new(command);
        command.current_dir(PATH);
        if let Some(ref args) = args {
            for arg in args.iter() {
                command = command.arg(arg);
            }
        }
        command.spawn().ok()
    } else {
        None
    }
}
