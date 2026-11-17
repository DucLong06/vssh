pub mod db;
pub mod ssh_config;

use std::collections::HashMap;

use db::Meta;
use ssh_config::SshConfig;

/// One row of a host list: what the ssh config says about an alias, and what
/// the database adds to it. What the config leaves out stays empty, so a
/// frontend can tell "port 22" from "no port given".
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Host {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub meta: Meta,
}

/// Every host of the config, in file order. A host the database has not met
/// has empty metadata.
pub fn hosts(sshc: &SshConfig, metas: &HashMap<String, Meta>) -> Vec<Host> {
    let aliases = sshc.hosts().into_iter();
    aliases
        .map(|alias| Host {
            hostname: sshc.get(&alias, "HostName").map(str::to_string),
            user: sshc.get(&alias, "User").map(str::to_string),
            port: sshc.get(&alias, "Port").and_then(|p| p.parse().ok()),
            meta: metas.get(&alias).cloned().unwrap_or_default(),
            alias,
        })
        .collect()
}
