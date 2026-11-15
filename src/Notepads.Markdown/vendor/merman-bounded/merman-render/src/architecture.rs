use crate::architecture_metrics::{
    architecture_cytoscape_child_contribution_bounds, architecture_cytoscape_child_label_bounds,
    architecture_cytoscape_edge_label_metrics,
    architecture_measure_cytoscape_compound_child_bbox_extras,
    architecture_measure_cytoscape_final_node_bbox_extras,
    architecture_node_bbox_extras_to_manatee,
};
use crate::config::{config_f64, json_f64, value_at};
use crate::model::{
    ArchitectureCompoundBounds, ArchitectureCytoscapeServiceBounds,
    ArchitectureCytoscapeServiceLabelMetrics, ArchitectureDiagramLayout, Bounds, LayoutEdge,
    LayoutNode, LayoutPoint,
};
use crate::resources::{OperationWorkError, OperationWorkMeter};
use crate::text::{TextMeasurer, TextStyle};
use crate::{Error, Result};
use indexmap::IndexMap;
use merman_core::diagrams::architecture::{
    ArchitectureDiagramRenderModel, ArchitectureLayoutDirection,
};
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::Value;

struct ArchitectureManateeWorkControl<'a> {
    meter: &'a OperationWorkMeter,
    denied: Option<OperationWorkError>,
}

impl<'a> ArchitectureManateeWorkControl<'a> {
    fn new(meter: &'a OperationWorkMeter) -> Self {
        Self {
            meter,
            denied: None,
        }
    }

    fn take_denied(&mut self) -> Option<OperationWorkError> {
        self.denied.take()
    }
}

impl manatee::algo::fcose::WorkControl for ArchitectureManateeWorkControl<'_> {
    fn check(&mut self, units: usize) -> std::result::Result<(), manatee::WorkFailure> {
        match self.meter.preflight(units) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.denied = Some(error);
                Err(manatee::WorkFailure::Interrupted)
            }
        }
    }

    fn charge(&mut self, units: usize) -> std::result::Result<(), manatee::WorkFailure> {
        match self.meter.charge(units) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.denied = Some(error);
                Err(manatee::WorkFailure::Interrupted)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ArchitectureConstraintWork {
    alignment_group_count: usize,
    alignment_member_count: usize,
    relative_constraint_count: usize,
}

impl ArchitectureConstraintWork {
    fn checked_work_units(self) -> Option<usize> {
        self.alignment_group_count
            .checked_add(self.alignment_member_count)?
            .checked_add(self.relative_constraint_count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArchitectureAdapterWorkPlan {
    work_units: usize,
    declared_constraints: ArchitectureConstraintWork,
}

fn checked_architecture_adapter_work_plan_from_hint_lengths(
    node_count: usize,
    group_count: usize,
    edge_count: usize,
    hint_member_counts: impl IntoIterator<Item = usize>,
) -> Option<ArchitectureAdapterWorkPlan> {
    let (hint_members, alignment_groups, constraint_members, relative_constraints) =
        hint_member_counts.into_iter().try_fold(
            (0usize, 0usize, 0usize, 0usize),
            |(members, groups, constrained_members, relative), hint_member_count| {
                let members = members.checked_add(hint_member_count)?;
                if hint_member_count < 2 {
                    return Some((members, groups, constrained_members, relative));
                }
                Some((
                    members,
                    groups.checked_add(1)?,
                    constrained_members.checked_add(hint_member_count)?,
                    relative.checked_add(hint_member_count - 1)?,
                ))
            },
        )?;
    let spatial_planning_work_units = node_count
        .checked_mul(2)?
        .checked_add(edge_count.checked_mul(4)?)?;
    let work_units = node_count
        .checked_add(group_count)?
        .checked_add(edge_count)?
        .checked_add(hint_members)?
        .checked_add(spatial_planning_work_units)?;

    Some(ArchitectureAdapterWorkPlan {
        work_units,
        declared_constraints: ArchitectureConstraintWork {
            alignment_group_count: alignment_groups,
            alignment_member_count: constraint_members,
            relative_constraint_count: relative_constraints,
        },
    })
}

#[cfg(test)]
fn checked_architecture_adapter_work_plan(
    model: &ArchitectureModelView<'_>,
) -> Option<ArchitectureAdapterWorkPlan> {
    checked_architecture_adapter_work_plan_from_hint_lengths(
        model.nodes.len(),
        model.groups.len(),
        model.edges.len(),
        model.layout_hints.iter().map(|hint| hint.members.len()),
    )
}

fn checked_typed_architecture_adapter_work_plan(
    model: &ArchitectureDiagramRenderModel,
) -> Option<ArchitectureAdapterWorkPlan> {
    checked_architecture_adapter_work_plan_from_hint_lengths(
        model.nodes.len(),
        model.groups.len(),
        model.edges.len(),
        model.layout_hints.iter().map(|hint| hint.members.len()),
    )
}

fn checked_architecture_fcose_work_upper_bound(
    schedule: manatee::algo::fcose::FcoseIterationSchedule,
    constraints: ArchitectureConstraintWork,
) -> Option<usize> {
    let constraint_work_units = constraints.checked_work_units()?;
    let run_count = schedule.run_count();
    let executed_iterations = schedule.effective_max_iterations().checked_sub(1)?;
    let run_setup_work_units = constraint_work_units.checked_mul(run_count)?;
    let iteration_work_units = constraint_work_units
        .checked_mul(executed_iterations)?
        .checked_mul(run_count)?;

    schedule
        .maximum_work_units()
        .checked_add(constraint_work_units)?
        .checked_add(run_setup_work_units)?
        .checked_add(iteration_work_units)
}

const ARCHITECTURE_RELATIVE_DIRS: [(char, (i32, i32)); 4] =
    [('L', (-1, 0)), ('R', (1, 0)), ('T', (0, 1)), ('B', (0, -1))];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArchitectureRelativePositionPlan {
    distance: usize,
    queue_multiplicity: usize,
}

#[derive(Debug)]
struct ArchitectureRelativeSpatialPlan<'a> {
    inverse: FxHashMap<(i32, i32), &'a str>,
    queue_entry_count: usize,
    constraint_count: usize,
}

#[derive(Debug)]
struct ArchitectureRelativeConstraintPlan<'a> {
    spatial: Vec<ArchitectureRelativeSpatialPlan<'a>>,
    queue_entry_count: usize,
    constraint_count: usize,
}

impl ArchitectureRelativeConstraintPlan<'_> {
    fn checked_materialization_work_units(&self) -> Option<usize> {
        let constraint_bytes = self.constraint_count.checked_mul(std::mem::size_of::<
            manatee::algo::fcose::IndexedRelativePlacementConstraint,
        >())?;
        let queue_bytes = self
            .queue_entry_count
            .checked_mul(std::mem::size_of::<(i32, i32)>())?;
        if constraint_bytes > isize::MAX as usize || queue_bytes > isize::MAX as usize {
            return None;
        }
        // Each queued position is pushed, popped, marked visited, looked up, and probes all four
        // grid directions. Constraint construction is charged separately because declared pairs
        // still enqueue and traverse without emitting an FCoSE object.
        self.queue_entry_count
            .checked_mul(8)?
            .checked_add(self.constraint_count)
    }
}

fn checked_architecture_relative_planning_work_units(
    spatial_maps: &[IndexMap<&str, (i32, i32)>],
) -> Option<usize> {
    let source_positions = spatial_maps
        .iter()
        .try_fold(0usize, |total, map| total.checked_add(map.len()))?;
    spatial_maps
        .len()
        .checked_add(source_positions.checked_mul(6)?)
}

fn checked_architecture_relative_constraint_plan<'a>(
    spatial_maps: &[IndexMap<&'a str, (i32, i32)>],
    node_index_by_id: &FxHashMap<&'a str, usize>,
    declared_pairs: &FxHashSet<(usize, usize)>,
) -> Option<ArchitectureRelativeConstraintPlan<'a>> {
    let mut spatial = Vec::with_capacity(spatial_maps.len());
    let mut total_queue_entries = 0usize;
    let mut total_constraints = 0usize;

    for spatial_map in spatial_maps {
        let mut inv: FxHashMap<(i32, i32), &str> = FxHashMap::default();
        inv.reserve(spatial_map.len().saturating_mul(2));
        for (id, (x, y)) in spatial_map.iter() {
            inv.insert((*x, *y), *id);
        }

        let mut positions: FxHashMap<(i32, i32), ArchitectureRelativePositionPlan> =
            FxHashMap::default();
        positions.reserve(inv.len().saturating_mul(2));
        positions.insert(
            (0, 0),
            ArchitectureRelativePositionPlan {
                distance: 0,
                queue_multiplicity: 1,
            },
        );
        let mut unique_queue = std::collections::VecDeque::new();
        unique_queue.push_back((0, 0));
        let mut constraint_count = 0usize;

        while let Some(curr) = unique_queue.pop_front() {
            let curr_plan = *positions.get(&curr)?;
            let Some(&curr_id) = inv.get(&curr) else {
                continue;
            };
            let next_distance = curr_plan.distance.checked_add(1)?;
            for (_, (sx, sy)) in ARCHITECTURE_RELATIVE_DIRS {
                let new_pos = (curr.0.checked_add(sx)?, curr.1.checked_add(sy)?);
                let Some(&new_id) = inv.get(&new_pos) else {
                    continue;
                };
                let is_forward = match positions.get_mut(&new_pos) {
                    Some(existing) if existing.distance == next_distance => {
                        existing.queue_multiplicity = existing
                            .queue_multiplicity
                            .checked_add(curr_plan.queue_multiplicity)?;
                        true
                    }
                    Some(_) => false,
                    None => {
                        positions.insert(
                            new_pos,
                            ArchitectureRelativePositionPlan {
                                distance: next_distance,
                                queue_multiplicity: curr_plan.queue_multiplicity,
                            },
                        );
                        unique_queue.push_back(new_pos);
                        true
                    }
                };
                if !is_forward {
                    continue;
                }
                let Some(&curr_idx) = node_index_by_id.get(curr_id) else {
                    continue;
                };
                let Some(&new_idx) = node_index_by_id.get(new_id) else {
                    continue;
                };
                if declared_pairs.contains(&(curr_idx, new_idx))
                    || declared_pairs.contains(&(new_idx, curr_idx))
                {
                    continue;
                }
                constraint_count = constraint_count.checked_add(curr_plan.queue_multiplicity)?;
            }
        }

        let queue_entry_count = positions.values().try_fold(0usize, |total, position| {
            total.checked_add(position.queue_multiplicity)
        })?;
        total_queue_entries = total_queue_entries.checked_add(queue_entry_count)?;
        total_constraints = total_constraints.checked_add(constraint_count)?;
        spatial.push(ArchitectureRelativeSpatialPlan {
            inverse: inv,
            queue_entry_count,
            constraint_count,
        });
    }

    Some(ArchitectureRelativeConstraintPlan {
        spatial,
        queue_entry_count: total_queue_entries,
        constraint_count: total_constraints,
    })
}

fn materialize_architecture_relative_placement_constraints(
    plan: &ArchitectureRelativeConstraintPlan<'_>,
    node_index_by_id: &FxHashMap<&str, usize>,
    gap: f64,
    declared_pairs: &FxHashSet<(usize, usize)>,
) -> Option<Vec<manatee::algo::fcose::IndexedRelativePlacementConstraint>> {
    let mut relative = Vec::with_capacity(plan.constraint_count);

    for spatial in &plan.spatial {
        let output_start = relative.len();
        let mut materialized_queue_entries = 0usize;
        let mut pos_queue = std::collections::VecDeque::new();
        let mut visited_pos: FxHashSet<(i32, i32)> = FxHashSet::default();
        visited_pos.reserve(spatial.inverse.len().saturating_mul(2));
        pos_queue.push_back((0, 0));

        while let Some(curr) = pos_queue.pop_front() {
            materialized_queue_entries = materialized_queue_entries.checked_add(1)?;
            // Mermaid marks the current grid position as visited but does not skip duplicate
            // queued positions on pop. Preserve both duplicate constraints and their L/R/T/B
            // order after the linear admission planner has bounded the expansion.
            visited_pos.insert(curr);
            let Some(&curr_id) = spatial.inverse.get(&curr) else {
                continue;
            };
            for (dir, (sx, sy)) in ARCHITECTURE_RELATIVE_DIRS {
                let new_pos = (curr.0.checked_add(sx)?, curr.1.checked_add(sy)?);
                let Some(&new_id) = spatial.inverse.get(&new_pos) else {
                    continue;
                };
                if visited_pos.contains(&new_pos) {
                    continue;
                }
                pos_queue.push_back(new_pos);
                let Some(&curr_idx) = node_index_by_id.get(curr_id) else {
                    continue;
                };
                let Some(&new_idx) = node_index_by_id.get(new_id) else {
                    continue;
                };
                if declared_pairs.contains(&(curr_idx, new_idx))
                    || declared_pairs.contains(&(new_idx, curr_idx))
                {
                    continue;
                }

                relative.push(match dir {
                    'L' => manatee::algo::fcose::IndexedRelativePlacementConstraint {
                        left: Some(new_idx),
                        right: Some(curr_idx),
                        top: None,
                        bottom: None,
                        gap,
                    },
                    'R' => manatee::algo::fcose::IndexedRelativePlacementConstraint {
                        left: Some(curr_idx),
                        right: Some(new_idx),
                        top: None,
                        bottom: None,
                        gap,
                    },
                    'T' => manatee::algo::fcose::IndexedRelativePlacementConstraint {
                        left: None,
                        right: None,
                        top: Some(new_idx),
                        bottom: Some(curr_idx),
                        gap,
                    },
                    'B' => manatee::algo::fcose::IndexedRelativePlacementConstraint {
                        left: None,
                        right: None,
                        top: Some(curr_idx),
                        bottom: Some(new_idx),
                        gap,
                    },
                    _ => return None,
                });
            }
        }

        debug_assert_eq!(materialized_queue_entries, spatial.queue_entry_count);
        debug_assert_eq!(relative.len() - output_start, spatial.constraint_count);
    }

    debug_assert_eq!(relative.len(), plan.constraint_count);
    Some(relative)
}

fn config_bool(cfg: &Value, path: &[&str]) -> Option<bool> {
    let mut cur = cfg;
    for k in path {
        cur = cur.get(*k)?;
    }
    cur.as_bool()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchitectureNodeType {
    Service,
    Junction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    L,
    R,
    T,
    B,
}

impl Dir {
    fn from_char(ch: char) -> Option<Self> {
        match ch {
            'L' => Some(Self::L),
            'R' => Some(Self::R),
            'T' => Some(Self::T),
            'B' => Some(Self::B),
            _ => None,
        }
    }

    fn is_x(self) -> bool {
        matches!(self, Self::L | Self::R)
    }
}

fn dir_pair_key(source: Dir, target: Dir) -> Option<&'static str> {
    match (source, target) {
        (Dir::L, Dir::R) => Some("LR"),
        (Dir::L, Dir::T) => Some("LT"),
        (Dir::L, Dir::B) => Some("LB"),
        (Dir::R, Dir::L) => Some("RL"),
        (Dir::R, Dir::T) => Some("RT"),
        (Dir::R, Dir::B) => Some("RB"),
        (Dir::T, Dir::L) => Some("TL"),
        (Dir::T, Dir::R) => Some("TR"),
        (Dir::T, Dir::B) => Some("TB"),
        (Dir::B, Dir::L) => Some("BL"),
        (Dir::B, Dir::R) => Some("BR"),
        (Dir::B, Dir::T) => Some("BT"),
        _ => None,
    }
}

fn shift_position_by_arch_pair(x: i32, y: i32, pair: &str) -> (i32, i32) {
    // Port of Mermaid@11.12.2 `shiftPositionByArchitectureDirectionPair`.
    let bytes = pair.as_bytes();
    if bytes.len() != 2 {
        return (x, y);
    }
    let lhs = match bytes[0] as char {
        'L' => Dir::L,
        'R' => Dir::R,
        'T' => Dir::T,
        'B' => Dir::B,
        _ => return (x, y),
    };
    let rhs = match bytes[1] as char {
        'L' => Dir::L,
        'R' => Dir::R,
        'T' => Dir::T,
        'B' => Dir::B,
        _ => return (x, y),
    };

    if lhs.is_x() {
        if !rhs.is_x() {
            (
                x + if lhs == Dir::L { -1 } else { 1 },
                y + if rhs == Dir::T { 1 } else { -1 },
            )
        } else {
            (x + if lhs == Dir::L { -1 } else { 1 }, y)
        }
    } else if rhs.is_x() {
        (
            x + if rhs == Dir::L { 1 } else { -1 },
            y + if lhs == Dir::T { 1 } else { -1 },
        )
    } else {
        (x, y + if lhs == Dir::T { 1 } else { -1 })
    }
}

fn anchor_from_dir(dir: Dir) -> manatee::Anchor {
    match dir {
        Dir::L => manatee::Anchor::Left,
        Dir::R => manatee::Anchor::Right,
        Dir::T => manatee::Anchor::Top,
        Dir::B => manatee::Anchor::Bottom,
    }
}

fn js_to_uint32(value: f64) -> u64 {
    const UINT32_MODULUS: f64 = 4_294_967_296.0;
    value.trunc().rem_euclid(UINT32_MODULUS) as u64
}

fn architecture_seed_policy(
    value: Option<&Value>,
    operation_seed: u64,
) -> manatee::FcoseRandomPolicy {
    let numeric_seed = value
        .and_then(json_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(1.0);
    let is_json_number_zero = value
        .and_then(Value::as_f64)
        .is_some_and(|value| value == 0.0);
    let policy = if is_json_number_zero {
        // Mermaid leaves Math.random in place for architecture.seed=0. Merman captures that
        // operation-owned stream at session start, then consumes it continuously across FCoSE
        // reruns instead of introducing a second process-global random source.
        manatee::FcoseRandomPolicy::seeded(manatee::FcoseRandomSource::Mulberry32, operation_seed)
            .with_reset_seed_each_run(false)
    } else {
        manatee::FcoseRandomPolicy::seeded(
            manatee::FcoseRandomSource::Mulberry32,
            js_to_uint32(numeric_seed),
        )
        .with_reset_seed_each_run(true)
    };
    policy.with_seed_offset(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupAlignment {
    Horizontal,
    Vertical,
    Bend,
}

fn dir_alignment(a: Option<char>, b: Option<char>) -> GroupAlignment {
    let (Some(a), Some(b)) = (a.and_then(Dir::from_char), b.and_then(Dir::from_char)) else {
        return GroupAlignment::Bend;
    };
    if a.is_x() != b.is_x() {
        GroupAlignment::Bend
    } else if a.is_x() {
        GroupAlignment::Horizontal
    } else {
        GroupAlignment::Vertical
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FlattenAlignmentsWorkPlan {
    direction_bucket_count: usize,
    group_count: usize,
    source_member_count: usize,
    pair_count: usize,
    expanded_member_count: usize,
    output_key_bound: usize,
    sort_work_units: usize,
    work_units: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FlattenAlignmentsMetadata {
    direction_bucket_count: usize,
    group_count: usize,
}

impl FlattenAlignmentsMetadata {
    fn checked_work_units(self) -> Option<usize> {
        self.direction_bucket_count.checked_add(self.group_count)
    }
}

fn checked_flatten_alignments_metadata(
    alignment_obj: &IndexMap<i32, IndexMap<String, Vec<usize>>>,
) -> Option<FlattenAlignmentsMetadata> {
    let group_count = alignment_obj
        .values()
        .try_fold(0usize, |groups, bucket| groups.checked_add(bucket.len()))?;
    Some(FlattenAlignmentsMetadata {
        direction_bucket_count: alignment_obj.len(),
        group_count,
    })
}

fn checked_ceil_log2(value: usize) -> Option<usize> {
    if value <= 1 {
        return Some(0);
    }
    usize::BITS
        .checked_sub((value - 1).leading_zeros())
        .map(|bits| bits as usize)
}

fn checked_sort_work_units(item_count: usize) -> Option<usize> {
    item_count.checked_mul(checked_ceil_log2(item_count)?)
}

fn checked_unordered_pair_count(item_count: usize) -> Option<usize> {
    if item_count < 2 {
        return Some(0);
    }

    let predecessor = item_count - 1;
    if item_count.is_multiple_of(2) {
        (item_count / 2).checked_mul(predecessor)
    } else {
        item_count.checked_mul(predecessor / 2)
    }
}

fn checked_flatten_alignment_bucket_cardinality(
    group_count: usize,
    source_member_count: usize,
) -> Option<(usize, usize, usize)> {
    match group_count {
        0 => Some((0, 0, 0)),
        1 => Some((0, 0, 1)),
        _ => {
            let pair_count = checked_unordered_pair_count(group_count)?;
            let expanded_member_count =
                source_member_count.checked_mul(group_count.checked_sub(1)?)?;
            let output_key_bound = pair_count.checked_mul(2)?;
            Some((pair_count, expanded_member_count, output_key_bound))
        }
    }
}

impl FlattenAlignmentsWorkPlan {
    fn checked(alignment_obj: &IndexMap<i32, IndexMap<String, Vec<usize>>>) -> Option<Self> {
        let metadata = checked_flatten_alignments_metadata(alignment_obj)?;
        Self::checked_with_metadata(alignment_obj, metadata)
    }

    fn checked_with_metadata(
        alignment_obj: &IndexMap<i32, IndexMap<String, Vec<usize>>>,
        metadata: FlattenAlignmentsMetadata,
    ) -> Option<Self> {
        let mut plan = Self {
            direction_bucket_count: metadata.direction_bucket_count,
            group_count: metadata.group_count,
            ..Self::default()
        };
        let mut numeric_direction_count = 0usize;
        let mut numeric_output_key_bound = 0usize;

        for (&dir, alignments) in alignment_obj {
            if dir >= 0 {
                numeric_direction_count = numeric_direction_count.checked_add(1)?;
                if !alignments.is_empty() {
                    numeric_output_key_bound = numeric_output_key_bound.checked_add(1)?;
                }
            }

            let (bucket_source_member_count, numeric_group_count) = alignments.iter().try_fold(
                (0usize, 0usize),
                |(members, numeric_groups), (key, group)| {
                    Some((
                        members.checked_add(group.len())?,
                        if js_array_index_key(key).is_some() {
                            numeric_groups.checked_add(1)?
                        } else {
                            numeric_groups
                        },
                    ))
                },
            )?;
            plan.source_member_count = plan
                .source_member_count
                .checked_add(bucket_source_member_count)?;

            let (pair_count, expanded_member_count, output_key_bound) =
                checked_flatten_alignment_bucket_cardinality(
                    alignments.len(),
                    bucket_source_member_count,
                )?;
            plan.pair_count = plan.pair_count.checked_add(pair_count)?;
            plan.expanded_member_count = plan
                .expanded_member_count
                .checked_add(expanded_member_count)?;
            plan.output_key_bound = plan.output_key_bound.checked_add(output_key_bound)?;

            plan.sort_work_units = plan
                .sort_work_units
                .checked_add(checked_sort_work_units(numeric_group_count)?)?;
        }

        plan.sort_work_units = plan
            .sort_work_units
            .checked_add(checked_sort_work_units(numeric_direction_count)?)?
            .checked_add(checked_sort_work_units(numeric_output_key_bound)?)?;

        plan.work_units = plan
            .direction_bucket_count
            .checked_add(plan.group_count)?
            .checked_add(plan.source_member_count)?
            .checked_add(plan.pair_count)?
            .checked_add(plan.expanded_member_count)?
            .checked_add(plan.output_key_bound)?
            .checked_add(plan.sort_work_units)?;
        Some(plan)
    }
}

fn js_array_index_key(key: &str) -> Option<u32> {
    if key == "0" {
        return Some(0);
    }
    if key.is_empty() || key.starts_with('0') || !key.as_bytes().iter().all(u8::is_ascii_digit) {
        return None;
    }
    key.parse::<u32>().ok().filter(|index| *index < u32::MAX)
}

fn js_object_i32_key_index_order<V>(obj: &IndexMap<i32, V>) -> Vec<usize> {
    let mut array_indices: Vec<(i32, usize)> = Vec::new();
    let mut other_indices: Vec<usize> = Vec::new();
    for (index, (&key, _)) in obj.iter().enumerate() {
        if key >= 0 {
            array_indices.push((key, index));
        } else {
            other_indices.push(index);
        }
    }
    array_indices.sort_unstable_by_key(|(key, _)| *key);
    array_indices
        .into_iter()
        .map(|(_, index)| index)
        .chain(other_indices)
        .collect()
}

fn js_object_string_key_index_order<K: AsRef<str>, V>(obj: &IndexMap<K, V>) -> Vec<usize> {
    let mut array_indices: Vec<(u32, usize)> = Vec::new();
    let mut other_indices: Vec<usize> = Vec::new();
    for (index, (key, _)) in obj.iter().enumerate() {
        if let Some(array_index) = js_array_index_key(key.as_ref()) {
            array_indices.push((array_index, index));
        } else {
            other_indices.push(index);
        }
    }
    array_indices.sort_unstable_by_key(|(key, _)| *key);
    array_indices
        .into_iter()
        .map(|(_, index)| index)
        .chain(other_indices)
        .collect()
}

fn flatten_alignments(
    alignment_obj: &IndexMap<i32, IndexMap<String, Vec<usize>>>,
    alignment_dir: GroupAlignment,
    group_alignments: &std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, GroupAlignment>,
    >,
    work_meter: &OperationWorkMeter,
) -> Result<Vec<Vec<usize>>> {
    // Mirror Mermaid's `flattenAlignments(...)` + `Object.values(...)` ordering.
    //
    // Mermaid uses plain JS objects keyed by row/col number. Enumeration order puts
    // non-negative integer keys first (ascending), then other string keys (insertion
    // order). We reproduce that here to keep the first element of each alignment group
    // stable, since `cose-base` uses it to seed dummy-node positions.
    // The scalar scan does not allocate or clone members. Charge its complete execution plan
    // before creating ordered index vectors, output keys, or pair-expanded member arrays.
    let metadata = checked_flatten_alignments_metadata(alignment_obj)
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    work_meter.preflight(
        metadata
            .checked_work_units()
            .ok_or_else(|| work_meter.arithmetic_overflow())?,
    )?;
    let work_plan = FlattenAlignmentsWorkPlan::checked(alignment_obj)
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    work_meter.charge(work_plan.work_units)?;

    let mut prev: IndexMap<String, Vec<usize>> = IndexMap::new();

    for dir_index in js_object_i32_key_index_order(alignment_obj) {
        let (&dir, alignments) = alignment_obj
            .get_index(dir_index)
            .expect("direction index came from this alignment object");
        let group_order = js_object_string_key_index_order(alignments);
        let mut cnt = 0usize;
        let dir_key = dir.to_string();
        if group_order.len() == 1 {
            let (_, node_ids) = alignments
                .get_index(group_order[0])
                .expect("group index came from this alignment bucket");
            prev.insert(dir_key, node_ids.clone());
            continue;
        }
        for i in 0..group_order.len().saturating_sub(1) {
            for j in (i + 1)..group_order.len() {
                let (a_group_id, a_node_ids) = alignments
                    .get_index(group_order[i])
                    .expect("group index came from this alignment bucket");
                let (b_group_id, b_node_ids) = alignments
                    .get_index(group_order[j])
                    .expect("group index came from this alignment bucket");
                let alignment = group_alignments
                    .get(a_group_id)
                    .and_then(|m| m.get(b_group_id))
                    .copied();

                if alignment == Some(alignment_dir)
                    || a_group_id == "default"
                    || b_group_id == "default"
                {
                    if let Some(node_ids) = prev.get_mut(&dir_key) {
                        node_ids.extend(a_node_ids.iter().copied());
                        node_ids.extend(b_node_ids.iter().copied());
                    } else {
                        let mut node_ids = Vec::new();
                        node_ids.extend(a_node_ids.iter().copied());
                        node_ids.extend(b_node_ids.iter().copied());
                        prev.insert(dir_key.clone(), node_ids);
                    }
                } else {
                    let key_a = format!("{dir}-{cnt}");
                    cnt += 1;
                    prev.insert(key_a, a_node_ids.clone());
                    let key_b = format!("{dir}-{cnt}");
                    cnt += 1;
                    prev.insert(key_b, b_node_ids.clone());
                }
            }
        }
    }

    // `Object.values(prev)` ordering.
    let output_len = prev.len();
    let mut numeric_values: Vec<(u32, Vec<usize>)> = Vec::new();
    let mut other_values: Vec<Vec<usize>> = Vec::new();
    for (key, value) in prev {
        if let Some(index) = js_array_index_key(&key) {
            numeric_values.push((index, value));
        } else {
            other_values.push(value);
        }
    }
    numeric_values.sort_unstable_by_key(|(index, _)| *index);

    let mut out = Vec::with_capacity(output_len);
    out.extend(numeric_values.into_iter().map(|(_, value)| value));
    out.extend(other_values);
    Ok(out)
}

#[derive(Debug, Clone, Copy)]
struct ArchitectureNodeView<'a> {
    id: &'a str,
    node_type: ArchitectureNodeType,
    title: Option<&'a str>,
    in_group: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
struct ArchitectureGroupView<'a> {
    id: &'a str,
    in_group: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
struct ArchitectureEdgeView<'a> {
    lhs_id: &'a str,
    rhs_id: &'a str,
    lhs_dir: Option<char>,
    rhs_dir: Option<char>,
    title: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct ArchitectureLayoutHintView<'a> {
    direction: ArchitectureLayoutDirection,
    members: Vec<&'a str>,
}

#[derive(Debug, Clone)]
struct ArchitectureModelView<'a> {
    nodes: Vec<ArchitectureNodeView<'a>>,
    groups: Vec<ArchitectureGroupView<'a>>,
    edges: Vec<ArchitectureEdgeView<'a>>,
    layout_hints: Vec<ArchitectureLayoutHintView<'a>>,
}

#[cfg(test)]
std::thread_local! {
    static TYPED_ARCHITECTURE_PROJECTION_COUNT: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

#[cfg(test)]
fn reset_typed_architecture_projection_count() {
    TYPED_ARCHITECTURE_PROJECTION_COUNT.set(0);
}

#[cfg(test)]
fn typed_architecture_projection_count() -> usize {
    TYPED_ARCHITECTURE_PROJECTION_COUNT.get()
}

impl<'a> ArchitectureModelView<'a> {
    fn from_typed(model: &'a ArchitectureDiagramRenderModel) -> Self {
        #[cfg(test)]
        TYPED_ARCHITECTURE_PROJECTION_COUNT
            .set(TYPED_ARCHITECTURE_PROJECTION_COUNT.get().saturating_add(1));

        let nodes = model
            .nodes
            .iter()
            .map(|n| ArchitectureNodeView {
                id: n.id.as_str(),
                node_type: match n.node_type {
                    merman_core::diagrams::architecture::ArchitectureRenderNodeType::Service => {
                        ArchitectureNodeType::Service
                    }
                    merman_core::diagrams::architecture::ArchitectureRenderNodeType::Junction => {
                        ArchitectureNodeType::Junction
                    }
                },
                title: n.title.as_deref(),
                in_group: n.in_group.as_deref(),
            })
            .collect();

        let groups = model
            .groups
            .iter()
            .map(|g| ArchitectureGroupView {
                id: g.id.as_str(),
                in_group: g.in_group.as_deref(),
            })
            .collect();

        let edges = model
            .edges
            .iter()
            .map(|e| ArchitectureEdgeView {
                lhs_id: e.lhs_id.as_str(),
                rhs_id: e.rhs_id.as_str(),
                lhs_dir: Some(e.lhs_dir),
                rhs_dir: Some(e.rhs_dir),
                title: e.title.as_deref(),
            })
            .collect();

        let layout_hints = model
            .layout_hints
            .iter()
            .map(|hint| ArchitectureLayoutHintView {
                direction: hint.direction,
                members: hint.members.iter().map(String::as_str).collect(),
            })
            .collect();

        Self {
            nodes,
            groups,
            edges,
            layout_hints,
        }
    }
}

#[derive(Debug)]
struct ArchitectureSpatialTraversal<'a> {
    spatial_maps: Vec<IndexMap<&'a str, (i32, i32)>>,
    incident_edges: FxHashMap<&'a str, Vec<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArchitectureSpatialComponentAdmission {
    exact_traversal_work_units: Option<usize>,
    js_enumeration_work_units: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct ArchitectureSpatialAdmission {
    components: Vec<ArchitectureSpatialComponentAdmission>,
    preflight_work_units: usize,
}

fn checked_architecture_spatial_admission<'a>(
    node_ids: &[&'a str],
    adj_list: &FxHashMap<&'a str, IndexMap<&'static str, &'a str>>,
) -> Option<ArchitectureSpatialAdmission> {
    let mut globally_visited: FxHashSet<&str> = FxHashSet::default();
    globally_visited.reserve(node_ids.len().saturating_mul(2));
    let mut components = Vec::with_capacity(node_ids.len());
    let mut preflight_work_units = 0usize;
    let mut distances: FxHashMap<&str, usize> = FxHashMap::default();
    let mut multiplicities: FxHashMap<&str, usize> = FxHashMap::default();
    let mut discovery = Vec::new();
    let mut unique_queue = std::collections::VecDeque::new();

    for &start_id in node_ids {
        if globally_visited.contains(start_id) {
            continue;
        }

        distances.clear();
        multiplicities.clear();
        discovery.clear();
        unique_queue.clear();
        distances.insert(start_id, 0);
        unique_queue.push_back(start_id);

        while let Some(id) = unique_queue.pop_front() {
            discovery.push(id);
            let next_distance = distances.get(id)?.checked_add(1)?;
            let Some(adj) = adj_list.get(id) else {
                continue;
            };
            for &rhs_id in adj.values() {
                if globally_visited.contains(rhs_id) || distances.contains_key(rhs_id) {
                    continue;
                }
                distances.insert(rhs_id, next_distance);
                unique_queue.push_back(rhs_id);
            }
        }

        let numeric_key_count = discovery
            .iter()
            .filter(|id| js_array_index_key(id).is_some())
            .count();
        let js_enumeration_work_units = if numeric_key_count == 0 {
            0
        } else {
            discovery
                .len()
                .checked_mul(3)?
                .checked_add(checked_sort_work_units(numeric_key_count)?)?
        };

        multiplicities.insert(start_id, 1);
        let mut queue_entry_count = 0usize;
        let mut adjacency_scan_count = 0usize;
        let mut exact_traversal = true;
        'propagate: for &id in &discovery {
            let distance = *distances.get(id)?;
            let multiplicity = *multiplicities.get(id)?;
            queue_entry_count = queue_entry_count.checked_add(multiplicity)?;
            let Some(adj) = adj_list.get(id) else {
                continue;
            };
            adjacency_scan_count =
                adjacency_scan_count.checked_add(multiplicity.checked_mul(adj.len())?)?;
            let next_distance = distance.checked_add(1)?;
            for &rhs_id in adj.values() {
                let Some(&rhs_distance) = distances.get(rhs_id) else {
                    // The target belongs to an earlier one-way component and is already visited
                    // when Mermaid starts this component.
                    continue;
                };
                if rhs_distance == distance {
                    // Same-layer edges make the exact duplicate queue depend on first-pop order.
                    // Keep this component on the runtime-charged path without throwing away exact
                    // preflight evidence from independent strictly layered components.
                    exact_traversal = false;
                    break 'propagate;
                }
                if rhs_distance == next_distance {
                    let entry = multiplicities.entry(rhs_id).or_default();
                    *entry = entry.checked_add(multiplicity)?;
                }
            }
        }

        let exact_traversal_work_units = if exact_traversal {
            let queue_bytes = queue_entry_count.checked_mul(std::mem::size_of::<&str>())?;
            if queue_bytes > isize::MAX as usize {
                return None;
            }
            Some(
                queue_entry_count
                    .checked_mul(2)?
                    .checked_add(adjacency_scan_count)?,
            )
        } else {
            None
        };
        let component = ArchitectureSpatialComponentAdmission {
            exact_traversal_work_units,
            js_enumeration_work_units,
        };
        preflight_work_units = preflight_work_units
            .checked_add(component.exact_traversal_work_units.unwrap_or(0))?
            .checked_add(component.js_enumeration_work_units)?;
        components.push(component);
        globally_visited.extend(discovery.iter().copied());
    }

    Some(ArchitectureSpatialAdmission {
        components,
        preflight_work_units,
    })
}

fn order_architecture_spatial_map_like_js<'a>(
    spatial: IndexMap<&'a str, (i32, i32)>,
    work_units: usize,
    work_meter: &OperationWorkMeter,
) -> Result<IndexMap<&'a str, (i32, i32)>> {
    if work_units == 0 {
        return Ok(spatial);
    }

    let order_bytes = spatial
        .len()
        .checked_mul(std::mem::size_of::<usize>())
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    if order_bytes > isize::MAX as usize {
        return Err(work_meter.arithmetic_overflow().into());
    }
    work_meter.charge(work_units)?;

    let order = js_object_string_key_index_order(&spatial);
    let mut ordered = IndexMap::with_capacity(spatial.len());
    for index in order {
        let Some((&id, &position)) = spatial.get_index(index) else {
            return Err(work_meter.arithmetic_overflow().into());
        };
        ordered.insert(id, position);
    }
    Ok(ordered)
}

fn build_architecture_spatial_maps<'a>(
    model: &ArchitectureModelView<'a>,
    node_ids: &[&'a str],
    work_meter: &OperationWorkMeter,
) -> Result<ArchitectureSpatialTraversal<'a>> {
    let mut incident_edges: FxHashMap<&'a str, Vec<usize>> = FxHashMap::default();
    incident_edges.reserve(model.nodes.len().saturating_mul(2));
    for (edge_idx, e) in model.edges.iter().enumerate() {
        incident_edges.entry(e.lhs_id).or_default().push(edge_idx);
        incident_edges.entry(e.rhs_id).or_default().push(edge_idx);
    }

    let mut adj_list: FxHashMap<&'a str, IndexMap<&'static str, &'a str>> = FxHashMap::default();
    adj_list.reserve(model.nodes.len().saturating_mul(2));
    for &id in node_ids {
        let mut adj: IndexMap<&'static str, &str> = IndexMap::new();
        let Some(edges) = incident_edges.get(id) else {
            adj_list.insert(id, adj);
            continue;
        };
        for &edge_idx in edges {
            let e = &model.edges[edge_idx];
            let (rhs_id, lhs_dir, rhs_dir) = if e.lhs_id == id {
                (e.rhs_id, e.lhs_dir, e.rhs_dir)
            } else {
                (e.lhs_id, e.rhs_dir, e.lhs_dir)
            };
            let (Some(lhs_dir), Some(rhs_dir)) = (
                lhs_dir.and_then(Dir::from_char),
                rhs_dir.and_then(Dir::from_char),
            ) else {
                continue;
            };
            let Some(pair) = dir_pair_key(lhs_dir, rhs_dir) else {
                continue;
            };
            if let Some(existing) = adj.get_mut(pair) {
                *existing = rhs_id;
            } else {
                adj.insert(pair, rhs_id);
            }
        }
        adj_list.insert(id, adj);
    }

    let admission = checked_architecture_spatial_admission(node_ids, &adj_list)
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    work_meter.preflight(admission.preflight_work_units)?;
    let mut component_admissions = admission.components.into_iter();

    // Mermaid marks a node as visited when it is dequeued, but intentionally processes every
    // queued copy. A later dequeue may therefore overwrite the coordinates of descendants that
    // are still waiting in the queue. Charge before every queue mutation and adjacency scan so
    // this source-compatible path multiplicity cannot allocate or scan beyond the work budget.
    let mut spatial_maps: Vec<IndexMap<&str, (i32, i32)>> = Vec::new();
    let mut visited: FxHashSet<&str> = FxHashSet::default();
    visited.reserve(model.nodes.len().saturating_mul(2));
    for &start_id in node_ids {
        if visited.contains(start_id) {
            continue;
        }
        let component_admission = component_admissions
            .next()
            .ok_or_else(|| work_meter.arithmetic_overflow())?;

        let mut spatial: IndexMap<&str, (i32, i32)> = IndexMap::new();
        let mut queue: std::collections::VecDeque<&str> = std::collections::VecDeque::new();
        work_meter.charge(1)?;
        spatial.insert(start_id, (0, 0));
        queue.push_back(start_id);

        while !queue.is_empty() {
            work_meter.charge(1)?;
            let Some(id) = queue.pop_front() else {
                break;
            };
            visited.insert(id);
            let Some(&(pos_x, pos_y)) = spatial.get(id) else {
                continue;
            };
            let Some(adj) = adj_list.get(id) else {
                continue;
            };
            work_meter.charge(adj.len())?;
            for (&pair, &rhs_id) in adj.iter() {
                if visited.contains(rhs_id) {
                    continue;
                }
                // Admit the enqueue before updating either local structure. On rejection the
                // partially built component is dropped and no caller-visible plan is returned.
                work_meter.charge(1)?;
                let (nx, ny) = shift_position_by_arch_pair(pos_x, pos_y, pair);
                spatial.insert(rhs_id, (nx, ny));
                queue.push_back(rhs_id);
            }
        }

        let spatial = order_architecture_spatial_map_like_js(
            spatial,
            component_admission.js_enumeration_work_units,
            work_meter,
        )?;
        spatial_maps.push(spatial);
    }

    if component_admissions.next().is_some() {
        return Err(work_meter.arithmetic_overflow().into());
    }

    Ok(ArchitectureSpatialTraversal {
        spatial_maps,
        incident_edges,
    })
}

struct ArchitectureFcoseNodeBoundsExtrasInput<'m, 'a> {
    model: &'m ArchitectureModelView<'a>,
    text_measurer: &'m dyn TextMeasurer,
    icon_size: f64,
    font_size_px: f64,
}

const CYTOSCAPE_DEFAULT_FONT_FAMILY: &str = "Helvetica Neue,Helvetica,sans-serif";

fn architecture_cytoscape_text_style(font_size_px: f64) -> TextStyle {
    TextStyle {
        // Mermaid sets only `font-size` on Architecture nodes, so Cytoscape retains its own
        // default canvas font family rather than inheriting Mermaid's root Trebuchet stack.
        font_family: Some(CYTOSCAPE_DEFAULT_FONT_FAMILY.to_string()),
        font_size: font_size_px,
        font_weight: None,
        font_style: None,
    }
}

fn architecture_cytoscape_edge_text_style() -> TextStyle {
    TextStyle {
        font_family: Some(CYTOSCAPE_DEFAULT_FONT_FAMILY.to_string()),
        ..TextStyle::default()
    }
}

fn architecture_fcose_node_bounds_extras<'a>(
    input: ArchitectureFcoseNodeBoundsExtrasInput<'_, 'a>,
) -> FxHashMap<&'a str, manatee::BoundsExtras> {
    // Capture Cytoscape's custom compound-child bbox for grouped leaves and its final element bbox
    // for top-level leaves without changing layout node size. Manatee consumes the selected extras
    // for compound sizing and relocation.
    //
    // Relocation-centering stays inside manatee's indexed graph adapter; keeping it out of this
    // renderer-side helper avoids a second, unused pre-layout bbox model.
    let ArchitectureFcoseNodeBoundsExtrasInput {
        model,
        text_measurer,
        icon_size,
        font_size_px,
    } = input;
    let text_style = architecture_cytoscape_text_style(font_size_px);

    let mut node_bounds_extras: FxHashMap<&str, manatee::BoundsExtras> = FxHashMap::default();
    node_bounds_extras.reserve(model.nodes.len().saturating_mul(2));
    for n in &model.nodes {
        let bounds_extras = if n.in_group.is_some() {
            architecture_measure_cytoscape_compound_child_bbox_extras(
                n.title,
                text_measurer,
                &text_style,
                icon_size,
                font_size_px,
            )
        } else {
            architecture_measure_cytoscape_final_node_bbox_extras(
                n.title,
                text_measurer,
                &text_style,
                icon_size,
                font_size_px,
            )
        };
        node_bounds_extras.insert(
            n.id,
            architecture_node_bbox_extras_to_manatee(bounds_extras),
        );
    }

    node_bounds_extras
}

#[derive(Debug, Clone)]
struct ArchitectureFcoseInputPlan<'a> {
    compound_ids: Vec<&'a str>,
    graph: manatee::algo::fcose::IndexedGraph,
    options: manatee::algo::fcose::IndexedFcoseOptions,
    random_policy: manatee::FcoseRandomPolicy,
}

struct ArchitectureFcoseInputPlanInput<'m, 'a> {
    model: &'m ArchitectureModelView<'a>,
    layout_nodes: &'m [LayoutNode],
    node_bounds_extras: &'m FxHashMap<&'a str, manatee::BoundsExtras>,
    text_measurer: &'m dyn TextMeasurer,
    work_meter: &'m OperationWorkMeter,
    icon_size: f64,
    padding_px: f64,
    ideal_edge_length_multiplier: f64,
    same_group_edge_elasticity: f64,
    fcose_randomize: bool,
    fcose_node_separation: f64,
    fcose_num_iter: usize,
    fcose_random_policy: manatee::FcoseRandomPolicy,
}

fn build_architecture_fcose_input_plan<'a>(
    input: ArchitectureFcoseInputPlanInput<'_, 'a>,
) -> Result<ArchitectureFcoseInputPlan<'a>> {
    let ArchitectureFcoseInputPlanInput {
        model,
        layout_nodes,
        node_bounds_extras,
        text_measurer,
        work_meter,
        icon_size,
        padding_px,
        ideal_edge_length_multiplier,
        same_group_edge_elasticity,
        fcose_randomize,
        fcose_node_separation,
        fcose_num_iter,
        fcose_random_policy,
    } = input;

    if layout_nodes.len() != model.nodes.len() {
        return Err(Error::InvalidModel {
            message: format!(
                "architecture FCoSE input node count mismatch: model={} layout={}",
                model.nodes.len(),
                layout_nodes.len()
            ),
        });
    }

    let node_ids: Vec<&str> = model.nodes.iter().map(|n| n.id).collect();
    let compound_ids: Vec<&str> = model.groups.iter().map(|g| g.id).collect();

    for (idx, (model_node, layout_node)) in model.nodes.iter().zip(layout_nodes).enumerate() {
        if layout_node.id != model_node.id {
            return Err(Error::InvalidModel {
                message: format!(
                    "architecture FCoSE input node order mismatch at {idx}: model={} layout={}",
                    model_node.id, layout_node.id
                ),
            });
        }
    }

    // Deterministic component discovery: mimic Mermaid's `Object.keys(notVisited)[0]` by walking
    // node order and taking the first not-yet-visited id for each component.
    let ArchitectureSpatialTraversal {
        spatial_maps,
        incident_edges,
    } = build_architecture_spatial_maps(model, &node_ids, work_meter)?;

    let mut node_group: std::collections::BTreeMap<&str, Option<&str>> =
        std::collections::BTreeMap::new();
    for n in &model.nodes {
        node_group.insert(n.id, n.in_group);
    }

    let mut node_index_by_id: FxHashMap<&str, usize> = FxHashMap::default();
    node_index_by_id.reserve(model.nodes.len().saturating_mul(2));
    for (idx, &id) in node_ids.iter().enumerate() {
        node_index_by_id.insert(id, idx);
    }

    let mut compound_index_by_id: FxHashMap<&str, usize> = FxHashMap::default();
    compound_index_by_id.reserve(model.groups.len().saturating_mul(2));
    for (idx, g) in model.groups.iter().enumerate() {
        compound_index_by_id.insert(g.id, idx);
    }

    // Track how groups connect (used when flattening alignment arrays across groups).
    //
    // Mermaid builds this while reducing `this.nodes` and each node's `service.edges` list in
    // `ArchitectureDB.getDataStructures()`. The same edge can therefore update the map once
    // per endpoint, and later endpoint traversal overwrites earlier alignment values. Do not
    // collapse this to a single global edge pass: fixtures with mixed core/data edges rely on
    // the source traversal order to decide which group alignment survives.
    let mut group_alignments: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, GroupAlignment>,
    > = std::collections::BTreeMap::new();
    for &id in &node_ids {
        let Some(edge_indices) = incident_edges.get(id) else {
            continue;
        };
        for &edge_idx in edge_indices {
            let e = &model.edges[edge_idx];
            let Some(lhs_group) = node_group.get(e.lhs_id).and_then(|v| *v) else {
                continue;
            };
            let Some(rhs_group) = node_group.get(e.rhs_id).and_then(|v| *v) else {
                continue;
            };
            if lhs_group == rhs_group {
                continue;
            }
            let alignment = dir_alignment(e.lhs_dir, e.rhs_dir);
            if alignment == GroupAlignment::Bend {
                continue;
            }
            group_alignments
                .entry(lhs_group.to_string())
                .or_default()
                .insert(rhs_group.to_string(), alignment);
            group_alignments
                .entry(rhs_group.to_string())
                .or_default()
                .insert(lhs_group.to_string(), alignment);
        }
    }

    let mut horizontal_all: Vec<Vec<usize>> = Vec::new();
    let mut vertical_all: Vec<Vec<usize>> = Vec::new();
    for spatial_map in &spatial_maps {
        let mut horizontal_alignments: IndexMap<i32, IndexMap<String, Vec<usize>>> =
            IndexMap::new();
        let mut vertical_alignments: IndexMap<i32, IndexMap<String, Vec<usize>>> = IndexMap::new();

        for (id, (x, y)) in spatial_map {
            let id = *id;
            let Some(&node_idx) = node_index_by_id.get(id) else {
                continue;
            };
            let node_group = node_group
                .get(id)
                .and_then(|v| *v)
                .unwrap_or("default")
                .to_string();

            horizontal_alignments
                .entry(*y)
                .or_default()
                .entry(node_group.clone())
                .or_default()
                .push(node_idx);

            vertical_alignments
                .entry(*x)
                .or_default()
                .entry(node_group)
                .or_default()
                .push(node_idx);
        }

        let horiz_map = flatten_alignments(
            &horizontal_alignments,
            GroupAlignment::Horizontal,
            &group_alignments,
            work_meter,
        )?;
        let vert_map = flatten_alignments(
            &vertical_alignments,
            GroupAlignment::Vertical,
            &group_alignments,
            work_meter,
        )?;

        for v in horiz_map {
            if v.len() > 1 {
                horizontal_all.push(v);
            }
        }
        for v in vert_map {
            if v.len() > 1 {
                vertical_all.push(v);
            }
        }
    }

    let mut declared_members: FxHashSet<usize> = FxHashSet::default();
    let mut declared_pairs: FxHashSet<(usize, usize)> = FxHashSet::default();
    let mut declared_relative: Vec<manatee::algo::fcose::IndexedRelativePlacementConstraint> =
        Vec::new();
    let gap = ideal_edge_length_multiplier * icon_size;
    let mut layout_hint_indices: Vec<(ArchitectureLayoutDirection, Vec<usize>)> = Vec::new();

    for hint in &model.layout_hints {
        if hint.members.len() < 2 {
            continue;
        }
        let mut members = Vec::with_capacity(hint.members.len());
        for member in &hint.members {
            let Some(&idx) = node_index_by_id.get(*member) else {
                return Err(Error::InvalidModel {
                    message: format!("architecture layout hint member not found: {member}"),
                });
            };
            declared_members.insert(idx);
            members.push(idx);
        }
        for pair in members.windows(2) {
            let a = pair[0];
            let b = pair[1];
            declared_pairs.insert((a, b));
            declared_pairs.insert((b, a));
            match hint.direction {
                ArchitectureLayoutDirection::Row => {
                    declared_relative.push(
                        manatee::algo::fcose::IndexedRelativePlacementConstraint {
                            left: Some(a),
                            right: Some(b),
                            top: None,
                            bottom: None,
                            gap,
                        },
                    );
                }
                ArchitectureLayoutDirection::Column => {
                    declared_relative.push(
                        manatee::algo::fcose::IndexedRelativePlacementConstraint {
                            left: None,
                            right: None,
                            top: Some(a),
                            bottom: Some(b),
                            gap,
                        },
                    );
                }
            }
        }
        layout_hint_indices.push((hint.direction, members));
    }

    if !declared_members.is_empty() {
        horizontal_all.retain(|group| !group.iter().any(|idx| declared_members.contains(idx)));
        vertical_all.retain(|group| !group.iter().any(|idx| declared_members.contains(idx)));
    }
    for (direction, members) in &layout_hint_indices {
        match direction {
            ArchitectureLayoutDirection::Row => horizontal_all.push(members.clone()),
            ArchitectureLayoutDirection::Column => vertical_all.push(members.clone()),
        }
    }

    // RelativePlacementConstraint (gap between borders).
    //
    // Mermaid's visited-on-pop BFS intentionally emits duplicate constraints. Count that
    // expansion on the unique coordinate graph before allocating the duplicate queue or output
    // vector, then admit the complete FCoSE constraint schedule while it is still cheap to reject.
    let relative_planning_work = checked_architecture_relative_planning_work_units(&spatial_maps)
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    work_meter.charge(relative_planning_work)?;
    let relative_plan = checked_architecture_relative_constraint_plan(
        &spatial_maps,
        &node_index_by_id,
        &declared_pairs,
    )
    .ok_or_else(|| work_meter.arithmetic_overflow())?;
    let alignment_group_count = horizontal_all
        .len()
        .checked_add(vertical_all.len())
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    let alignment_member_count = horizontal_all
        .iter()
        .chain(&vertical_all)
        .try_fold(0usize, |total, group| total.checked_add(group.len()))
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    let relative_constraint_count = declared_relative
        .len()
        .checked_add(relative_plan.constraint_count)
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    let actual_constraints = ArchitectureConstraintWork {
        alignment_group_count,
        alignment_member_count,
        relative_constraint_count,
    };
    let schedule = manatee::algo::fcose::FcoseIterationSchedule::from_normalized_graph_counts(
        fcose_num_iter,
        model.nodes.len(),
        model.groups.len(),
        model.edges.len(),
        true,
    )
    .map_err(|_| work_meter.arithmetic_overflow())?;
    let kernel_admission =
        checked_architecture_fcose_work_upper_bound(schedule, actual_constraints)
            .ok_or_else(|| work_meter.arithmetic_overflow())?;
    let relative_materialization_work = relative_plan
        .checked_materialization_work_units()
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    work_meter.preflight(
        relative_materialization_work
            .checked_add(kernel_admission)
            .ok_or_else(|| work_meter.arithmetic_overflow())?,
    )?;
    work_meter.charge(relative_materialization_work)?;

    let automatic_relative = materialize_architecture_relative_placement_constraints(
        &relative_plan,
        &node_index_by_id,
        gap,
        &declared_pairs,
    )
    .ok_or_else(|| work_meter.arithmetic_overflow())?;
    let mut relative = declared_relative;
    relative.extend(automatic_relative);

    let mut edges: Vec<manatee::algo::fcose::IndexedEdge> = Vec::new();
    let mut default_edge_length_sum = 0.0f64;
    let mut default_edge_length_cnt = 0.0f64;
    let edge_text_style = architecture_cytoscape_edge_text_style();

    // Cytoscape FCoSE de-duplicates multiple edges between the same two nodes when building
    // its internal layout graph:
    //
    // `sourceNode.getEdgesBetween(targetNode).length == 0`
    //
    // This means bidirectional/multi edges still render in the final SVG, but only the first
    // edge between each undirected node pair contributes forces to the layout.
    //
    // Without this, our spring forces can cancel in small symmetric graphs, which makes the
    // final spacing (and thus the root `viewBox/max-width`) diverge from Mermaid baselines.
    let mut seen_undirected_layout_edges: FxHashSet<(usize, usize)> = FxHashSet::default();

    for e in &model.edges {
        let Some(&a_idx) = node_index_by_id.get(e.lhs_id) else {
            return Err(Error::InvalidModel {
                message: format!("edge lhs node not found: {}", e.lhs_id),
            });
        };
        let Some(&b_idx) = node_index_by_id.get(e.rhs_id) else {
            return Err(Error::InvalidModel {
                message: format!("edge rhs node not found: {}", e.rhs_id),
            });
        };
        let (k1, k2) = if a_idx <= b_idx {
            (a_idx, b_idx)
        } else {
            (b_idx, a_idx)
        };
        if !seen_undirected_layout_edges.insert((k1, k2)) {
            continue;
        }

        let lhs_g = node_group.get(e.lhs_id).and_then(|v| *v);
        let rhs_g = node_group.get(e.rhs_id).and_then(|v| *v);
        let same_parent = lhs_g == rhs_g;

        let base_ideal_length = if same_parent {
            ideal_edge_length_multiplier * icon_size
        } else {
            0.5 * icon_size
        };
        default_edge_length_sum += base_ideal_length;
        default_edge_length_cnt += 1.0;

        let elasticity = if same_parent {
            same_group_edge_elasticity
        } else {
            0.001
        };

        let source_anchor = e.lhs_dir.and_then(Dir::from_char).map(anchor_from_dir);
        let target_anchor = e.rhs_dir.and_then(Dir::from_char).map(anchor_from_dir);
        let curve_style_segments = match (
            e.lhs_dir.and_then(Dir::from_char),
            e.rhs_dir.and_then(Dir::from_char),
        ) {
            (Some(a), Some(b)) => a.is_x() != b.is_x(),
            _ => false,
        };

        let (label_width, label_height) = match e.title.map(str::trim).filter(|t| !t.is_empty()) {
            Some(label) => {
                let metrics = architecture_cytoscape_edge_label_metrics(
                    label,
                    text_measurer,
                    &edge_text_style,
                );
                (Some(metrics.width), Some(metrics.height))
            }
            None => (None, None),
        };
        edges.push(manatee::algo::fcose::IndexedEdge {
            source: a_idx,
            target: b_idx,
            label_width,
            label_height,
            source_anchor,
            target_anchor,
            curve_style_segments,
            ideal_length: base_ideal_length,
            elasticity,
        });
    }

    let default_edge_length = if default_edge_length_cnt > 0.0 {
        default_edge_length_sum / default_edge_length_cnt
    } else {
        50.0
    };

    let mut indexed_nodes: Vec<manatee::algo::fcose::IndexedNode> =
        Vec::with_capacity(layout_nodes.len());
    for (idx, n) in layout_nodes.iter().enumerate() {
        let model_node = &model.nodes[idx];
        let parent =
            match model_node.in_group {
                Some(group_id) => Some(*compound_index_by_id.get(group_id).ok_or_else(|| {
                    Error::InvalidModel {
                        message: format!("node parent group not found: {}/{}", n.id, group_id),
                    }
                })?),
                None => None,
            };
        indexed_nodes.push(manatee::algo::fcose::IndexedNode {
            parent,
            width: n.width,
            height: n.height,
            // Mermaid Architecture feeds Cytoscape node `position()` values directly
            // into the SVG `translate(x,y)` for the icon box (i.e. it treats the
            // Cytoscape "center" as a top-left anchor). This keeps the coordinate convention
            // consistent across nodes, edges, and viewBox in upstream baselines.
            x: n.x,
            y: n.y,
            bounds_extras: node_bounds_extras
                .get(model_node.id)
                .copied()
                .unwrap_or_default(),
        });
    }

    let mut indexed_compounds: Vec<manatee::algo::fcose::IndexedCompound> =
        Vec::with_capacity(model.groups.len());
    for g in &model.groups {
        let parent =
            match g.in_group {
                Some(parent_id) => Some(*compound_index_by_id.get(parent_id).ok_or_else(|| {
                    Error::InvalidModel {
                        message: format!("compound parent group not found: {}/{}", g.id, parent_id),
                    }
                })?),
                None => None,
            };
        indexed_compounds.push(manatee::algo::fcose::IndexedCompound { parent });
    }

    let graph = manatee::algo::fcose::IndexedGraph {
        nodes: indexed_nodes,
        edges,
        compounds: indexed_compounds,
    };

    // Mermaid Architecture styles group nodes with `padding: ${db.getConfigField('padding')}px`
    // before running FCoSE, and CoSE uses that per-compound padding when updating bounds.
    let compound_padding_px = padding_px;
    let options = manatee::algo::fcose::IndexedFcoseOptions {
        alignment_constraint: Some(manatee::algo::fcose::IndexedAlignmentConstraint {
            horizontal: horizontal_all,
            vertical: vertical_all,
        }),
        relative_placement_constraint: relative,
        default_edge_length: Some(default_edge_length),
        randomize: fcose_randomize,
        node_separation: Some(fcose_node_separation),
        num_iter: Some(fcose_num_iter),
        compound_padding: Some(compound_padding_px),
        relocate_center: None,
        // Mermaid Architecture runs the layout twice (`layout.run()` inside `layoutstop`),
        // while the additive random policy models each independently wrapped call.
        rerun: true,
        random_seed: fcose_random_policy.seed(),
        random_seed_offset: None,
    };

    Ok(ArchitectureFcoseInputPlan {
        compound_ids,
        graph,
        options,
        random_policy: fcose_random_policy,
    })
}

fn architecture_cytoscape_service_bounds<'a>(
    model: &ArchitectureModelView<'a>,
    nodes: &[LayoutNode],
    text_measurer: &dyn TextMeasurer,
    icon_size: f64,
    font_size_px: f64,
) -> Vec<ArchitectureCytoscapeServiceBounds> {
    let text_style = architecture_cytoscape_text_style(font_size_px);
    let mut node_by_id: FxHashMap<&str, &LayoutNode> = FxHashMap::default();
    node_by_id.reserve(nodes.len().saturating_mul(2));
    for node in nodes {
        node_by_id.insert(node.id.as_str(), node);
    }

    let mut out = Vec::new();
    for node in &model.nodes {
        if node.node_type != ArchitectureNodeType::Service {
            continue;
        }
        let Some(layout_node) = node_by_id.get(node.id).copied() else {
            continue;
        };
        let body_bounds = Bounds {
            min_x: layout_node.x,
            min_y: layout_node.y,
            max_x: layout_node.x + icon_size,
            max_y: layout_node.y + icon_size,
        };
        let label_bounds = architecture_cytoscape_child_label_bounds(
            node.title,
            text_measurer,
            &text_style,
            font_size_px,
        );
        let label_metrics = label_bounds.map(|label| ArchitectureCytoscapeServiceLabelMetrics {
            text_width: label.metrics.width,
            half_width: label.metrics.half_width,
        });
        let contribution =
            architecture_cytoscape_child_contribution_bounds(&body_bounds, label_bounds.as_ref());
        out.push(ArchitectureCytoscapeServiceBounds {
            id: node.id.to_string(),
            in_group: node.in_group.map(str::to_string),
            body_bounds: contribution.body_bounds,
            label_bounds: contribution.label_bounds,
            label_metrics,
            union_bounds: contribution.union_bounds,
        });
    }
    out
}

fn compute_bounds(nodes: &[LayoutNode], edges: &[LayoutEdge]) -> Option<Bounds> {
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for n in nodes {
        // Architecture renderer uses top-left anchored `translate(x, y)` for nodes.
        pts.push((n.x, n.y));
        pts.push((n.x + n.width, n.y + n.height));
    }
    for e in edges {
        for p in &e.points {
            pts.push((p.x, p.y));
        }
    }
    Bounds::from_points(pts)
}

fn architecture_bounds_from_layout_rect(rect: manatee::graph::LayoutRect) -> Bounds {
    Bounds {
        min_x: rect.left,
        min_y: rect.top,
        max_x: rect.left + rect.width,
        max_y: rect.top + rect.height,
    }
}

#[derive(Debug, Clone, Default)]
struct ArchitectureFcoseResultProjection {
    compound_bounds: Vec<ArchitectureCompoundBounds>,
}

fn project_architecture_fcose_result(
    plan: &ArchitectureFcoseInputPlan<'_>,
    nodes: &mut [LayoutNode],
    result: manatee::algo::fcose::IndexedLayoutResult,
) -> ArchitectureFcoseResultProjection {
    for (idx, n) in nodes.iter_mut().enumerate() {
        if let Some(p) = result.node_positions.get(idx) {
            n.x = p.x;
            n.y = p.y;
        }
    }

    let mut compound_bounds = Vec::with_capacity(plan.compound_ids.len());
    for (idx, group_id) in plan.compound_ids.iter().enumerate() {
        if let Some(b) = result.compound_bounds.get(idx) {
            compound_bounds.push(ArchitectureCompoundBounds {
                id: (*group_id).to_string(),
                bounds: architecture_bounds_from_layout_rect(*b),
            });
        }
    }

    ArchitectureFcoseResultProjection { compound_bounds }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArchitectureLayoutAdmission {
    fcose_num_iter: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArchitectureLayoutAdmissionPlan {
    fcose_num_iter: usize,
    adapter_work_units: usize,
    preflight_work_units: usize,
}

fn checked_architecture_layout_admission_plan(
    node_count: usize,
    group_count: usize,
    edge_count: usize,
    adapter_work_plan: ArchitectureAdapterWorkPlan,
    effective_config: &Value,
) -> Option<ArchitectureLayoutAdmissionPlan> {
    let fcose_num_iter = manatee::algo::fcose::FcoseIterationSchedule::normalize_configured_number(
        config_f64(effective_config, &["architecture", "numIter"]),
    )
    .ok()?;
    let kernel_admission_units = if node_count == 0 {
        0
    } else {
        let schedule = manatee::algo::fcose::FcoseIterationSchedule::from_normalized_graph_counts(
            fcose_num_iter,
            node_count,
            group_count,
            edge_count,
            true,
        )
        .ok()?;
        checked_architecture_fcose_work_upper_bound(
            schedule,
            adapter_work_plan.declared_constraints,
        )?
    };
    let preflight_work_units = adapter_work_plan
        .work_units
        .checked_add(kernel_admission_units)?;

    Some(ArchitectureLayoutAdmissionPlan {
        fcose_num_iter,
        adapter_work_units: adapter_work_plan.work_units,
        preflight_work_units,
    })
}

fn admit_architecture_layout(
    node_count: usize,
    group_count: usize,
    edge_count: usize,
    adapter_work_plan: ArchitectureAdapterWorkPlan,
    effective_config: &Value,
    work_meter: &OperationWorkMeter,
) -> Result<ArchitectureLayoutAdmission> {
    let plan = checked_architecture_layout_admission_plan(
        node_count,
        group_count,
        edge_count,
        adapter_work_plan,
        effective_config,
    )
    .ok_or_else(|| work_meter.arithmetic_overflow())?;
    work_meter.preflight(plan.preflight_work_units)?;
    work_meter.charge(plan.adapter_work_units)?;

    Ok(ArchitectureLayoutAdmission {
        fcose_num_iter: plan.fcose_num_iter,
    })
}

pub(crate) fn layout_architecture_diagram_typed(
    model: &ArchitectureDiagramRenderModel,
    effective_config: &Value,
    text_measurer: &dyn TextMeasurer,
    operation_seed: u64,
    work_meter: &OperationWorkMeter,
) -> Result<ArchitectureDiagramLayout> {
    let adapter_work_plan = checked_typed_architecture_adapter_work_plan(model)
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    let admission = admit_architecture_layout(
        model.nodes.len(),
        model.groups.len(),
        model.edges.len(),
        adapter_work_plan,
        effective_config,
        work_meter,
    )?;
    let model = ArchitectureModelView::from_typed(model);
    layout_architecture_diagram_model_admitted(
        &model,
        effective_config,
        text_measurer,
        operation_seed,
        work_meter,
        admission,
    )
}

#[cfg(test)]
fn layout_architecture_diagram_model(
    model: &ArchitectureModelView<'_>,
    effective_config: &Value,
    text_measurer: &dyn TextMeasurer,
    operation_seed: u64,
    work_meter: &OperationWorkMeter,
) -> Result<ArchitectureDiagramLayout> {
    let adapter_work_plan = checked_architecture_adapter_work_plan(model)
        .ok_or_else(|| work_meter.arithmetic_overflow())?;
    let admission = admit_architecture_layout(
        model.nodes.len(),
        model.groups.len(),
        model.edges.len(),
        adapter_work_plan,
        effective_config,
        work_meter,
    )?;
    layout_architecture_diagram_model_admitted(
        model,
        effective_config,
        text_measurer,
        operation_seed,
        work_meter,
        admission,
    )
}

fn layout_architecture_diagram_model_admitted(
    model: &ArchitectureModelView<'_>,
    effective_config: &Value,
    text_measurer: &dyn TextMeasurer,
    operation_seed: u64,
    work_meter: &OperationWorkMeter,
    admission: ArchitectureLayoutAdmission,
) -> Result<ArchitectureDiagramLayout> {
    let icon_size = config_f64(effective_config, &["architecture", "iconSize"]).unwrap_or(80.0);
    let icon_size = icon_size.max(1.0);
    let half_icon = icon_size / 2.0;
    let padding_px = config_f64(effective_config, &["architecture", "padding"]).unwrap_or(40.0);
    let padding_px = padding_px.max(0.0);
    let font_size_px = config_f64(effective_config, &["architecture", "fontSize"]).unwrap_or(16.0);
    let font_size_px = font_size_px.max(1.0);
    let fcose_randomize =
        config_bool(effective_config, &["architecture", "randomize"]).unwrap_or(false);
    let fcose_node_separation = config_f64(effective_config, &["architecture", "nodeSeparation"])
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(75.0);
    let ideal_edge_length_multiplier = config_f64(
        effective_config,
        &["architecture", "idealEdgeLengthMultiplier"],
    )
    .filter(|v| v.is_finite() && *v > 0.0)
    .unwrap_or(1.5);
    let same_group_edge_elasticity =
        config_f64(effective_config, &["architecture", "edgeElasticity"])
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(0.45);
    let fcose_num_iter = admission.fcose_num_iter;
    let fcose_random_policy = architecture_seed_policy(
        value_at(effective_config, &["architecture", "seed"]),
        operation_seed,
    );

    let node_bounds_extras =
        architecture_fcose_node_bounds_extras(ArchitectureFcoseNodeBoundsExtrasInput {
            model,
            text_measurer,
            icon_size,
            font_size_px,
        });
    let mut nodes: Vec<LayoutNode> = Vec::new();

    // Emit nodes in Mermaid model order (stable for snapshots and close to upstream).
    for n in &model.nodes {
        nodes.push(LayoutNode {
            id: n.id.to_string(),
            // Cytoscape nodes default to `{ x: 0, y: 0 }` centers before the first layout run.
            // Our SVG model uses a top-left anchored `<g transform="translate(x,y)">` for the
            // 80x80 icon box, so convert `(0,0)` center into top-left.
            x: 0.0,
            y: 0.0,
            width: icon_size,
            height: icon_size,
            is_cluster: false,
            label_width: None,
            label_height: None,
        });
    }
    let mut fcose_compound_bounds: Vec<ArchitectureCompoundBounds> = Vec::new();

    if !nodes.is_empty() {
        let plan = build_architecture_fcose_input_plan(ArchitectureFcoseInputPlanInput {
            model,
            layout_nodes: &nodes,
            node_bounds_extras: &node_bounds_extras,
            text_measurer,
            work_meter,
            icon_size,
            padding_px,
            ideal_edge_length_multiplier,
            same_group_edge_elasticity,
            fcose_randomize,
            fcose_node_separation,
            fcose_num_iter,
            fcose_random_policy,
        })?;
        let mut work_control = ArchitectureManateeWorkControl::new(work_meter);
        let result = manatee::algo::fcose::layout_indexed_with_random_policy_and_work_control(
            &plan.graph,
            &plan.options,
            plan.random_policy,
            &mut work_control,
        )
        .map_err(|error| match error {
            manatee::Error::WorkFailure(manatee::WorkFailure::Interrupted) => work_control
                .take_denied()
                .map(Error::from)
                .unwrap_or_else(|| Error::InvalidModel {
                    message: "manatee work control interrupted without a resource error"
                        .to_string(),
                }),
            manatee::Error::WorkFailure(manatee::WorkFailure::ArithmeticOverflow) => {
                Error::from(work_meter.arithmetic_overflow())
            }
            error => Error::InvalidModel {
                message: format!("manatee layout failed: {error}"),
            },
        })?;
        let projection = project_architecture_fcose_result(&plan, &mut nodes, result);
        fcose_compound_bounds = projection.compound_bounds;
    }

    let cytoscape_service_bounds = architecture_cytoscape_service_bounds(
        model,
        &nodes,
        text_measurer,
        icon_size,
        font_size_px,
    );

    let mut node_by_id: FxHashMap<&str, &LayoutNode> = FxHashMap::default();
    node_by_id.reserve(nodes.len());
    for n in &nodes {
        node_by_id.insert(n.id.as_str(), n);
    }

    let mut edges: Vec<LayoutEdge> = Vec::new();
    for (idx, e) in model.edges.iter().enumerate() {
        let Some(&a) = node_by_id.get(e.lhs_id) else {
            return Err(Error::InvalidModel {
                message: format!("edge lhs node not found: {}", e.lhs_id),
            });
        };
        let Some(&b) = node_by_id.get(e.rhs_id) else {
            return Err(Error::InvalidModel {
                message: format!("edge rhs node not found: {}", e.rhs_id),
            });
        };

        fn endpoint(
            x: f64,
            y: f64,
            dir: Option<char>,
            icon_size: f64,
            half_icon: f64,
        ) -> (f64, f64) {
            match dir {
                Some('L') => (x, y + half_icon),
                Some('R') => (x + icon_size, y + half_icon),
                Some('T') => (x + half_icon, y),
                Some('B') => (x + half_icon, y + icon_size),
                _ => (x + half_icon, y + half_icon),
            }
        }

        let (sx, sy) = endpoint(a.x, a.y, e.lhs_dir, icon_size, half_icon);
        let (tx, ty) = endpoint(b.x, b.y, e.rhs_dir, icon_size, half_icon);

        fn cytoscape_segments_weight_distance_for_point(
            source: (f64, f64),
            target: (f64, f64),
            point: (f64, f64),
        ) -> Option<(f64, f64)> {
            // Mermaid Architecture uses Cytoscape `curve-style: segments` for XY edges and derives
            // `segment-weights`/`segment-distances` from a chosen 90° bend point.
            //
            // Reference: `repo-ref/mermaid/packages/mermaid/src/diagrams/architecture/architectureRenderer.ts`
            let (s_x, s_y) = source;
            let (t_x, t_y) = target;
            let (p_x, p_y) = point;

            if s_x == t_x || s_y == t_y {
                return None;
            }

            let denom_x = s_x - t_x;
            if denom_x == 0.0 {
                return None;
            }

            let slope = (s_y - t_y) / denom_x;
            let d =
                (p_y - s_y + ((s_x - p_x) * (s_y - t_y)) / denom_x) / (1.0 + slope * slope).sqrt();

            let w = ((p_y - s_y).powi(2) + (p_x - s_x).powi(2) - d.powi(2))
                .max(0.0)
                .sqrt();
            let dist_ab = ((t_x - s_x).powi(2) + (t_y - s_y).powi(2)).sqrt();
            if dist_ab == 0.0 {
                return None;
            }
            let mut w = w / dist_ab;

            // Ensure that the sign of `d` matches the left/right side of the line from source to
            // target, and that the sign of `w` matches whether the point is "behind" the source.
            let delta1 = (t_x - s_x) * (p_y - s_y) - (t_y - s_y) * (p_x - s_x);
            let delta1 = if delta1 >= 0.0 { 1.0 } else { -1.0 };
            let delta2 = (t_x - s_x) * (p_x - s_x) + (t_y - s_y) * (p_y - s_y);
            let delta2 = if delta2 >= 0.0 { 1.0 } else { -1.0 };

            let d = d.abs() * delta1;
            w *= delta2;

            Some((w, d))
        }

        fn cytoscape_segments_point_from_weight_distance(
            source: (f64, f64),
            target: (f64, f64),
            weight: f64,
            distance: f64,
        ) -> Option<(f64, f64)> {
            // Cytoscape "segments" curve point (for a single segment) is defined by:
            // - `weight`: normalized distance along the source->target vector
            // - `distance`: signed perpendicular offset from the line
            //
            // We reconstruct the implied bend point so our headless routing matches the
            // upstream browser output.
            let (s_x, s_y) = source;
            let (t_x, t_y) = target;
            let dx = t_x - s_x;
            let dy = t_y - s_y;
            let dist_ab = (dx * dx + dy * dy).sqrt();
            if dist_ab == 0.0 {
                return None;
            }

            let ux = dx / dist_ab;
            let uy = dy / dist_ab;
            // Left-hand normal of the line.
            let nx = -uy;
            let ny = ux;

            let along = weight * dist_ab;
            Some((
                s_x + ux * along + nx * distance,
                s_y + uy * along + ny * distance,
            ))
        }
        // Mirror Mermaid Architecture edge routing:
        //
        // - Non-XY edges use Cytoscape `curve-style: straight`, and Mermaid draws a 3-point
        //   polyline using `edge.midpoint()`, which is the midpoint of the straight segment.
        // - XY edges (`curve-style: segments`) are post-processed to create a single 90° bend.
        //   Mermaid then draws a 3-point polyline where the midpoint corresponds to that bend.
        //
        // Note: Group/junction endpoint shifts are applied later during SVG emission; these
        // layout points represent the raw Cytoscape endpoints.
        let is_xy = match (
            e.lhs_dir.and_then(Dir::from_char),
            e.rhs_dir.and_then(Dir::from_char),
        ) {
            (Some(a), Some(b)) => a.is_x() != b.is_x(),
            _ => false,
        };
        let mid = if is_xy {
            let (point_x, point_y) = if matches!(e.lhs_dir, Some('T' | 'B')) {
                (sx, ty)
            } else {
                (tx, sy)
            };
            let (w, d) = cytoscape_segments_weight_distance_for_point(
                (sx, sy),
                (tx, ty),
                (point_x, point_y),
            )
            .unwrap_or((0.0, 0.0));
            let (mx, my) = cytoscape_segments_point_from_weight_distance((sx, sy), (tx, ty), w, d)
                .unwrap_or((point_x, point_y));
            LayoutPoint { x: mx, y: my }
        } else {
            LayoutPoint {
                x: (sx + tx) / 2.0,
                y: (sy + ty) / 2.0,
            }
        };
        edges.push(LayoutEdge {
            id: format!("edge-{idx}"),
            from: e.lhs_id.to_string(),
            to: e.rhs_id.to_string(),
            from_cluster: None,
            to_cluster: None,
            points: vec![
                LayoutPoint { x: sx, y: sy },
                mid,
                LayoutPoint { x: tx, y: ty },
            ],
            label: None,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        });
    }
    let bounds = compute_bounds(&nodes, &edges);

    Ok(ArchitectureDiagramLayout {
        nodes,
        edges,
        cytoscape_service_bounds,
        fcose_compound_bounds,
        bounds,
    })
}

#[cfg(test)]
mod tests {
    fn layout_node(id: &str, width: f64, height: f64) -> crate::model::LayoutNode {
        crate::model::LayoutNode {
            id: id.to_string(),
            x: 0.0,
            y: 0.0,
            width,
            height,
            is_cluster: false,
            label_width: None,
            label_height: None,
        }
    }

    fn layout_rect(left: f64, top: f64, width: f64, height: f64) -> manatee::LayoutRect {
        manatee::LayoutRect {
            left,
            top,
            width,
            height,
        }
    }

    fn build_test_plan<'a>(
        model: &'a super::ArchitectureModelView<'a>,
        layout_nodes: &[crate::model::LayoutNode],
        node_bounds_extras: &rustc_hash::FxHashMap<&'a str, manatee::BoundsExtras>,
    ) -> super::ArchitectureFcoseInputPlan<'a> {
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let work_meter = crate::resources::OperationWorkMeter::new(
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input(),
        );
        super::build_architecture_fcose_input_plan(super::ArchitectureFcoseInputPlanInput {
            model,
            layout_nodes,
            node_bounds_extras,
            text_measurer: &measurer,
            work_meter: &work_meter,
            icon_size: 80.0,
            padding_px: 40.0,
            ideal_edge_length_multiplier: 1.5,
            same_group_edge_elasticity: 0.45,
            fcose_randomize: false,
            fcose_node_separation: 75.0,
            fcose_num_iter: 2500,
            fcose_random_policy: manatee::FcoseRandomPolicy::seeded(
                manatee::FcoseRandomSource::Mulberry32,
                1,
            )
            .with_seed_offset(0)
            .with_reset_seed_each_run(true),
        })
        .expect("build architecture FCoSE input plan")
    }

    fn single_node_model<'a>() -> super::ArchitectureModelView<'a> {
        super::ArchitectureModelView {
            nodes: vec![super::ArchitectureNodeView {
                id: "api",
                node_type: super::ArchitectureNodeType::Service,
                title: None,
                in_group: None,
            }],
            groups: Vec::new(),
            edges: Vec::new(),
            layout_hints: Vec::new(),
        }
    }

    fn architecture_duplicate_pop_counterexample() -> super::ArchitectureModelView<'static> {
        super::ArchitectureModelView {
            nodes: ["n0", "n1", "n2", "n3", "n4"]
                .into_iter()
                .map(|id| super::ArchitectureNodeView {
                    id,
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: None,
                })
                .collect(),
            groups: Vec::new(),
            // Mermaid processes these in declaration order. The final edge re-enqueues `n3`, so
            // its second dequeue must overwrite the still-unvisited `n4` back to `(1, 0)`.
            edges: vec![
                super::ArchitectureEdgeView {
                    lhs_id: "n0",
                    rhs_id: "n2",
                    lhs_dir: Some('L'),
                    rhs_dir: Some('B'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "n1",
                    rhs_id: "n2",
                    lhs_dir: Some('L'),
                    rhs_dir: Some('R'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "n1",
                    rhs_id: "n4",
                    lhs_dir: Some('B'),
                    rhs_dir: Some('R'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "n0",
                    rhs_id: "n3",
                    lhs_dir: Some('R'),
                    rhs_dir: Some('L'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "n3",
                    rhs_id: "n4",
                    lhs_dir: Some('R'),
                    rhs_dir: Some('L'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "n2",
                    rhs_id: "n3",
                    lhs_dir: Some('R'),
                    rhs_dir: Some('T'),
                    title: None,
                },
            ],
            layout_hints: Vec::new(),
        }
    }

    fn architecture_layered_diamond() -> super::ArchitectureModelView<'static> {
        super::ArchitectureModelView {
            nodes: ["d0", "d1", "d2", "d3"]
                .into_iter()
                .map(|id| super::ArchitectureNodeView {
                    id,
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: None,
                })
                .collect(),
            groups: Vec::new(),
            edges: vec![
                super::ArchitectureEdgeView {
                    lhs_id: "d0",
                    rhs_id: "d1",
                    lhs_dir: Some('L'),
                    rhs_dir: Some('R'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "d0",
                    rhs_id: "d2",
                    lhs_dir: Some('T'),
                    rhs_dir: Some('B'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "d1",
                    rhs_id: "d3",
                    lhs_dir: Some('T'),
                    rhs_dir: Some('B'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "d2",
                    rhs_id: "d3",
                    lhs_dir: Some('L'),
                    rhs_dir: Some('R'),
                    title: None,
                },
            ],
            layout_hints: Vec::new(),
        }
    }

    fn architecture_numeric_id_collision() -> super::ArchitectureModelView<'static> {
        super::ArchitectureModelView {
            nodes: ["1", "10", "3", "2"]
                .into_iter()
                .map(|id| super::ArchitectureNodeView {
                    id,
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: None,
                })
                .collect(),
            groups: Vec::new(),
            // Mermaid inserts `10` before `2`, but `Object.entries()` later enumerates numeric
            // property keys as `1, 2, 3, 10`. The later `10` entry must therefore win the shared
            // `(1, 0)` inverse-coordinate slot used by automatic relative constraints.
            edges: vec![
                super::ArchitectureEdgeView {
                    lhs_id: "1",
                    rhs_id: "10",
                    lhs_dir: Some('R'),
                    rhs_dir: Some('L'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "1",
                    rhs_id: "3",
                    lhs_dir: Some('T'),
                    rhs_dir: Some('B'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "3",
                    rhs_id: "2",
                    lhs_dir: Some('R'),
                    rhs_dir: Some('B'),
                    title: None,
                },
            ],
            layout_hints: Vec::new(),
        }
    }

    fn flatten_alignment_object(
        group_sizes: &[usize],
    ) -> indexmap::IndexMap<i32, indexmap::IndexMap<String, Vec<usize>>> {
        let mut next_member = 0usize;
        let mut groups = indexmap::IndexMap::new();
        for (group_index, &group_size) in group_sizes.iter().enumerate() {
            let members = (next_member..next_member + group_size).collect();
            next_member += group_size;
            groups.insert(format!("group-{group_index}"), members);
        }
        indexmap::IndexMap::from([(0, groups)])
    }

    #[test]
    fn flatten_alignment_work_plan_counts_pair_expansion() {
        let alignment_obj = flatten_alignment_object(&[2, 3, 4]);

        let plan = super::FlattenAlignmentsWorkPlan::checked(&alignment_obj)
            .expect("flatten work plan should fit");

        assert_eq!(plan.direction_bucket_count, 1);
        assert_eq!(plan.group_count, 3);
        assert_eq!(plan.source_member_count, 9);
        assert_eq!(plan.pair_count, 3);
        assert_eq!(plan.expanded_member_count, 18);
        assert_eq!(plan.output_key_bound, 6);
        assert_eq!(plan.sort_work_units, 0);
        assert_eq!(plan.work_units, 40);
    }

    #[test]
    fn flatten_alignment_metadata_and_sort_work_are_preflighted() {
        use crate::resources::{
            OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
        };

        let mut numeric_groups = indexmap::IndexMap::new();
        numeric_groups.insert("10".to_string(), vec![0, 1]);
        numeric_groups.insert("2".to_string(), vec![2, 3, 4]);
        numeric_groups.insert("1".to_string(), vec![5, 6, 7, 8]);
        let alignment_obj = indexmap::IndexMap::from([(0, numeric_groups)]);
        let metadata = super::checked_flatten_alignments_metadata(&alignment_obj)
            .expect("flatten metadata should fit");
        let work_plan =
            super::FlattenAlignmentsWorkPlan::checked_with_metadata(&alignment_obj, metadata)
                .expect("flatten work plan should fit");

        assert_eq!(metadata.checked_work_units(), Some(4));
        assert_eq!(work_plan.sort_work_units, 6);
        assert_eq!(work_plan.work_units, 46);

        let metadata_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 3)
            .unwrap();
        let metadata_meter = OperationWorkMeter::new(metadata_policy);
        let metadata_error = super::flatten_alignments(
            &alignment_obj,
            super::GroupAlignment::Horizontal,
            &Default::default(),
            &metadata_meter,
        )
        .unwrap_err();
        let crate::Error::ResourceLimitExceeded(metadata_error) = metadata_error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(metadata_error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(metadata_error.actual, 4);
        assert_eq!(metadata_meter.used(), 0);

        let linear_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(
                ResourceLimitId::MaxLayoutWorkUnits,
                work_plan.work_units - work_plan.sort_work_units,
            )
            .unwrap();
        let linear_meter = OperationWorkMeter::new(linear_policy);
        let sort_error = super::flatten_alignments(
            &alignment_obj,
            super::GroupAlignment::Horizontal,
            &Default::default(),
            &linear_meter,
        )
        .unwrap_err();
        let crate::Error::ResourceLimitExceeded(sort_error) = sort_error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(sort_error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(sort_error.actual, work_plan.work_units);
        assert_eq!(linear_meter.used(), 0);
    }

    #[test]
    fn flatten_alignment_budget_rejects_before_pair_expansion() {
        use crate::resources::{
            OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
        };

        let alignment_obj = flatten_alignment_object(&[2, 3, 4]);
        let original = alignment_obj.clone();
        let work_plan = super::FlattenAlignmentsWorkPlan::checked(&alignment_obj)
            .expect("flatten work plan should fit");
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(
                ResourceLimitId::MaxLayoutWorkUnits,
                work_plan.work_units - 1,
            )
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        let error = super::flatten_alignments(
            &alignment_obj,
            super::GroupAlignment::Horizontal,
            &Default::default(),
            &meter,
        )
        .unwrap_err();

        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.actual, work_plan.work_units);
        assert_eq!(error.max, work_plan.work_units - 1);
        assert_eq!(meter.used(), 0);
        assert_eq!(alignment_obj, original);
    }

    #[test]
    fn flatten_alignments_preserves_javascript_key_order_and_duplicates() {
        use crate::resources::{OperationWorkMeter, RenderResourcePolicy};

        let mut alignment_obj = indexmap::IndexMap::new();
        let mut negative_groups = indexmap::IndexMap::new();
        negative_groups.insert("10".to_string(), vec![10, 10]);
        negative_groups.insert("2".to_string(), vec![2, 2]);
        negative_groups.insert("01".to_string(), vec![1, 1]);
        alignment_obj.insert(-1, negative_groups);
        alignment_obj.insert(
            2,
            indexmap::IndexMap::from([("solo-two".to_string(), vec![20, 20])]),
        );
        alignment_obj.insert(
            0,
            indexmap::IndexMap::from([("solo-zero".to_string(), vec![0, 0])]),
        );

        let mut group_alignments = std::collections::BTreeMap::new();
        group_alignments.insert(
            "2".to_string(),
            std::collections::BTreeMap::from([
                ("10".to_string(), super::GroupAlignment::Horizontal),
                ("01".to_string(), super::GroupAlignment::Horizontal),
            ]),
        );
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        let flattened = super::flatten_alignments(
            &alignment_obj,
            super::GroupAlignment::Horizontal,
            &group_alignments,
            &meter,
        )
        .expect("flatten alignments");

        assert_eq!(
            flattened,
            vec![
                vec![0, 0],
                vec![20, 20],
                vec![2, 2, 10, 10, 2, 2, 1, 1],
                vec![10, 10],
                vec![1, 1],
            ]
        );
        assert_eq!(super::js_array_index_key("01"), None);
        assert_eq!(super::js_array_index_key("4294967294"), Some(u32::MAX - 1));
        assert_eq!(super::js_array_index_key("4294967295"), None);
    }

    #[test]
    fn checked_architecture_and_flatten_work_helpers_reject_overflow() {
        let schedule =
            manatee::algo::fcose::FcoseIterationSchedule::from_normalized_counts(5, 1, 0, true)
                .unwrap();
        let declared_constraints = super::ArchitectureConstraintWork {
            alignment_group_count: usize::MAX,
            alignment_member_count: 1,
            relative_constraint_count: 0,
        };

        assert_eq!(
            super::checked_architecture_fcose_work_upper_bound(schedule, declared_constraints),
            None
        );
        assert_eq!(super::checked_unordered_pair_count(usize::MAX), None);
        assert_eq!(
            super::checked_flatten_alignment_bucket_cardinality(usize::MAX, 1),
            None
        );
        assert_eq!(super::checked_sort_work_units(usize::MAX), None);
    }

    #[test]
    fn architecture_declared_constraint_upper_bound_matches_kernel_formula() {
        let model = super::ArchitectureModelView {
            nodes: Vec::new(),
            groups: Vec::new(),
            edges: Vec::new(),
            layout_hints: vec![
                super::ArchitectureLayoutHintView {
                    direction:
                        merman_core::diagrams::architecture::ArchitectureLayoutDirection::Row,
                    members: vec!["a", "b"],
                },
                super::ArchitectureLayoutHintView {
                    direction:
                        merman_core::diagrams::architecture::ArchitectureLayoutDirection::Column,
                    members: vec!["c", "d", "e"],
                },
                super::ArchitectureLayoutHintView {
                    direction:
                        merman_core::diagrams::architecture::ArchitectureLayoutDirection::Row,
                    members: vec!["f", "g", "h", "i"],
                },
            ],
        };
        let adapter = super::checked_architecture_adapter_work_plan(&model)
            .expect("adapter work plan should fit");
        let schedule = manatee::algo::fcose::FcoseIterationSchedule::from_normalized_graph_counts(
            5, 1, 1, 1, true,
        )
        .unwrap();

        assert_eq!(adapter.work_units, 9);
        assert_eq!(adapter.declared_constraints.alignment_group_count, 3);
        assert_eq!(adapter.declared_constraints.alignment_member_count, 9);
        assert_eq!(adapter.declared_constraints.relative_constraint_count, 6);
        assert_eq!(
            super::checked_architecture_fcose_work_upper_bound(
                schedule,
                adapter.declared_constraints,
            ),
            Some(472)
        );
    }

    #[test]
    fn architecture_work_admission_is_exact_and_non_consuming_on_rejection() {
        use crate::resources::{
            OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
        };

        let model = single_node_model();
        let config = serde_json::json!({"architecture": {"numIter": 5, "randomize": false}});
        let measurer = crate::text::DeterministicTextMeasurer::default();

        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 55)
            .unwrap();
        let exact_meter = OperationWorkMeter::new(exact_policy);
        super::layout_architecture_diagram_model(&model, &config, &measurer, 1, &exact_meter)
            .unwrap();
        assert_eq!(exact_meter.used(), 55);

        let narrow_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 9)
            .unwrap();
        let narrow_meter = OperationWorkMeter::new(narrow_policy);
        let error =
            super::layout_architecture_diagram_model(&model, &config, &measurer, 1, &narrow_meter)
                .unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        // Admission now includes the adapter's source-sized spatial planning pass before any
        // allocation or FCoSE work. Keep the rejection assertion aligned with that full plan.
        assert_eq!(error.actual, 22);
        assert_eq!(error.max, 9);
        assert_eq!(narrow_meter.used(), 0);
    }

    #[test]
    fn typed_architecture_rejects_before_materializing_the_adapter_projection() {
        use crate::resources::{
            OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
        };
        use merman_core::{Engine, ParseOptions, RenderSemanticModel};

        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "architecture-beta\nservice api(server)[API]\n",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let RenderSemanticModel::Architecture(model) = parsed.model() else {
            panic!("expected Architecture model");
        };
        let config = serde_json::json!({"architecture": {"numIter": 5, "randomize": false}});
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let adapter_work_plan = super::checked_typed_architecture_adapter_work_plan(model)
            .expect("typed adapter work plan");
        let admission_plan = super::checked_architecture_layout_admission_plan(
            model.nodes.len(),
            model.groups.len(),
            model.edges.len(),
            adapter_work_plan,
            &config,
        )
        .expect("typed Architecture admission plan");

        super::reset_typed_architecture_projection_count();
        let narrow_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(
                ResourceLimitId::MaxLayoutWorkUnits,
                admission_plan.preflight_work_units - 1,
            )
            .unwrap();
        let narrow_meter = OperationWorkMeter::new(narrow_policy);
        let error =
            super::layout_architecture_diagram_typed(model, &config, &measurer, 1, &narrow_meter)
                .unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.actual, admission_plan.preflight_work_units);
        assert_eq!(narrow_meter.used(), 0);
        assert_eq!(super::typed_architecture_projection_count(), 0);

        super::reset_typed_architecture_projection_count();
        let admission_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(
                ResourceLimitId::MaxLayoutWorkUnits,
                admission_plan.preflight_work_units,
            )
            .unwrap();
        let admission_meter = OperationWorkMeter::new(admission_policy);
        let _ = super::layout_architecture_diagram_typed(
            model,
            &config,
            &measurer,
            1,
            &admission_meter,
        );
        assert_eq!(super::typed_architecture_projection_count(), 1);
        assert!(admission_meter.used() >= admission_plan.adapter_work_units);

        super::reset_typed_architecture_projection_count();
        let baseline_meter =
            OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        super::layout_architecture_diagram_typed(model, &config, &measurer, 1, &baseline_meter)
            .unwrap();
        let exact_work = baseline_meter.used();
        assert!(exact_work >= admission_plan.preflight_work_units);
        assert_eq!(super::typed_architecture_projection_count(), 1);

        super::reset_typed_architecture_projection_count();
        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, exact_work)
            .unwrap();
        let exact_meter = OperationWorkMeter::new(exact_policy);
        super::layout_architecture_diagram_typed(model, &config, &measurer, 1, &exact_meter)
            .unwrap();
        assert_eq!(exact_meter.used(), exact_work);
        assert_eq!(super::typed_architecture_projection_count(), 1);
    }

    #[test]
    fn architecture_spatial_bfs_preserves_mermaid_duplicate_pop_constraint_order() {
        use crate::resources::{OperationWorkMeter, RenderResourcePolicy, ResourceLimitId};

        let model = architecture_duplicate_pop_counterexample();
        let node_ids = model.nodes.iter().map(|node| node.id).collect::<Vec<_>>();
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 35)
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        let traversal = super::build_architecture_spatial_maps(&model, &node_ids, &meter)
            .expect("exact Mermaid BFS work budget");
        let spatial_maps = traversal.spatial_maps;

        assert_eq!(meter.used(), 35);
        assert_eq!(spatial_maps.len(), 1);
        assert_eq!(spatial_maps[0].get("n4"), Some(&(1, 0)));

        let node_index_by_id = node_ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index))
            .collect::<rustc_hash::FxHashMap<_, _>>();
        let declared_pairs = rustc_hash::FxHashSet::default();
        let relative_plan = super::checked_architecture_relative_constraint_plan(
            &spatial_maps,
            &node_index_by_id,
            &declared_pairs,
        )
        .expect("relative constraint plan");
        let constraints = super::materialize_architecture_relative_placement_constraints(
            &relative_plan,
            &node_index_by_id,
            120.0,
            &declared_pairs,
        )
        .expect("relative constraints");

        assert_eq!(
            constraints
                .iter()
                .map(|constraint| {
                    (
                        constraint.left,
                        constraint.right,
                        constraint.top,
                        constraint.bottom,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (Some(3), Some(4), None, None),
                (None, None, Some(3), Some(1)),
                (Some(2), Some(1), None, None),
            ]
        );
    }

    #[test]
    fn architecture_spatial_bfs_exact_admission_matches_layered_diamond_work() {
        use crate::resources::{
            OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
        };

        let model = architecture_layered_diamond();
        let node_ids = model.nodes.iter().map(|node| node.id).collect::<Vec<_>>();

        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 20)
            .unwrap();
        let exact_meter = OperationWorkMeter::new(exact_policy);
        super::build_architecture_spatial_maps(&model, &node_ids, &exact_meter)
            .expect("the exact layered-diamond traversal budget");
        assert_eq!(exact_meter.used(), 20);

        let narrow_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 19)
            .unwrap();
        let narrow_meter = OperationWorkMeter::new(narrow_policy);
        let error = super::build_architecture_spatial_maps(&model, &node_ids, &narrow_meter)
            .expect_err("the layered diamond must be rejected before queue materialization");
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.actual, 20);
        assert_eq!(error.max, 19);
        assert_eq!(narrow_meter.used(), 0);
    }

    #[test]
    fn architecture_spatial_bfs_keeps_exact_preflight_for_mixed_components() {
        use crate::resources::{
            OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
        };

        let mut model = architecture_duplicate_pop_counterexample();
        let diamond = architecture_layered_diamond();
        model.nodes.extend(diamond.nodes);
        model.edges.extend(diamond.edges);
        let node_ids = model.nodes.iter().map(|node| node.id).collect::<Vec<_>>();
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 19)
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        let error = super::build_architecture_spatial_maps(&model, &node_ids, &meter)
            .expect_err("the exact diamond must remain preflighted beside a runtime component");
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.actual, 20);
        assert_eq!(error.max, 19);
        assert_eq!(meter.used(), 0);
    }

    #[test]
    fn architecture_spatial_maps_follow_javascript_numeric_property_order() {
        use crate::resources::{OperationWorkMeter, RenderResourcePolicy};

        let model = architecture_numeric_id_collision();
        let node_ids = model.nodes.iter().map(|node| node.id).collect::<Vec<_>>();
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let traversal = super::build_architecture_spatial_maps(&model, &node_ids, &meter)
            .expect("numeric architecture spatial map");

        assert_eq!(meter.used(), 34);
        assert_eq!(
            traversal.spatial_maps[0]
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec!["1", "2", "3", "10"]
        );

        let node_index_by_id = node_ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index))
            .collect::<rustc_hash::FxHashMap<_, _>>();
        let declared_pairs = rustc_hash::FxHashSet::default();
        let relative_plan = super::checked_architecture_relative_constraint_plan(
            &traversal.spatial_maps,
            &node_index_by_id,
            &declared_pairs,
        )
        .expect("relative constraint plan");
        let constraints = super::materialize_architecture_relative_placement_constraints(
            &relative_plan,
            &node_index_by_id,
            120.0,
            &declared_pairs,
        )
        .expect("relative constraints");

        assert_eq!(
            constraints
                .iter()
                .map(|constraint| {
                    (
                        constraint.left,
                        constraint.right,
                        constraint.top,
                        constraint.bottom,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (Some(0), Some(1), None, None),
                (None, None, Some(2), Some(0)),
            ]
        );
    }

    #[test]
    fn architecture_spatial_bfs_rejects_before_over_budget_enqueue() {
        use crate::resources::{
            OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
            ResourceLimitPhase,
        };

        let model = architecture_duplicate_pop_counterexample();
        let node_ids = model.nodes.iter().map(|node| node.id).collect::<Vec<_>>();
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 4)
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        let error = super::build_architecture_spatial_maps(&model, &node_ids, &meter)
            .expect_err("the first child enqueue must exceed the budget");
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };

        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(error.actual, 5);
        assert_eq!(error.max, 4);
        assert_eq!(meter.used(), 4);
    }

    #[test]
    fn architecture_rejects_repeated_constraint_work_before_adapter_materialization() {
        use crate::resources::{OperationWorkMeter, RenderResourcePolicy, ResourceLimitId};

        let model = super::ArchitectureModelView {
            nodes: vec![
                super::ArchitectureNodeView {
                    id: "a",
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: None,
                },
                super::ArchitectureNodeView {
                    id: "b",
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: None,
                },
            ],
            groups: Vec::new(),
            edges: Vec::new(),
            layout_hints: (0..8)
                .map(|_| super::ArchitectureLayoutHintView {
                    direction:
                        merman_core::diagrams::architecture::ArchitectureLayoutDirection::Row,
                    members: vec!["a", "b"],
                })
                .collect(),
        };
        let config = serde_json::json!({"architecture": {"numIter": 5, "randomize": false}});
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 100)
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        let error = super::layout_architecture_diagram_model(&model, &config, &measurer, 1, &meter)
            .unwrap_err();

        assert!(matches!(error, crate::Error::ResourceLimitExceeded(_)));
        assert_eq!(meter.used(), 0);
    }

    #[test]
    fn architecture_rejects_grid_constraint_expansion_before_duplicate_materialization() {
        use crate::resources::{
            OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause, ResourceLimitId,
            ResourceLimitPhase,
        };

        let ids = (0..12 * 12)
            .map(|index| format!("node-{index}"))
            .collect::<Vec<_>>();
        let nodes = ids
            .iter()
            .map(|id| super::ArchitectureNodeView {
                id,
                node_type: super::ArchitectureNodeType::Service,
                title: None,
                in_group: None,
            })
            .collect::<Vec<_>>();
        let mut edges = Vec::new();
        for y in 0..12 {
            for x in 0..12 {
                let source = y * 12 + x;
                if x + 1 < 12 {
                    edges.push(super::ArchitectureEdgeView {
                        lhs_id: ids[source].as_str(),
                        rhs_id: ids[source + 1].as_str(),
                        lhs_dir: Some('R'),
                        rhs_dir: Some('L'),
                        title: None,
                    });
                }
                if y + 1 < 12 {
                    edges.push(super::ArchitectureEdgeView {
                        lhs_id: ids[source].as_str(),
                        rhs_id: ids[source + 12].as_str(),
                        lhs_dir: Some('T'),
                        rhs_dir: Some('B'),
                        title: None,
                    });
                }
            }
        }
        let model = super::ArchitectureModelView {
            nodes,
            groups: Vec::new(),
            edges,
            layout_hints: Vec::new(),
        };
        let config = serde_json::json!({"architecture": {"numIter": 1, "randomize": false}});
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let policy = RenderResourcePolicy::interactive()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 800_000)
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        let error = super::layout_architecture_diagram_model(&model, &config, &measurer, 1, &meter)
            .unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };

        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(error.max, 800_000);
        assert!(error.actual > 1_000_000);
        assert!(
            meter.used() < 100_000,
            "duplicate BFS/materialization work must not be consumed after rejection"
        );
    }

    #[test]
    fn architecture_num_iter_overflow_fails_under_unlimited_policy() {
        use crate::resources::{OperationWorkMeter, RenderResourcePolicy, ResourceLimitCause};

        let model = single_node_model();
        let config = serde_json::json!({"architecture": {"numIter": f64::MAX}});
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        let error = super::layout_architecture_diagram_model(&model, &config, &measurer, 1, &meter)
            .unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected layout work resource error");
        };
        assert_eq!(error.cause, ResourceLimitCause::ArithmeticOverflow);
        assert_eq!(error.limit, "max_layout_work_units");
        assert_eq!(meter.used(), 0);
    }

    #[test]
    fn architecture_fcose_input_plan_preserves_minimal_graph_order_and_edge_input() {
        let model = super::ArchitectureModelView {
            nodes: vec![
                super::ArchitectureNodeView {
                    id: "api",
                    node_type: super::ArchitectureNodeType::Service,
                    title: Some("API"),
                    in_group: None,
                },
                super::ArchitectureNodeView {
                    id: "db",
                    node_type: super::ArchitectureNodeType::Service,
                    title: Some("DB"),
                    in_group: None,
                },
            ],
            groups: Vec::new(),
            edges: vec![super::ArchitectureEdgeView {
                lhs_id: "api",
                rhs_id: "db",
                lhs_dir: Some('R'),
                rhs_dir: Some('L'),
                title: Some("reads"),
            }],
            layout_hints: Vec::new(),
        };
        let layout_nodes = vec![
            layout_node("api", 80.0, 80.0),
            layout_node("db", 80.0, 80.0),
        ];
        let node_bounds_extras = rustc_hash::FxHashMap::default();

        let plan = build_test_plan(&model, &layout_nodes, &node_bounds_extras);

        assert_eq!(plan.graph.nodes.len(), 2);
        assert_eq!(plan.graph.nodes[0].width, 80.0);
        assert_eq!(plan.graph.nodes[0].height, 80.0);
        assert_eq!(plan.graph.edges.len(), 1);
        assert_eq!(plan.graph.edges[0].source, 0);
        assert_eq!(plan.graph.edges[0].target, 1);
        assert_eq!(
            plan.graph.edges[0].source_anchor,
            Some(manatee::Anchor::Right)
        );
        assert_eq!(
            plan.graph.edges[0].target_anchor,
            Some(manatee::Anchor::Left)
        );
        assert!(plan.graph.edges[0].label_width.unwrap_or_default() > 0.0);
        assert_eq!(plan.options.default_edge_length, Some(120.0));
    }

    #[test]
    fn architecture_seed_zero_uses_an_operation_owned_continuous_stream() {
        let seed_zero = serde_json::json!(0);
        let random_policy = super::architecture_seed_policy(Some(&seed_zero), 77);
        let model = super::ArchitectureModelView {
            nodes: vec![super::ArchitectureNodeView {
                id: "api",
                node_type: super::ArchitectureNodeType::Service,
                title: Some("API"),
                in_group: None,
            }],
            groups: Vec::new(),
            edges: Vec::new(),
            layout_hints: Vec::new(),
        };
        let layout_nodes = vec![layout_node("api", 80.0, 80.0)];
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let work_meter = crate::resources::OperationWorkMeter::new(
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input(),
        );
        let plan =
            super::build_architecture_fcose_input_plan(super::ArchitectureFcoseInputPlanInput {
                model: &model,
                layout_nodes: &layout_nodes,
                node_bounds_extras: &Default::default(),
                text_measurer: &measurer,
                work_meter: &work_meter,
                icon_size: 80.0,
                padding_px: 40.0,
                ideal_edge_length_multiplier: 1.5,
                same_group_edge_elasticity: 0.45,
                fcose_randomize: false,
                fcose_node_separation: 75.0,
                fcose_num_iter: 2500,
                fcose_random_policy: random_policy,
            })
            .expect("build Architecture FCoSE input plan");

        assert_eq!(
            plan.random_policy.source(),
            manatee::FcoseRandomSource::Mulberry32
        );
        assert!(!plan.random_policy.resets_seed_each_run());
        assert_eq!(plan.random_policy.seed(), 77);
        assert_eq!(plan.random_policy.seed_offset(), Some(0));
    }

    #[test]
    fn architecture_seed_uses_javascript_to_uint32_semantics() {
        assert_eq!(super::js_to_uint32(1.9), 1);
        assert_eq!(super::js_to_uint32(-1.9), u64::from(u32::MAX));
        assert_eq!(super::js_to_uint32(4_294_967_297.0), 1);
        let wraps_to_zero = serde_json::json!(4_294_967_296.0);
        assert_eq!(
            super::architecture_seed_policy(Some(&wraps_to_zero), 77).seed(),
            0,
            "JavaScript checks seed === 0 before coercing the enabled seed with >>> 0"
        );
    }

    #[test]
    fn architecture_seed_distinguishes_json_number_zero_from_string_zero() {
        let number_zero = serde_json::json!(0);
        let string_zero = serde_json::json!("0");

        let number_policy = super::architecture_seed_policy(Some(&number_zero), 77);
        let string_policy = super::architecture_seed_policy(Some(&string_zero), 77);

        assert_eq!(number_policy.seed(), 77);
        assert!(!number_policy.resets_seed_each_run());
        assert_eq!(string_policy.seed(), 0);
        assert_eq!(
            string_policy.source(),
            manatee::FcoseRandomSource::Mulberry32
        );
        assert!(string_policy.resets_seed_each_run());
    }

    #[test]
    fn architecture_fcose_input_plan_applies_layout_hints() {
        let model = super::ArchitectureModelView {
            nodes: vec![
                super::ArchitectureNodeView {
                    id: "db1",
                    node_type: super::ArchitectureNodeType::Service,
                    title: Some("DB1"),
                    in_group: None,
                },
                super::ArchitectureNodeView {
                    id: "db2",
                    node_type: super::ArchitectureNodeType::Service,
                    title: Some("DB2"),
                    in_group: None,
                },
                super::ArchitectureNodeView {
                    id: "db3",
                    node_type: super::ArchitectureNodeType::Service,
                    title: Some("DB3"),
                    in_group: None,
                },
                super::ArchitectureNodeView {
                    id: "join",
                    node_type: super::ArchitectureNodeType::Junction,
                    title: None,
                    in_group: None,
                },
            ],
            groups: Vec::new(),
            edges: Vec::new(),
            layout_hints: vec![
                super::ArchitectureLayoutHintView {
                    direction:
                        merman_core::diagrams::architecture::ArchitectureLayoutDirection::Row,
                    members: vec!["db1", "db2", "db3"],
                },
                super::ArchitectureLayoutHintView {
                    direction:
                        merman_core::diagrams::architecture::ArchitectureLayoutDirection::Column,
                    members: vec!["db2", "join"],
                },
            ],
        };
        let layout_nodes = vec![
            layout_node("db1", 80.0, 80.0),
            layout_node("db2", 80.0, 80.0),
            layout_node("db3", 80.0, 80.0),
            layout_node("join", 40.0, 40.0),
        ];
        let node_bounds_extras = rustc_hash::FxHashMap::default();

        let plan = build_test_plan(&model, &layout_nodes, &node_bounds_extras);
        let alignment = plan
            .options
            .alignment_constraint
            .as_ref()
            .expect("alignment constraint");

        assert!(
            alignment.horizontal.iter().any(|group| group == &[0, 1, 2]),
            "align row should become a horizontal alignment: {:?}",
            alignment.horizontal
        );
        assert!(
            alignment.vertical.iter().any(|group| group == &[1, 3]),
            "align column should become a vertical alignment: {:?}",
            alignment.vertical
        );
        assert!(plan.options.relative_placement_constraint.iter().any(|c| {
            c.left == Some(0) && c.right == Some(1) && c.top.is_none() && c.bottom.is_none()
        }));
        assert!(plan.options.relative_placement_constraint.iter().any(|c| {
            c.left == Some(1) && c.right == Some(2) && c.top.is_none() && c.bottom.is_none()
        }));
        assert!(plan.options.relative_placement_constraint.iter().any(|c| {
            c.top == Some(1) && c.bottom == Some(3) && c.left.is_none() && c.right.is_none()
        }));
    }

    #[test]
    fn architecture_fcose_input_plan_preserves_nested_compound_parents() {
        let model = super::ArchitectureModelView {
            nodes: vec![
                super::ArchitectureNodeView {
                    id: "api",
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: Some("platform"),
                },
                super::ArchitectureNodeView {
                    id: "db",
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: Some("core"),
                },
            ],
            groups: vec![
                super::ArchitectureGroupView {
                    id: "core",
                    in_group: None,
                },
                super::ArchitectureGroupView {
                    id: "platform",
                    in_group: Some("core"),
                },
            ],
            edges: Vec::new(),
            layout_hints: Vec::new(),
        };
        let layout_nodes = vec![
            layout_node("api", 80.0, 80.0),
            layout_node("db", 80.0, 80.0),
        ];
        let node_bounds_extras = rustc_hash::FxHashMap::default();

        let plan = build_test_plan(&model, &layout_nodes, &node_bounds_extras);

        assert_eq!(plan.compound_ids, vec!["core", "platform"]);
        assert_eq!(plan.graph.compounds.len(), 2);
        assert_eq!(plan.graph.compounds[0].parent, None);
        assert_eq!(plan.graph.compounds[1].parent, Some(0));
        assert_eq!(plan.graph.nodes[0].parent, Some(1));
        assert_eq!(plan.graph.nodes[1].parent, Some(0));
    }

    #[test]
    fn architecture_fcose_input_plan_uses_layout_node_size_and_bounds_extras() {
        let model = super::ArchitectureModelView {
            nodes: vec![super::ArchitectureNodeView {
                id: "api",
                node_type: super::ArchitectureNodeType::Service,
                title: Some("API"),
                in_group: None,
            }],
            groups: Vec::new(),
            edges: Vec::new(),
            layout_hints: Vec::new(),
        };
        let layout_nodes = vec![layout_node("api", 96.0, 72.0)];
        let mut node_bounds_extras = rustc_hash::FxHashMap::default();
        node_bounds_extras.insert(
            "api",
            manatee::BoundsExtras {
                left: 5.0,
                right: 6.0,
                top: 7.0,
                bottom: 8.0,
            },
        );

        let plan = build_test_plan(&model, &layout_nodes, &node_bounds_extras);

        let node = plan.graph.nodes[0];
        assert_eq!(node.width, 96.0);
        assert_eq!(node.height, 72.0);
        assert_eq!(node.bounds_extras.left, 5.0);
        assert_eq!(node.bounds_extras.right, 6.0);
        assert_eq!(node.bounds_extras.top, 7.0);
        assert_eq!(node.bounds_extras.bottom, 8.0);
    }

    #[test]
    fn architecture_fcose_input_plan_deduplicates_undirected_layout_edges() {
        let model = super::ArchitectureModelView {
            nodes: vec![
                super::ArchitectureNodeView {
                    id: "api",
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: None,
                },
                super::ArchitectureNodeView {
                    id: "db",
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: None,
                },
            ],
            groups: Vec::new(),
            edges: vec![
                super::ArchitectureEdgeView {
                    lhs_id: "api",
                    rhs_id: "db",
                    lhs_dir: Some('R'),
                    rhs_dir: Some('L'),
                    title: None,
                },
                super::ArchitectureEdgeView {
                    lhs_id: "db",
                    rhs_id: "api",
                    lhs_dir: Some('L'),
                    rhs_dir: Some('R'),
                    title: None,
                },
            ],
            layout_hints: Vec::new(),
        };
        let layout_nodes = vec![
            layout_node("api", 80.0, 80.0),
            layout_node("db", 80.0, 80.0),
        ];
        let node_bounds_extras = rustc_hash::FxHashMap::default();

        let plan = build_test_plan(&model, &layout_nodes, &node_bounds_extras);

        assert_eq!(plan.graph.edges.len(), 1);
        assert_eq!(plan.graph.edges[0].source, 0);
        assert_eq!(plan.graph.edges[0].target, 1);
    }

    #[test]
    fn architecture_fcose_result_projection_updates_nodes_and_maps_compound_bounds() {
        let model = super::ArchitectureModelView {
            nodes: vec![
                super::ArchitectureNodeView {
                    id: "api",
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: Some("core"),
                },
                super::ArchitectureNodeView {
                    id: "db",
                    node_type: super::ArchitectureNodeType::Service,
                    title: None,
                    in_group: None,
                },
            ],
            groups: vec![super::ArchitectureGroupView {
                id: "core",
                in_group: None,
            }],
            edges: Vec::new(),
            layout_hints: Vec::new(),
        };
        let mut layout_nodes = vec![
            layout_node("api", 80.0, 80.0),
            layout_node("db", 80.0, 80.0),
        ];
        let node_bounds_extras = rustc_hash::FxHashMap::default();
        let plan = build_test_plan(&model, &layout_nodes, &node_bounds_extras);

        let result = manatee::algo::fcose::IndexedLayoutResult {
            node_positions: vec![
                manatee::Point { x: 10.0, y: 20.0 },
                manatee::Point { x: 30.0, y: 40.0 },
            ],
            compound_positions: vec![manatee::Point { x: 50.0, y: 60.0 }],
            compound_bounds: vec![layout_rect(5.0, 6.0, 100.0, 120.0)],
        };

        let projection = super::project_architecture_fcose_result(&plan, &mut layout_nodes, result);

        assert_eq!((layout_nodes[0].x, layout_nodes[0].y), (10.0, 20.0));
        assert_eq!((layout_nodes[1].x, layout_nodes[1].y), (30.0, 40.0));
        assert_eq!(projection.compound_bounds.len(), 1);
        assert_eq!(projection.compound_bounds[0].id, "core");
        assert_eq!(projection.compound_bounds[0].bounds.min_x, 5.0);
        assert_eq!(projection.compound_bounds[0].bounds.max_y, 126.0);
    }

    #[test]
    fn architecture_fcose_node_bounds_extras_feed_label_bounds() {
        let model = super::ArchitectureModelView {
            nodes: vec![
                super::ArchitectureNodeView {
                    id: "api",
                    node_type: super::ArchitectureNodeType::Service,
                    title: Some("API"),
                    in_group: Some("core"),
                },
                super::ArchitectureNodeView {
                    id: "external",
                    node_type: super::ArchitectureNodeType::Service,
                    title: Some("API"),
                    in_group: None,
                },
            ],
            groups: vec![super::ArchitectureGroupView {
                id: "core",
                in_group: None,
            }],
            edges: Vec::new(),
            layout_hints: Vec::new(),
        };
        let measurer = crate::text::DeterministicTextMeasurer::default();

        let node_bounds_extras = super::architecture_fcose_node_bounds_extras(
            super::ArchitectureFcoseNodeBoundsExtrasInput {
                model: &model,
                text_measurer: &measurer,
                icon_size: 80.0,
                font_size_px: 16.0,
            },
        );
        let grouped = node_bounds_extras.get("api").expect("api node extras");
        let top_level = node_bounds_extras
            .get("external")
            .expect("external node extras");

        assert_eq!(grouped.top, 1.0);
        assert_eq!(grouped.bottom, 18.0);
        assert_eq!(grouped.left, 1.0);
        assert_eq!(grouped.right, 1.0);
        assert_eq!(top_level.top, 1.0);
        assert_eq!(top_level.bottom, 19.0);
        assert_eq!(top_level.left, 1.0);
        assert_eq!(top_level.right, 1.0);
    }

    #[test]
    fn architecture_fcose_edge_label_style_keeps_cytoscape_defaults() {
        let node_style = super::architecture_cytoscape_text_style(18.0);
        let edge_style = super::architecture_cytoscape_edge_text_style();

        assert_eq!(node_style.font_size, 18.0);
        assert_eq!(
            node_style.font_family.as_deref(),
            Some(super::CYTOSCAPE_DEFAULT_FONT_FAMILY)
        );
        assert_eq!(edge_style.font_size, 16.0);
        assert_eq!(
            edge_style.font_family.as_deref(),
            Some(super::CYTOSCAPE_DEFAULT_FONT_FAMILY)
        );
    }

    #[test]
    fn architecture_relative_constraints_preserve_mermaid_duplicate_bfs_pops() {
        let mut spatial_map = indexmap::IndexMap::new();
        spatial_map.insert("ingress", (0, 0));
        spatial_map.insert("fork", (1, 0));
        spatial_map.insert("auth", (2, 0));
        spatial_map.insert("api", (1, -1));
        spatial_map.insert("join", (2, -1));
        spatial_map.insert("db", (3, -1));
        spatial_map.insert("cache", (2, -2));

        let mut node_index_by_id = rustc_hash::FxHashMap::default();
        for (idx, id) in ["ingress", "auth", "api", "db", "cache", "fork", "join"]
            .into_iter()
            .enumerate()
        {
            node_index_by_id.insert(id, idx);
        }

        let spatial_maps = [spatial_map];
        let declared_pairs = rustc_hash::FxHashSet::default();
        let plan = super::checked_architecture_relative_constraint_plan(
            &spatial_maps,
            &node_index_by_id,
            &declared_pairs,
        )
        .expect("relative constraint plan");
        let constraints = super::materialize_architecture_relative_placement_constraints(
            &plan,
            &node_index_by_id,
            120.0,
            &declared_pairs,
        )
        .expect("relative constraints materialize");

        assert_eq!(plan.constraint_count, 9);
        assert_eq!(constraints.len(), plan.constraint_count);

        assert_eq!(
            constraints
                .iter()
                .filter(|c| c.left == Some(6) && c.right == Some(3))
                .count(),
            2,
            "Mermaid processes the duplicate queued join position before db is visited",
        );
        assert_eq!(
            constraints
                .iter()
                .filter(|c| c.top == Some(6) && c.bottom == Some(4))
                .count(),
            2,
            "Mermaid processes the duplicate queued join position before cache is visited",
        );
    }

    #[test]
    fn architecture_relative_plan_counts_grid_path_multiplicity_without_expansion() {
        let ids = (0..12 * 12)
            .map(|index| format!("node-{index}"))
            .collect::<Vec<_>>();
        let mut spatial_map = indexmap::IndexMap::new();
        let mut node_index_by_id = rustc_hash::FxHashMap::default();
        for y in 0..12 {
            for x in 0..12 {
                let index = y * 12 + x;
                let id = ids[index].as_str();
                spatial_map.insert(id, (x as i32, y as i32));
                node_index_by_id.insert(id, index);
            }
        }

        let plan = super::checked_architecture_relative_constraint_plan(
            &[spatial_map],
            &node_index_by_id,
            &rustc_hash::FxHashSet::default(),
        )
        .expect("12x12 grid count should fit usize");

        assert_eq!(plan.queue_entry_count, 2_704_155);
        assert_eq!(plan.constraint_count, 2_704_154);
        assert_eq!(plan.spatial.len(), 1);
        assert_eq!(plan.spatial[0].queue_entry_count, 2_704_155);
        assert_eq!(plan.spatial[0].constraint_count, 2_704_154);
    }

    #[test]
    fn architecture_relative_plan_fails_closed_on_path_multiplicity_overflow() {
        let ids = (0..35 * 35)
            .map(|index| format!("node-{index}"))
            .collect::<Vec<_>>();
        let mut spatial_map = indexmap::IndexMap::new();
        let mut node_index_by_id = rustc_hash::FxHashMap::default();
        for y in 0..35 {
            for x in 0..35 {
                let index = y * 35 + x;
                let id = ids[index].as_str();
                spatial_map.insert(id, (x as i32, y as i32));
                node_index_by_id.insert(id, index);
            }
        }

        assert!(
            super::checked_architecture_relative_constraint_plan(
                &[spatial_map],
                &node_index_by_id,
                &rustc_hash::FxHashSet::default(),
            )
            .is_none()
        );
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn architecture_relative_plan_rejects_unrepresentable_materialization_capacity() {
        let width = 34usize;
        let height = 33usize;
        let ids = (0..width * height)
            .map(|index| format!("node-{index}"))
            .collect::<Vec<_>>();
        let mut spatial_map = indexmap::IndexMap::new();
        let mut node_index_by_id = rustc_hash::FxHashMap::default();
        for y in 0..height {
            for x in 0..width {
                let index = y * width + x;
                let id = ids[index].as_str();
                spatial_map.insert(id, (x as i32, y as i32));
                node_index_by_id.insert(id, index);
            }
        }

        let plan = super::checked_architecture_relative_constraint_plan(
            &[spatial_map],
            &node_index_by_id,
            &rustc_hash::FxHashSet::default(),
        )
        .expect("path multiplicity should still fit usize");

        assert!(plan.checked_materialization_work_units().is_none());
    }

    #[test]
    fn architecture_relative_plan_matches_materialization_for_all_small_grid_shapes() {
        let positions: [(&str, (i32, i32)); 9] = [
            ("p0", (0, 0)),
            ("p1", (-1, 0)),
            ("p2", (1, 0)),
            ("p3", (0, 1)),
            ("p4", (0, -1)),
            ("p5", (-1, 1)),
            ("p6", (1, 1)),
            ("p7", (-1, -1)),
            ("p8", (1, -1)),
        ];

        for mask in 0usize..(1usize << (positions.len() - 1)) {
            let mut spatial_map = indexmap::IndexMap::new();
            spatial_map.insert(positions[0].0, positions[0].1);
            for (bit, &(id, position)) in positions[1..].iter().enumerate() {
                if mask & (1usize << bit) != 0 {
                    spatial_map.insert(id, position);
                }
            }
            let node_index_by_id = spatial_map
                .keys()
                .enumerate()
                .map(|(index, &id)| (id, index))
                .collect::<rustc_hash::FxHashMap<_, _>>();

            let mut declared_variants = vec![rustc_hash::FxHashSet::default()];
            if let Some(&right) = node_index_by_id.get("p1") {
                declared_variants.push(rustc_hash::FxHashSet::from_iter([(0, right)]));
            }
            let mut all_adjacent = rustc_hash::FxHashSet::default();
            for (&lhs_id, &(lhs_x, lhs_y)) in &spatial_map {
                let lhs = node_index_by_id[lhs_id];
                for (&rhs_id, &(rhs_x, rhs_y)) in &spatial_map {
                    if (lhs_x - rhs_x).abs() + (lhs_y - rhs_y).abs() == 1 {
                        all_adjacent.insert((lhs, node_index_by_id[rhs_id]));
                    }
                }
            }
            declared_variants.push(all_adjacent);

            for declared_pairs in declared_variants {
                let spatial_maps = [spatial_map.clone()];
                let plan = super::checked_architecture_relative_constraint_plan(
                    &spatial_maps,
                    &node_index_by_id,
                    &declared_pairs,
                )
                .expect("3x3 relative plan should fit");
                let materialized = super::materialize_architecture_relative_placement_constraints(
                    &plan,
                    &node_index_by_id,
                    120.0,
                    &declared_pairs,
                )
                .expect("3x3 relative constraints materialize");
                assert_eq!(
                    plan.constraint_count,
                    materialized.len(),
                    "constraint count mismatch for mask {mask:#011b}",
                );
            }
        }
    }
}
