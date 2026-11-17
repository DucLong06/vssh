//! Telnet and serial are stubs: such hosts are recognised by a URL scheme in
//! `HostName` (`telnet://host`, `serial:///dev/ttyUSB0`) and refused with a
//! clear message. Only SSH is implemented. See docs/PLAN.md.

use anyhow::{bail, Result};

pub fn ensure_ssh(hostname: &str) -> Result<()> {
    for (scheme, name) in [("telnet://", "Telnet"), ("serial://", "Serial")] {
        if hostname.starts_with(scheme) {
            bail!("{name} is not implemented yet");
        }
    }
    Ok(())
}
