// Provider Credential Store (spec section 24).
//
// Wraps the OS secure storage (secure_keystore) with per-provider credential
// management. SQLite only stores the credential_ref (account id); the actual
// API key lives in DPAPI (Windows) / Keychain (macOS) / 0600 file (Linux).

use serde::{Deserialize, Serialize};

use crate::provider::config::ProviderId;
use crate::secure_keystore;

/// Manages cloud-provider credentials via OS secure storage.
pub struct ProviderCredentialStore;

/// Status of a provider's credential (for UI display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialStatus {
    pub provider: ProviderId,
    pub credential_ref: String,
    pub configured: bool,
}

impl ProviderCredentialStore {
    /// Store a credential for a provider. Returns the credential_ref used.
    pub fn store(_provider: ProviderId, credential_ref: &str, api_key: &str) -> Result<(), String> {
        let store = secure_keystore::platform_key_store();
        store.store(credential_ref, api_key).map_err(|e| format!("存储凭证失败: {e}"))
    }

    /// Retrieve a credential by ref.
    pub fn retrieve(credential_ref: &str) -> Result<Option<String>, String> {
        let store = secure_keystore::platform_key_store();
        store.retrieve(credential_ref).map_err(|e| format!("读取凭证失败: {e}"))
    }

    /// Delete a credential.
    pub fn delete(credential_ref: &str) -> Result<(), String> {
        let store = secure_keystore::platform_key_store();
        store.delete(credential_ref).map_err(|e| format!("删除凭证失败: {e}"))
    }

    /// Check whether a credential exists for the given ref.
    pub fn is_configured(credential_ref: &str) -> bool {
        match Self::retrieve(credential_ref) {
            Ok(Some(v)) => !v.is_empty(),
            _ => false,
        }
    }

    /// Get the credential status for a provider (configured or not).
    pub fn status(provider: ProviderId, credential_ref: &str) -> CredentialStatus {
        CredentialStatus {
            provider,
            credential_ref: credential_ref.to_string(),
            configured: Self::is_configured(credential_ref),
        }
    }

    /// Resolve the actual API key for a provider config.
    /// Returns Ok(None) if no credential is configured (provider may still work
    /// if it doesn't require one, e.g. a local custom server).
    pub fn resolve_key(credential_ref: &str) -> Result<Option<String>, String> {
        Self::retrieve(credential_ref)
    }
}
