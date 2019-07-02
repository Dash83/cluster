#![feature(proc_macro_hygiene, decl_macro)]

extern crate cluster;
extern crate git2;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;
extern crate serde;
extern crate toml;

use cluster::Experiment;

use git2::Repository;

use rocket::response::Redirect;
use rocket::State;

use rocket_contrib::json::JsonValue;
use rocket_contrib::templates::Template;

use std::fs;
use std::sync::Mutex;

#[get("/")]
fn index(experiment: State<'_, Mutex<Experiment>>) -> Template {
    Template::render("index", &*experiment.lock().unwrap())
}

#[get("/ready/<hostname>")]
fn ready(hostname: String, experiment: State<'_, Mutex<Experiment>>) -> JsonValue {
    experiment.lock().unwrap().set_running(&hostname, true);
    json!({ "status": "ok" })
}

#[get("/repo")]
fn get_repo(experiment: State<'_, Mutex<Experiment>>) -> JsonValue {
    json!({
        "status": "ok",
        "url": experiment.lock().unwrap().url()
    })
}

#[get("/repo/<url>")]
fn set_repo(
    url: String,
    repo: State<'_, Mutex<Repository>>,
    experiment: State<'_, Mutex<Experiment>>,
) -> Redirect {
    fs::remove_dir_all(cluster::PATH).unwrap_or(());
    match Repository::clone(&url, cluster::PATH) {
        Ok(cloned) => {
            *repo.lock().unwrap() = cloned;
            let mut experiment = experiment.lock().unwrap();
            *experiment = Experiment::load(cluster::experiment_path(), &url)
                .expect("failed to parse deployment.toml");
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
    fs::remove_dir_all(cluster::PATH).unwrap_or(());
    *repo.lock().unwrap() =
        Repository::clone(experiment.url(), cluster::PATH).expect("failed to clone repository");
    *experiment = Experiment::load(cluster::experiment_path(), experiment.url())
        .expect("failed to parse deployment.toml");
    Redirect::to(uri!(index))
}

#[get("/restart")]
fn restart(experiment: State<'_, Mutex<Experiment>>) -> Redirect {
    experiment.lock().unwrap().restart();
    Redirect::to(uri!(index))
}

#[get("/status")]
fn status(experiment: State<'_, Mutex<Experiment>>) -> JsonValue {
    json!({
        "status": "ok",
        "hosts": experiment.lock().unwrap().running_hosts(),
    })
}

#[get("/status/<hostname>")]
fn host_status(hostname: String, experiment: State<'_, Mutex<Experiment>>) -> JsonValue {
    let experiment = experiment.lock().unwrap();
    if let Some(host) = experiment.get(&hostname) {
        json!({
            "status": "ok",
            "url": experiment.url(),
            "host": hostname,
            "running": host.running().to_string(),
            "restarted": experiment.restarted().to_string(),
        })
    } else {
        json!({
            "status": "ok",
            "url": experiment.url(),
            "host": hostname,
            "restarted": experiment.restarted().to_string(),
        })
    }
}

fn init() -> (Repository, Experiment) {
    fs::remove_dir_all(cluster::PATH).unwrap_or(());
    let repo =
        Repository::clone(cluster::DEFAULT, cluster::PATH).expect("failed to clone repository");
    let experiment = Experiment::load(cluster::experiment_path(), cluster::DEFAULT)
        .expect("failed to parse deployment.toml");
    (repo, experiment)
}

fn main() {
    let (repo, experiment) = init();
    rocket::ignite()
        .manage(Mutex::new(repo))
        .manage(Mutex::new(experiment))
        .mount("/", routes![index])
        .mount(
            "/api/",
            routes![
                ready,
                get_repo,
                set_repo,
                update,
                status,
                host_status,
                restart
            ],
        )
        .attach(Template::fairing())
        .launch();
}
