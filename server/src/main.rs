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
    ($key:expr, $val:expr) => {
        json!({
            "status": "ok",
            $key: $val,
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
        instance
            .host(id, |host| ok!("host", host))
            .unwrap_or(err!())
    }

    pub mod status {
        use super::*;

        #[get("/<id>/idle")]
        pub fn idle(id: HostId, instance: State<Instance>) -> JsonValue {
            instance.host(id, |host| {
                host.refresh();
                host.set_state(HostState::Idle)
            });
            ok!()
        }

        #[get("/<id>/running/<invocation>")]
        pub fn running(
            id: HostId,
            invocation: InvocationId,
            instance: State<Instance>,
        ) -> JsonValue {
            instance.host(id, |host| {
                host.refresh();
                host.set_state(HostState::Running(invocation))
            });
            ok!()
        }

        #[get("/<id>/errored/<invocation>")]
        pub fn errored(
            id: HostId,
            invocation: InvocationId,
            instance: State<Instance>,
        ) -> JsonValue {
            instance.host(id, |host| {
                host.refresh();
                host.set_state(HostState::Errored(invocation))
            });
            ok!()
        }

        #[get("/<id>/compressing/<invocation>")]
        pub fn compressing(
            id: HostId,
            invocation: InvocationId,
            instance: State<Instance>,
        ) -> JsonValue {
            instance.host(id, |host| {
                host.refresh();
                host.set_state(HostState::Compressing(invocation))
            });
            ok!()
        }

        #[get("/<id>/uploading/<invocation>")]
        pub fn uploading(
            id: HostId,
            invocation: InvocationId,
            instance: State<Instance>,
        ) -> JsonValue {
            instance.host(id, |host| {
                host.refresh();
                host.set_state(HostState::Uploading(invocation));
            });
            ok!()
        }

        #[get("/<id>/done/<invocation>")]
        pub fn done(id: HostId, invocation: InvocationId, instance: State<Instance>) -> JsonValue {
            instance.host(id, |host| {
                host.refresh();
                host.set_state(HostState::Done(invocation))
            });
            ok!()
        }
    }
}

#[get("/hosts")]
fn hosts(instance: State<Instance>) -> JsonValue {
    instance.hosts(|iter| ok!("hosts", iter.collect::<Vec<_>>()))
}

#[get("/invocation")]
fn invocation(instance: State<Instance>) -> JsonValue {
    match instance.current_invocation() {
        Some(id) => instance
            .invocation(id, |invocation| ok!("invocation", invocation))
            .unwrap_or(err!()),
        _ => err!(),
    }
}

#[get("/invoke/<url>")]
fn invoke(url: String, instance: State<Instance>) -> JsonValue {
    match instance.invoke(&url) {
        Ok(id) => instance
            .invocation(id, |invocation| ok!("invocation", invocation))
            .unwrap_or(err!()),
        Err(err) => err!(err),
    }
}

#[get("/reinvoke/<id>")]
fn reinvoke(id: InvocationId, instance: State<Instance>) -> JsonValue {
    match instance.reinvoke(id) {
        Ok(id) => instance
            .invocation(id, |invocation| ok!("invocation", invocation))
            .unwrap_or(err!()),
        Err(err) => err!(err),
    }
}

#[catch(404)]
#[allow(unused)]
fn not_found(request: &Request) -> JsonValue {
    err!()
}

#[catch(500)]
#[allow(unused)]
fn internal_error(request: &Request) -> JsonValue {
    err!()
}

fn main() {
    rocket::ignite()
        .manage(Instance::new("experiment/"))
        .register(catchers![internal_error, not_found])
        .mount("/static", StaticFiles::from("static/"))
        .mount("/api", routes![hosts, invoke, reinvoke])
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
