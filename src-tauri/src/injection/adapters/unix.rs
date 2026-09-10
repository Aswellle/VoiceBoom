//! macOS/Linux injection adapter.
//!
//! Phase 8: Platform adapter for non-Windows systems.

use super::super::model::{InjectionResult, InjectionTarget, TargetValidation};

/// Execute injection on macOS/Linux.
pub fn inject(target: &InjectionTarget, text: &str) -> InjectionResult {
    // Validate target first.
    match validate_target(target) {
        TargetValidation::Valid => {}
        TargetValidation::FocusChanged => {
            return InjectionResult::TargetChanged {
                captured: target.exe.clone(),
            };
        }
        TargetValidation::WindowDestroyed | TargetValidation::ProcessExited => {
            return InjectionResult::Failed {
                reason: "目标窗口已关闭".into(),
            };
        }
        TargetValidation::PermissionDenied => {
            return InjectionResult::PermissionDenied {
                reason: "权限不足".into(),
            };
        }
    }

    // Use the existing inject.rs implementation.
    let mode = crate::inject::InjectionMode::Clipboard;
    crate::inject::inject(text, &mode)
}

/// Validate the target on macOS/Linux.
fn validate_target(target: &InjectionTarget) -> TargetValidation {
    if target.is_valid() {
        TargetValidation::Valid
    } else {
        // On non-Windows, we can't easily distinguish focus change from window destruction.
        TargetValidation::FocusChanged
    }
}
