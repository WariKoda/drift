//! Protocol- and UI-independent drift policies.
pub mod config;
pub mod diff;
pub mod error;
pub mod local;
pub mod pathmap;
pub mod project;
pub mod staging;
pub mod store;

mod ftp;
pub mod remote;
mod sftp;

pub mod tlstrust;
