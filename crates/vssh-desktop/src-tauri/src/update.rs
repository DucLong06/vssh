//! A newer version, fetched and installed (D15): only when a person asks
//! for it. The release's `.deb` comes from GitHub with `wget`, is read by
//! `dpkg-deb`, and is installed by `dpkg` under `pkexec`, which asks for the
//! password in the desktop's own dialog. The window names a version and
//! nothing else: the address, the file and the command line are made here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{anyhow, ensure, Context, Result};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::{Backend, Reply};

/// Where a release's files are.
const DOWNLOADS: &str = "https://github.com/DucLong06/vssh/releases/download";

/// The app's package, as `scripts/desktop-deb.sh` and the release name it.
const PACKAGE: &str = "vssh-desktop";

/// The version a tag names: numbers and dots, with a `v` in front or
/// without. Nothing else gets into an address, a path or a command line.
fn version(tag: &str) -> Result<&str> {
    let ver = tag.strip_prefix('v').unwrap_or(tag);
    let number = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    ensure!(ver.split('.').all(number), "{tag:?} is not a version");
    Ok(ver)
}

/// The release's package for this app: the one a release has, for amd64.
fn deb(ver: &str) -> String {
    format!("{PACKAGE}_{ver}_amd64.deb")
}

/// Where that package is kept on this machine: under the app's own
/// directory, which no other user writes to.
fn kept(backend: &Backend, ver: &str) -> Result<PathBuf> {
    Ok(backend.data_dir()?.join("update").join(deb(ver)))
}

/// What a tool said on its way out.
fn said(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr).trim().to_string()
}

/// The file is a package of this app, in that version: `dpkg-deb` reads it
/// and says so. An empty file, half a file or another package is refused.
fn verify(file: &Path, ver: &str) -> Result<()> {
    let size = fs::metadata(file).map_or(0, |file| file.len());
    ensure!(size > 0, "{} is not there, or empty", file.display());
    let fields = Command::new("dpkg-deb")
        .arg("-f")
        .arg(file)
        .args(["Package", "Version"])
        .output()
        .context("cannot run dpkg-deb")?;
    ensure!(
        fields.status.success(),
        "dpkg-deb cannot read {}: {}",
        file.display(),
        said(&fields.stderr)
    );
    let fields = String::from_utf8_lossy(&fields.stdout);
    let has = |line: String| fields.lines().any(|field| field == line);
    ensure!(
        has(format!("Package: {PACKAGE}")) && has(format!("Version: {ver}")),
        "{} is not {PACKAGE} {ver}",
        file.display()
    );
    Ok(())
}

/// Whether the app that runs is the `.deb`'s, on the architecture a release
/// has one for: only then is a newer `.deb` an upgrade, and the app that
/// starts again the new one. An AppImage, an `.rpm` and a build of one's own
/// are left to the releases page.
#[tauri::command(async)]
pub fn update_possible() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let owner = Command::new("dpkg-query")
        .arg("-S")
        .arg(exe)
        .stderr(Stdio::null())
        .output();
    let ours = format!("{PACKAGE}: ");
    cfg!(target_arch = "x86_64")
        && owner.is_ok_and(|out| out.status.success() && out.stdout.starts_with(ours.as_bytes()))
}

fn download<R: Runtime>(app: &AppHandle<R>, backend: &Backend, tag: &str) -> Result<()> {
    let ver = version(tag)?;
    let file = kept(backend, ver)?;
    let dir = file.parent().context("the package has no folder")?;
    // An older one, or half of this one, is not kept.
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir)?;
    // wget reads the proxy from the lower-case names only.
    let proxy = ["https_proxy", "no_proxy"]
        .into_iter()
        .filter(|name| std::env::var_os(name).is_none())
        .filter_map(|name| Some((name, std::env::var_os(name.to_uppercase())?)));
    let mut wget = Command::new("wget")
        .args(["-nv", "--tries=2", "--timeout=30", "-O"])
        .arg(&file)
        .arg(format!("{DOWNLOADS}/{tag}/{}", deb(ver)))
        .envs(proxy)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run wget")?;
    // How far it is, is how big the file is by now.
    let status = loop {
        if let Some(status) = wget.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(200));
        let here = fs::metadata(&file).map_or(0, |file| file.len());
        let _ = app.emit("update-progress", here);
    };
    let wget = wget.wait_with_output()?;
    ensure!(
        status.success(),
        "wget could not fetch it: {}",
        said(&wget.stderr)
    );
    verify(&file, ver)
}

/// Fetches the `.deb` of the release with that tag, through the proxy the
/// environment names if it names one, and holds it to be this app's package
/// in that version. `update-progress` carries the bytes that are here so far.
#[tauri::command(async)]
pub fn update_download<R: Runtime>(
    app: AppHandle<R>,
    tag: String,
    backend: State<'_, Backend>,
) -> Reply<()> {
    Ok(download(&app, &backend, &tag)?)
}

fn install(backend: &Backend, tag: &str) -> Result<Option<PathBuf>> {
    let ver = version(tag)?;
    let file = kept(backend, ver)?;
    // Read again: this is what root is about to install.
    verify(&file, ver)?;
    // shortcut: dpkg installs no dependency, so a release that needs a library this machine lacks is left unconfigured, with dpkg's words as the error; `apt-get install <file>` under pkexec once a release adds one.
    // pkexec gives what it runs a bare environment: `env` puts the one word back.
    let dpkg = Command::new("pkexec")
        .args(["env", "DEBIAN_FRONTEND=noninteractive", "dpkg", "-i"])
        .arg(&file)
        .stdin(Stdio::null())
        .output();
    match dpkg {
        Ok(done) if done.status.success() => {
            let _ = fs::remove_file(&file);
            Ok(None)
        }
        // 126 and 127 are pkexec's own: the answer was no, or there was
        // nobody to ask through. Anything else is dpkg's.
        Ok(done) if !matches!(done.status.code(), Some(126 | 127)) => {
            Err(anyhow!("{}", said(&done.stderr)).context("dpkg could not install the package"))
        }
        _ => {
            if let Some(dir) = file.parent() {
                let _ = Command::new("xdg-open").arg(dir).status();
            }
            Ok(Some(file))
        }
    }
}

/// Installs what `update_download` fetched: `dpkg -i` under `pkexec`, which
/// asks for the password in the desktop's dialog. Answers nothing once it is
/// installed. Where pkexec is missing, has nobody to ask through or was told
/// no, nothing is installed: the answer is the file, for `sudo apt install`
/// by hand, and its folder is opened.
#[tauri::command(async)]
pub fn update_install(tag: String, backend: State<'_, Backend>) -> Reply<Option<PathBuf>> {
    Ok(install(&backend, &tag)?)
}

/// Ends the app and starts it again: the binary the package just put there.
#[tauri::command]
pub fn update_restart<R: Runtime>(app: AppHandle<R>) {
    app.request_restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_numbers_and_dots_and_nothing_that_could_leave_the_address() {
        assert_eq!(version("v0.3.0").unwrap(), "0.3.0");
        assert_eq!(version("0.10.12").unwrap(), "0.10.12");
        assert_eq!(deb("0.3.0"), "vssh-desktop_0.3.0_amd64.deb");
        let wrong = [
            "",
            "v",
            "v1.",
            ".1",
            "1..2",
            "vv1.0",
            "v1.0-rc1",
            "1.0/../../x",
            "1.0 -O /etc/x",
            "١.٢",
        ];
        for tag in wrong {
            assert!(version(tag).is_err(), "{tag:?}");
        }
    }

    /// A package as `scripts/desktop-deb.sh` builds one, with nothing in it.
    fn package(dir: &Path, name: &str, ver: &str) -> PathBuf {
        let root = dir.join(format!("{name}-{ver}"));
        fs::create_dir_all(root.join("DEBIAN")).unwrap();
        let control = format!(
            "Package: {name}\nVersion: {ver}\nArchitecture: all\nMaintainer: t <t@example.com>\nDescription: t\n"
        );
        fs::write(root.join("DEBIAN/control"), control).unwrap();
        let file = dir.join(format!("{name}_{ver}.deb"));
        let built = Command::new("dpkg-deb")
            .args(["--build", "--root-owner-group"])
            .args([&root, &file])
            .output()
            .expect("dpkg-deb is not installed");
        assert!(built.status.success(), "{}", said(&built.stderr));
        file
    }

    #[test]
    fn only_this_apps_package_in_the_version_asked_for_is_let_through() {
        let dir = std::env::temp_dir().join(format!("vssh-update-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let ours = package(&dir, PACKAGE, "0.3.0");
        verify(&ours, "0.3.0").unwrap();

        // Another version than the one asked for; another package.
        let why = verify(&ours, "0.3.1").unwrap_err().to_string();
        assert!(why.ends_with("is not vssh-desktop 0.3.1"), "{why}");
        let other = package(&dir, "other", "0.3.0");
        assert!(verify(&other, "0.3.0").is_err());

        // Half a download, an empty one, none at all.
        let whole = fs::read(&ours).unwrap();
        let half = dir.join("half.deb");
        fs::write(&half, &whole[..whole.len() / 4]).unwrap();
        let why = verify(&half, "0.3.0").unwrap_err().to_string();
        assert!(why.starts_with("dpkg-deb cannot read"), "{why}");
        let empty = dir.join("empty.deb");
        fs::write(&empty, "").unwrap();
        let why = verify(&empty, "0.3.0").unwrap_err().to_string();
        assert!(why.ends_with("is not there, or empty"), "{why}");
        assert!(verify(&dir.join("none.deb"), "0.3.0").is_err());

        fs::remove_dir_all(&dir).unwrap();
    }
}
