#![forbid(unsafe_code)]
// Rustc lint groups
#![warn(future_incompatible)]
#![warn(rust_2018_idioms)]
#![warn(unused)]
// Rustc lints
#![warn(noop_method_call)]
#![warn(single_use_lifetimes)]
// Clippy lints
#![warn(clippy::use_self)]

#[cfg(feature = "tokio")]
pub mod tokio;

use rusqlite::Connection;

/// An action that can be performed on a [`Connection`].
///
/// Both commands and queries are considered actions. Commands usually have a
/// return type of `()`, while queries return the result of the query.
///
/// Actions are usually passed to a vault which will then execute them and
/// return the result. The way in which this occurs depends on the vault.
pub trait Action {
    type Result;
    fn run(self, conn: &mut Connection) -> rusqlite::Result<Self::Result>;
}
