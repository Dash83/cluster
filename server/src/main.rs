#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use cluster::Experiment;

use git2::Repository;

use multipart::server::Multipart;

use rocket::data::{self, FromDataSimple};
use rocket::response::Redirect;
use rocket::{Data, Outcome, Request, State};

use rocket_contrib::json::JsonValue;
use rocket_contrib::serve::StaticFiles;
use rocket_contrib::templates::Template;

use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;

const LOG_DIR: &str = "logs/";

#[derive(Debug)]
struct LogUpload(());

impl FromDataSimple for LogUpload {
    type Error = ();

    fn from_data(request: &Request, data: Data) -> data::Outcome<Self, Self::Error> {
        let ct = request.headers().get_one("Content-Type").unwrap();
        let idx = ct.find("boundary=").unwrap();
        let boundary = &ct[(idx + "boundary=".len())..];
        let mut body = vec![];
        data.stream_to(&mut body).unwrap();
        let mut mp = Multipart::with_body(Cursor::new(body), boundary);
        mp.foreach_entry(|mut entry| {
            if &*entry.headers.name == "log" {
                let filename = entry.headers.filename.unwrap();
                let path = Path::new(&filename);
                entry
                    .data
                    .save()
                    .with_path(Path::new(LOG_DIR).join(path.file_name().unwrap()));
            }
        })
        .unwrap();
        Outcome::Success(LogUpload(()))
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

#[post("/upload", data = "<upload>")]
#[allow(unused)]
fn upload(upload: LogUpload) -> JsonValue {
    json!({ "status": "ok" })
}

#[get("/logs")]
fn logs() -> JsonValue {
    let mut entries = vec![];
    for entry in fs::read_dir(Path::new(LOG_DIR)).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_dir() {
            let name = path.file_name().unwrap().to_owned();
            entries.push(name.to_string_lossy().into_owned());
        }
    }
    json!({
        "status": "ok",
        "entries": entries,
    })
}

#[get("/logs/clear")]
fn clear_logs() -> JsonValue {
    fs::remove_dir_all(Path::new(LOG_DIR)).unwrap();
    fs::create_dir_all(LOG_DIR).unwrap();
    json!({ "status": "ok" })
}

#[catch(404)]
#[allow(unused)]
fn not_found(request: &Request) -> JsonValue {
    json!({ "status": "err" })
}

#[catch(500)]
#[allow(unused)]
fn internal_error(request: &Request) -> JsonValue {
    json!({ "status": "err" })
}

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

fn main() {
    fs::create_dir_all(LOG_DIR).unwrap();
    rocket::ignite()
        .manage::<Mutex<Option<Experiment>>>(Mutex::new(None))
        .register(catchers![internal_error, not_found])
        .mount("/logs", StaticFiles::from(LOG_DIR))
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
                restart,
                upload,
                logs,
                clear_logs,
            ],
        )
        .attach(Template::fairing())
        .launch();
}
