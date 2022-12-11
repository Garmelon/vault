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

pub trait Action {
    type Result;
    fn run(self, conn: &mut Connection) -> rusqlite::Result<Self::Result>;
}
