//! The update (D15), as far as it goes with no network and no root: what the
//! window may ask for. The download, the password dialog and the restart are
//! `scripts/gui-smoke.sh`'s, on the real app.

use serde_json::json;

use super::{call, home, other, window, Fake};

#[test]
fn the_window_names_a_version_and_cannot_make_the_app_fetch_or_install_anything_else() {
    let home = home("update-tag");
    let (_app, window) = window(&home, Fake::silent());
    for tag in [
        "",
        "latest",
        "v1.0/../../../evil",
        "v1.0 --post-file=/etc/passwd",
        "https://example.com/x.deb",
    ] {
        let refused = Err(other(&format!("{tag:?} is not a version")));
        assert_eq!(
            call(&window, "update_download", json!({ "tag": tag })),
            refused
        );
        assert_eq!(
            call(&window, "update_install", json!({ "tag": tag })),
            refused
        );
    }
    // Refused before anything was touched or run.
    assert!(!home.join("data/update").exists());
}

#[test]
fn nothing_is_installed_that_was_not_fetched_and_found_whole() {
    let home = home("update-install");
    let (_app, window) = window(&home, Fake::silent());
    let deb = home.join("data/update/vssh-desktop_0.3.0_amd64.deb");
    let install = || call(&window, "update_install", json!({ "tag": "v0.3.0" }));
    // No download before it: there is no file.
    let missing = format!("{} is not there, or empty", deb.display());
    assert_eq!(install(), Err(other(&missing)));
    // A file by that name that is no package: dpkg-deb says so, and pkexec is not come to.
    std::fs::create_dir_all(deb.parent().unwrap()).unwrap();
    std::fs::write(&deb, "<html>404</html>").unwrap();
    let why = install().unwrap_err();
    let why = why["chain"][0].as_str().unwrap();
    assert!(
        why.starts_with(&format!("dpkg-deb cannot read {}", deb.display())),
        "{why}"
    );
    assert!(deb.exists());
}

#[test]
fn a_build_that_is_no_package_of_this_machine_is_not_offered_the_update() {
    let home = home("update-possible");
    let (_app, window) = window(&home, Fake::silent());
    // The test binary under target/ belongs to no .deb.
    assert_eq!(
        call(&window, "update_possible", json!({})),
        Ok(json!(false))
    );
}
