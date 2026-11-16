//! Runs the shell test of the installer's language selection.

use std::process::Command;

#[test]
fn installer_picks_its_language() {
    let out = Command::new("sh")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/install_lang.sh"
        ))
        .output()
        .expect("sh is available");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("language selection: ok"), "{text}");
}
