//! Generic IMAP adapter: connects to an IMAP server and fetches Inbox headers, on tokio.

use std::fmt::Debug;
use std::sync::Arc;

use async_imap::Client;
use async_imap::imap_proto::{Address, Envelope};
use async_imap::types::{Fetch, Flag};
use futures_util::StreamExt;
use mail_parser::HeaderForm;
use mailapp_core::{MessageHeader, Result};
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::crypto::ring;
use tokio_rustls::rustls::pki_types::ServerName;

// Callers name the session this crate returns without depending on async-imap themselves.
pub use async_imap::Session;

/// Headers handed over at a time: one store transaction each, and the list fills as they arrive.
const BATCH: usize = 500;

/// Connects over implicit TLS (IMAPS, usually port 993) and logs in with a password.
pub async fn connect(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
) -> Result<Session<TlsStream<TcpStream>>> {
    // The OS verifier trusts what the user's system trusts, including company or self-hosted CAs.
    let config = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_platform_verifier()?
        .with_no_client_auth();
    let tcp = TcpStream::connect((host, port)).await?;
    let tls = TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from(host.to_owned())?, tcp)
        .await?;
    login(tls, user, password).await
}

/// Reads the server greeting on an open connection, then logs in with a password.
pub async fn login<T>(stream: T, user: &str, password: &str) -> Result<Session<T>>
where
    T: AsyncRead + AsyncWrite + Unpin + Debug + Send,
{
    let mut client = Client::new(stream);
    client
        .read_response()
        .await?
        .ok_or("the server closed the connection before its greeting")?;
    client
        .login(user, password)
        .await
        .map_err(|(e, _)| e.into())
}

/// Fetches the headers of the Inbox messages whose UID is above `after_uid`, oldest first, and
/// hands them to `on_batch` a few hundred at a time. Pass the highest UID already stored to get
/// only new mail, or 0 for everything. After an error, drop the session: part of the server's
/// answer may still be unread.
pub async fn fetch_inbox_headers<T>(
    session: &mut Session<T>,
    after_uid: u32,
    mut on_batch: impl FnMut(Vec<MessageHeader>) -> Result<()>,
) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + Debug + Send,
{
    // shortcut: UIDVALIDITY is not checked, nor are flag changes and deletions of stored
    // messages; M1.2 brings them with CONDSTORE/QRESYNC.
    session.select("INBOX").await?;
    let range = format!("{}:*", after_uid.saturating_add(1));
    let mut batches = session
        .uid_fetch(range, "(UID FLAGS INTERNALDATE ENVELOPE)")
        .await?
        .chunks(BATCH);
    while let Some(batch) = batches.next().await {
        let mut headers = Vec::with_capacity(batch.len());
        for fetch in batch {
            // The range `n:*` always includes the newest message, even when its UID is below n.
            match header_of(&fetch?)? {
                Some(header) if header.uid > after_uid => headers.push(header),
                _ => {}
            }
        }
        if !headers.is_empty() {
            on_batch(headers)?;
        }
    }
    Ok(())
}

/// `None` for a FETCH without an envelope: an unsolicited flag update, not an answer to ours.
fn header_of(fetch: &Fetch) -> Result<Option<MessageHeader>> {
    let Some(envelope) = fetch.envelope() else {
        return Ok(None);
    };
    let uid = fetch.uid.ok_or("FETCH response without a UID")?;
    // The server's arrival time rather than the Date header: always there, already parsed, and
    // the sender cannot forge it.
    let date = fetch
        .internal_date()
        .ok_or("FETCH response without an INTERNALDATE")?;
    let seen = fetch.flags().any(|flag| flag == Flag::Seen);
    Ok(Some(to_header(uid, envelope, seen, date.timestamp())))
}

fn to_header(uid: u32, envelope: &Envelope, seen: bool, date: i64) -> MessageHeader {
    let from = envelope.from.as_deref().and_then(<[_]>::first);
    MessageHeader {
        uid,
        sender: from.map(sender).unwrap_or_default(),
        subject: envelope.subject.as_deref().map(decode).unwrap_or_default(),
        date,
        unread: !seen,
    }
}

fn sender(address: &Address) -> String {
    let name = address.name.as_deref().map(decode).unwrap_or_default();
    if !name.is_empty() {
        return name;
    }
    let mailbox = String::from_utf8_lossy(address.mailbox.as_deref().unwrap_or_default());
    match address.host.as_deref() {
        Some(host) => format!("{mailbox}@{}", String::from_utf8_lossy(host)),
        None => mailbox.into_owned(),
    }
}

/// Servers send header text as written: RFC 2047 encoded words (`=?UTF-8?Q?Caf=C3=A9?=`) included.
fn decode(raw: &[u8]) -> String {
    HeaderForm::Text
        .parse(raw)
        .value()
        .as_text()
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use async_imap::imap_proto::{AttributeValue, Response};

    use super::*;

    /// Maps the ENVELOPE of a raw FETCH line, as a server would send it.
    fn map(envelope: &str, seen: bool) -> MessageHeader {
        let line = format!("* 1 FETCH (ENVELOPE {envelope})\r\n");
        let Ok((_, Response::Fetch(_, attributes))) = Response::parse(line.as_bytes()) else {
            panic!("not a FETCH response: {line}");
        };
        let Some(AttributeValue::Envelope(envelope)) = attributes.first() else {
            panic!("no envelope in {line}");
        };
        to_header(7, envelope, seen, 1_700_000_000)
    }

    #[test]
    fn maps_an_envelope() {
        assert_eq!(
            map(
                r#"("Mon, 6 Nov 2023 10:00:00 +0000" "Lunch?" (("Ada Lovelace" NIL "ada" "example.com")) NIL NIL NIL NIL NIL NIL "<1@example.com>")"#,
                false,
            ),
            MessageHeader {
                uid: 7,
                sender: "Ada Lovelace".into(),
                subject: "Lunch?".into(),
                date: 1_700_000_000,
                unread: true,
            }
        );
    }

    #[test]
    fn decodes_encoded_words() {
        let header = map(
            r#"(NIL "=?UTF-8?Q?Caf=C3=A9_au_lait?= =?ISO-8859-1?B?6XTp?=" (("=?ISO-8859-1?Q?Andr=E9?=" NIL "andre" "example.com")) NIL NIL NIL NIL NIL NIL NIL)"#,
            true,
        );
        assert_eq!(header.subject, "Café au laitété");
        assert_eq!(header.sender, "André");
        assert!(!header.unread);
    }

    #[test]
    fn falls_back_on_the_address_and_empty_fields() {
        let header = map(
            r#"(NIL NIL ((NIL NIL "ada" "example.com")) NIL NIL NIL NIL NIL NIL NIL)"#,
            false,
        );
        assert_eq!(header.sender, "ada@example.com");
        assert_eq!(header.subject, "");

        let header = map(r#"(NIL NIL NIL NIL NIL NIL NIL NIL NIL NIL)"#, false);
        assert_eq!(header.sender, "");
    }
}
