use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::error::Error;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

pub const PATH: &str = "experiment/";
pub const DEPLOYMENT: &str = "deployment.toml";
const LOG_DIR_DEFAULT: &str = "logs/";

#[derive(Serialize, Deserialize, Default)]
pub struct Experiment {
    name: String,
    #[serde(skip_deserializing)]
    url: String,
    #[serde(skip)]
    restarted: bool,
    command: Option<String>,
    args: Option<Vec<String>>,
    hosts: HashMap<String, Host>,
    #[serde(default = "default_log_dir")]
    log_dir: PathBuf,
    #[serde(default = "default_gen_logs")]
    gen_logs: bool,
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
        fs::create_dir_all(&experiment.log_dir)?;
        Ok(experiment)
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn gen_logs(&self) -> bool {
        self.gen_logs
    }

    pub fn get(&self, host: &str) -> Option<&Host> {
        self.hosts.get(host)
    }

    pub fn get_mut(&mut self, host: &str) -> Option<&mut Host> {
        self.hosts.get_mut(host)
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

    pub fn restart(&mut self) {
        self.restarted = true;
        for (_, host) in self.hosts.iter_mut() {
            host.running = false;
        }
    }

    pub fn restarted(&self) -> bool {
        self.restarted
    }

    pub fn gen_command(&self) -> Option<Command> {
        gen_command(&self.command, &self.args)
    }

    pub fn log_path(&self) -> &Path {
        &self.log_dir
    }

    pub fn as_log_path<P: AsRef<Path>>(&self, log: P) -> PathBuf {
        self.log_dir.join(log)
    }

    pub fn clear_logs(&self) {
        fs::remove_dir_all(&self.log_dir).unwrap_or(());
        fs::create_dir_all(&self.log_dir).unwrap();
    }
}

impl Host {
    pub fn running(&self) -> bool {
        self.running
    }

    pub fn gen_command(&self) -> Option<Command> {
        gen_command(&self.command, &self.args)
    }
}

pub fn experiment_path() -> String {
    format!("{}{}", PATH, DEPLOYMENT)
}

fn gen_command(command: &Option<String>, args: &Option<Vec<String>>) -> Option<Command> {
    if let Some(ref command) = command {
        let mut command = Command::new(command);
        command.current_dir(PATH);
        if let Some(ref args) = args {
            for arg in args.iter() {
                command.arg(arg);
            }
        }
        Some(command)
    } else {
        None
    }
}

fn default_log_dir() -> PathBuf {
    env::current_dir().unwrap().join(LOG_DIR_DEFAULT)
}

fn default_gen_logs() -> bool {
    false
}
