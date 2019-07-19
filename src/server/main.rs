#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

mod instance;

use cluster::host::{HostId, HostState};
use cluster::invocation::InvocationId;

use multipart::server::Multipart;

use rocket::data::{self, FromDataSimple};
use rocket::http::Status;
use rocket::{Data, Outcome, Request, State};

use rocket_contrib::json::JsonValue;
use rocket_contrib::serve::StaticFiles;
use rocket_contrib::templates::Template;

use self::instance::Instance;

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use uuid::Uuid;

const LOG_DIR: &str = "logs/";

macro_rules! ok {
    () => {
        json!({ "status": "ok" })
    };
    ($val:expr) => {
        json!({
            "status": "ok",
            "payload": $val,
        })
    };
}

macro_rules! err {
    () => {
        json!({ "status": "err" })
    };
    ($err:expr) => {
        json!({
            "status": "err",
            "msg": format!("{}", $err),
        })
    };
}

#[derive(Debug)]
struct LogUpload(PathBuf);

impl FromDataSimple for LogUpload {
    type Error = ();

    fn from_data(request: &Request, data: Data) -> data::Outcome<Self, Self::Error> {
        if let Some(content_type) = request.headers().get_one("Content-Type") {
            if let Some(index) = content_type.find("boundary=") {
                let boundary = &content_type[(index + "boundary=".len())..];
                let mut body = vec![];
                if data.stream_to(&mut body).is_ok() {
                    let mut multipart = Multipart::with_body(Cursor::new(body), boundary);
                    let mut path = None;
                    if multipart
                        .foreach_entry(|mut entry| {
                            if &*entry.headers.name == "log" {
                                let log_path = Path::new(LOG_DIR)
                                    .join(&format!("{}", Uuid::new_v4()))
                                    .with_extension("tar.gz");
                                entry.data.save().memory_threshold(0).with_path(&log_path);
                                path = Some(log_path);
                            }
                        })
                        .is_ok()
                    {
                        if let Some(path) = path {
                            return Outcome::Success(LogUpload(path));
                        }
                    }
                }
            }
        }
        Outcome::Failure((Status::InternalServerError, ()))
    }
}

mod host {
    use super::*;

    #[get("/register/<hostname>")]
    pub fn register(hostname: String, instance: State<Instance>) -> JsonValue {
        match instance.register(&hostname) {
            Ok(id) => host(id, instance),
            Err(err) => err!(err),
        }
    }

    #[get("/<id>")]
    pub fn host(id: HostId, instance: State<Instance>) -> JsonValue {
        instance.host(id, |host| ok!(host)).unwrap_or(err!())
    }

    pub mod status {
        use super::*;

        #[inline]
        fn set_state(instance: State<Instance>, id: HostId, state: HostState) -> JsonValue {
            instance
                .host(id, |host| {
                    host.refresh();
                    host.set_state(state);
                    ok!()
                })
                .unwrap_or(err!())
        }

        #[get("/<host>/idle")]
        pub fn idle(host: HostId, instance: State<Instance>) -> JsonValue {
            set_state(instance, host, HostState::Idle)
        }

        #[get("/<host>/running/<id>")]
        pub fn running(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            set_state(instance, host, HostState::Running { id })
        }

        #[get("/<host>/errored/<id>")]
        pub fn errored(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            set_state(instance, host, HostState::Errored { id })
        }

        #[get("/<host>/compressing/<id>")]
        pub fn compressing(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            set_state(instance, host, HostState::Compressing { id })
        }

        #[get("/<host>/uploading/<id>")]
        pub fn uploading(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            set_state(instance, host, HostState::Uploading { id })
        }

        #[get("/<host>/done/<id>")]
        pub fn done(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            set_state(instance, host, HostState::Done { id })
        }
    }
}

#[get("/")]
fn index() -> Template {
    Template::render("index", {})
}

#[get("/hosts")]
fn hosts(instance: State<Instance>) -> JsonValue {
    instance.hosts(|iter| ok!(iter.collect::<Vec<_>>()))
}

#[get("/current")]
fn current(instance: State<Instance>) -> JsonValue {
    instance
        .current_invocation()
        .map(|id| ok!(id))
        .unwrap_or(err!())
}

#[get("/invocation/<id>")]
fn invocation(id: InvocationId, instance: State<Instance>) -> JsonValue {
    instance
        .invocation(id, |invocation| ok!(invocation))
        .unwrap_or(err!())
}

#[get("/invocations")]
fn invocations(instance: State<Instance>) -> JsonValue {
    instance.invocations(|iter| {
        ok!(iter
            .map(|invocation| invocation.record())
            .collect::<Vec<_>>())
    })
}

#[get("/invoke/<url>")]
fn invoke(url: String, instance: State<Instance>) -> JsonValue {
    match instance.invoke(&url) {
        Ok(id) => instance
            .invocation(id, |invocation| ok!(invocation))
            .unwrap_or(err!()),
        Err(err) => err!(err),
    }
}

#[get("/reinvoke/<id>")]
fn reinvoke(id: InvocationId, instance: State<Instance>) -> JsonValue {
    match instance.reinvoke(id) {
        Ok(id) => instance
            .invocation(id, |invocation| ok!(invocation))
            .unwrap_or(err!()),
        Err(err) => err!(err),
    }
}

#[post("/upload/<id>/<host>", data = "<upload>")]
fn upload(
    upload: LogUpload,
    id: InvocationId,
    host: HostId,
    instance: State<Instance>,
) -> JsonValue {
    instance
        .invocation(id, |invocation| {
            instance
                .host(host, |host| {
                    invocation.add_log(host, upload.0);
                    json!({ "status": "ok" })
                })
                .unwrap_or(err!())
        })
        .unwrap_or(err!())
}

#[catch(404)]
fn not_found(_request: &Request) -> JsonValue {
    err!("page not found")
}

#[catch(500)]
fn internal_error(_request: &Request) -> JsonValue {
    err!("internal server error")
}

fn main() {
    fs::create_dir(LOG_DIR).unwrap_or(());
    rocket::ignite()
        .manage(Instance::new("experiment/"))
        .register(catchers![internal_error, not_found])
        .mount("/static", StaticFiles::from("static/"))
        .mount("/logs", StaticFiles::from("logs/"))
        .mount("/", routes![index])
        .mount(
            "/api",
            routes![
                hosts,
                current,
                invocation,
                invocations,
                invoke,
                reinvoke,
                upload
            ],
        )
        .mount("/api/host", routes![host::host, host::register])
        .mount(
            "/api/host/status",
            routes![
                host::status::idle,
                host::status::running,
                host::status::errored,
                host::status::compressing,
                host::status::uploading,
                host::status::done
            ],
        )
        .attach(Template::fairing())
        .launch();
}
