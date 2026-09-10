//! Windows injection adapter.
//!
//! Phase 7: Connect existing win-text-inject to the injection pipeline.

use super::super::model::{InjectionMethod, InjectionResult, InjectionTarget, TargetValidation};

/// Execute injection on Windows using the vendored win-text-inject crate.
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
    // Use the existing inject.rs implementation and map results.
    let mode = crate::inject::InjectionMode::Clipboard;
    let outcome = crate::inject::inject(text, &mode);
    match outcome {
        crate::inject::InjectionResult::Injected => InjectionResult::Injected {
            method: InjectionMethod::ClipboardPaste,
            verified: true,
        },
        crate::inject::InjectionResult::ClipboardFallback => InjectionResult::ClipboardFallback {
            reason: "UIPI 阻止确认".into(),
        },
        crate::inject::InjectionResult::PermissionDenied => InjectionResult::PermissionDenied {
            reason: "权限不足".into(),
        },
        crate::inject::InjectionResult::TargetUnavailable => InjectionResult::Failed {
            reason: "目标不可用".into(),
        },
        crate::inject::InjectionResult::Failed { reason } => InjectionResult::Failed { reason },
    }
}

/// Validate the target on Windows.
fn validate_target(target: &InjectionTarget) -> TargetValidation {
    if target.is_valid() {
        TargetValidation::Valid
    } else if target.is_foreground() {
        TargetValidation::FocusChanged
    } else {
        TargetValidation::WindowDestroyed
    }
}
