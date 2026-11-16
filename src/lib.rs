//! vssh: a terminal SSH connection manager. The binary is a thin wrapper;
//! the modules live here so integration tests can reach them.

pub mod config;
pub mod proto;
pub mod sftp;
pub mod ssh;
pub mod store;
pub mod ui;
pub mod vault;
