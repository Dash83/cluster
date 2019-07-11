#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use cluster_server::host::{HostId, HostState};
use cluster_server::invocation::InvocationId;
use cluster_server::Instance;

use rocket::Request;
use rocket::State;

use rocket_contrib::json::JsonValue;
use rocket_contrib::serve::StaticFiles;
use rocket_contrib::templates::Template;

mod host {
    use super::*;

    #[get("/register/<hostname>")]
    pub fn register(hostname: String, instance: State<Instance>) -> JsonValue {
        match instance.register(&hostname) {
            Ok(id) => host(id, instance),
            Err(err) => json!({
                "status": "err",
                "msg": format!("{}", err),
            }),
        }
    }

    #[get("/<id>")]
    pub fn host(id: HostId, instance: State<Instance>) -> JsonValue {
        instance
            .host(id, |host| {
                json!({
                    "status": "ok",
                    "host": host,
                })
            })
            .unwrap_or(json!({ "status": "err" }))
    }

    #[get("/<id>/idle")]
    pub fn idle(id: HostId, instance: State<Instance>) -> JsonValue {
        instance.host(id, |host| host.set_state(HostState::Idle));
        json!({ "status": "ok" })
    }

    #[get("/<id>/running/<invocation>")]
    pub fn running(id: HostId, invocation: InvocationId, instance: State<Instance>) -> JsonValue {
        instance.host(id, |host| host.set_state(HostState::Running(invocation)));
        json!({ "status": "ok" })
    }

    #[get("/<id>/errored/<invocation>")]
    pub fn errored(id: HostId, invocation: InvocationId, instance: State<Instance>) -> JsonValue {
        instance.host(id, |host| host.set_state(HostState::Errored(invocation)));
        json!({ "status": "ok" })
    }

    #[get("/<id>/compressing/<invocation>")]
    pub fn compressing(
        id: HostId,
        invocation: InvocationId,
        instance: State<Instance>,
    ) -> JsonValue {
        instance.host(id, |host| {
            host.set_state(HostState::Compressing(invocation))
        });
        json!({ "status": "ok" })
    }

    #[get("/<id>/uploading/<invocation>")]
    pub fn uploading(id: HostId, invocation: InvocationId, instance: State<Instance>) -> JsonValue {
        instance.host(id, |host| host.set_state(HostState::Uploading(invocation)));
        json!({ "status": "ok" })
    }

    #[get("/<id>/done/<invocation>")]
    pub fn done(id: HostId, invocation: InvocationId, instance: State<Instance>) -> JsonValue {
        instance.host(id, |host| host.set_state(HostState::Done(invocation)));
        json!({ "status": "ok" })
    }
}

#[get("/hosts")]
pub fn hosts(instance: State<Instance>) -> JsonValue {
    instance.hosts(|iter| {
        json!({
            "status": "ok",
            "hosts": iter.collect::<Vec<_>>(),
        })
    })
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

fn main() {
    rocket::ignite()
        .manage(Instance::new())
        .register(catchers![internal_error, not_found])
        .mount("/static", StaticFiles::from("static/"))
        .mount("/api", routes![hosts])
        .mount(
            "/api/host",
            routes![
                host::register,
                host::host,
                host::idle,
                host::running,
                host::errored,
                host::compressing,
                host::uploading,
                host::done
            ],
        )
        .attach(Template::fairing())
        .launch();
}
