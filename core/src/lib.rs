//! This crate implements the core logic of the taskchampion sync protocol.
//!
//! This should be considered a reference implementation, with [the protocol
//! documentation](https://gothenburgbitfactory.org/taskchampion/sync-protocol.html). representing
//! the authoritative definition of the protocol. Other implementations are encouraged.
//!
//! This crate uses an abstract storage backend. Note that this does not implement the
//! HTTP-specific portions of the protocol, nor provide any storage implementations.
//!
//! ## Usage
//!
//! To use, create a new [`Server`] instance and call the relevant protocol API methods. The
//! arguments and return values correspond closely to the protocol documentation.

mod error;
mod inmemory;
mod server;
mod storage;

pub use error::*;
pub use inmemory::*;
pub use server::*;
pub use storage::*;
