use std::{any::Any, result, thread};

use rusqlite::{Connection, Transaction};
use tokio::sync::{mpsc, oneshot};

use crate::Action;

/// Wrapper trait around [`Action`] that turns `Box<Self>` into a `Self` and the
/// action's return type into `Box<dyn Any + Send>`.
///
/// This way, the trait that users of this crate interact with is kept simpler.
trait ActionWrapper {
    fn run(self: Box<Self>, conn: &mut Connection) -> rusqlite::Result<Box<dyn Any + Send>>;
}

impl<T: Action> ActionWrapper for T
where
    T::Result: Send + 'static,
{
    fn run(self: Box<Self>, conn: &mut Connection) -> rusqlite::Result<Box<dyn Any + Send>> {
        let result = (*self).run(conn)?;
        Ok(Box::new(result))
    }
}

/// Command to be sent via the mpsc channel to the vault thread.
enum Command {
    Action(
        Box<dyn ActionWrapper + Send>,
        oneshot::Sender<rusqlite::Result<Box<dyn Any + Send>>>,
    ),
    Stop(oneshot::Sender<()>),
}

/// Error that can occur during execution of an [`Action`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The vault thread has stopped.
    #[error("vault thread has stopped")]
    Stopped,

    /// A [`rusqlite::Error`] occurred while running the action.
    #[error("{0}")]
    Rusqlite(#[from] rusqlite::Error),
}

pub type Result<R> = result::Result<R, Error>;

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

fn run(mut conn: Connection, mut rx: mpsc::UnboundedReceiver<Command>) {
    while let Some(command) = rx.blocking_recv() {
        match command {
            Command::Action(action, tx) => {
                let result = action.run(&mut conn);
                let _ = tx.send(result);
            }
            Command::Stop(tx) => {
                drop(conn);
                drop(tx);
                break;
            }
        }
    }
}

#[derive(Clone)]
pub struct TokioVault {
    tx: mpsc::UnboundedSender<Command>,
}

impl TokioVault {
    /// Launch a new thread to run database queries on, and return a
    /// [`TokioVault`] for communication with that thread.
    ///
    /// It is recommended to set a few pragmas before calling this function, for
    /// example:
    /// - `journal_mode` to `"wal"`
    /// - `foreign_keys` to `true`
    /// - `trusted_schema` to `false`
    pub fn launch(conn: Connection, migrations: &[Migration]) -> rusqlite::Result<Self> {
        Self::launch_and_prepare(conn, migrations, |_| Ok(()))
    }

    /// Launch a new thread to run database queries on, and return a
    /// [`TokioVault`] for communication with that thread.
    ///
    /// The `prepare` parameter allows access to the database before a new
    /// thread is launched but after all migrations have occurred. This can be
    /// used for things that need to run after the migrations, yet whose failure
    /// will prevent the database connection from being usable. An example would
    /// be creating temporary tables based on existing data.
    ///
    /// It is recommended to set a few pragmas before calling this function, for
    /// example:
    /// - `journal_mode` to `"wal"`
    /// - `foreign_keys` to `true`
    /// - `trusted_schema` to `false`
    pub fn launch_and_prepare(
        mut conn: Connection,
        migrations: &[Migration],
        prepare: impl FnOnce(&mut Connection) -> rusqlite::Result<()>,
    ) -> rusqlite::Result<Self> {
        migrate(&mut conn, migrations)?;
        prepare(&mut conn)?;

        let (tx, rx) = mpsc::unbounded_channel();
        thread::spawn(move || run(conn, rx));
        Ok(Self { tx })
    }

    /// Execute an [`Action`] and return the result.
    pub async fn execute<A>(&self, action: A) -> Result<A::Result>
    where
        A: Action + Send + 'static,
        A::Result: Send,
    {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Action(Box::new(action), tx))
            .map_err(|_| Error::Stopped)?;

        let result = rx.await.map_err(|_| Error::Stopped)??;

        // The ActionWrapper runs Action::run, which returns Action::Result. It
        // then wraps this into Any, which we're now trying to downcast again to
        // Action::Result. This should always work.
        let result = result.downcast().unwrap();

        Ok(*result)
    }

    /// Stop the vault thread.
    ///
    /// Returns when the vault has been stopped successfully.
    pub async fn stop(&self) {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(Command::Stop(tx));
        let _ = rx.await;
    }
}
