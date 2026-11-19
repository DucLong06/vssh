//! SQLite store for everything `~/.ssh/config` has no field for.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, Result};

use crate::ssh::{HopAuth, JumpAuth};

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Meta {
    pub grp: String,
    /// Space or comma separated.
    pub tags: String,
    pub notes: String,
    pub pinned: bool,
    /// Unix seconds; 0 = never.
    pub last_connected: i64,
    pub connects: i64,
    /// Not watched: a dashboard asks it for nothing on its own.
    pub paused: bool,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Snippet {
    /// 0 = not saved yet.
    pub id: i64,
    /// Empty = global.
    pub host: String,
    pub name: String,
    pub command: String,
}

pub struct Db(Connection);

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    fn init(c: Connection) -> Result<Self> {
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS hosts(
                alias TEXT PRIMARY KEY,
                grp TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '',
                notes TEXT NOT NULL DEFAULT '',
                pinned INTEGER NOT NULL DEFAULT 0,
                last_connected INTEGER NOT NULL DEFAULT 0,
                connects INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS snippets(
                id INTEGER PRIMARY KEY,
                host TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL,
                command TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS jump_auth(
                host TEXT NOT NULL,
                hop TEXT NOT NULL,
                key TEXT NOT NULL,
                user TEXT NOT NULL DEFAULT '',
                PRIMARY KEY(host, hop));
             CREATE TABLE IF NOT EXISTS flags(name TEXT PRIMARY KEY);",
        )?;
        // A database from before hosts could be left unwatched; one that has
        // the column says so, which is all this error can be.
        let _ = c.execute(
            "ALTER TABLE hosts ADD COLUMN paused INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(Self(c))
    }

    /// Makes sure every ssh config host has a metadata row. One transaction,
    /// so a first run with hundreds of hosts is a single fsync.
    pub fn import(&self, aliases: &[String]) -> Result<()> {
        let tx = self.0.unchecked_transaction()?;
        for a in aliases {
            tx.execute("INSERT OR IGNORE INTO hosts(alias) VALUES (?1)", [a])?;
        }
        tx.commit()
    }

    pub fn metas(&self) -> Result<HashMap<String, Meta>> {
        let mut st = self.0.prepare(
            "SELECT alias, grp, tags, notes, pinned, last_connected, connects, paused FROM hosts",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get(0)?,
                Meta {
                    grp: r.get(1)?,
                    tags: r.get(2)?,
                    notes: r.get(3)?,
                    pinned: r.get(4)?,
                    last_connected: r.get(5)?,
                    connects: r.get(6)?,
                    paused: r.get(7)?,
                },
            ))
        })?;
        rows.collect()
    }

    /// Saves the user-edited fields; usage stats are only changed by `touch`.
    pub fn set_meta(&self, alias: &str, m: &Meta) -> Result<()> {
        self.0.execute(
            "INSERT INTO hosts(alias, grp, tags, notes, pinned) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(alias) DO UPDATE SET grp = ?2, tags = ?3, notes = ?4, pinned = ?5",
            params![alias, m.grp, m.tags, m.notes, m.pinned],
        )?;
        Ok(())
    }

    /// Whether a dashboard leaves the host alone; no part of `set_meta`,
    /// which a form saves that does not show it.
    pub fn set_paused(&self, alias: &str, paused: bool) -> Result<()> {
        self.0.execute(
            "INSERT INTO hosts(alias, paused) VALUES (?1, ?2)
             ON CONFLICT(alias) DO UPDATE SET paused = ?2",
            params![alias, paused],
        )?;
        Ok(())
    }

    /// A switch of the app, by name: off until it is set.
    pub fn flag(&self, name: &str) -> Result<bool> {
        let sql = "SELECT EXISTS(SELECT 1 FROM flags WHERE name = ?1)";
        self.0.query_row(sql, [name], |row| row.get(0))
    }

    pub fn set_flag(&self, name: &str, on: bool) -> Result<()> {
        let sql = match on {
            true => "INSERT OR IGNORE INTO flags(name) VALUES (?1)",
            false => "DELETE FROM flags WHERE name = ?1",
        };
        self.0.execute(sql, [name])?;
        Ok(())
    }

    /// The keys jump hosts were given, by host and hop.
    pub fn jump_auth(&self) -> Result<JumpAuth> {
        let mut st = self
            .0
            .prepare("SELECT host, hop, key, user FROM jump_auth")?;
        let rows = st.query_map([], |r| {
            let auth = HopAuth {
                key: r.get(2)?,
                user: r.get(3)?,
            };
            Ok(((r.get(0)?, r.get(1)?), auth))
        })?;
        rows.collect()
    }

    /// What `host`'s jump hosts are given from now on: these, by hop, and
    /// nothing of what was there.
    pub fn set_jump_auth(&self, host: &str, hops: &[(String, HopAuth)]) -> Result<()> {
        let tx = self.0.unchecked_transaction()?;
        tx.execute("DELETE FROM jump_auth WHERE host = ?1", [host])?;
        for (hop, auth) in hops {
            tx.execute(
                "INSERT OR REPLACE INTO jump_auth(host, hop, key, user) VALUES (?1, ?2, ?3, ?4)",
                params![host, hop, auth.key, auth.user],
            )?;
        }
        tx.commit()
    }

    /// Records a successful connection.
    pub fn touch(&self, alias: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO hosts(alias, last_connected, connects) VALUES (?1, unixepoch(), 1)
             ON CONFLICT(alias) DO UPDATE
             SET last_connected = unixepoch(), connects = connects + 1",
            [alias],
        )?;
        Ok(())
    }

    /// Local time as `YYYYMMDD-HHMMSS`, for file names. SQLite is the date
    /// library this program already links.
    pub fn timestamp(&self) -> Result<String> {
        let sql = "SELECT strftime('%Y%m%d-%H%M%S', 'now', 'localtime')";
        self.0.query_row(sql, [], |row| row.get(0))
    }

    pub fn rename(&self, old: &str, new: &str) -> Result<()> {
        self.0
            .execute("UPDATE hosts SET alias = ?2 WHERE alias = ?1", [old, new])?;
        self.0
            .execute("UPDATE snippets SET host = ?2 WHERE host = ?1", [old, new])?;
        // Its road is its own under the new name; where it is another
        // host's hop, that host's ProxyJump still says the old one.
        self.0
            .execute("UPDATE jump_auth SET host = ?2 WHERE host = ?1", [old, new])?;
        Ok(())
    }

    /// Removes the host's metadata and its host-specific snippets.
    pub fn delete(&self, alias: &str) -> Result<()> {
        self.0
            .execute("DELETE FROM hosts WHERE alias = ?1", [alias])?;
        self.0
            .execute("DELETE FROM snippets WHERE host = ?1", [alias])?;
        self.0
            .execute("DELETE FROM jump_auth WHERE host = ?1", [alias])?;
        Ok(())
    }

    pub fn snippets(&self) -> Result<Vec<Snippet>> {
        let mut st = self
            .0
            .prepare("SELECT id, host, name, command FROM snippets ORDER BY host, name")?;
        let rows = st.query_map([], |r| {
            Ok(Snippet {
                id: r.get(0)?,
                host: r.get(1)?,
                name: r.get(2)?,
                command: r.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Saves a snippet and returns its id: a new one if it had none.
    pub fn save_snippet(&self, s: &Snippet) -> Result<i64> {
        let id = (s.id != 0).then_some(s.id);
        self.0.execute(
            "INSERT OR REPLACE INTO snippets(id, host, name, command) VALUES (?1, ?2, ?3, ?4)",
            params![id, s.host, s.name, s.command],
        )?;
        Ok(self.0.last_insert_rowid())
    }

    pub fn delete_snippet(&self, id: i64) -> Result<()> {
        self.0.execute("DELETE FROM snippets WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_and_snippets() {
        let db = Db::init(Connection::open_in_memory().unwrap()).unwrap();
        let stamp = db.timestamp().unwrap();
        assert_eq!(stamp.len(), 15, "{stamp}");
        assert!(stamp.starts_with("20") && stamp.as_bytes()[8] == b'-');
        db.import(&["a".into(), "b".into()]).unwrap();
        db.import(&["a".into()]).unwrap();
        assert_eq!(db.metas().unwrap().len(), 2);

        let meta = Meta {
            grp: "prod".into(),
            tags: "web eu".into(),
            pinned: true,
            ..Meta::default()
        };
        db.set_meta("a", &meta).unwrap();
        db.touch("a").unwrap();
        db.touch("a").unwrap();
        // Editing metadata must not reset the usage stats.
        db.set_meta("a", &meta).unwrap();
        db.rename("a", "c").unwrap();
        let got = &db.metas().unwrap()["c"];
        assert_eq!(
            (got.grp.as_str(), got.pinned, got.connects),
            ("prod", true, 2)
        );
        assert!(got.last_connected > 0);

        let mut s = Snippet {
            id: 0,
            host: "c".into(),
            name: "up".into(),
            command: "uptime".into(),
        };
        // Saving says which row it was: a new one, or the one it replaced.
        assert_eq!(db.save_snippet(&s).unwrap(), 1);
        s = db.snippets().unwrap().remove(0);
        s.command = "uptime -p".into();
        assert_eq!(db.save_snippet(&s).unwrap(), 1);
        assert_eq!(db.snippets().unwrap(), [s.clone()]);

        db.delete("c").unwrap();
        assert!(db.snippets().unwrap().is_empty());
        assert!(!db.metas().unwrap().contains_key("c"));
    }

    #[test]
    fn what_the_ssh_config_has_no_word_for() {
        let db = Db::init(Connection::open_in_memory().unwrap()).unwrap();
        // A host is watched until someone says otherwise, and the form's save leaves that alone.
        db.import(&["a".into()]).unwrap();
        assert!(!db.metas().unwrap()["a"].paused);
        db.set_paused("a", true).unwrap();
        db.set_meta("a", &Meta::default()).unwrap();
        assert!(db.metas().unwrap()["a"].paused);
        db.set_paused("a", false).unwrap();
        assert!(!db.metas().unwrap()["a"].paused);

        assert!(!db.flag("watch_paused").unwrap());
        db.set_flag("watch_paused", true).unwrap();
        db.set_flag("watch_paused", true).unwrap();
        assert!(db.flag("watch_paused").unwrap());
        db.set_flag("watch_paused", false).unwrap();
        assert!(!db.flag("watch_paused").unwrap());

        let auth = |key: &str, user: &str| HopAuth {
            key: key.into(),
            user: user.into(),
        };
        let hops = [
            ("bastion".to_string(), auth("~/.ssh/hop", "")),
            ("ops@gw:2222".to_string(), auth("/keys/gw", "jump")),
        ];
        db.set_jump_auth("a", &hops).unwrap();
        db.set_jump_auth("b", &hops[..1]).unwrap();
        let key = |host: &str, hop: &str| (host.to_string(), hop.to_string());
        let kept = db.jump_auth().unwrap();
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[&key("a", "ops@gw:2222")], auth("/keys/gw", "jump"));
        // Saved again, a host's hops are those it is saved with.
        db.set_jump_auth("a", &hops[..1]).unwrap();
        assert!(!db
            .jump_auth()
            .unwrap()
            .contains_key(&key("a", "ops@gw:2222")));
        // They go with the host: to its new name, and away with it.
        db.rename("a", "c").unwrap();
        assert_eq!(db.jump_auth().unwrap()[&key("c", "bastion")], hops[0].1);
        db.delete("c").unwrap();
        assert_eq!(db.jump_auth().unwrap().len(), 1);
        // A database that has the column already opens as before.
        Db::init(db.0).unwrap();
    }
}
