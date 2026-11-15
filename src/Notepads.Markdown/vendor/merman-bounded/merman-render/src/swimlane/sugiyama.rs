use super::config::SwimlaneConfig;
use super::deterministic_identifier_cmp;
use super::working::{WorkingEdge, WorkingLayout, WorkingNode, WorkingNodeKind};
use crate::model::SwimlaneDirection;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
struct CycleResult {
    edges: Vec<WorkingEdge>,
    reversed_logical_ids: HashSet<String>,
}

#[derive(Debug)]
struct Layering {
    layers: Vec<Vec<String>>,
    rank_of: HashMap<String, usize>,
    dummy: HashSet<String>,
}

#[derive(Debug, Clone)]
struct ProperEdge {
    chain_id: String,
    from: String,
    to: String,
    ref_from: String,
    ref_to: String,
}

#[derive(Debug, Clone)]
struct WeightedLaneEdge {
    left: String,
    right: String,
    weight: usize,
}

#[derive(Debug, Clone)]
struct LaneOrderCandidate {
    order: Vec<String>,
    cost: usize,
    source_distance: usize,
}

fn weighted_lane_edges(layout: &WorkingLayout) -> Vec<WeightedLaneEdge> {
    let source_index: HashMap<&str, usize> = layout
        .top_lane_order
        .iter()
        .enumerate()
        .map(|(index, lane)| (lane.as_str(), index))
        .collect();
    let mut weights: HashMap<(String, String), usize> = HashMap::new();
    for edge in &layout.original_edges {
        let (Some(source_lane), Some(target_lane)) =
            (layout.top_lane_of(&edge.from), layout.top_lane_of(&edge.to))
        else {
            continue;
        };
        if source_lane == target_lane {
            continue;
        }
        let (Some(&source_position), Some(&target_position)) =
            (source_index.get(source_lane), source_index.get(target_lane))
        else {
            continue;
        };
        let pair = if source_position <= target_position {
            (source_lane.to_string(), target_lane.to_string())
        } else {
            (target_lane.to_string(), source_lane.to_string())
        };
        *weights.entry(pair).or_default() += 1;
    }
    let mut output: Vec<WeightedLaneEdge> = weights
        .into_iter()
        .map(|((left, right), weight)| WeightedLaneEdge {
            left,
            right,
            weight,
        })
        .collect();
    output.sort_by(|a, b| {
        deterministic_identifier_cmp(&a.left, &b.left)
            .then_with(|| deterministic_identifier_cmp(&a.right, &b.right))
    });
    output
}

fn lane_arrangement_cost(order: &[String], weights: &[WeightedLaneEdge]) -> usize {
    let positions: HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(index, lane)| (lane.as_str(), index))
        .collect();
    weights
        .iter()
        .filter_map(|edge| {
            Some(
                edge.weight
                    * positions
                        .get(edge.left.as_str())?
                        .abs_diff(*positions.get(edge.right.as_str())?),
            )
        })
        .sum()
}

fn lane_source_distance(order: &[String], source_index: &HashMap<&str, usize>) -> usize {
    order
        .iter()
        .enumerate()
        .map(|(index, lane)| {
            index.abs_diff(source_index.get(lane.as_str()).copied().unwrap_or(index))
        })
        .sum()
}

fn greedy_lane_switch(
    start: &[String],
    weights: &[WeightedLaneEdge],
    source_index: &HashMap<&str, usize>,
) -> LaneOrderCandidate {
    let mut order = start.to_vec();
    let mut cost = lane_arrangement_cost(&order, weights);
    for _ in 0..order.len().max(1) {
        let mut changed = false;
        for index in 0..order.len().saturating_sub(1) {
            order.swap(index, index + 1);
            let next = lane_arrangement_cost(&order, weights);
            if next < cost {
                cost = next;
                changed = true;
            } else {
                order.swap(index, index + 1);
            }
        }
        if !changed {
            break;
        }
    }
    LaneOrderCandidate {
        source_distance: lane_source_distance(&order, source_index),
        order,
        cost,
    }
}

fn fnv1a_utf16(input: &str) -> u32 {
    let mut hash = 2_166_136_261_u32;
    for code_unit in input.encode_utf16() {
        hash ^= u32::from(code_unit);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

fn mulberry32(state: &mut u32) -> f64 {
    *state = state.wrapping_add(0x6d2b_79f5);
    let mut value = *state;
    value = (value ^ (value >> 15)).wrapping_mul(value | 1);
    value ^= value.wrapping_add((value ^ (value >> 7)).wrapping_mul(value | 61));
    f64::from(value ^ (value >> 14)) / 4_294_967_296.0
}

fn deterministic_shuffle(order: &[String], seed: u32) -> Vec<String> {
    let mut shuffled = order.to_vec();
    let mut state = seed;
    for index in (1..shuffled.len()).rev() {
        let swap_index = (mulberry32(&mut state) * (index + 1) as f64).floor() as usize;
        shuffled.swap(index, swap_index);
    }
    shuffled
}

fn optimized_lane_order(layout: &WorkingLayout) -> Vec<String> {
    let source_order = layout.top_lane_order.clone();
    if source_order.len() < 2 {
        return source_order;
    }
    let weights = weighted_lane_edges(layout);
    if weights.is_empty() {
        return source_order;
    }
    let source_index: HashMap<&str, usize> = source_order
        .iter()
        .enumerate()
        .map(|(index, lane)| (lane.as_str(), index))
        .collect();
    let mut best = greedy_lane_switch(&source_order, &weights, &source_index);
    let weight_signature = weights
        .iter()
        .map(|edge| format!("{}:{}:{}", edge.left, edge.right, edge.weight))
        .collect::<Vec<_>>()
        .join("|");
    for restart in 0..8 {
        let seed = fnv1a_utf16(&format!(
            "{}#{}#{}",
            source_order.join("|"),
            weight_signature,
            restart
        ));
        let start = deterministic_shuffle(&source_order, seed);
        let candidate = greedy_lane_switch(&start, &weights, &source_index);
        if candidate.cost < best.cost
            || (candidate.cost == best.cost && candidate.source_distance < best.source_distance)
        {
            best = candidate;
        }
    }
    best.order
}

fn normalized_edges(layout: &WorkingLayout) -> Vec<WorkingEdge> {
    let mut seen = HashSet::new();
    layout
        .graph_edges
        .iter()
        .filter(|edge| layout.nodes.contains_key(&edge.from) && layout.nodes.contains_key(&edge.to))
        .filter(|edge| seen.insert(edge.layout_key()))
        .cloned()
        .collect()
}

fn remove_cycles(layout: &WorkingLayout) -> CycleResult {
    let edges = normalized_edges(layout);
    let mut outgoing: HashMap<&str, Vec<usize>> = HashMap::new();
    for id in layout.nodes.keys() {
        outgoing.entry(id).or_default();
    }
    for (index, edge) in edges.iter().enumerate() {
        outgoing.entry(&edge.from).or_default().push(index);
    }
    for indices in outgoing.values_mut() {
        indices.sort_by(|left, right| {
            let a = &edges[*left];
            let b = &edges[*right];
            deterministic_identifier_cmp(&a.to, &b.to)
                .then_with(|| deterministic_identifier_cmp(&a.id, &b.id))
        });
    }

    fn visit(
        node: &str,
        outgoing: &HashMap<&str, Vec<usize>>,
        edges: &[WorkingEdge],
        colors: &mut HashMap<String, u8>,
        reversed: &mut HashSet<usize>,
    ) {
        colors.insert(node.to_string(), 1);
        if let Some(indices) = outgoing.get(node) {
            for &index in indices {
                let edge = &edges[index];
                match colors.get(&edge.to).copied().unwrap_or(0) {
                    0 => visit(&edge.to, outgoing, edges, colors, reversed),
                    1 => {
                        reversed.insert(index);
                    }
                    _ => {}
                }
            }
        }
        colors.insert(node.to_string(), 2);
    }

    let mut node_ids: Vec<&str> = layout.nodes.keys().map(String::as_str).collect();
    node_ids.sort_by(|left, right| deterministic_identifier_cmp(left, right));
    let mut colors = HashMap::new();
    let mut reversed_indices = HashSet::new();
    for id in node_ids {
        if colors.get(id).copied().unwrap_or(0) == 0 {
            visit(id, &outgoing, &edges, &mut colors, &mut reversed_indices);
        }
    }

    let mut reversed_logical_ids = HashSet::new();
    let edges = edges
        .into_iter()
        .enumerate()
        .map(|(index, mut edge)| {
            if reversed_indices.contains(&index) {
                std::mem::swap(&mut edge.from, &mut edge.to);
                edge.reversed_for_layout = true;
                reversed_logical_ids.insert(edge.reference_id.clone());
            }
            edge
        })
        .collect();
    CycleResult {
        edges,
        reversed_logical_ids,
    }
}

fn successor_map(
    node_ids: impl Iterator<Item = String>,
    edges: &[WorkingEdge],
) -> HashMap<String, Vec<String>> {
    let mut successors: HashMap<String, Vec<String>> =
        node_ids.map(|id| (id, Vec::new())).collect();
    for edge in edges {
        successors
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }
    for values in successors.values_mut() {
        values.sort_by(|left, right| deterministic_identifier_cmp(left, right));
    }
    successors
}

fn indegrees(
    node_ids: impl Iterator<Item = String>,
    edges: &[WorkingEdge],
) -> HashMap<String, usize> {
    let mut indegree: HashMap<String, usize> = node_ids.map(|id| (id, 0)).collect();
    for edge in edges {
        *indegree.entry(edge.to.clone()).or_default() += 1;
    }
    indegree
}

fn topo_order(layout: &WorkingLayout, edges: &[WorkingEdge], by_generation: bool) -> Vec<String> {
    let mut indegree = indegrees(layout.nodes.keys().cloned(), edges);
    let successors = successor_map(layout.nodes.keys().cloned(), edges);
    let mut frontier: Vec<String> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    frontier.sort_by(|left, right| deterministic_identifier_cmp(left, right));
    let mut output = Vec::with_capacity(layout.nodes.len());

    while !frontier.is_empty() {
        let current_generation: Vec<String> = if by_generation {
            std::mem::take(&mut frontier)
        } else {
            vec![frontier.remove(0)]
        };
        for id in &current_generation {
            output.push(id.clone());
        }
        let mut next = Vec::new();
        for id in current_generation {
            for successor in successors.get(&id).into_iter().flatten() {
                let degree = indegree.entry(successor.clone()).or_default();
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    if by_generation {
                        next.push(successor.clone());
                    } else {
                        let insertion = frontier
                            .iter()
                            .position(|candidate| {
                                !deterministic_identifier_cmp(candidate, successor).is_lt()
                            })
                            .unwrap_or(frontier.len());
                        frontier.insert(insertion, successor.clone());
                    }
                }
            }
        }
        if by_generation {
            next.sort_by(|left, right| deterministic_identifier_cmp(left, right));
            frontier = next;
        }
    }

    if output.len() == layout.nodes.len() {
        output
    } else {
        let mut fallback: Vec<String> = layout.nodes.keys().cloned().collect();
        fallback.sort_by(|left, right| deterministic_identifier_cmp(left, right));
        fallback
    }
}

fn incoming<'a>(edges: &'a [WorkingEdge], node: &str) -> impl Iterator<Item = &'a WorkingEdge> {
    edges.iter().filter(move |edge| edge.to == node)
}

fn build_layers(
    layout: &WorkingLayout,
    order: &[String],
    rank_of: &HashMap<String, usize>,
) -> Vec<Vec<String>> {
    let max_rank = order
        .iter()
        .filter(|id| {
            layout
                .nodes
                .get(*id)
                .is_some_and(WorkingNode::is_layout_node)
        })
        .map(|id| rank_of.get(id).copied().unwrap_or(0))
        .max()
        .unwrap_or(0);
    let mut layers = vec![Vec::new(); max_rank + 1];
    for id in order {
        if !layout
            .nodes
            .get(id)
            .is_some_and(WorkingNode::is_layout_node)
        {
            continue;
        }
        layers[rank_of.get(id).copied().unwrap_or(0)].push(id.clone());
    }
    layers
}

fn lane_aware_layering(
    layout: &WorkingLayout,
    edges: &[WorkingEdge],
    config: SwimlaneConfig,
) -> Layering {
    let order = topo_order(layout, edges, layout.direction == SwimlaneDirection::Lr);
    let mut rank_of: HashMap<String, usize> = HashMap::new();
    let mut next_free: HashMap<String, usize> = HashMap::new();

    for id in &order {
        let Some(node) = layout.nodes.get(id) else {
            continue;
        };
        if node.is_group() {
            continue;
        }
        let lane = node.top_lane_id.clone().unwrap_or_else(|| node.id.clone());
        let mut base = 0;
        for edge in incoming(edges, id) {
            let predecessor_rank = rank_of.get(&edge.from).copied().unwrap_or(0);
            let predecessor_lane = layout.top_lane_of(&edge.from).unwrap_or(edge.from.as_str());
            let weight = if config.ignore_cross_lane_edges && predecessor_lane != lane {
                0
            } else {
                1
            };
            base = base.max(predecessor_rank + weight);
        }
        let layer = base.max(next_free.get(&lane).copied().unwrap_or(0));
        rank_of.insert(id.clone(), layer);
        next_free.insert(lane, layer + 1);
    }

    let layers = build_layers(layout, &order, &rank_of);
    Layering {
        layers,
        rank_of,
        dummy: HashSet::new(),
    }
}

fn gravity_layering(
    layout: &WorkingLayout,
    edges: &[WorkingEdge],
    config: SwimlaneConfig,
) -> Layering {
    let order = topo_order(layout, edges, false);
    let mut rank_of: HashMap<String, usize> = HashMap::new();
    for id in &order {
        let Some(node) = layout.nodes.get(id) else {
            continue;
        };
        if node.is_group() {
            continue;
        }
        let predecessors: Vec<&WorkingEdge> = incoming(edges, id).collect();
        let rank = if predecessors.is_empty() {
            0
        } else if predecessors.len() == 1 {
            let edge = predecessors[0];
            let source_lane = layout.top_lane_of(&edge.from);
            let target_lane = layout.top_lane_of(id);
            if source_lane != target_lane {
                rank_of.get(&edge.from).copied().unwrap_or(0)
            } else {
                rank_of.get(&edge.from).copied().unwrap_or(0) + 1
            }
        } else {
            predecessors
                .iter()
                .map(|edge| rank_of.get(&edge.from).copied().unwrap_or(0) + 1)
                .max()
                .unwrap_or(0)
        };
        rank_of.insert(id.clone(), rank);
    }

    // Mermaid's gravity pass performs eight deterministic forward/backward relaxations.
    let mut predecessors: HashMap<String, Vec<String>> = HashMap::new();
    let mut successors: HashMap<String, Vec<String>> = HashMap::new();
    for id in layout.nodes.keys() {
        predecessors.insert(id.clone(), Vec::new());
        successors.insert(id.clone(), Vec::new());
    }
    for edge in edges {
        predecessors
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
        successors
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }
    for _ in 0..8 {
        let mut changed = false;
        for traversal in [order.clone(), order.iter().rev().cloned().collect()] {
            for id in traversal {
                if layout.nodes.get(&id).is_some_and(WorkingNode::is_group) {
                    continue;
                }
                let preds = predecessors.get(&id).map(Vec::as_slice).unwrap_or(&[]);
                let succs = successors.get(&id).map(Vec::as_slice).unwrap_or(&[]);
                if preds.is_empty() && succs.is_empty() {
                    continue;
                }
                let lower = preds
                    .iter()
                    .map(|pred| rank_of.get(pred).copied().unwrap_or(0) + 1)
                    .max()
                    .unwrap_or(0);
                let upper = succs
                    .iter()
                    .filter_map(|succ| {
                        rank_of
                            .get(succ)
                            .copied()
                            .map(|rank| rank.saturating_sub(1))
                    })
                    .min();
                let pred_average = if preds.is_empty() {
                    rank_of.get(&id).copied().unwrap_or(0) as f64
                } else {
                    preds
                        .iter()
                        .map(|pred| rank_of.get(pred).copied().unwrap_or(0) as f64 + 1.0)
                        .sum::<f64>()
                        / preds.len() as f64
                };
                let succ_average = if succs.is_empty() {
                    rank_of.get(&id).copied().unwrap_or(0) as f64
                } else {
                    succs
                        .iter()
                        .map(|succ| rank_of.get(succ).copied().unwrap_or(0) as f64 - 1.0)
                        .sum::<f64>()
                        / succs.len() as f64
                };
                let desired = ((pred_average + succ_average) / 2.0).round().max(0.0) as usize;
                let next = upper.map_or(desired.max(lower), |upper| {
                    desired.clamp(lower, upper.max(lower))
                });
                if rank_of.insert(id.clone(), next) != Some(next) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    if config.optimize_ranks_by_crossings {
        optimize_ranks_by_crossings(layout, edges, &order, &mut rank_of);
    }

    let layers = build_layers(layout, &order, &rank_of);
    Layering {
        layers,
        rank_of,
        dummy: HashSet::new(),
    }
}

fn rank_crossing_score(
    layout: &WorkingLayout,
    edges: &[WorkingEdge],
    order: &[String],
    rank_of: &HashMap<String, usize>,
) -> usize {
    let layers = build_layers(layout, order, rank_of);
    let mut score = 0;
    for index in 0..layers.len().saturating_sub(1) {
        let upper_index: HashMap<&str, usize> = layers[index]
            .iter()
            .enumerate()
            .map(|(position, id)| (id.as_str(), position))
            .collect();
        let lower_index: HashMap<&str, usize> = layers[index + 1]
            .iter()
            .enumerate()
            .map(|(position, id)| (id.as_str(), position))
            .collect();
        let mut pairs: Vec<(usize, usize)> = edges
            .iter()
            .filter_map(|edge| {
                Some((
                    *upper_index.get(edge.from.as_str())?,
                    *lower_index.get(edge.to.as_str())?,
                ))
            })
            .collect();
        pairs.sort_unstable();
        for left in 0..pairs.len() {
            for right in (left + 1)..pairs.len() {
                score += usize::from(pairs[left].1 > pairs[right].1);
            }
        }
    }
    score
}

fn optimize_ranks_by_crossings(
    layout: &WorkingLayout,
    edges: &[WorkingEdge],
    order: &[String],
    rank_of: &mut HashMap<String, usize>,
) {
    let mut best = rank_crossing_score(layout, edges, order, rank_of);
    for _ in 0..4 {
        let mut changed = false;
        let mut nodes: Vec<String> = layout
            .nodes
            .values()
            .filter(|node| node.is_layout_node())
            .map(|node| node.id.clone())
            .collect();
        nodes.sort_by(|left, right| {
            rank_of
                .get(right)
                .copied()
                .unwrap_or(0)
                .cmp(&rank_of.get(left).copied().unwrap_or(0))
                .then_with(|| deterministic_identifier_cmp(left, right))
        });
        for id in nodes {
            let current = rank_of.get(&id).copied().unwrap_or(0);
            if current == 0 {
                continue;
            }
            let lower = incoming(edges, &id)
                .map(|edge| rank_of.get(&edge.from).copied().unwrap_or(0) + 1)
                .max()
                .unwrap_or(0);
            if lower >= current {
                continue;
            }
            rank_of.insert(id.clone(), lower);
            let candidate = rank_crossing_score(layout, edges, order, rank_of);
            if candidate < best {
                best = candidate;
                changed = true;
            } else {
                rank_of.insert(id, current);
            }
        }
        if !changed {
            break;
        }
    }
}

fn make_proper_layering(
    layout: &mut WorkingLayout,
    mut layering: Layering,
    edges: &[WorkingEdge],
) -> (Layering, Vec<ProperEdge>) {
    let mut sorted = edges.to_vec();
    sorted.sort_by(|a, b| {
        deterministic_identifier_cmp(&a.id, &b.id)
            .then_with(|| deterministic_identifier_cmp(&a.from, &b.from))
            .then_with(|| deterministic_identifier_cmp(&a.to, &b.to))
    });
    let mut proper_edges = Vec::new();
    let mut dummy_sequence = 0;

    for edge in sorted {
        let source_rank = layering.rank_of.get(&edge.from).copied().unwrap_or(0);
        let target_rank = layering.rank_of.get(&edge.to).copied().unwrap_or(0);
        if target_rank <= source_rank + 1 {
            proper_edges.push(ProperEdge {
                chain_id: edge.id.clone(),
                from: edge.from.clone(),
                to: edge.to.clone(),
                ref_from: edge.from,
                ref_to: edge.to,
            });
            continue;
        }

        let mut previous = edge.from.clone();
        let ref_from = edge.from.clone();
        let ref_to = edge.to.clone();
        for rank in (source_rank + 1)..target_rank {
            let id = format!("placeholder-{dummy_sequence}");
            dummy_sequence += 1;
            layout.nodes.insert(
                id.clone(),
                WorkingNode {
                    id: id.clone(),
                    label: String::new(),
                    label_type: "text".to_string(),
                    shape: "rect".to_string(),
                    kind: WorkingNodeKind::Dummy,
                    parent_id: None,
                    top_lane_id: None,
                    requested_dir: None,
                    padding: 0.0,
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                    label_width: 0.0,
                    label_height: 0.0,
                    layer: rank,
                    order: 0,
                    content_top: None,
                    title_rect: None,
                },
            );
            while layering.layers.len() <= rank {
                layering.layers.push(Vec::new());
            }
            layering.layers[rank].push(id.clone());
            layering.rank_of.insert(id.clone(), rank);
            layering.dummy.insert(id.clone());
            proper_edges.push(ProperEdge {
                chain_id: edge.id.clone(),
                from: previous,
                to: id.clone(),
                ref_from: ref_from.clone(),
                ref_to: ref_to.clone(),
            });
            previous = id;
        }
        proper_edges.push(ProperEdge {
            chain_id: edge.id,
            from: previous,
            to: edge.to,
            ref_from,
            ref_to,
        });
    }

    (layering, proper_edges)
}

fn median(mut values: Vec<usize>) -> f64 {
    if values.is_empty() {
        return f64::INFINITY;
    }
    values.sort_unstable();
    if values.len() % 2 == 1 {
        values[values.len() / 2] as f64
    } else {
        (values[values.len() / 2 - 1] + values[values.len() / 2]) as f64 / 2.0
    }
}

fn barycenter(values: &[usize]) -> f64 {
    if values.is_empty() {
        f64::INFINITY
    } else {
        values.iter().sum::<usize>() as f64 / values.len() as f64
    }
}

fn crossings_between(upper: &[String], lower: &[String], edges: &[ProperEdge]) -> usize {
    let upper_index: HashMap<&str, usize> = upper
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let lower_index: HashMap<&str, usize> = lower
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let mut pairs: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|edge| {
            Some((
                *upper_index.get(edge.from.as_str())?,
                *lower_index.get(edge.to.as_str())?,
            ))
        })
        .collect();
    pairs.sort_unstable();
    let mut crossings = 0;
    for left in 0..pairs.len() {
        for right in (left + 1)..pairs.len() {
            if pairs[left].1 > pairs[right].1 {
                crossings += 1;
            }
        }
    }
    crossings
}

fn reorder_layer(
    layout: &WorkingLayout,
    fixed: &[String],
    target: &[String],
    edges: &[ProperEdge],
    downward: bool,
) -> Vec<String> {
    let fixed_index: HashMap<&str, usize> = fixed
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let current_index: HashMap<&str, usize> = target
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let mut neighbors: HashMap<&str, Vec<usize>> =
        target.iter().map(|id| (id.as_str(), Vec::new())).collect();
    for edge in edges {
        let (fixed_id, target_id) = if downward {
            (edge.from.as_str(), edge.to.as_str())
        } else {
            (edge.to.as_str(), edge.from.as_str())
        };
        if let (Some(&position), Some(bucket)) =
            (fixed_index.get(fixed_id), neighbors.get_mut(target_id))
        {
            bucket.push(position);
        }
    }

    let mut by_lane: HashMap<Option<&str>, Vec<String>> = HashMap::new();
    for id in target {
        by_lane
            .entry(layout.top_lane_of(id))
            .or_default()
            .push(id.clone());
    }
    let sort_bucket = |bucket: &mut Vec<String>| {
        bucket.sort_by(|a, b| {
            let score_a = median(neighbors.get(a.as_str()).cloned().unwrap_or_default());
            let score_b = median(neighbors.get(b.as_str()).cloned().unwrap_or_default());
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    current_index
                        .get(a.as_str())
                        .cmp(&current_index.get(b.as_str()))
                })
                .then_with(|| deterministic_identifier_cmp(a, b))
        });
    };

    let mut output = Vec::with_capacity(target.len());
    for lane in &layout.top_lane_order {
        if let Some(mut bucket) = by_lane.remove(&Some(lane.as_str())) {
            sort_bucket(&mut bucket);
            output.extend(bucket);
        }
    }
    if let Some(mut bucket) = by_lane.remove(&None) {
        sort_bucket(&mut bucket);
        for id in bucket {
            let score = barycenter(neighbors.get(id.as_str()).map(Vec::as_slice).unwrap_or(&[]));
            let insertion = if score.is_finite() {
                output
                    .iter()
                    .position(|placed| {
                        score
                            < barycenter(
                                neighbors
                                    .get(placed.as_str())
                                    .map(Vec::as_slice)
                                    .unwrap_or(&[]),
                            )
                    })
                    .unwrap_or(output.len())
            } else {
                output.len()
            };
            output.insert(insertion, id);
        }
    }
    let mut remaining: Vec<Vec<String>> = by_lane.into_values().collect();
    remaining.sort_by(|a, b| {
        a.first()
            .zip(b.first())
            .map_or(Ordering::Equal, |(left, right)| {
                deterministic_identifier_cmp(left, right)
            })
    });
    for mut bucket in remaining {
        sort_bucket(&mut bucket);
        output.extend(bucket);
    }
    output
}

fn transpose_improve(
    layout: &WorkingLayout,
    fixed: &[String],
    current: &mut [String],
    other: Option<&[String]>,
    edges: &[ProperEdge],
    fixed_is_upper: bool,
) {
    let score = |order: &[String]| {
        let fixed_score = if fixed_is_upper {
            crossings_between(fixed, order, edges)
        } else {
            crossings_between(order, fixed, edges)
        };
        let other_score = other.map_or(0, |other| {
            if fixed_is_upper {
                crossings_between(order, other, edges)
            } else {
                crossings_between(other, order, edges)
            }
        });
        fixed_score + other_score
    };
    let mut best = score(current);
    let mut improved = true;
    while improved {
        improved = false;
        for index in 0..current.len().saturating_sub(1) {
            if layout.top_lane_of(&current[index]) != layout.top_lane_of(&current[index + 1]) {
                continue;
            }
            current.swap(index, index + 1);
            let candidate = score(current);
            if candidate < best {
                best = candidate;
                improved = true;
            } else {
                current.swap(index, index + 1);
            }
        }
    }
}

fn order_layers(
    layout: &WorkingLayout,
    mut layers: Vec<Vec<String>>,
    edges: &[ProperEdge],
) -> Vec<Vec<String>> {
    for _ in 0..3 {
        for index in 1..layers.len() {
            let reordered = reorder_layer(layout, &layers[index - 1], &layers[index], edges, true);
            layers[index] = reordered;
            let fixed = layers[index - 1].clone();
            let other = layers.get(index + 1).cloned();
            transpose_improve(
                layout,
                &fixed,
                &mut layers[index],
                other.as_deref(),
                edges,
                true,
            );
        }
        for index in (0..layers.len().saturating_sub(1)).rev() {
            let reordered = reorder_layer(layout, &layers[index + 1], &layers[index], edges, false);
            layers[index] = reordered;
            let fixed = layers[index + 1].clone();
            let other = index
                .checked_sub(1)
                .and_then(|other| layers.get(other))
                .cloned();
            transpose_improve(
                layout,
                &fixed,
                &mut layers[index],
                other.as_deref(),
                edges,
                false,
            );
        }
    }
    layers
}

fn assign_coordinates(
    layout: &mut WorkingLayout,
    layers: &[Vec<String>],
    edges: &[ProperEdge],
    config: SwimlaneConfig,
) {
    let is_horizontal = matches!(
        layout.direction,
        SwimlaneDirection::Lr | SwimlaneDirection::Rl
    );
    let layer_heights: Vec<f64> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .filter_map(|id| layout.nodes.get(id))
                .map(|node| node.height)
                .fold(0.0, f64::max)
        })
        .collect();
    let mut extra_gaps = vec![0.0; layers.len()];
    if is_horizontal {
        for index in 0..layers.len().saturating_sub(1) {
            let max_width = |layer: &[String]| {
                layer
                    .iter()
                    .filter_map(|id| layout.nodes.get(id))
                    .map(|node| node.width)
                    .fold(0.0, f64::max)
            };
            let normal = layer_heights[index] / 2.0 + layer_heights[index + 1] / 2.0;
            let required = (max_width(&layers[index]) + max_width(&layers[index + 1])) / 2.0;
            extra_gaps[index] = (required - normal - config.layer_gap).max(0.0);
        }
    }

    let mut used_lanes = HashSet::new();
    let mut has_null_lane = false;
    for id in layers.iter().flatten() {
        if let Some(lane) = layout.top_lane_of(id) {
            used_lanes.insert(lane.to_string());
        } else {
            has_null_lane = true;
        }
    }
    let mut lane_columns: Vec<Option<String>> = Vec::new();
    if has_null_lane {
        lane_columns.push(None);
    }
    lane_columns.extend(
        layout
            .top_lane_order
            .iter()
            .filter(|lane| used_lanes.contains(*lane))
            .cloned()
            .map(Some),
    );
    let mut lane_width: HashMap<Option<String>, f64> = HashMap::new();
    for layer in layers {
        let mut by_lane: HashMap<Option<String>, Vec<&WorkingNode>> = HashMap::new();
        for id in layer {
            let Some(node) = layout.nodes.get(id) else {
                continue;
            };
            by_lane
                .entry(node.top_lane_id.clone())
                .or_default()
                .push(node);
        }
        for (lane, nodes) in by_lane {
            let width = nodes.iter().map(|node| node.width).sum::<f64>()
                + config.node_gap * nodes.len().saturating_sub(1) as f64;
            lane_width
                .entry(lane)
                .and_modify(|current| *current = current.max(width))
                .or_insert(width);
        }
    }
    let widths: Vec<f64> = lane_columns
        .iter()
        .map(|lane| lane_width.get(lane).copied().unwrap_or(0.0))
        .collect();
    let lane_gap = config.node_gap * 2.0;
    let total_width =
        widths.iter().sum::<f64>() + lane_gap * lane_columns.len().saturating_sub(1) as f64;
    let mut cursor = -total_width / 2.0;
    let mut lane_centers = HashMap::new();
    for (index, lane) in lane_columns.iter().enumerate() {
        lane_centers.insert(lane.clone(), cursor + widths[index] / 2.0);
        cursor += widths[index];
        if index + 1 < lane_columns.len() {
            cursor += lane_gap;
        }
    }

    let mut y_offset = 0.0;
    for (layer_index, layer) in layers.iter().enumerate() {
        let layer_height = layer_heights[layer_index];
        let mut by_lane: HashMap<Option<String>, Vec<String>> = HashMap::new();
        for id in layer {
            let lane = layout
                .nodes
                .get(id)
                .and_then(|node| node.top_lane_id.clone());
            by_lane.entry(lane).or_default().push(id.clone());
        }
        for lane in &lane_columns {
            let ids = by_lane.get(lane).cloned().unwrap_or_default();
            let Some(center) = lane_centers.get(lane).copied() else {
                continue;
            };
            let width = ids
                .iter()
                .filter_map(|id| layout.nodes.get(id))
                .map(|node| node.width)
                .sum::<f64>()
                + config.node_gap * ids.len().saturating_sub(1) as f64;
            let mut x = center - width / 2.0;
            for (order, id) in ids.iter().enumerate() {
                if let Some(node) = layout.nodes.get_mut(id) {
                    node.x = x + node.width / 2.0;
                    node.y = y_offset + layer_height / 2.0;
                    node.layer = layer_index;
                    node.order = order;
                    x += node.width + config.node_gap;
                }
            }
        }
        y_offset += layer_height + config.layer_gap + extra_gaps[layer_index];
    }

    let mut chains: HashMap<&str, Vec<&ProperEdge>> = HashMap::new();
    for edge in edges {
        chains.entry(&edge.chain_id).or_default().push(edge);
    }
    for chain in chains.values() {
        let Some(first) = chain.first() else {
            continue;
        };
        let Some(source) = layout.nodes.get(&first.ref_from) else {
            continue;
        };
        let Some(target) = layout.nodes.get(&first.ref_to) else {
            continue;
        };
        let midpoint = ((source.x + target.x) / 2.0).round();
        let mut involved = HashSet::new();
        for edge in chain {
            involved.insert(edge.from.clone());
            involved.insert(edge.to.clone());
        }
        for id in involved {
            if let Some(node) = layout.nodes.get_mut(&id)
                && node.kind == WorkingNodeKind::Dummy
            {
                node.x = midpoint;
            }
        }
    }
}

pub(super) fn run(layout: &mut WorkingLayout, config: SwimlaneConfig) -> HashSet<String> {
    if config.automatic_lane_ordering {
        layout.top_lane_order = optimized_lane_order(layout);
    }
    let cycle_result = remove_cycles(layout);
    let layering = if config.ignore_cross_lane_edges {
        lane_aware_layering(layout, &cycle_result.edges, config)
    } else {
        gravity_layering(layout, &cycle_result.edges, config)
    };
    let (proper, proper_edges) = make_proper_layering(layout, layering, &cycle_result.edges);
    let ordered = order_layers(layout, proper.layers, &proper_edges);
    assign_coordinates(layout, &ordered, &proper_edges, config);
    cycle_result.reversed_logical_ids
}
