//! `vssh-desktop --version`: what a package check asks a machine that has no
//! display, so it must answer before the window is thought of.
use std::process::Command;

#[test]
fn version_is_printed_without_a_display() {
    let out = Command::new(env!("CARGO_BIN_EXE_vssh-desktop"))
        .arg("--version")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("the app could not be started");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("vssh-desktop {}\n", env!("CARGO_PKG_VERSION"))
    );
}
