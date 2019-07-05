#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use cluster::Experiment;

use git2::Repository;

use rocket::response::Redirect;
use rocket::State;

use rocket_contrib::json::JsonValue;
use rocket_contrib::templates::Template;

use std::fs;
use std::sync::Mutex;

fn reclone(url: &str, experiment: State<'_, Mutex<Option<Experiment>>>) {
    fs::remove_dir_all(cluster::PATH).unwrap_or(());
    match Repository::clone(&url, cluster::PATH) {
        Ok(_) => {
            let mut experiment = experiment.lock().unwrap();
            if let Ok(manifest) = Experiment::load(cluster::experiment_path(), &url) {
                *experiment = Some(manifest)
            } else {
                *experiment = None
            }
        }
        _ => *experiment.lock().unwrap() = None,
    }
}

#[get("/")]
fn index(experiment: State<'_, Mutex<Option<Experiment>>>) -> Template {
    Template::render("index", &*experiment.lock().unwrap())
}

#[get("/ready/<hostname>")]
fn ready(hostname: String, experiment: State<'_, Mutex<Option<Experiment>>>) -> JsonValue {
    if let Some(ref mut experiment) = *experiment.lock().unwrap() {
        experiment.set_running(&hostname, true);
    }
    json!({ "status": "ok" })
}

#[get("/repo")]
fn get_repo(experiment: State<'_, Mutex<Option<Experiment>>>) -> JsonValue {
    if let Some(ref mut experiment) = *experiment.lock().unwrap() {
        json!({
            "status": "ok",
            "url": experiment.url(),
        })
    } else {
        json!({ "status": "err" })
    }
}

#[get("/repo/<url>")]
fn set_repo(url: String, experiment: State<'_, Mutex<Option<Experiment>>>) -> Redirect {
    reclone(&url, experiment);
    Redirect::to(uri!(index))
}

#[get("/update")]
fn update(experiment: State<'_, Mutex<Option<Experiment>>>) -> Redirect {
    let url = if let Some(ref experiment) = *experiment.lock().unwrap() {
        experiment.url().to_string()
    } else {
        return Redirect::to(uri!(index));
    };
    reclone(&url, experiment);
    Redirect::to(uri!(index))
}

#[get("/restart")]
fn restart(experiment: State<'_, Mutex<Option<Experiment>>>) -> Redirect {
    if let Some(ref mut experiment) = *experiment.lock().unwrap() {
        experiment.restart();
    }
    Redirect::to(uri!(index))
}

#[get("/status")]
fn status(experiment: State<'_, Mutex<Option<Experiment>>>) -> JsonValue {
    if let Some(ref experiment) = *experiment.lock().unwrap() {
        json!({
            "status": "ok",
            "hosts": experiment.running_hosts(),
        })
    } else {
        json!({ "status": "err" })
    }
}

#[get("/status/<hostname>")]
fn host_status(hostname: String, experiment: State<'_, Mutex<Option<Experiment>>>) -> JsonValue {
    if let Some(ref experiment) = *experiment.lock().unwrap() {
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
    } else {
        json!({
            "status": "ok",
            "host": hostname,
        })
    }
}

fn main() {
    rocket::ignite()
        .manage::<Mutex<Option<Experiment>>>(Mutex::new(None))
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
