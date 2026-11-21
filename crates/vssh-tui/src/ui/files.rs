//! State of one SFTP tab: local pane (index 0) and remote pane (index 1).

use std::path::Path;
use std::{fs, io};

use vssh_core::sftp::{list_local, Cmd, Entry, Reply};
use vssh_core::task::Sender;

pub const LOCAL: usize = 0;
pub const REMOTE: usize = 1;

pub struct SftpView {
    tx: Sender<Cmd>,
    pub dirs: [String; 2],
    pub lists: [Vec<Entry>; 2],
    pub sel: [usize; 2],
    /// Focused pane.
    pub side: usize,
    /// Bytes done and total of the running transfer.
    pub progress: Option<(u64, u64)>,
}

fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

fn parent(dir: &str) -> String {
    match dir.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/".into(),
        Some((up, _)) => up.into(),
    }
}

impl SftpView {
    pub fn new(tx: Sender<Cmd>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
        let mut cwd = cwd.to_string_lossy().into_owned();
        // Names are joined with `/` here, which Windows reads too: one kind in a path.
        if cfg!(windows) {
            cwd = cwd.replace('\\', "/");
        }
        let mut view = Self {
            tx,
            dirs: [cwd, "connecting…".into()],
            lists: [vec![], vec![]],
            sel: [0, 0],
            side: LOCAL,
            progress: None,
        };
        let _ = view.refresh_local();
        view
    }

    fn refresh_local(&mut self) -> io::Result<()> {
        self.lists[LOCAL] = list_local(Path::new(&self.dirs[LOCAL]))?;
        self.clamp();
        Ok(())
    }

    fn clamp(&mut self) {
        for side in [LOCAL, REMOTE] {
            self.sel[side] = self.sel[side].min(self.lists[side].len().saturating_sub(1));
        }
    }

    fn send(&self, cmd: Cmd) -> String {
        match self.tx.send(cmd) {
            Ok(()) => String::new(),
            Err(_) => "not connected".into(),
        }
    }

    fn local(&mut self, res: io::Result<()>) -> String {
        match res.and_then(|()| self.refresh_local()) {
            Ok(()) => String::new(),
            Err(e) => e.to_string(),
        }
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.lists[self.side].get(self.sel[self.side])
    }

    fn selected_path(&self) -> Option<(String, bool)> {
        self.selected()
            .map(|e| (join(&self.dirs[self.side], &e.name), e.is_dir))
    }

    /// True if the other pane already has an entry with this name.
    pub fn other_has(&self, name: &str) -> bool {
        self.lists[self.side ^ 1].iter().any(|e| e.name == name)
    }

    pub fn move_sel(&mut self, delta: isize) {
        let last = self.lists[self.side].len().saturating_sub(1);
        let sel = &mut self.sel[self.side];
        *sel = sel.saturating_add_signed(delta).min(last);
    }

    /// Every action returns a status line; empty when there is nothing to say.
    pub fn refresh(&mut self) -> String {
        match self.side {
            LOCAL => self.local(Ok(())),
            _ => self.send(Cmd::List(self.dirs[REMOTE].clone())),
        }
    }

    fn chdir(&mut self, dir: String) -> String {
        if self.side == REMOTE {
            return self.send(Cmd::List(dir));
        }
        // Only move if the directory can actually be listed.
        match list_local(Path::new(&dir)) {
            Ok(list) => {
                self.lists[LOCAL] = list;
                self.dirs[LOCAL] = dir;
                self.sel[LOCAL] = 0;
                String::new()
            }
            Err(e) => e.to_string(),
        }
    }

    pub fn enter(&mut self) -> String {
        match self.selected_path() {
            Some((path, true)) => self.chdir(path),
            _ => String::new(),
        }
    }

    pub fn up(&mut self) -> String {
        let dir = &self.dirs[self.side];
        let up = match self.side {
            // A path of this machine is the system's to climb: `C:/a` is under `C:/`, which is its own top.
            LOCAL => Path::new(dir)
                .parent()
                .map(|up| up.to_string_lossy().into_owned()),
            _ => Some(parent(dir)),
        };
        self.chdir(up.unwrap_or_else(|| dir.clone()))
    }

    /// Copies the selected file to the directory shown in the other pane.
    pub fn copy(&mut self) -> String {
        let Some(e) = self.selected().filter(|e| !e.is_dir) else {
            return "only files can be copied".into();
        };
        let local = join(&self.dirs[LOCAL], &e.name);
        let remote = join(&self.dirs[REMOTE], &e.name);
        self.progress = Some((0, e.size));
        self.send(match self.side {
            LOCAL => Cmd::Upload(local.into(), remote),
            _ => Cmd::Download(remote, local.into()),
        })
    }

    pub fn mkdir(&mut self, name: &str) -> String {
        let path = join(&self.dirs[self.side], name);
        match self.side {
            LOCAL => self.local(fs::create_dir(path)),
            _ => self.send(Cmd::Mkdir(path)),
        }
    }

    pub fn rename(&mut self, old: &str, new: &str) -> String {
        let dir = &self.dirs[self.side];
        let (from, to) = (join(dir, old), join(dir, new));
        if self.lists[self.side].iter().any(|e| e.name == new) {
            return format!("{new} already exists");
        }
        match self.side {
            LOCAL => self.local(fs::rename(from, to)),
            _ => self.send(Cmd::Rename(from, to)),
        }
    }

    /// Deletes the selected file or empty directory.
    pub fn delete(&mut self) -> String {
        let Some((path, is_dir)) = self.selected_path() else {
            return String::new();
        };
        match (self.side, is_dir) {
            (LOCAL, true) => self.local(fs::remove_dir(path)),
            (LOCAL, false) => self.local(fs::remove_file(path)),
            _ => self.send(Cmd::Delete(path, is_dir)),
        }
    }

    pub fn on_reply(&mut self, reply: Reply) -> String {
        match reply {
            Reply::Listing(dir, list) => {
                if dir != self.dirs[REMOTE] {
                    self.sel[REMOTE] = 0;
                }
                self.dirs[REMOTE] = dir;
                self.lists[REMOTE] = list;
                self.clamp();
                String::new()
            }
            Reply::Progress(done, total) => {
                self.progress = Some((done, total));
                String::new()
            }
            Reply::Done(msg) => {
                self.progress = None;
                // A download changed the local directory.
                let _ = self.refresh_local();
                msg
            }
            Reply::Error(e) => {
                self.progress = None;
                e
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths() {
        assert_eq!(join("/", "a"), "/a");
        assert_eq!(join("/home/u", "a"), "/home/u/a");
        assert_eq!(parent("/home/u"), "/home");
        assert_eq!(parent("/home"), "/");
        assert_eq!(parent("/"), "/");
    }

    #[test]
    fn local_pane_operations() {
        let dir = std::env::temp_dir().join(format!("vssh-files-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("b.txt"), "hello").unwrap();

        let (tx, mut rx) = vssh_core::task::channel();
        let mut v = SftpView::new(tx);
        assert_eq!(v.chdir(dir.to_string_lossy().into_owned()), "");
        // Directories sort first.
        let names: Vec<_> = v.lists[LOCAL].iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["sub", "b.txt"]);

        // Up is the directory above, on a Windows drive too; back down by its name.
        assert_eq!(v.up(), "");
        assert_eq!(Path::new(&v.dirs[LOCAL]), dir.parent().unwrap());
        let here = dir.file_name().unwrap().to_string_lossy();
        assert_eq!(v.chdir(join(&v.dirs[LOCAL], &here)), "");
        assert_eq!(Path::new(&v.dirs[LOCAL]), dir);

        assert_eq!(v.copy(), "only files can be copied");
        v.move_sel(5);
        assert_eq!(v.selected().unwrap().name, "b.txt");
        assert_eq!(v.rename("b.txt", "sub"), "sub already exists");
        assert_eq!(v.rename("b.txt", "c.txt"), "");
        assert_eq!(v.mkdir("new"), "");
        assert!(dir.join("c.txt").exists() && dir.join("new").is_dir());

        // Copying from the local pane queues an upload into the remote dir.
        v.dirs[REMOTE] = "/srv".into();
        v.sel[LOCAL] = 2;
        assert_eq!(v.copy(), "");
        match rx.try_recv().unwrap() {
            Cmd::Upload(local, remote) => {
                assert_eq!(local, dir.join("c.txt"));
                assert_eq!(remote, "/srv/c.txt");
            }
            _ => panic!("expected an upload"),
        }
        assert_eq!(v.progress, Some((0, 5)));

        assert_eq!(v.delete(), "");
        assert!(!dir.join("c.txt").exists());
        // A non-empty or missing directory is an error, not a silent no-op.
        assert_ne!(v.chdir(dir.join("nope").to_string_lossy().into_owned()), "");
        assert_eq!(Path::new(&v.dirs[LOCAL]), dir);
        fs::remove_dir_all(dir).unwrap();
    }
}
