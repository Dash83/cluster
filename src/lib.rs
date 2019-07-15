#[macro_use]
extern crate lazy_static;

use git2::build::CheckoutBuilder;
use git2::{ObjectType, Oid, Repository, ResetType};

pub mod descriptor;
pub mod host;
pub mod invocation;

pub fn rewind(repo: &Repository, commit: &str) -> Result<(), git2::Error> {
    let object = commit
        .parse::<Oid>()
        .and_then(|oid| repo.find_object(oid, Some(ObjectType::Commit)))?;
    let mut checkout = CheckoutBuilder::new();
    repo.reset(&object, ResetType::Hard, Some(checkout.force()))
}
