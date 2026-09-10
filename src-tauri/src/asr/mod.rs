pub mod adapters;
pub mod engine_trait;
pub mod session;
pub mod streaming;

// Re-export the new session-oriented types as the primary API.
pub use session::{AsrEvent, AsrSession, FakeAsrSession};
pub use adapters::openai_realtime::OpenaiRealtimeAdapter;
// Legacy types remain available for backward compatibility during migration.
pub use engine_trait::{AsrConfig, AsrEngineType, AsrResult, StreamingAsrEngine};
