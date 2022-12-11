# Vault

A Rust library for interacting with a SQLite database from multiple threads.

Because single SQLite connections aren't built to be threadsafe, the easiest way
to interact with a SQLite database from multiple threads or async tasks is to
spawn a separate thread for the connection. This thread receives commands or
requests via a channel and returns the results to the requester. This way, all
operations done on the database are automatically serialized.
