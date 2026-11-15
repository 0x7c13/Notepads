use merman_core::diagrams::sequence::{SequenceMessage, SequenceMessageKind};
use std::collections::BTreeMap;

pub(crate) fn sequence_activation_start_x(center_x: f64, stacked_size: usize, width: f64) -> f64 {
    center_x + (((stacked_size as f64) - 1.0) * width) / 2.0
}

pub(crate) fn sequence_activation_stack_bounds(
    depth: usize,
    center_x: f64,
    width: f64,
) -> (f64, f64) {
    let mut left = center_x - 1.0;
    let mut right = center_x + 1.0;
    if depth == 0 {
        return (left, right);
    }

    let first_start_x = sequence_activation_start_x(center_x, 0, width);
    let last_start_x = sequence_activation_start_x(center_x, depth - 1, width);
    left = left.min(first_start_x).min(last_start_x);
    right = right.max(first_start_x + width).max(last_start_x + width);
    (left, right)
}

pub(super) struct SequenceActivationState {
    width: f64,
    depths: BTreeMap<usize, usize>,
}

impl SequenceActivationState {
    pub(super) fn new(width: f64) -> Self {
        Self {
            width,
            depths: BTreeMap::new(),
        }
    }

    pub(super) fn handle_directive(
        &mut self,
        msg: &SequenceMessage,
        actor_index: &std::collections::HashMap<&str, usize>,
        actor_centers_x: &[f64],
    ) -> bool {
        match msg.semantic_kind() {
            SequenceMessageKind::ActivationStart => {
                let Some(actor_id) = msg.from.as_deref() else {
                    return true;
                };
                let Some(&idx) = actor_index.get(actor_id) else {
                    return true;
                };
                if actor_centers_x.get(idx).is_none() {
                    return true;
                }
                let depth = self.depths.entry(idx).or_default();
                *depth = depth.saturating_add(1);
                true
            }
            SequenceMessageKind::ActivationEnd => {
                let Some(actor_id) = msg.from.as_deref() else {
                    return true;
                };
                let Some(&idx) = actor_index.get(actor_id) else {
                    return true;
                };
                if let Some(depth) = self.depths.get_mut(&idx) {
                    *depth = depth.saturating_sub(1);
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn actor_bounds(&self, actor_index: usize, center_x: f64) -> (f64, f64) {
        sequence_activation_stack_bounds(
            self.depths.get(&actor_index).copied().unwrap_or_default(),
            center_x,
            self.width,
        )
    }

    pub(super) fn width(&self) -> f64 {
        self.width
    }
}

#[cfg(test)]
mod tests {
    use super::{sequence_activation_stack_bounds, sequence_activation_start_x};

    #[test]
    fn activation_start_x_matches_mermaid_stack_offsets() {
        assert_eq!(sequence_activation_start_x(100.0, 0, 10.0), 95.0);
        assert_eq!(sequence_activation_start_x(100.0, 1, 10.0), 100.0);
        assert_eq!(sequence_activation_start_x(100.0, 2, 10.0), 105.0);
    }

    #[test]
    fn activation_stack_bounds_fold_full_active_stack() {
        assert_eq!(
            sequence_activation_stack_bounds(0, 100.0, 10.0),
            (99.0, 101.0)
        );
        assert_eq!(
            sequence_activation_stack_bounds(1, 100.0, 10.0),
            (95.0, 105.0)
        );
        assert_eq!(
            sequence_activation_stack_bounds(2, 100.0, 10.0),
            (95.0, 110.0)
        );
        assert_eq!(
            sequence_activation_stack_bounds(2, 100.0, -10.0),
            (99.0, 101.0)
        );
    }

    #[test]
    fn activation_stack_bounds_only_need_progression_endpoints() {
        assert_eq!(
            sequence_activation_stack_bounds(10_000, 100.0, 10.0),
            (95.0, 50_100.0)
        );
    }
}
