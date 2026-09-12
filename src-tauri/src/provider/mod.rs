// Cloud Provider Registry (spec section 10, 38).
//
// Unifies cloud ASR providers behind a single registry so the app can:
//   - List available providers and their status
//   - Store per-provider config (endpoint, model, credential_ref) in SQLite
//   - Resolve credentials via OS secure storage (never plaintext in SQLite)
//   - Support auto-fallback: Local → Cloud (spec section 12)
//
// Architecture:
//   ProviderRegistry
//       ↓
//   ProviderConfig (provider + model + endpoint + credential_ref)
//       ↓
//   CredentialStore (OS secure storage via secure_keystore)
//       ↓
//   ProviderFactory → AsrSession

#![allow(dead_code)]
pub mod config;
pub mod credential;
pub mod registry;

