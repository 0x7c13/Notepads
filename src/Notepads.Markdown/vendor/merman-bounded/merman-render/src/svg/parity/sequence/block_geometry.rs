use super::super::*;
use super::SequenceEmitCheckpoints;
use super::model::SequenceSvgModel;
use crate::sequence::{
    SEQUENCE_FRAME_GEOM_PAD_PX, SEQUENCE_FRAME_SIDE_PAD_PX, SEQUENCE_SELF_MESSAGE_FRAME_EXTRA_Y_PX,
};
use merman_core::diagrams::sequence::SequenceMessage;
use rustc_hash::FxHashMap;

pub(super) fn frame_x_from_actors(
    model: &SequenceSvgModel,
    nodes_by_id: &FxHashMap<&str, &LayoutNode>,
    checkpoints: SequenceEmitCheckpoints<'_>,
) -> Result<Option<(f64, f64)>> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for (actor_index, actor_id) in model.actor_order.iter().enumerate() {
        checkpoints.checkpoint_loop(actor_index)?;
        let node_id = format!("actor-top-{actor_id}");
        let Some(n) = nodes_by_id.get(node_id.as_str()).copied() else {
            return Ok(None);
        };
        min_x = min_x.min(n.x);
        max_x = max_x.max(n.x);
    }
    if !min_x.is_finite() || !max_x.is_finite() {
        return Ok(None);
    }
    checkpoints.checkpoint()?;
    Ok(Some((
        min_x - SEQUENCE_FRAME_SIDE_PAD_PX,
        max_x + SEQUENCE_FRAME_SIDE_PAD_PX,
    )))
}

#[derive(Debug, Clone, Copy)]
enum SelfOnlyActor<'a> {
    None,
    One(&'a str),
    Mixed,
}

impl<'a> SelfOnlyActor<'a> {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, value) | (value, Self::None) => value,
            (Self::One(left), Self::One(right)) if left == right => Self::One(left),
            _ => Self::Mixed,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SequenceBlockGeometry<'a> {
    geom_min_x: f64,
    geom_max_x: f64,
    min_actor_center_x: f64,
    max_actor_center_x: f64,
    min_actor_left_x: f64,
    frame_min_y: f64,
    frame_max_y: f64,
    self_only_actor: SelfOnlyActor<'a>,
}

impl<'a> SequenceBlockGeometry<'a> {
    pub(super) fn empty() -> Self {
        Self {
            geom_min_x: f64::INFINITY,
            geom_max_x: f64::NEG_INFINITY,
            min_actor_center_x: f64::INFINITY,
            max_actor_center_x: f64::NEG_INFINITY,
            min_actor_left_x: f64::INFINITY,
            frame_min_y: f64::INFINITY,
            frame_max_y: f64::NEG_INFINITY,
            self_only_actor: SelfOnlyActor::None,
        }
    }

    pub(super) fn from_message(
        msg: &'a SequenceMessage,
        actor_nodes_by_id: &FxHashMap<&str, &LayoutNode>,
        edges_by_id: &FxHashMap<&str, &crate::model::LayoutEdge>,
        nodes_by_id: &FxHashMap<&str, &LayoutNode>,
    ) -> Self {
        let mut geometry = Self::empty();

        let note_node_id = format!("note-{}", msg.id);
        let note = nodes_by_id.get(note_node_id.as_str()).copied();
        if let Some(note) = note {
            geometry.geom_min_x = note.x - note.width / 2.0 - SEQUENCE_FRAME_GEOM_PAD_PX;
            geometry.geom_max_x = note.x + note.width / 2.0 + SEQUENCE_FRAME_GEOM_PAD_PX;
        }

        let mut frame_y_range =
            note.map(|note| (note.y - note.height / 2.0, note.y + note.height / 2.0));

        if let (Some(from), Some(to)) = (msg.from.as_deref(), msg.to.as_deref()) {
            geometry.self_only_actor = if from == to {
                SelfOnlyActor::One(from)
            } else {
                SelfOnlyActor::Mixed
            };

            for actor_id in [from, to] {
                let Some(actor) = actor_nodes_by_id.get(actor_id).copied() else {
                    continue;
                };
                geometry.min_actor_center_x = geometry.min_actor_center_x.min(actor.x);
                geometry.max_actor_center_x = geometry.max_actor_center_x.max(actor.x);
                geometry.min_actor_left_x =
                    geometry.min_actor_left_x.min(actor.x - actor.width / 2.0);
            }

            let edge_id = format!("msg-{}", msg.id);
            if let Some(edge) = edges_by_id.get(edge_id.as_str()).copied() {
                for point in &edge.points {
                    geometry.geom_min_x = geometry.geom_min_x.min(point.x);
                    geometry.geom_max_x = geometry.geom_max_x.max(point.x);
                }
                if let Some(label) = edge.label.as_ref() {
                    geometry.geom_min_x = geometry
                        .geom_min_x
                        .min(label.x - label.width / 2.0 - SEQUENCE_FRAME_GEOM_PAD_PX);
                    geometry.geom_max_x = geometry
                        .geom_max_x
                        .max(label.x + label.width / 2.0 + SEQUENCE_FRAME_GEOM_PAD_PX);
                }
                if let Some(line_y) = edge.points.first().map(|point| point.y) {
                    let frame_extra = if from == to {
                        SEQUENCE_SELF_MESSAGE_FRAME_EXTRA_Y_PX
                    } else {
                        0.0
                    };
                    frame_y_range = Some((line_y, line_y + frame_extra));
                }
            }
        }

        if let Some((min_y, max_y)) = frame_y_range {
            geometry.frame_min_y = min_y;
            geometry.frame_max_y = max_y;
        }
        geometry
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.geom_min_x = self.geom_min_x.min(other.geom_min_x);
        self.geom_max_x = self.geom_max_x.max(other.geom_max_x);
        self.min_actor_center_x = self.min_actor_center_x.min(other.min_actor_center_x);
        self.max_actor_center_x = self.max_actor_center_x.max(other.max_actor_center_x);
        self.min_actor_left_x = self.min_actor_left_x.min(other.min_actor_left_x);
        self.frame_min_y = self.frame_min_y.min(other.frame_min_y);
        self.frame_max_y = self.frame_max_y.max(other.frame_max_y);
        self.self_only_actor = self.self_only_actor.merge(other.self_only_actor);
    }

    pub(super) fn merged(mut self, other: Self) -> Self {
        self.merge(other);
        self
    }

    pub(super) fn frame_x(
        self,
        actor_nodes_by_id: &FxHashMap<&str, &LayoutNode>,
    ) -> Option<(f64, f64, f64)> {
        if !self.min_actor_center_x.is_finite() || !self.max_actor_center_x.is_finite() {
            return None;
        }

        let mut x1 = self.min_actor_center_x - SEQUENCE_FRAME_SIDE_PAD_PX;
        let mut x2 = self.max_actor_center_x + SEQUENCE_FRAME_SIDE_PAD_PX;
        if self.geom_min_x.is_finite() {
            x1 = x1.min(self.geom_min_x);
        }
        if self.geom_max_x.is_finite() {
            x2 = x2.max(self.geom_max_x);
        }

        if let SelfOnlyActor::One(actor_id) = self.self_only_actor
            && let Some(actor) = actor_nodes_by_id.get(actor_id).copied()
        {
            let left = actor.x - actor.width / 2.0;
            let right = actor.x + actor.width / 2.0;
            let minimum_x1 = left - 5.0;
            let minimum_x2 = right + 15.0;
            if (x2 - x1) < (minimum_x2 - minimum_x1) - 1.0 {
                x1 = x1.min(minimum_x1);
                x2 = x2.max(minimum_x2);
            }
        }

        Some((x1, x2, self.min_actor_left_x))
    }

    pub(super) fn frame_y_range(self) -> Option<(f64, f64)> {
        (self.frame_min_y.is_finite() && self.frame_max_y.is_finite())
            .then_some((self.frame_min_y, self.frame_max_y))
    }

    #[cfg(test)]
    pub(super) fn test_y_range(min_y: f64, max_y: f64) -> Self {
        Self {
            frame_min_y: min_y,
            frame_max_y: max_y,
            ..Self::empty()
        }
    }
}
