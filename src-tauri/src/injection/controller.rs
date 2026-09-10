//! Injection Controller.
//!
//! Phase 3: Central controller for managing injection lifecycle.
//!
//! Architecture Lock D: Old session results must not inject into new session.
//! Architecture Lock E: Same utterance can only be injected once.
//! Architecture Lock F: Target change must result in ClipboardFallback.

use std::collections::HashSet;
use std::sync::Mutex;

use super::model::{
    InjectionKey, InjectionMode, InjectionRequest, InjectionResult, InjectionState,
    InjectionTarget, TargetValidation,
};

/// Manages injection lifecycle: dedupe, stale session guard, in-flight protection.
pub struct InjectionController {
    /// Set of completed injection keys (session_id + utterance_id).
    completed: Mutex<HashSet<InjectionKey>>,
    /// Set of in-flight injection keys.
    in_flight: Mutex<HashSet<InjectionKey>>,
}

impl InjectionController {
    pub fn new() -> Self {
        Self {
            completed: Mutex::new(HashSet::new()),
            in_flight: Mutex::new(HashSet::new()),
        }
    }

    /// Validate an injection request before execution.
    ///
    /// Returns:
    /// - `Ok(())` if injection should proceed
    /// - `Err(InjectionResult)` if injection should be skipped (with reason)
    pub fn validate(
        &self,
        request: &InjectionRequest,
        active_session_id: Option<&str>,
    ) -> Result<(), InjectionResult> {
        let key = InjectionKey::new(&request.session_id, &request.utterance_id);

        // Architecture Lock E: Same utterance can only be injected once.
        if self.completed.lock().unwrap().contains(&key) {
            return Err(InjectionResult::DuplicateIgnored);
        }

        // Architecture Lock D: Old session results must not inject into new session.
        if let Some(active_id) = active_session_id {
            if active_id != request.session_id {
                return Err(InjectionResult::StaleSession);
            }
        }

        // Check if already in-flight.
        if self.in_flight.lock().unwrap().contains(&key) {
            return Err(InjectionResult::DuplicateIgnored);
        }

        Ok(())
    }

    /// Mark an injection as in-flight.
    pub fn mark_in_flight(&self, key: &InjectionKey) {
        self.in_flight.lock().unwrap().insert(key.clone());
    }

    /// Mark an injection as completed.
    pub fn mark_completed(&self, key: &InjectionKey) {
        self.in_flight.lock().unwrap().remove(key);
        self.completed.lock().unwrap().insert(key.clone());
    }

    /// Mark an injection as failed.
    pub fn mark_failed(&self, key: &InjectionKey) {
        self.in_flight.lock().unwrap().remove(key);
    }

    /// Validate the target before injection.
    ///
    /// Architecture Lock F: Target change must result in ClipboardFallback.
    pub fn validate_target(&self, target: &InjectionTarget) -> TargetValidation {
        if !target.is_valid() {
            if target.is_foreground() {
                // Window exists but different process
                TargetValidation::FocusChanged
            } else {
                TargetValidation::WindowDestroyed
            }
        } else {
            TargetValidation::Valid
        }
    }

    /// Create an injection request from session data.
    pub fn create_request(
        &self,
        session_id: impl Into<String>,
        utterance_id: impl Into<String>,
        text: impl Into<String>,
        target: InjectionTarget,
        mode: InjectionMode,
    ) -> InjectionRequest {
        InjectionRequest {
            session_id: session_id.into(),
            utterance_id: utterance_id.into(),
            text: text.into(),
            target,
            mode,
            created_at: std::time::Instant::now(),
        }
    }
}

impl Default for InjectionController {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_target() -> InjectionTarget {
        InjectionTarget::new(12345, 100, "notepad.exe".into(), "Notepad".into())
    }

    #[test]
    fn test_dedupe_same_utterance() {
        let controller = InjectionController::new();
        let target = make_target();

        let request = controller.create_request("sess-1", "utt-1", "hello", target.clone(), InjectionMode::Clipboard);

        // First validation should pass.
        assert!(controller.validate(&request, Some("sess-1")).is_ok());

        // Mark as completed.
        controller.mark_completed(&InjectionKey::new("sess-1", "utt-1"));

        // Second validation should fail with DuplicateIgnored.
        let result = controller.validate(&request, Some("sess-1"));
        assert!(matches!(result, Err(InjectionResult::DuplicateIgnored)));
    }

    #[test]
    fn test_stale_session() {
        let controller = InjectionController::new();
        let target = make_target();

        // Request from old session.
        let request = controller.create_request("sess-old", "utt-1", "hello", target, InjectionMode::Clipboard);

        // Active session is different.
        let result = controller.validate(&request, Some("sess-new"));
        assert!(matches!(result, Err(InjectionResult::StaleSession)));
    }

    #[test]
    fn test_in_flight_protection() {
        let controller = InjectionController::new();
        let target = make_target();

        let request = controller.create_request("sess-1", "utt-1", "hello", target, InjectionMode::Clipboard);

        // Mark as in-flight.
        controller.mark_in_flight(&InjectionKey::new("sess-1", "utt-1"));

        // Validation should fail.
        let result = controller.validate(&request, Some("sess-1"));
        assert!(matches!(result, Err(InjectionResult::DuplicateIgnored)));
    }

    #[test]
    fn test_target_validation() {
        let controller = InjectionController::new();
        let target = make_target();

        // On non-Windows, target is valid for 30s.
        let validation = controller.validate_target(&target);
        assert_eq!(validation, TargetValidation::Valid);
    }
}
