//! A file of this machine, chosen in the desktop's own file dialog (D14):
//! a key for a host or for a jump host, another ssh config to import.

use std::path::{Path, PathBuf};

use tauri::{Runtime, WebviewWindow};
use tokio::sync::oneshot;
use vssh_core::config::home;

use crate::Reply;

/// A path as a person would type it: what is under the home starts with `~`.
fn short(path: &Path) -> String {
    let home = home();
    match path.strip_prefix(&home) {
        // What goes into an ssh config is written with `/` on Windows too.
        Ok(rest) if !home.as_os_str().is_empty() && cfg!(windows) => {
            format!("~/{}", rest.display()).replace('\\', "/")
        }
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

/// The key a chosen file stands for: the public half names the private one
/// next to it, which is the one a login reads.
fn private(path: &str) -> &str {
    path.strip_suffix(".pub").unwrap_or(path)
}

/// What a key is called most of the time; anything else is under "all files".
#[cfg(any(target_os = "linux", test))]
const KEYS: &[&str] = &["id_*", "*.pub", "*.pem", "*.key"];

/// Opens the dialog over the window, in `~/.ssh`, and answers once it is
/// closed: the file, or nothing. On GTK's own thread.
#[cfg(target_os = "linux")]
fn choose<R: Runtime>(
    window: &WebviewWindow<R>,
    title: &str,
    filters: Option<(String, String)>,
    answer: oneshot::Sender<Option<PathBuf>>,
) {
    use std::cell::RefCell;

    use gtk::prelude::*;
    use gtk::{FileChooserAction, FileChooserNative, FileFilter, ResponseType};

    let parent = window.gtk_window().ok();
    let action = FileChooserAction::Open;
    let dialog = FileChooserNative::new(Some(title), parent.as_ref(), action, None, None);
    let _ = dialog.set_current_folder(home().join(".ssh"));
    if let Some((keys, all)) = filters {
        for (name, patterns) in [(keys, KEYS), (all, &["*"][..])] {
            let filter = FileFilter::new();
            filter.set_name(Some(&name));
            patterns
                .iter()
                .for_each(|pattern| filter.add_pattern(pattern));
            dialog.add_filter(filter);
        }
    }
    // Nothing else holds a dialog that is no widget of the window: it is kept
    // by its own answer, and let go with it.
    let kept = RefCell::new(Some((dialog.clone(), answer)));
    dialog.connect_response(move |_, response| {
        if let Some((dialog, answer)) = kept.take() {
            let chosen = (response == ResponseType::Accept).then(|| dialog.filename());
            let _ = answer.send(chosen.flatten());
            dialog.destroy();
        }
    });
    dialog.show();
}

/// The same on Windows and macOS, in the system's own dialog.
// shortcut: that dialog filters by a file's ending, so `id_*` cannot be said: a key with no ending is chosen by its `.pub` (which answers the key), or under "all files".
#[cfg(not(target_os = "linux"))]
fn choose<R: Runtime>(
    window: &WebviewWindow<R>,
    title: &str,
    filters: Option<(String, String)>,
    answer: oneshot::Sender<Option<PathBuf>>,
) {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = window.dialog().file().set_parent(window).set_title(title);
    dialog = dialog.set_directory(home().join(".ssh"));
    if let Some((keys, all)) = filters {
        dialog = dialog.add_filter(keys, &["pub", "pem", "key"]);
        dialog = dialog.add_filter(all, &["*"]);
    }
    dialog.pick_file(move |chosen| {
        let _ = answer.send(chosen.and_then(|file| file.into_path().ok()));
    });
}

/// Asks for a file in the desktop's own dialog and answers its path, `~`
/// for the home; `None` if the dialog was closed without one. `keys`: the
/// file is an SSH key, and these are the names of the dialog's two filters,
/// keys and all files; the public half of a key answers its private one.
#[tauri::command]
pub async fn pick_file<R: Runtime>(
    window: WebviewWindow<R>,
    title: String,
    keys: Option<(String, String)>,
) -> Reply<Option<String>> {
    let (answer, chosen) = oneshot::channel();
    let is_key = keys.is_some();
    let over = window.clone();
    window
        .run_on_main_thread(move || choose(&over, &title, keys, answer))
        .map_err(anyhow::Error::from)?;
    let path = chosen.await.ok().flatten().map(|path| short(&path));
    Ok(path.map(|path| match is_key {
        true => private(&path).to_string(),
        false => path,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_path_is_written_as_a_person_would() {
        std::env::set_var("HOME", "/home/long");
        assert_eq!(
            short(Path::new("/home/long/.ssh/id_ed25519")),
            "~/.ssh/id_ed25519"
        );
        // Another user's home is not this one's, whatever it starts with.
        assert_eq!(
            short(Path::new("/home/longer/.ssh/key")),
            "/home/longer/.ssh/key"
        );
        assert_eq!(short(Path::new("/etc/ssh/key")), "/etc/ssh/key");
        // The public half stands for the key; a name that only has "pub" in it is itself.
        assert_eq!(private("~/.ssh/id_ed25519.pub"), "~/.ssh/id_ed25519");
        assert_eq!(private("~/.ssh/pub.key"), "~/.ssh/pub.key");
        // Every pattern is one GTK reads as a glob, and `id_*` is among them.
        assert!(KEYS.contains(&"id_*") && KEYS.contains(&"*.pub"));
        // No home, no `~`.
        std::env::set_var("HOME", "");
        assert_eq!(short(Path::new("/etc/ssh/key")), "/etc/ssh/key");
    }
}
