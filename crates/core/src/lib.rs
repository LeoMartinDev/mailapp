//! The mail logic: model, rules, actions, queries, events and the ports (traits) that infrastructure implements. No I/O.

/// The error every port returns until the core has a real error model (M1.1).
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

/// One Inbox message as the list shows it: what IMAP's ENVELOPE and FLAGS give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageHeader {
    /// IMAP UID, unique in the Inbox.
    pub uid: u32,
    /// The From display name, or its address when it has none.
    pub sender: String,
    pub subject: String,
    /// Seconds since the Unix epoch.
    pub date: i64,
    pub unread: bool,
}

/// Where Inbox headers are kept between launches.
pub trait HeaderStore {
    /// Inserts the headers, replacing those with the same UID.
    fn upsert_headers(&mut self, headers: &[MessageHeader]) -> Result<()>;
    fn count_headers(&self) -> Result<u64>;
    /// Headers sorted newest first, skipping `offset` of them.
    fn headers_page(&self, offset: u64, limit: u64) -> Result<Vec<MessageHeader>>;
}

/// The OS secret store (Keychain, Credential Manager): the only place secrets live.
pub trait SecretStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>>;
    fn set(&self, name: &str, secret: &[u8]) -> Result<()>;
}
