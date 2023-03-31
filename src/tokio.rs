//! A vault for use with [`tokio`].

use std::{any::Any, error, fmt, thread};

use rusqlite::Connection;
use tokio::sync::{mpsc, oneshot};

use crate::{Action, Migration};

/// Wrapper trait around [`Action`] that turns `Box<Self>` into a `Self` and the
/// action's return type into `Box<dyn Any + Send>`.
///
/// This way, the trait that users of this crate interact with is kept simpler.
trait ActionWrapper {
    fn run(
        self: Box<Self>,
        conn: &mut Connection,
    ) -> Result<Box<dyn Any + Send>, Box<dyn Any + Send>>;
}

impl<T: Action> ActionWrapper for T
where
    T::Output: Send + 'static,
    T::Error: Send + 'static,
{
    fn run(
        self: Box<Self>,
        conn: &mut Connection,
    ) -> Result<Box<dyn Any + Send>, Box<dyn Any + Send>> {
        match (*self).run(conn) {
            Ok(result) => Ok(Box::new(result)),
            Err(err) => Err(Box::new(err)),
        }
    }
}

/// Command to be sent via the mpsc channel to the vault thread.
enum Command {
    Action(
        Box<dyn ActionWrapper + Send>,
        oneshot::Sender<Result<Box<dyn Any + Send>, Box<dyn Any + Send>>>,
    ),
    Stop(oneshot::Sender<()>),
}

/// Error that can occur during execution of an [`Action`].
#[derive(Debug)]
pub enum Error<E> {
    /// The vault's thread has been stopped and its sqlite connection closed.
    Stopped,
    /// An error was returned by the [`Action`].
    Action(E),
}

impl<E: fmt::Display> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => "vault has been stopped".fmt(f),
            Self::Action(err) => err.fmt(f),
        }
    }
}

impl<E: error::Error> error::Error for Error<E> {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Stopped => None,
            Self::Action(err) => err.source(),
        }
    }
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

/// A vault for use with [`tokio`].
#[derive(Debug, Clone)]
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
        crate::migrate(&mut conn, migrations)?;
        prepare(&mut conn)?;

        let (tx, rx) = mpsc::unbounded_channel();
        thread::spawn(move || run(conn, rx));
        Ok(Self { tx })
    }

    /// Execute an [`Action`] and return the result.
    pub async fn execute<A>(&self, action: A) -> Result<A::Output, Error<A::Error>>
    where
        A: Action + Send + 'static,
        A::Output: Send,
        A::Error: Send,
    {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Action(Box::new(action), tx))
            .map_err(|_| Error::Stopped)?;

        let result = rx.await.map_err(|_| Error::Stopped)?;

        // The ActionWrapper runs Action::run, which returns
        // Result<Action::Result, Action::Error>. It then wraps the
        // Action::Result and Action::Error into Any, which we're now trying to
        // downcast again to Action::Result and Action::Error. This should
        // always work.
        match result {
            Ok(result) => {
                let result = *result.downcast::<A::Output>().unwrap();
                Ok(result)
            }
            Err(err) => {
                let err = *err.downcast::<A::Error>().unwrap();
                Err(Error::Action(err))
            }
        }
    }

    /// Stop the vault's thread and close its sqlite connection.
    ///
    /// Returns once the vault has been stopped.
    pub async fn stop(&self) {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(Command::Stop(tx));
        let _ = rx.await;
    }
}
