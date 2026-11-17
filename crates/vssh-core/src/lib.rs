//! vssh without a user interface: connections, SFTP, the vault and the
//! stores. The TUI and the desktop app are both frontends of this crate, so
//! nothing here may depend on a terminal UI library
//! (`scripts/check-core-deps.sh` holds that line).

pub mod config;
pub mod conn_err;
pub mod proto;
pub mod sftp;
pub mod ssh;
pub mod store;
pub mod vault;
