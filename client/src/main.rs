use chrono::Utc;

use cluster::{Experiment, Host};

use git2::Repository;

use flate2::write::GzEncoder;
use flate2::Compression;

use nix::sys::signal;
use nix::unistd::{fork, setpgid, ForkResult, Pid};

use reqwest::multipart;

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::{env, fs, mem, thread, time};

struct Client {
    hostname: String,
    server: String,
    experiment: Option<Experiment>,
    executor: Option<Pid>,
    log: Option<PathBuf>,
}

impl Client {
    fn new(server: &str) -> Client {
        Client {
            hostname: gethostname::gethostname().into_string().unwrap(),
            server: server.to_string(),
            experiment: None,
            executor: None,
            log: None,
        }
    }

    fn poll(&mut self) {
        if let Ok(mut response) = reqwest::get(&format!(
            "http://{}/api/status/{}",
            self.server, self.hostname
        )) {
            let response = response.json::<HashMap<String, String>>().unwrap();
            match response
                .get("running")
                .map(|x| x.parse::<bool>().unwrap_or(true))
            {
                Some(false) => {
                    let restarted = response.get("restarted").unwrap().parse::<bool>().unwrap();
                    self.kill();
                    self.log = if !restarted || self.experiment.is_none() {
                        let url = response.get("url").unwrap();
                        fs::remove_dir_all(cluster::PATH).unwrap_or(());
                        if let Ok(_) = Repository::clone(url, cluster::PATH) {
                            if let Ok(_) = reqwest::get(&format!(
                                "http://{}/api/ready/{}",
                                self.server, self.hostname
                            )) {
                                self.experiment = Some(
                                    Experiment::load(cluster::experiment_path(), url).unwrap(),
                                );
                                self.invoke()
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        self.kill();
                        if let Ok(_) = reqwest::get(&format!(
                            "http://{}/api/ready/{}",
                            self.server, self.hostname
                        )) {
                            self.invoke()
                        } else {
                            None
                        }
                    };
                }
                Some(true) => {}
                None => self.kill(),
            }
        }
    }

    fn kill(&mut self) {
        let mut child = None;
        mem::swap(&mut self.executor, &mut child);
        if let Some(child) = child {
            println!("killing child process...");
            let child = Pid::from_raw(-child.as_raw());
            match signal::kill(child, signal::SIGTERM) {
                _ => {}
            }
            match signal::kill(child, signal::SIGKILL) {
                _ => {}
            }
            println!("done");
        }
        let mut log = None;
        mem::swap(&mut self.log, &mut log);
        if let Some(ref log) = log {
            if let Some(ref experiment) = self.experiment {
                println!("compressing logs...");
                let path = log.with_extension("tar.gz");
                let tar_gz = File::create(&path).unwrap();
                let enc = GzEncoder::new(tar_gz, Compression::default());
                let mut tar = tar::Builder::new(enc);
                tar.append_dir_all(".", experiment.log_path()).unwrap();
                println!("done");
                println!("uploading logs...");
                let form = multipart::Form::new()
                    .file("log", &*log.with_extension("tar.gz").to_string_lossy())
                    .unwrap();
                reqwest::Client::new()
                    .post(&format!("http://{}/api/upload", self.server))
                    .multipart(form)
                    .send()
                    .unwrap();
                println!("done");
                fs::remove_file(path).unwrap_or(());
            }
        }
    }

    fn invoke(&mut self) -> Option<PathBuf> {
        if let Some(ref experiment) = self.experiment {
            experiment.clear_logs();
            let name = format!(
                "{}@{}-{}",
                self.hostname,
                experiment.name(),
                Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
            );
            let path = experiment.as_log_path(&name);
            match fork() {
                Ok(ForkResult::Parent { child, .. }) => self.executor = Some(child),
                Ok(ForkResult::Child) => {
                    setpgid(Pid::from_raw(0), Pid::from_raw(0)).unwrap();
                    if let Some(ref mut experiment) = self.experiment {
                        if !experiment.gen_logs() {
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
                            if let Ok(_) = experiment.invoke_as(&path) {
                                if let Ok(_) =
                                    experiment.get_mut(&self.hostname).unwrap().invoke_as(&path)
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
            Some(name.into())
        } else {
            None
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.kill()
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
    let mut server = String::from("192.168.100.1:8000");
    for arg in env::args() {
        server = arg.to_string();
    }
    let client = Arc::new(Mutex::new(Client::new(&server)));
    {
        let client = Arc::clone(&client);
        ctrlc::set_handler(move || {
            client.lock().unwrap().kill();
            process::exit(0);
        })
        .unwrap();
    }
    loop {
        thread::sleep(time::Duration::from_millis(500));
        client.lock().unwrap().poll();
    }
}
