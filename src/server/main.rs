#![feature(proc_macro_hygiene, decl_macro)]

extern crate git2;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;
extern crate serde;
extern crate toml;

use git2::Repository;

use rocket::response::Redirect;
use rocket::State;

use rocket_contrib::json::JsonValue;
use rocket_contrib::templates::Template;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::sync::Mutex;

const PATH: &'static str = "experiment/";
const DEFAULT: &'static str = "https://github.com/doctorn/cluster_example.git";
const DEPLOYMENT: &'static str = "deployment.toml";

#[derive(Serialize, Deserialize)]
struct Experiment {
    name: String,
    #[serde(skip_deserializing)]
    url: String,
    setup: Vec<String>,
    hosts: HashMap<String, Host>,
}

#[derive(Serialize, Deserialize)]
struct Host {
    setup: Vec<String>,
    #[serde(skip_deserializing)]
    running: bool,
}

#[get("/")]
fn index(experiment: State<'_, Mutex<Experiment>>) -> Template {
    Template::render("index", &*experiment.lock().unwrap())
}

#[get("/ready/<hostname>")]
fn ready(hostname: String, experiment: State<'_, Mutex<Experiment>>) {
    if let Some(host) = experiment.lock().unwrap().hosts.get_mut(&hostname) {
        host.running = true
    }
}

#[get("/repo")]
fn get_repo(experiment: State<'_, Mutex<Experiment>>) -> JsonValue {
    json!({
        "url": experiment.lock().unwrap().url.clone()
    })
}

#[get("/repo/<url>")]
fn set_repo(
    url: String,
    repo: State<'_, Mutex<Repository>>,
    experiment: State<'_, Mutex<Experiment>>,
) -> Redirect {
    fs::remove_dir_all(PATH).unwrap_or(());
    match Repository::clone(&url, PATH) {
        Ok(cloned) => {
            *repo.lock().unwrap() = cloned;
            let mut experiment = experiment.lock().unwrap();
            *experiment = parse_experiment(url).expect("failed to parse deployment.toml");
        }
        _ => {}
    }
    Redirect::to(uri!(index))
}

#[get("/update")]
fn update(
    repo: State<'_, Mutex<Repository>>,
    experiment: State<'_, Mutex<Experiment>>,
) -> Redirect {
    let mut experiment = experiment.lock().unwrap();
    fs::remove_dir_all(PATH).unwrap_or(());
    *repo.lock().unwrap() =
        Repository::clone(&experiment.url, PATH).expect("failed to clone repository");
    *experiment =
        parse_experiment(experiment.url.clone()).expect("failed to parse deployment.toml");
    Redirect::to(uri!(index))
}

#[get("/status")]
fn status(experiment: State<'_, Mutex<Experiment>>) -> JsonValue {
    let mut running = vec![];
    for (name, host) in experiment.lock().unwrap().hosts.iter() {
        if host.running {
            running.push(name.clone());
        }
    }
    json!(running)
}

fn parse_experiment(url: String) -> Result<Experiment, ()> {
    if let Ok(mut file) = fs::File::open(format!("{}{}", PATH, DEPLOYMENT)) {
        let mut contents = String::new();
        if let Ok(_) = file.read_to_string(&mut contents) {
            if let Ok(mut experiment) = toml::from_str::<Experiment>(&contents) {
                experiment.url = url;
                return Ok(experiment);
            }
        }
    }
    Err(())
}

fn init() -> (Repository, Experiment) {
    fs::remove_dir_all(PATH).unwrap_or(());
    let repo = Repository::clone(DEFAULT, PATH).expect("failed to clone repository");
    let experiment =
        parse_experiment(DEFAULT.to_string()).expect("failed to parse deployment.toml");
    (repo, experiment)
}

fn main() {
    let (repo, experiment) = init();
    rocket::ignite()
        .manage(Mutex::new(repo))
        .manage(Mutex::new(experiment))
        .mount("/", routes![index])
        .mount("/api/", routes![ready, get_repo, set_repo, update, status])
        .attach(Template::fairing())
        .launch();
}
