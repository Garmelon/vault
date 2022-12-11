use rusqlite::Connection;

pub trait DbExecute {
    fn run(self, conn: &mut Connection) -> rusqlite::Result<()>;
}

pub trait DbQuery {
    type Result;
    fn run(self, conn: &mut Connection) -> rusqlite::Result<Self::Result>;
}
