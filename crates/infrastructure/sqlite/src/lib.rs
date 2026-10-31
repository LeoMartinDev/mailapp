//! SQLite adapter: implements the core's mail store and command journal, on SQLCipher with FTS5.

use std::path::Path;

use mailapp_core::{HeaderStore, MessageHeader, Result, SecretStore};
use rusqlite::{Connection, params};

/// Name of the database key in the OS secret store.
const KEY_NAME: &str = "database-key";

/// Schema changes in order; `PRAGMA user_version` counts those already applied. Append, never edit.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE inbox_headers (
        uid INTEGER PRIMARY KEY,
        sender TEXT NOT NULL,
        subject TEXT NOT NULL,
        date INTEGER NOT NULL,
        unread INTEGER NOT NULL
    ) STRICT;
    CREATE INDEX inbox_headers_by_date ON inbox_headers (date);
"];

/// The local database, encrypted with SQLCipher.
pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    /// Opens or creates the database at `path`. The key is generated on first launch and kept in
    /// `secrets`; later launches read it back, and a wrong key fails here.
    pub fn open(path: &Path, secrets: &dyn SecretStore) -> Result<Self> {
        let key = database_key(secrets)?;
        let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut conn = Connection::open(path)?;
        // The x'…' form hands SQLCipher the raw 256-bit key instead of a passphrase to stretch.
        conn.pragma_update(None, "key", format!("x'{hex}'"))?;
        migrate(&mut conn)?;
        Ok(Self { conn })
    }
}

fn database_key(secrets: &dyn SecretStore) -> Result<[u8; 32]> {
    if let Some(key) = secrets.get(KEY_NAME)? {
        return key
            .try_into()
            .map_err(|_| "the database key in the secret store is not 32 bytes".into());
    }
    let mut key = [0; 32];
    getrandom::fill(&mut key)?;
    secrets.set(KEY_NAME, &key)?;
    Ok(key)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    let applied: usize = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (version, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version + 1)?;
    }
    Ok(tx.commit()?)
}

impl HeaderStore for SqliteStore {
    fn upsert_headers(&mut self, headers: &[MessageHeader]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO inbox_headers (uid, sender, subject, date, unread)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for h in headers {
                insert.execute(params![h.uid, h.sender, h.subject, h.date, h.unread])?;
            }
        }
        Ok(tx.commit()?)
    }

    fn count_headers(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM inbox_headers", [], |row| row.get(0))?)
    }

    fn highest_uid(&self) -> Result<u32> {
        Ok(self.conn.query_row(
            "SELECT coalesce(max(uid), 0) FROM inbox_headers",
            [],
            |row| row.get(0),
        )?)
    }

    fn headers_page(&self, offset: u64, limit: u64) -> Result<Vec<MessageHeader>> {
        let mut select = self.conn.prepare_cached(
            "SELECT uid, sender, subject, date, unread FROM inbox_headers
             ORDER BY date DESC, uid DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = select.query_map(params![limit, offset], |row| {
            Ok(MessageHeader {
                uid: row.get(0)?,
                sender: row.get(1)?,
                subject: row.get(2)?,
                date: row.get(3)?,
                unread: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemorySecrets(RefCell<HashMap<String, Vec<u8>>>);

    impl SecretStore for MemorySecrets {
        fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.0.borrow().get(name).cloned())
        }

        fn set(&self, name: &str, secret: &[u8]) -> Result<()> {
            self.0.borrow_mut().insert(name.into(), secret.to_vec());
            Ok(())
        }
    }

    fn header(uid: u32, date: i64, subject: &str) -> MessageHeader {
        MessageHeader {
            uid,
            sender: "Ada".into(),
            subject: subject.into(),
            date,
            unread: true,
        }
    }

    #[test]
    fn headers_round_trip_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut store =
            SqliteStore::open(&dir.path().join("db"), &MemorySecrets::default()).unwrap();
        assert_eq!(store.highest_uid().unwrap(), 0);

        store
            .upsert_headers(&[
                header(1, 100, "old"),
                header(2, 300, "new"),
                header(3, 200, "mid"),
            ])
            .unwrap();
        let read = MessageHeader {
            unread: false,
            ..header(3, 200, "mid")
        };
        store.upsert_headers(std::slice::from_ref(&read)).unwrap();

        assert_eq!(store.count_headers().unwrap(), 3);
        assert_eq!(store.highest_uid().unwrap(), 3);
        assert_eq!(
            store.headers_page(0, 10).unwrap(),
            [header(2, 300, "new"), read.clone(), header(1, 100, "old")]
        );
        assert_eq!(store.headers_page(1, 1).unwrap(), [read]);
    }

    #[test]
    fn file_is_encrypted_and_needs_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let mut store = SqliteStore::open(&path, &MemorySecrets::default()).unwrap();
        store
            .upsert_headers(&[header(1, 100, "Payslip October")])
            .unwrap();
        drop(store);

        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3\0"));
        assert!(!bytes.windows(15).any(|w| w == b"Payslip October"));

        // A fresh secret store means a new random key.
        assert!(SqliteStore::open(&path, &MemorySecrets::default()).is_err());
    }

    #[test]
    fn reopening_keeps_the_key_data_and_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let secrets = MemorySecrets::default();
        let mut store = SqliteStore::open(&path, &secrets).unwrap();
        store.upsert_headers(&[header(1, 100, "hello")]).unwrap();
        drop(store);
        let key = secrets.get(KEY_NAME).unwrap().unwrap();

        // Running the first migration again would fail: its table already exists.
        let store = SqliteStore::open(&path, &secrets).unwrap();
        assert_eq!(secrets.get(KEY_NAME).unwrap().unwrap(), key);
        assert_eq!(store.count_headers().unwrap(), 1);
        let version: usize = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len());
    }
}
