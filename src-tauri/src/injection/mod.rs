//! Text injection pipeline.
//!
//! Phase 1-3: Domain model, target capture, injection controller.
//!
//! Architecture Lock A: Target must be captured at Press time.
//! Architecture Lock B: Injection must be bound to session_id.
//! Architecture Lock C: Injection must not re-acquire foreground target.
//! Architecture Lock D: Old session results must not inject into new session.
//! Architecture Lock E: Same utterance can only be injected once.
//! Architecture Lock F: Target change must result in ClipboardFallback.

pub mod adapters;
pub mod controller;
pub mod model;

pub use controller::InjectionController;
pub use model::{
    InjectionKey, InjectionMethod, InjectionMode, InjectionResult,
    InjectionTarget, TargetValidation,
};
