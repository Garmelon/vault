#[cfg(feature = "tokio")]
pub mod tokio;

use rusqlite::Connection;

pub trait Action {
    type Result;
    fn run(self, conn: &mut Connection) -> rusqlite::Result<Self::Result>;
}
