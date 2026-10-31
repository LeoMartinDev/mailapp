//! Syncs a real IMAP Inbox into the encrypted store. Needs GreenMail on localhost:3143:
//! `docker run --rm -p 3143:3143 greenmail/standalone:2.1.15`, then
//! `cargo test -p mailapp-imap -- --ignored`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use async_imap::Session;
use mailapp_core::{HeaderStore, Result, SecretStore};
use mailapp_imap::{fetch_inbox_headers, login};
use mailapp_sqlite::SqliteStore;
use tokio::net::TcpStream;

/// GreenMail's plain IMAP port: its TLS port has a self-signed certificate the OS rightly rejects.
const GREENMAIL: &str = "localhost:3143";

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

async fn append(session: &mut Session<TcpStream>, subject: &str) {
    let message = format!("From: ada@example.com\r\nSubject: {subject}\r\n\r\nHello\r\n");
    session.append("INBOX", None, None, message).await.unwrap();
}

/// One sync run as the app does it: fetch above the highest stored UID. Returns the number of
/// headers fetched and of batches.
async fn sync(session: &mut Session<TcpStream>, store: &mut SqliteStore) -> (usize, usize) {
    let (mut fetched, mut batches) = (0, 0);
    let after_uid = store.highest_uid().unwrap();
    fetch_inbox_headers(session, after_uid, |headers| {
        fetched += headers.len();
        batches += 1;
        store.upsert_headers(&headers)
    })
    .await
    .unwrap();
    (fetched, batches)
}

#[tokio::test]
#[ignore = "needs GreenMail on localhost:3143 (see the top of this file); CI runs it in the integration job"]
async fn syncs_the_inbox_then_only_new_mail() {
    // GreenMail creates any account on first login: a fresh one per run starts with an empty Inbox.
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let user = format!("sync-{}@localhost", nanos.as_nanos());
    let tcp = TcpStream::connect(GREENMAIL).await.unwrap();
    let mut session = login(tcp, &user, "password").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(&dir.path().join("db"), &MemorySecrets::default()).unwrap();

    let first = "From: =?UTF-8?Q?Andr=C3=A9?= <andre@example.com>\r\n\
                 Subject: =?UTF-8?Q?Caf=C3=A9?=\r\n\r\nHello\r\n";
    session
        .append(
            "INBOX",
            Some(r"(\Seen)"),
            Some(r#""06-Nov-2023 10:00:00 +0000""#),
            first,
        )
        .await
        .unwrap();
    // Enough messages for more than one batch.
    for i in 1..=600 {
        append(&mut session, &format!("Message {i}")).await;
    }

    let (fetched, batches) = sync(&mut session, &mut store).await;
    assert_eq!(fetched, 601);
    assert!(batches > 1, "{batches} batch(es)");
    assert_eq!(store.count_headers().unwrap(), 601);
    let oldest = store.headers_page(600, 1).unwrap().remove(0);
    assert_eq!(oldest.sender, "André");
    assert_eq!(oldest.subject, "Café");
    assert_eq!(oldest.date, 1_699_264_800);
    assert!(!oldest.unread);
    let newest = store.headers_page(0, 1).unwrap().remove(0);
    assert_eq!(newest.uid, store.highest_uid().unwrap());
    assert_eq!(newest.sender, "ada@example.com");
    assert_eq!(newest.subject, "Message 600");
    assert!(newest.unread);

    append(&mut session, "Late 1").await;
    append(&mut session, "Late 2").await;
    assert_eq!(sync(&mut session, &mut store).await.0, 2);
    assert_eq!(store.count_headers().unwrap(), 603);

    assert_eq!(sync(&mut session, &mut store).await.0, 0);
    session.logout().await.unwrap();
}
