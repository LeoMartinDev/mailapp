//! OS secret store adapter: implements the core's `SecretStore` port with keyring-rs (Keychain on macOS and iOS, Credential Manager on Windows).

use std::sync::Arc;

use keyring_core::{CredentialStore, Entry, Error};
use mailapp_core::{Result, SecretStore};

/// Every mailapp secret is filed under this service name.
const SERVICE: &str = "mailapp";

pub struct KeyringSecretStore(Arc<CredentialStore>);

impl KeyringSecretStore {
    pub fn new() -> Result<Self> {
        // The legacy keychain accepts unsigned builds; macOS's protected store needs a signed app.
        #[cfg(target_os = "macos")]
        let store = apple_native_keyring_store::keychain::Store::new()?;
        #[cfg(target_os = "ios")]
        let store = apple_native_keyring_store::protected::Store::new()?;
        #[cfg(windows)]
        let store = windows_native_keyring_store::Store::new()?;
        Ok(Self(store))
    }

    fn entry(&self, name: &str) -> Result<Entry> {
        Ok(self.0.build(SERVICE, name, None)?)
    }
}

impl SecretStore for KeyringSecretStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        match self.entry(name)?.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<()> {
        Ok(self.entry(name)?.set_secret(secret)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "writes to the real OS secret store, which can prompt or be locked on CI runners; run with --ignored"]
    fn round_trips_a_secret_in_the_os_store() {
        let store = KeyringSecretStore::new().unwrap();
        let name = "test-round-trip";

        store.set(name, &[0, 1, 255]).unwrap();
        assert_eq!(store.get(name).unwrap(), Some(vec![0, 1, 255]));

        store.entry(name).unwrap().delete_credential().unwrap();
        assert_eq!(store.get(name).unwrap(), None);
    }
}
