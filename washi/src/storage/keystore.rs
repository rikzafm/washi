//! The database key: 256 random bits, wrapped by the key protector (DPAPI) and kept in `key.bin`.

use crate::platform::KeyProtector;
use std::path::Path;
use zeroize::Zeroizing;

#[derive(Debug)]
pub enum KeyStoreError {
    /// `key.bin` could not be unwrapped: altered, truncated, or from another Windows user/machine.
    Unreadable(String),
    Io(std::io::Error),
}

impl std::fmt::Display for KeyStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyStoreError::Unreadable(why) => write!(
                f,
                "Washi could not read its encryption key (key.bin). It may be damaged, or it \
                 belongs to a different Windows account or computer. ({why})"
            ),
            KeyStoreError::Io(e) => write!(f, "Washi could not access its key file: {e}"),
        }
    }
}
impl std::error::Error for KeyStoreError {}

impl From<std::io::Error> for KeyStoreError {
    fn from(e: std::io::Error) -> Self {
        KeyStoreError::Io(e)
    }
}

/// Load the key from `key_path`, creating and storing a new one on first run.
/// The caller should drop (and thereby wipe) the returned key once the DB is unlocked.
pub fn load_or_create(key_path: &Path, protector: &dyn KeyProtector) -> Result<Zeroizing<[u8; 32]>, KeyStoreError> {
    match std::fs::read(key_path) {
        Ok(wrapped) => {
            let raw = Zeroizing::new(
                protector.unprotect(&wrapped).map_err(|e| KeyStoreError::Unreadable(e.0))?,
            );
            let key: [u8; 32] = raw
                .as_slice()
                .try_into()
                .map_err(|_| KeyStoreError::Unreadable("unexpected key length".into()))?;
            Ok(Zeroizing::new(key))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let key = Zeroizing::new(protector.generate_key());
            let wrapped = protector.protect(key.as_slice()).map_err(|e| KeyStoreError::Unreadable(e.0))?;
            let tmp = key_path.with_extension("bin.tmp");
            std::fs::write(&tmp, wrapped)?;
            std::fs::rename(&tmp, key_path)?;
            Ok(key)
        }
        Err(e) => Err(e.into()),
    }
}
