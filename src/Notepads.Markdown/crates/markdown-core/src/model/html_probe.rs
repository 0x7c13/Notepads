// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::CoreError;
use crate::policy::Operation;

const WINDOW_BYTES: usize = 4096;

pub(super) fn block_candidates(
    text: &str,
    html: bool,
    directives: bool,
    op: &mut Operation,
) -> Result<(bool, bool), CoreError> {
    op.checkpoint_now()?;
    let bytes = text.as_bytes();
    let mut found = (false, false);
    for start in (0..bytes.len()).step_by(WINDOW_BYTES) {
        let end = (start + WINDOW_BYTES).min(bytes.len());
        let candidate_end = if directives && !found.1 {
            (end + 2).min(bytes.len())
        } else {
            end
        };
        op.scan((candidate_end - start) as u64)?;
        if html && !found.0 {
            found.0 = bytes[start..end].contains(&b'<');
        }
        if directives && !found.1 {
            found.1 = bytes[start..candidate_end]
                .windows(3)
                .any(|value| value == b":::");
        }
        if (!html || found.0) && (!directives || found.1) {
            break;
        }
    }
    op.checkpoint_now()?;
    Ok(found)
}

#[cfg(test)]
pub(super) fn contains_marker(text: &str, op: &mut Operation) -> Result<bool, CoreError> {
    block_candidates(text, true, false, op).map(|found| found.0)
}

pub(super) fn contains_inline_region(text: &str, op: &mut Operation) -> Result<bool, CoreError> {
    op.checkpoint_now()?;
    let bytes = text.as_bytes();
    for start in (0..bytes.len()).step_by(WINDOW_BYTES) {
        let end = (start + WINDOW_BYTES + 7).min(bytes.len());
        let window = &bytes[start..end];
        op.scan(window.len() as u64)?;
        if crate::html::ruby::marker(window) || crate::html::picture::marker(window) {
            op.checkpoint_now()?;
            return Ok(true);
        }
    }
    op.checkpoint_now()?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{CancellationToken, ModelLimits, SourceBudget, SourceLimits};

    #[test]
    fn no_marker_pays_for_every_byte_across_utf8_windows() {
        let text = format!(
            "{}😀{}",
            "a".repeat(WINDOW_BYTES - 1),
            "中".repeat(WINDOW_BYTES)
        );
        let budget =
            SourceBudget::new_with_model_limits(SourceLimits::default(), ModelLimits::default())
                .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        assert!(!contains_marker(&text, &mut op).unwrap());
        assert_eq!(op.stats().unwrap().scanned_bytes(), text.len() as u64);
    }

    #[test]
    fn marker_stops_after_its_admitted_window() {
        let text = format!(
            "{}<{}",
            "a".repeat(WINDOW_BYTES),
            "b".repeat(WINDOW_BYTES * 2)
        );
        let budget =
            SourceBudget::new_with_model_limits(SourceLimits::default(), ModelLimits::default())
                .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        assert!(contains_marker(&text, &mut op).unwrap());
        assert_eq!(
            op.stats().unwrap().scanned_bytes(),
            (WINDOW_BYTES * 2) as u64
        );
    }

    #[test]
    fn cancellation_rejects_before_scanning() {
        let budget =
            SourceBudget::new_with_model_limits(SourceLimits::default(), ModelLimits::default())
                .unwrap();
        let token = CancellationToken::new();
        token.cancel();
        let mut op = budget.begin(token);
        assert_eq!(contains_marker("<", &mut op), Err(CoreError::Cancelled));
        assert_eq!(op.stats().unwrap().scanned_bytes(), 0);
    }

    #[test]
    fn short_scan_observes_completion_deadline_for_both_results() {
        const LIMIT: u64 = 500_000_000;
        for budget in [SourceBudget::task_fixture()] {
            for text in ["plain", "<"] {
                let mut op = budget.begin(CancellationToken::new());
                op.set_clock_samples_for_test(&[LIMIT - 1, LIMIT]);
                let error = CoreError::DeadlineExceeded {
                    elapsed_ns: LIMIT,
                    limit_ns: LIMIT,
                };
                assert_eq!(contains_marker(text, &mut op), Err(error));
                assert_eq!(op.first_fault(), Some(error));
                if let Some(stats) = op.stats() {
                    assert_eq!(stats.scanned_bytes(), text.len() as u64);
                }
            }
        }
    }
}
