//! SQLite store for everything `~/.ssh/config` has no field for.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, Result};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub grp: String,
    /// Space or comma separated.
    pub tags: String,
    pub notes: String,
    pub pinned: bool,
    /// Unix seconds; 0 = never.
    pub last_connected: i64,
    pub connects: i64,
}

#[derive(Clone, Debug, PartialEq)]
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
                command TEXT NOT NULL);",
        )?;
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
            "SELECT alias, grp, tags, notes, pinned, last_connected, connects FROM hosts",
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
        Ok(())
    }

    /// Removes the host's metadata and its host-specific snippets.
    pub fn delete(&self, alias: &str) -> Result<()> {
        self.0
            .execute("DELETE FROM hosts WHERE alias = ?1", [alias])?;
        self.0
            .execute("DELETE FROM snippets WHERE host = ?1", [alias])?;
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

    pub fn save_snippet(&self, s: &Snippet) -> Result<()> {
        let id = (s.id != 0).then_some(s.id);
        self.0.execute(
            "INSERT OR REPLACE INTO snippets(id, host, name, command) VALUES (?1, ?2, ?3, ?4)",
            params![id, s.host, s.name, s.command],
        )?;
        Ok(())
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
        db.save_snippet(&s).unwrap();
        s = db.snippets().unwrap().remove(0);
        s.command = "uptime -p".into();
        db.save_snippet(&s).unwrap();
        assert_eq!(db.snippets().unwrap(), [s.clone()]);

        db.delete("c").unwrap();
        assert!(db.snippets().unwrap().is_empty());
        assert!(!db.metas().unwrap().contains_key("c"));
    }
}
