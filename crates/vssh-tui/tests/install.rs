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

/// `sh install.sh` in a checkout builds that checkout. It tells one by the
/// manifest of the `vssh` package, wherever the workspace keeps it.
#[test]
fn installer_recognises_this_checkout() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let script = std::fs::read_to_string(format!("{root}/install.sh")).unwrap();
    let probe = script
        .lines()
        .find(|l| l.contains(r#"grep -qs '^name = "vssh"$'"#))
        .expect("install.sh looks for the vssh manifest");
    let manifest = probe.split("$dir/").nth(1).unwrap().split('"').next();
    let manifest = std::fs::read_to_string(format!("{root}/{}", manifest.unwrap())).unwrap();
    assert!(manifest.lines().any(|l| l == r#"name = "vssh""#), "{probe}");
}
