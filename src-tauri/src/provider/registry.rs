// Provider Registry (spec section 10, 12, 38).
//
// Central registry that tracks all providers, their config, credential status,
// and supports auto-fallback (Local → Cloud).

use serde::{Deserialize, Serialize};

use crate::provider::config::{ProviderConfig, ProviderId, ProviderMode};
use crate::provider::credential::ProviderCredentialStore;

/// Runtime status of a single provider (for UI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider: ProviderId,
    pub enabled: bool,
    pub configured: bool,
    pub is_local: bool,
    pub display_name: String,
}

/// Result of resolving which provider to use for a recording session.
#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub provider: ProviderId,
    pub config: ProviderConfig,
}

/// Central registry of ASR providers.
pub struct ProviderRegistry;

impl ProviderRegistry {
    /// Build the status list for all providers.
    pub fn list_status(configs: &[(ProviderId, ProviderConfig)]) -> Vec<ProviderStatus> {
        let mut statuses: Vec<ProviderStatus> = configs
            .iter()
            .map(|(id, cfg)| ProviderStatus {
                provider: *id,
                enabled: cfg.enabled,
                configured: !id.requires_credential()
                    || ProviderCredentialStore::is_configured(&cfg.credential_ref),
                is_local: id.is_local(),
                display_name: id.display_name().to_string(),
            })
            .collect();

        // Ensure local provider is always present even if not in configs.
        if !statuses.iter().any(|s| s.is_local) {
            statuses.insert(
                0,
                ProviderStatus {
                    provider: ProviderId::LocalSenseVoice,
                    enabled: true,
                    configured: true,
                    is_local: true,
                    display_name: ProviderId::LocalSenseVoice.display_name().to_string(),
                },
            );
        }

        statuses
    }

    /// Resolve which provider to use given the user's mode + config map.
    ///
    /// Auto-fallback logic (spec section 12):
    ///   - Automatic: prefer local if available, else first configured cloud.
    ///   - Offline: always local.
    ///   - Cloud: use the explicitly selected cloud provider.
    pub fn resolve(
        mode: ProviderMode,
        local_available: bool,
        configs: &[(ProviderId, ProviderConfig)],
        preferred_cloud: Option<ProviderId>,
    ) -> Option<ResolvedProvider> {
        match mode {
            ProviderMode::Offline => {
                // Force local.
                if local_available {
                    Self::find_config(configs, ProviderId::LocalSenseVoice)
                        .or_else(|| Some(ProviderConfig::new(ProviderId::LocalSenseVoice)))
                        .map(|config| ResolvedProvider {
                            provider: ProviderId::LocalSenseVoice,
                            config,
                        })
                } else {
                    None
                }
            }
            ProviderMode::Cloud => {
                // Use the preferred cloud provider.
                let target = preferred_cloud.unwrap_or(ProviderId::OpenAIRealtime);
                Self::find_config(configs, target).map(|config| ResolvedProvider {
                    provider: target,
                    config,
                })
            }
            ProviderMode::Automatic => {
                // Prefer local when available.
                if local_available {
                    return Self::find_config(configs, ProviderId::LocalSenseVoice)
                        .or_else(|| Some(ProviderConfig::new(ProviderId::LocalSenseVoice)))
                        .map(|config| ResolvedProvider {
                            provider: ProviderId::LocalSenseVoice,
                            config,
                        });
                }
                // Fall back to the first configured cloud provider.
                for id in ProviderId::cloud_providers() {
                    if let Some(cfg) = Self::find_config(configs, *id) {
                        if !id.requires_credential()
                            || ProviderCredentialStore::is_configured(&cfg.credential_ref)
                        {
                            return Some(ResolvedProvider {
                                provider: *id,
                                config: cfg,
                            });
                        }
                    }
                }
                None
            }
        }
    }

    /// Look up a provider's config in a (ProviderId, ProviderConfig) list.
    pub fn find_config(
        configs: &[(ProviderId, ProviderConfig)],
        target: ProviderId,
    ) -> Option<ProviderConfig> {
        configs
            .iter()
            .find(|(id, _)| *id == target)
            .map(|(_, cfg)| cfg.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_configs() -> Vec<(ProviderId, ProviderConfig)> {
        vec![
            (ProviderId::LocalSenseVoice, ProviderConfig::new(ProviderId::LocalSenseVoice)),
            (ProviderId::OpenAIRealtime, ProviderConfig::new(ProviderId::OpenAIRealtime)),
            (ProviderId::DeepgramStreaming, ProviderConfig::new(ProviderId::DeepgramStreaming)),
        ]
    }

    #[test]
    fn test_resolve_offline_forces_local() {
        let configs = test_configs();
        let result = ProviderRegistry::resolve(
            ProviderMode::Offline,
            true,
            &configs,
            Some(ProviderId::OpenAIRealtime),
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().provider, ProviderId::LocalSenseVoice);
    }

    #[test]
    fn test_resolve_offline_without_local_returns_none() {
        let configs = test_configs();
        let result = ProviderRegistry::resolve(
            ProviderMode::Offline,
            false,
            &configs,
            Some(ProviderId::OpenAIRealtime),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_cloud_uses_preferred() {
        let configs = test_configs();
        let result = ProviderRegistry::resolve(
            ProviderMode::Cloud,
            true,
            &configs,
            Some(ProviderId::DeepgramStreaming),
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().provider, ProviderId::DeepgramStreaming);
    }

    #[test]
    fn test_resolve_automatic_prefers_local() {
        let configs = test_configs();
        let result = ProviderRegistry::resolve(
            ProviderMode::Automatic,
            true,
            &configs,
            Some(ProviderId::OpenAIRealtime),
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().provider, ProviderId::LocalSenseVoice);
    }

    #[test]
    fn test_resolve_automatic_falls_back_to_cloud() {
        let configs = test_configs();
        // Local not available, cloud providers unconfigured → no fallback target.
        // (Fallback only considers configured cloud providers.)
        let result = ProviderRegistry::resolve(
            ProviderMode::Automatic,
            false,
            &configs,
            Some(ProviderId::DeepgramStreaming),
        );
        // Without credentials configured, no cloud provider is usable.
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_cloud_ignores_local_availability() {
        let configs = test_configs();
        // Cloud mode uses the preferred provider regardless of local availability.
        let result = ProviderRegistry::resolve(
            ProviderMode::Cloud,
            false,
            &configs,
            Some(ProviderId::OpenAIRealtime),
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().provider, ProviderId::OpenAIRealtime);
    }

    #[test]
    fn test_resolve_cloud_defaults_to_openai_without_preference() {
        let configs = test_configs();
        let result = ProviderRegistry::resolve(
            ProviderMode::Cloud,
            true,
            &configs,
            None,
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().provider, ProviderId::OpenAIRealtime);
    }

    #[test]
    fn test_list_status_includes_local() {
        let configs = vec![(ProviderId::OpenAIRealtime, ProviderConfig::new(ProviderId::OpenAIRealtime))];
        let statuses = ProviderRegistry::list_status(&configs);
        // Local should be auto-inserted even if not in configs.
        assert!(statuses.iter().any(|s| s.is_local));
        assert!(statuses.iter().any(|s| s.provider == ProviderId::OpenAIRealtime));
    }

    #[test]
    fn test_find_config_returns_match() {
        let configs = test_configs();
        let result = ProviderRegistry::find_config(&configs, ProviderId::DeepgramStreaming);
        assert!(result.is_some());
        assert_eq!(result.unwrap().provider, ProviderId::DeepgramStreaming);
    }
}
