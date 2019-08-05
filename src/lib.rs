#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{Config, Cred, FetchOptions, ObjectType, Oid, RemoteCallbacks, Repository, ResetType};

use std::fs;
use std::path::Path;

pub mod descriptor;
pub mod host;
pub mod invocation;

pub fn clone<P: AsRef<Path>>(url: &str, path: P) -> Result<Repository, git2::Error> {
    info!("cloning {}", url);
    fs::remove_dir_all(&path).unwrap_or(());
    let mut remote_callbacks = RemoteCallbacks::new();
    remote_callbacks.credentials(|_, _, _| {
        Config::find_global()
            .and_then(|path| Config::open(&path))
            .and_then(|config| Cred::credential_helper(&config, "https://github.com/", None))
    });
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(remote_callbacks);
    let repo = RepoBuilder::new()
        .fetch_options(fetch_options)
        .clone(url, path.as_ref())?;
    info!("cloned {}", url);
    Ok(repo)
}

pub fn rewind(repo: &Repository, commit: &str) -> Result<(), git2::Error> {
    info!("fetching origin/master...");
    match repo.find_remote("origin") {
        // To update, fetch and reset --hard
        Ok(mut remote) => match remote.fetch(&["refs/heads/*:refs/heads/*"], None, None)
            .and_then(|_| repo.head())
            .map(|head_ref| head_ref.target().unwrap())
            .and_then(|head| repo.find_object(head, None))
            .and_then(|obj| repo.reset(&obj, ResetType::Hard, None))
        {
            Ok(_) => info!("fetched origin/master"),
            _ => warn!("failed to reset to FETCH_HEAD"),
        },
        _ => info!("failed to fetch origin/master"),
    }
    // Find the commit we want to rewind to
    let object = commit
        .parse::<Oid>()
        .and_then(|oid| repo.find_object(oid, Some(ObjectType::Commit)))?;
    let mut checkout = CheckoutBuilder::new();
    // Reset hard
    repo.reset(&object, ResetType::Hard, Some(checkout.force()))?;
    info!("jumped to commit {}", commit);
    Ok(())
}
