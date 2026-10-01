//! Protocol- and UI-independent drift policies.
pub mod config;
pub mod diff;
pub mod error;
pub mod local;
pub mod pathmap;
pub mod project;
pub mod staging;
pub mod store;

pub mod remote;
mod sftp;
