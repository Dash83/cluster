#[macro_use]
extern crate lazy_static;

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{Config, Cred, FetchOptions, ObjectType, Oid, RemoteCallbacks, Repository, ResetType};

use std::fs;
use std::path::Path;

pub mod descriptor;
pub mod host;
pub mod invocation;

pub fn clone<P: AsRef<Path>>(url: &str, path: P) -> Result<Repository, git2::Error> {
    fs::remove_dir_all(&path).unwrap_or(());
    let mut remote_callbacks = RemoteCallbacks::new();
    remote_callbacks.credentials(|_, _, _| {
        Config::find_global()
            .and_then(|path| Config::open(&path))
            .and_then(|config| Cred::credential_helper(&config, "https://github.com/", None))
    });
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(remote_callbacks);
    RepoBuilder::new()
        .fetch_options(fetch_options)
        .clone(url, path.as_ref())
}

pub fn rewind(repo: &Repository, commit: &str) -> Result<(), git2::Error> {
    let object = commit
        .parse::<Oid>()
        .and_then(|oid| repo.find_object(oid, Some(ObjectType::Commit)))?;
    let mut checkout = CheckoutBuilder::new();
    repo.reset(&object, ResetType::Hard, Some(checkout.force()))
}
