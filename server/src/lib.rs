//! This is a binary crate meant to supply a working server implementation.
//!
//! Use `cargo install taskchampion-sync-server` to install.
//!
//! This crate is not meant to be used outside of the binaries, and its interface may change without
//! warning. For programmatic server implementations, see
//!
//! * https://crates.io/crates/taskchampion-sync-server-core
//! * https://crates.io/crates/taskchampion-sync-server-storage-sqlite
//! * https://crates.io/crates/taskchampion-sync-server-storage-postgres

#![deny(clippy::all)]

pub mod api;
pub mod args;
pub mod web;
