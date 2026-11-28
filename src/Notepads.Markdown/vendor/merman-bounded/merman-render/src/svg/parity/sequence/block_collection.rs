use super::SequenceEmitCheckpoints;
use super::block_geometry::SequenceBlockGeometry;
use super::model::SequenceSvgModel;
use crate::Result;
use crate::model::{LayoutEdge, LayoutNode, SequenceBlockLayout};
use merman_core::diagrams::sequence::{
    SequenceControlKind, SequenceControlRole, SequenceMessage, SequenceMessageKind,
};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub(super) struct AltSection<'a> {
    pub(super) label_id: &'a str,
    pub(super) raw_label: &'a str,
    pub(super) geometry: SequenceBlockGeometry<'a>,
    pub(super) separator_y: Option<f64>,
}

#[derive(Debug, Clone)]
pub(super) enum SequenceBlock<'a> {
    Alt {
        control_id: &'a str,
        sections: Vec<AltSection<'a>>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Opt {
        control_id: &'a str,
        label_id: &'a str,
        raw_label: &'a str,
        geometry: SequenceBlockGeometry<'a>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Break {
        control_id: &'a str,
        label_id: &'a str,
        raw_label: &'a str,
        geometry: SequenceBlockGeometry<'a>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Par {
        control_id: &'a str,
        sections: Vec<AltSection<'a>>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Loop {
        control_id: &'a str,
        label_id: &'a str,
        raw_label: &'a str,
        geometry: SequenceBlockGeometry<'a>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Critical {
        control_id: &'a str,
        sections: Vec<AltSection<'a>>,
        layout: Option<&'a SequenceBlockLayout>,
    },
}

#[derive(Debug, Clone)]
enum BlockStackEntry<'a> {
    Alt {
        sections: Vec<AltSection<'a>>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Loop {
        label_id: &'a str,
        raw_label: &'a str,
        geometry: SequenceBlockGeometry<'a>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Opt {
        label_id: &'a str,
        raw_label: &'a str,
        geometry: SequenceBlockGeometry<'a>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Break {
        label_id: &'a str,
        raw_label: &'a str,
        geometry: SequenceBlockGeometry<'a>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Par {
        sections: Vec<AltSection<'a>>,
        layout: Option<&'a SequenceBlockLayout>,
    },
    Critical {
        sections: Vec<AltSection<'a>>,
        layout: Option<&'a SequenceBlockLayout>,
    },
}

impl<'a> BlockStackEntry<'a> {
    fn include_geometry(&mut self, geometry: SequenceBlockGeometry<'a>) {
        match self {
            Self::Alt { sections, .. }
            | Self::Par { sections, .. }
            | Self::Critical { sections, .. } => {
                if let Some(section) = sections.last_mut() {
                    section.geometry.merge(geometry);
                }
            }
            Self::Loop {
                geometry: current, ..
            }
            | Self::Opt {
                geometry: current, ..
            }
            | Self::Break {
                geometry: current, ..
            } => current.merge(geometry),
        }
    }

    fn geometry(&self) -> SequenceBlockGeometry<'a> {
        match self {
            Self::Alt { sections, .. }
            | Self::Par { sections, .. }
            | Self::Critical { sections, .. } => sections
                .iter()
                .fold(SequenceBlockGeometry::empty(), |geometry, section| {
                    geometry.merged(section.geometry)
                }),
            Self::Loop { geometry, .. }
            | Self::Opt { geometry, .. }
            | Self::Break { geometry, .. } => *geometry,
        }
    }
}

pub(super) fn collect_sequence_blocks<'a>(
    model: &'a SequenceSvgModel,
    actor_nodes_by_id: &FxHashMap<&str, &LayoutNode>,
    edges_by_id: &FxHashMap<&str, &LayoutEdge>,
    nodes_by_id: &FxHashMap<&str, &LayoutNode>,
    block_layouts_by_id: &'a FxHashMap<String, SequenceBlockLayout>,
    checkpoints: SequenceEmitCheckpoints<'_>,
) -> Result<(Vec<Option<usize>>, Vec<SequenceBlock<'a>>)> {
    collect_sequence_blocks_with(
        model,
        block_layouts_by_id,
        |message| {
            SequenceBlockGeometry::from_message(
                message,
                actor_nodes_by_id,
                edges_by_id,
                nodes_by_id,
            )
        },
        checkpoints,
    )
}

fn collect_sequence_blocks_with<'a>(
    model: &'a SequenceSvgModel,
    block_layouts_by_id: &'a FxHashMap<String, SequenceBlockLayout>,
    mut message_geometry: impl FnMut(&'a SequenceMessage) -> SequenceBlockGeometry<'a>,
    checkpoints: SequenceEmitCheckpoints<'_>,
) -> Result<(Vec<Option<usize>>, Vec<SequenceBlock<'a>>)> {
    let mut blocks_by_end_index = vec![None; model.messages.len()];
    let mut blocks = Vec::with_capacity(model.messages.len().div_ceil(2));
    let mut stack = Vec::new();

    for (message_index, message) in model.messages.iter().enumerate() {
        checkpoints.checkpoint_loop(message_index)?;
        let raw_label = message.message_text();
        let control = message
            .control_semantics()
            .map(|semantics| (semantics.kind, semantics.role));
        match (message.semantic_kind(), control) {
            (SequenceMessageKind::Note, _) => {
                if !stack.is_empty() {
                    include_message_geometry(&mut stack, message_geometry(message));
                }
            }
            (_, Some((SequenceControlKind::Loop, SequenceControlRole::Start))) => {
                stack.push(BlockStackEntry::Loop {
                    label_id: message.id.as_str(),
                    raw_label,
                    geometry: SequenceBlockGeometry::empty(),
                    layout: block_layouts_by_id.get(&message.id),
                })
            }
            (_, Some((SequenceControlKind::Loop, SequenceControlRole::End))) => {
                if let Some(entry) = pop_and_propagate(&mut stack)
                    && let BlockStackEntry::Loop {
                        label_id,
                        raw_label,
                        geometry,
                        layout,
                    } = entry
                {
                    push_block(
                        &mut blocks_by_end_index,
                        &mut blocks,
                        message_index,
                        SequenceBlock::Loop {
                            control_id: message.id.as_str(),
                            label_id,
                            raw_label,
                            geometry,
                            layout,
                        },
                    );
                }
            }
            (_, Some((SequenceControlKind::Opt, SequenceControlRole::Start))) => {
                stack.push(BlockStackEntry::Opt {
                    label_id: message.id.as_str(),
                    raw_label,
                    geometry: SequenceBlockGeometry::empty(),
                    layout: block_layouts_by_id.get(&message.id),
                });
            }
            (_, Some((SequenceControlKind::Opt, SequenceControlRole::End))) => {
                if let Some(entry) = pop_and_propagate(&mut stack)
                    && let BlockStackEntry::Opt {
                        label_id,
                        raw_label,
                        geometry,
                        layout,
                    } = entry
                {
                    push_block(
                        &mut blocks_by_end_index,
                        &mut blocks,
                        message_index,
                        SequenceBlock::Opt {
                            control_id: message.id.as_str(),
                            label_id,
                            raw_label,
                            geometry,
                            layout,
                        },
                    );
                }
            }
            (_, Some((SequenceControlKind::Break, SequenceControlRole::Start))) => {
                stack.push(BlockStackEntry::Break {
                    label_id: message.id.as_str(),
                    raw_label,
                    geometry: SequenceBlockGeometry::empty(),
                    layout: block_layouts_by_id.get(&message.id),
                });
            }
            (_, Some((SequenceControlKind::Break, SequenceControlRole::End))) => {
                if let Some(entry) = pop_and_propagate(&mut stack)
                    && let BlockStackEntry::Break {
                        label_id,
                        raw_label,
                        geometry,
                        layout,
                    } = entry
                {
                    push_block(
                        &mut blocks_by_end_index,
                        &mut blocks,
                        message_index,
                        SequenceBlock::Break {
                            control_id: message.id.as_str(),
                            label_id,
                            raw_label,
                            geometry,
                            layout,
                        },
                    );
                }
            }
            (_, Some((SequenceControlKind::Alt, SequenceControlRole::Start))) => {
                stack.push(BlockStackEntry::Alt {
                    sections: vec![AltSection {
                        label_id: message.id.as_str(),
                        raw_label,
                        geometry: SequenceBlockGeometry::empty(),
                        separator_y: None,
                    }],
                    layout: block_layouts_by_id.get(&message.id),
                });
            }
            (_, Some((SequenceControlKind::Alt, SequenceControlRole::Separator))) => {
                if let Some(BlockStackEntry::Alt { sections, layout }) = stack.last_mut() {
                    let separator_y = layout
                        .as_deref()
                        .and_then(|layout| layout.section_ys_by_id.get(&message.id))
                        .copied();
                    sections.push(AltSection {
                        label_id: message.id.as_str(),
                        raw_label,
                        geometry: SequenceBlockGeometry::empty(),
                        separator_y,
                    });
                }
            }
            (_, Some((SequenceControlKind::Alt, SequenceControlRole::End))) => {
                if let Some(entry) = pop_and_propagate(&mut stack)
                    && let BlockStackEntry::Alt { sections, layout } = entry
                {
                    push_block(
                        &mut blocks_by_end_index,
                        &mut blocks,
                        message_index,
                        SequenceBlock::Alt {
                            control_id: message.id.as_str(),
                            sections,
                            layout,
                        },
                    );
                }
            }
            (
                _,
                Some((
                    SequenceControlKind::Par | SequenceControlKind::ParOver,
                    SequenceControlRole::Start,
                )),
            ) => stack.push(BlockStackEntry::Par {
                sections: vec![AltSection {
                    label_id: message.id.as_str(),
                    raw_label,
                    geometry: SequenceBlockGeometry::empty(),
                    separator_y: None,
                }],
                layout: block_layouts_by_id.get(&message.id),
            }),
            (_, Some((SequenceControlKind::Par, SequenceControlRole::Separator))) => {
                if let Some(BlockStackEntry::Par { sections, layout }) = stack.last_mut() {
                    let separator_y = layout
                        .as_deref()
                        .and_then(|layout| layout.section_ys_by_id.get(&message.id))
                        .copied();
                    sections.push(AltSection {
                        label_id: message.id.as_str(),
                        raw_label,
                        geometry: SequenceBlockGeometry::empty(),
                        separator_y,
                    });
                }
            }
            (_, Some((SequenceControlKind::Par, SequenceControlRole::End))) => {
                if let Some(entry) = pop_and_propagate(&mut stack)
                    && let BlockStackEntry::Par { sections, layout } = entry
                {
                    push_block(
                        &mut blocks_by_end_index,
                        &mut blocks,
                        message_index,
                        SequenceBlock::Par {
                            control_id: message.id.as_str(),
                            sections,
                            layout,
                        },
                    );
                }
            }
            (_, Some((SequenceControlKind::Critical, SequenceControlRole::Start))) => {
                stack.push(BlockStackEntry::Critical {
                    sections: vec![AltSection {
                        label_id: message.id.as_str(),
                        raw_label,
                        geometry: SequenceBlockGeometry::empty(),
                        separator_y: None,
                    }],
                    layout: block_layouts_by_id.get(&message.id),
                })
            }
            (_, Some((SequenceControlKind::Critical, SequenceControlRole::Separator))) => {
                if let Some(BlockStackEntry::Critical { sections, layout }) = stack.last_mut() {
                    let separator_y = layout
                        .as_deref()
                        .and_then(|layout| layout.section_ys_by_id.get(&message.id))
                        .copied();
                    sections.push(AltSection {
                        label_id: message.id.as_str(),
                        raw_label,
                        geometry: SequenceBlockGeometry::empty(),
                        separator_y,
                    });
                }
            }
            (_, Some((SequenceControlKind::Critical, SequenceControlRole::End))) => {
                if let Some(entry) = pop_and_propagate(&mut stack)
                    && let BlockStackEntry::Critical { sections, layout } = entry
                {
                    push_block(
                        &mut blocks_by_end_index,
                        &mut blocks,
                        message_index,
                        SequenceBlock::Critical {
                            control_id: message.id.as_str(),
                            sections,
                            layout,
                        },
                    );
                }
            }
            _ => {
                if !stack.is_empty() && message.from.is_some() && message.to.is_some() {
                    include_message_geometry(&mut stack, message_geometry(message));
                }
            }
        }
    }

    checkpoints.checkpoint()?;
    Ok((blocks_by_end_index, blocks))
}

fn include_message_geometry<'a>(
    stack: &mut [BlockStackEntry<'a>],
    geometry: SequenceBlockGeometry<'a>,
) {
    if let Some(entry) = stack.last_mut() {
        entry.include_geometry(geometry);
    }
}

fn pop_and_propagate<'a>(stack: &mut Vec<BlockStackEntry<'a>>) -> Option<BlockStackEntry<'a>> {
    let entry = stack.pop()?;
    if let Some(parent) = stack.last_mut() {
        parent.include_geometry(entry.geometry());
    }
    Some(entry)
}

fn push_block<'a>(
    blocks_by_end_index: &mut [Option<usize>],
    blocks: &mut Vec<SequenceBlock<'a>>,
    end_index: usize,
    block: SequenceBlock<'a>,
) {
    let block_index = blocks.len();
    blocks.push(block);
    if let Some(at_end) = blocks_by_end_index.get_mut(end_index) {
        *at_end = Some(block_index);
    }
}

#[cfg(test)]
mod tests {
    use super::{SequenceBlock, collect_sequence_blocks_with};
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use crate::svg::parity::sequence::block_geometry::SequenceBlockGeometry;
    use merman_core::diagrams::sequence::{
        SequenceDiagramRenderModel, SequenceMessage, SequenceMessagePayload,
    };
    use rustc_hash::FxHashMap;
    use std::collections::BTreeMap;

    fn checkpoints(meter: &OperationWorkMeter) -> super::SequenceEmitCheckpoints<'_> {
        super::SequenceEmitCheckpoints::new(meter)
    }

    fn message(
        id: String,
        message_type: i32,
        from: Option<&str>,
        to: Option<&str>,
    ) -> SequenceMessage {
        SequenceMessage {
            id,
            from: from.map(str::to_string),
            to: to.map(str::to_string),
            message_type,
            message: SequenceMessagePayload::Text(String::new()),
            wrap: false,
            activate: false,
            placement: None,
            central_connection: 0,
        }
    }

    fn model(messages: Vec<SequenceMessage>) -> SequenceDiagramRenderModel {
        SequenceDiagramRenderModel {
            acc_title: None,
            acc_descr: None,
            title: None,
            actor_order: Vec::new(),
            actors: BTreeMap::new(),
            boxes: Vec::new(),
            messages,
            notes: Vec::new(),
            created_actors: BTreeMap::new(),
            destroyed_actors: BTreeMap::new(),
            actor_lifecycles: None,
        }
    }

    #[test]
    fn deeply_nested_blocks_aggregate_each_message_once() {
        const DEPTH: usize = 2_048;
        const CONTENT_MESSAGES: usize = 2_048;

        let mut messages = Vec::with_capacity(DEPTH * 2 + CONTENT_MESSAGES);
        for index in 0..DEPTH {
            messages.push(message(format!("start-{index}"), 10, None, None));
        }
        for index in 0..CONTENT_MESSAGES {
            messages.push(message(format!("message-{index}"), 5, Some("A"), Some("B")));
        }
        for index in 0..DEPTH {
            messages.push(message(format!("end-{index}"), 11, None, None));
        }
        let model = model(messages);
        let block_layouts = FxHashMap::default();
        let mut y = 0.0;
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        let (blocks_by_end_index, blocks) = collect_sequence_blocks_with(
            &model,
            &block_layouts,
            |_| {
                let geometry = SequenceBlockGeometry::test_y_range(y, y + 1.0);
                y += 1.0;
                geometry
            },
            checkpoints(&meter),
        )
        .unwrap();

        assert_eq!(blocks.len(), DEPTH);
        assert_eq!(blocks_by_end_index.iter().flatten().count(), DEPTH);
        for block in blocks {
            let SequenceBlock::Loop { geometry, .. } = block else {
                panic!("expected a loop block");
            };
            assert_eq!(
                geometry.frame_y_range(),
                Some((0.0, CONTENT_MESSAGES as f64))
            );
        }
    }

    #[test]
    fn nested_block_geometry_stays_in_the_active_parent_section() {
        let model = model(vec![
            message("alt".to_string(), 12, None, None),
            message("first".to_string(), 5, Some("A"), Some("B")),
            message("loop".to_string(), 10, None, None),
            message("nested".to_string(), 5, Some("A"), Some("B")),
            message("loop-end".to_string(), 11, None, None),
            message("else".to_string(), 13, None, None),
            message("second".to_string(), 5, Some("A"), Some("B")),
            message("alt-end".to_string(), 14, None, None),
        ]);
        let block_layouts = FxHashMap::default();
        let mut y = 0.0;
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        let (_, blocks) = collect_sequence_blocks_with(
            &model,
            &block_layouts,
            |_| {
                let geometry = SequenceBlockGeometry::test_y_range(y, y + 1.0);
                y += 1.0;
                geometry
            },
            checkpoints(&meter),
        )
        .unwrap();

        let SequenceBlock::Alt { sections, .. } = &blocks[1] else {
            panic!("expected the outer alt block");
        };
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].geometry.frame_y_range(), Some((0.0, 2.0)));
        assert_eq!(sections[1].geometry.frame_y_range(), Some((2.0, 3.0)));
    }
}
