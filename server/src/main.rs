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

        #[get("/<host>/running/<id>")]
        pub fn running(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            instance.host(host, |host| {
                host.refresh();
                host.set_state(HostState::Running { id })
            });
            ok!()
        }

        #[get("/<host>/errored/<id>")]
        pub fn errored(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            instance.host(host, |host| {
                host.refresh();
                host.set_state(HostState::Errored { id })
            });
            ok!()
        }

        #[get("/<host>/compressing/<id>")]
        pub fn compressing(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            instance.host(host, |host| {
                host.refresh();
                host.set_state(HostState::Compressing { id })
            });
            ok!()
        }

        #[get("/<host>/uploading/<id>")]
        pub fn uploading(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            instance.host(host, |host| {
                host.refresh();
                host.set_state(HostState::Uploading { id });
            });
            ok!()
        }

        #[get("/<host>/done/<id>")]
        pub fn done(host: HostId, id: InvocationId, instance: State<Instance>) -> JsonValue {
            instance.host(host, |host| {
                host.refresh();
                host.set_state(HostState::Done { id })
            });
            ok!()
        }
    }
}

#[get("/")]
fn index() -> Template {
    Template::render("index", {})
}

#[get("/hosts")]
fn hosts(instance: State<Instance>) -> JsonValue {
    instance.hosts(|iter| ok!("hosts", iter.collect::<Vec<_>>()))
}

#[get("/current")]
fn current(instance: State<Instance>) -> JsonValue {
    instance
        .current_invocation()
        .map(|id| ok!("id", id))
        .unwrap_or(err!())
}

#[get("/invocation/<id>")]
fn invocation(id: InvocationId, instance: State<Instance>) -> JsonValue {
    instance
        .invocation(id, |invocation| ok!("invocation", invocation))
        .unwrap_or(err!())
}

#[get("/invocations")]
fn invocations(instance: State<Instance>) -> JsonValue {
    instance.invocations(|iter| {
        ok!(
            "invocations",
            iter.map(|invocation| invocation.record())
                .collect::<Vec<_>>()
        )
    })
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
fn not_found(_request: &Request) -> JsonValue {
    err!("page not found")
}

#[catch(500)]
fn internal_error(_request: &Request) -> JsonValue {
    err!("internal server error")
}

fn main() {
    rocket::ignite()
        .manage(Instance::new("experiment/"))
        .register(catchers![internal_error, not_found])
        .mount("/static", StaticFiles::from("static/"))
        .mount("/", routes![index])
        .mount(
            "/api",
            routes![hosts, current, invocation, invocations, invoke, reinvoke],
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
