//! A simple, single-threaded vault.
//!
//! This vault may be useful if you want to re-use existing [`Action`]s and
//! [`Migration`]s but don't need the additional guarantees and overhead of the
//! other vaults.

use rusqlite::Connection;

use crate::{Action, Migration};

/// A simple, single-threaded vault.
///
/// This vault may be useful if you want to re-use existing [`Action`]s and
/// [`Migration`]s but don't need the additional guarantees and overhead of the
/// other vaults.
#[derive(Debug)]
pub struct SimpleVault(Connection);

impl SimpleVault {
    /// Create a new vault from an existing [`Connection`], applying the
    /// migrations in the process.
    ///
    /// It is recommended to set a few pragmas before calling this function, for
    /// example:
    /// - `journal_mode` to `"wal"`
    /// - `foreign_keys` to `true`
    /// - `trusted_schema` to `false`
    pub fn new(conn: Connection, migrations: &[Migration]) -> rusqlite::Result<Self> {
        Self::new_and_prepare(conn, migrations, |_| Ok(()))
    }

    /// Create a new vault from an existing [`Connection`], applying the
    /// migrations in the process.
    ///
    /// The `prepare` parameter allows access to the database after all
    /// migrations have occurred. This parameter could be replaced by executing
    ///
    /// It is recommended to set a few pragmas before calling this function, for
    /// example:
    /// - `journal_mode` to `"wal"`
    /// - `foreign_keys` to `true`
    /// - `trusted_schema` to `false`
    /// an [`Action`] performing the same operations.
    pub fn new_and_prepare(
        mut conn: Connection,
        migrations: &[Migration],
        prepare: impl FnOnce(&mut Connection) -> rusqlite::Result<()>,
    ) -> rusqlite::Result<Self> {
        crate::migrate(&mut conn, migrations)?;
        prepare(&mut conn)?;
        Ok(Self(conn))
    }

    /// Execute an [`Action`] and return the result.
    pub fn execute<A: Action>(&mut self, action: A) -> Result<A::Result, A::Error> {
        action.run(&mut self.0)
    }
}
