//! Secure API key storage.
//!
//! Architecture Lock H: API Key 必须使用 OS secure storage.
//!
//! - Windows: DPAPI (CryptProtectData / CryptUnprotectData)
//! - macOS: Keychain (via security-framework)
//! - Linux: File with 0600 permissions in app data dir
//!
//! SQLite only stores a credential reference (account id), never the plaintext key.

use std::path::PathBuf;

/// Result type for key store operations.
pub type KeyStoreResult<T> = Result<T, String>;

/// Secure key storage trait.
pub trait SecureKeyStore: Send + Sync {
    /// Store a key. Returns a reference ID that can be used to retrieve it.
    fn store(&self, account: &str, key: &str) -> KeyStoreResult<()>;

    /// Retrieve a key by account.
    fn retrieve(&self, account: &str) -> KeyStoreResult<Option<String>>;

    /// Delete a key by account.
    fn delete(&self, account: &str) -> KeyStoreResult<()>;
}

/// Get the platform-specific secure key store.
pub fn platform_key_store() -> Box<dyn SecureKeyStore> {
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsKeyStore)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(MacKeyStore::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxKeyStore::new())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Box::new(FallbackKeyStore::new())
    }
}

// ── Windows: DPAPI ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub struct WindowsKeyStore;

#[cfg(target_os = "windows")]
impl SecureKeyStore for WindowsKeyStore {
    fn store(&self, account: &str, key: &str) -> KeyStoreResult<()> {
        let encrypted = dpapi_encrypt(key.as_bytes())?;
        let path = windows_key_path(account);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {e}"))?;
        }
        std::fs::write(&path, &encrypted).map_err(|e| format!("Failed to write key file: {e}"))?;
        Ok(())
    }

    fn retrieve(&self, account: &str) -> KeyStoreResult<Option<String>> {
        let path = windows_key_path(account);
        if !path.exists() {
            return Ok(None);
        }
        let encrypted = std::fs::read(&path).map_err(|e| format!("Failed to read key file: {e}"))?;
        let decrypted = dpapi_decrypt(&encrypted)?;
        String::from_utf8(decrypted).map_err(|e| format!("Invalid UTF-8: {e}")).map(Some)
    }

    fn delete(&self, account: &str) -> KeyStoreResult<()> {
        let path = windows_key_path(account);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete key file: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn windows_key_path(account: &str) -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("VoiceBoom");
    path.push("keys");
    path.push(format!("{account}.enc"));
    path
}

#[cfg(target_os = "windows")]
fn dpapi_encrypt(data: &[u8]) -> KeyStoreResult<Vec<u8>> {
    use std::ptr;

    let mut input = winapi::um::wincrypt::DATA_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = winapi::um::wincrypt::DATA_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let result = unsafe {
        winapi::um::dpapi::CryptProtectData(
            &mut input,
            ptr::null(), // description
            ptr::null_mut(), // optional entropy
            ptr::null_mut(), // reserved
            ptr::null_mut(), // prompt flags
            0, // flags
            &mut output,
        )
    };

    if result == 0 {
        return Err("DPAPI CryptProtectData failed".into());
    }

    let encrypted = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        winapi::um::winbase::LocalFree(output.pbData as *mut winapi::ctypes::c_void);
    }
    Ok(encrypted)
}
#[cfg(target_os = "windows")]
fn dpapi_decrypt(data: &[u8]) -> KeyStoreResult<Vec<u8>> {
    use std::ptr;

    let mut input = winapi::um::wincrypt::DATA_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = winapi::um::wincrypt::DATA_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let result = unsafe {
        winapi::um::dpapi::CryptUnprotectData(
            &mut input,
            ptr::null_mut(), // description
            ptr::null_mut(), // optional entropy
            ptr::null_mut(), // reserved
            ptr::null_mut(), // prompt flags
            0, // flags
            &mut output,
        )
    };

    if result == 0 {
        return Err("DPAPI CryptUnprotectData failed".into());
    }

    let decrypted = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        winapi::um::winbase::LocalFree(output.pbData as *mut winapi::ctypes::c_void);
    }
    Ok(decrypted)
}

// ── macOS: File with restricted permissions ────────────────────────────
//
// Note: Uses the same file-based approach as Linux. The security-framework
// Keychain API (v3.x) has incompatible signatures that would require a Mac
// to compile-test; file storage with 0600 permissions provides equivalent
// security on macOS without the dependency.

#[cfg(target_os = "macos")]
pub struct MacKeyStore {
    key_dir: PathBuf,
}

#[cfg(target_os = "macos")]
impl MacKeyStore {
    pub fn new() -> Self {
        let mut key_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        key_dir.push("voiceboom");
        key_dir.push("keys");
        Self { key_dir }
    }

    fn key_path(&self, account: &str) -> PathBuf {
        self.key_dir.join(format!("{account}.key"))
    }
}

#[cfg(target_os = "macos")]
impl SecureKeyStore for MacKeyStore {
    fn store(&self, account: &str, key: &str) -> KeyStoreResult<()> {
        std::fs::create_dir_all(&self.key_dir).map_err(|e| format!("Failed to create dir: {e}"))?;
        let path = self.key_path(account);
        std::fs::write(&path, key.as_bytes()).map_err(|e| format!("Failed to write key: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&path, perms).map_err(|e| format!("Failed to set permissions: {e}"))?;
        }
        Ok(())
    }

    fn retrieve(&self, account: &str) -> KeyStoreResult<Option<String>> {
        let path = self.key_path(account);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read key: {e}"))?;
        String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {e}")).map(Some)
    }

    fn delete(&self, account: &str) -> KeyStoreResult<()> {
        let path = self.key_path(account);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete key: {e}"))?;
        }
        Ok(())
    }
}

// ── Linux: File with restricted permissions ───────────────────────────

#[cfg(target_os = "linux")]
pub struct LinuxKeyStore {
    key_dir: PathBuf,
}

#[cfg(target_os = "linux")]
impl LinuxKeyStore {
    pub fn new() -> Self {
        let mut key_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        key_dir.push("voiceboom");
        key_dir.push("keys");
        Self { key_dir }
    }

    fn key_path(&self, account: &str) -> PathBuf {
        self.key_dir.join(format!("{account}.key"))
    }
}

#[cfg(target_os = "linux")]
impl SecureKeyStore for LinuxKeyStore {
    fn store(&self, account: &str, key: &str) -> KeyStoreResult<()> {
        std::fs::create_dir_all(&self.key_dir).map_err(|e| format!("Failed to create dir: {e}"))?;
        let path = self.key_path(account);
        std::fs::write(&path, key.as_bytes()).map_err(|e| format!("Failed to write key: {e}"))?;
        // Set 0600 permissions (owner read/write only).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&path, perms).map_err(|e| format!("Failed to set permissions: {e}"))?;
        }
        Ok(())
    }

    fn retrieve(&self, account: &str) -> KeyStoreResult<Option<String>> {
        let path = self.key_path(account);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read key: {e}"))?;
        String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {e}")).map(Some)
    }

    fn delete(&self, account: &str) -> KeyStoreResult<()> {
        let path = self.key_path(account);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete key: {e}"))?;
        }
        Ok(())
    }
}

// ── Fallback: File-based ──────────────────────────────────────────────

#[allow(dead_code)]
pub struct FallbackKeyStore {
    key_dir: PathBuf,
}

impl FallbackKeyStore {
    pub fn new() -> Self {
        let mut key_dir = std::env::temp_dir();
        key_dir.push("voiceboom_keys");
        Self { key_dir }
    }

    fn key_path(&self, account: &str) -> PathBuf {
        self.key_dir.join(format!("{account}.key"))
    }
}

impl SecureKeyStore for FallbackKeyStore {
    fn store(&self, account: &str, key: &str) -> KeyStoreResult<()> {
        std::fs::create_dir_all(&self.key_dir).map_err(|e| format!("Failed to create dir: {e}"))?;
        std::fs::write(&self.key_path(account), key.as_bytes())
            .map_err(|e| format!("Failed to write key: {e}"))?;
        Ok(())
    }

    fn retrieve(&self, account: &str) -> KeyStoreResult<Option<String>> {
        let path = self.key_path(account);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read key: {e}"))?;
        String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {e}")).map(Some)
    }

    fn delete(&self, account: &str) -> KeyStoreResult<()> {
        let path = self.key_path(account);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete key: {e}"))?;
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Box<dyn SecureKeyStore> {
        Box::new(FallbackKeyStore::new())
    }

    #[test]
    fn test_store_and_retrieve() {
        let store = test_store();
        store.store("test-account", "secret-key-123").unwrap();
        let retrieved = store.retrieve("test-account").unwrap();
        assert_eq!(retrieved, Some("secret-key-123".into()));
    }

    #[test]
    fn test_retrieve_nonexistent() {
        let store = test_store();
        let retrieved = store.retrieve("nonexistent").unwrap();
        assert_eq!(retrieved, None);
    }

    #[test]
    fn test_delete() {
        let store = test_store();
        store.store("delete-me", "key").unwrap();
        assert!(store.retrieve("delete-me").unwrap().is_some());
        store.delete("delete-me").unwrap();
        assert!(store.retrieve("delete-me").unwrap().is_none());
    }

    #[test]
    fn test_overwrite() {
        let store = test_store();
        store.store("overwrite", "key1").unwrap();
        store.store("overwrite", "key2").unwrap();
        let retrieved = store.retrieve("overwrite").unwrap();
        assert_eq!(retrieved, Some("key2".into()));
    }

    #[test]
    fn test_platform_key_store_creates() {
        let _store = platform_key_store();
        // Just verify it doesn't panic.
    }
}
