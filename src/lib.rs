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

pub mod simple;
#[cfg(feature = "tokio")]
pub mod tokio;

use rusqlite::{Connection, Transaction};

/// An action that can be performed on a [`Connection`].
///
/// Both commands and queries are considered actions. Commands usually have a
/// return type of `()`, while queries return the result of the query.
///
/// Actions are usually passed to a vault which will then execute them and
/// return the result. The way in which this occurs depends on the vault.
pub trait Action {
    type Output;
    type Error;
    fn run(self, conn: &mut Connection) -> Result<Self::Output, Self::Error>;
}

/// A single database migration.
///
/// It receives a [`Transaction`] to perform database operations in, its index
/// in the migration array and the size of the migration array. The latter two
/// might be useful for logging.
///
/// The transaction spans all migrations currently being performed. If any
/// single migration fails, all migrations are rolled back and the database is
/// unchanged.
///
/// The migration does not need to update the `user_version` or commit the
/// transaction.
pub type Migration = fn(&mut Transaction<'_>, usize, usize) -> rusqlite::Result<()>;

fn migrate(conn: &mut Connection, migrations: &[Migration]) -> rusqlite::Result<()> {
    let mut tx = conn.transaction()?;

    let user_version: usize =
        tx.query_row("SELECT * FROM pragma_user_version", [], |r| r.get(0))?;

    let total = migrations.len();
    assert!(user_version <= total, "malformed database schema");
    for (i, migration) in migrations.iter().enumerate().skip(user_version) {
        migration(&mut tx, i, total)?;
    }

    tx.pragma_update(None, "user_version", total)?;
    tx.commit()
}
