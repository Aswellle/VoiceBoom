pub mod adapters;
pub mod aggregator;
pub mod engine_trait;
pub mod latency;
pub mod session;
pub mod streaming;

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod failure_tests;

// Re-export the new session-oriented types as the primary API.
pub use session::{AsrEvent, AsrSession, FakeAsrSession};
pub use adapters::openai_realtime::OpenaiRealtimeAdapter;
pub use latency::{LatencyMetrics, LatencyTracker, LatencyRecord};
// Re-export core types for use by adapters and commands.
pub use engine_trait::{AsrConfig, AsrEngineType};
// Legacy types remain available for backward compatibility during migration.
pub use engine_trait::AsrResult;
pub use engine_trait::StreamingAsrEngine;
