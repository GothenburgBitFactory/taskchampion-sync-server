//! This crate implements the core logic of the taskchampion sync protocol.
//!
//! This should be considered a reference implementation, with [the protocol
//! documentation](https://gothenburgbitfactory.org/taskchampion/sync-protocol.html). representing
//! the authoritative definition of the protocol. Other implementations are encouraged.
//!
//! This crate uses an abstract storage backend. Note that this does not implement the
//! HTTP-specific portions of the protocol, nor provide any storage implementations.
//!
//! ## API Methods
//!
//! The following API methods are implemented. These methods are documented in more detail in
//! the protocol documentation.
//!
//! * [`add_version`]
//! * [`get_child_version`]
//! * [`add_snapshot`]
//! * [`get_snapshot`]
//!
//! Each API method takes:
//!
//! * [`StorageTxn`] to access storage. Methods which modify storage will commit the transaction before returning.
//! * [`ServerConfig`] providing basic configuration for the server's behavior.
//! * `client_id` and a [`Client`] providing the client metadata.

mod inmemory;
mod server;
mod storage;

pub use inmemory::*;
pub use server::*;
pub use storage::*;
