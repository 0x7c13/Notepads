use super::super::config::EPSILON;
use super::super::work_budget::{LayoutWorkBudget, sorting_work_units, unordered_pair_count};
use super::super::working::WorkingEdge;
use crate::Result;
use crate::model::LayoutPoint;
use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet};

const TRACK_SPACING: f64 = 10.0;
const MAX_CONFLICT_REDUCTION_PASSES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct SegmentRef {
    edge_index: usize,
    segment_index: usize,
    from: f64,
    to: f64,
}

#[derive(Debug, Default)]
struct Track {
    segments: Vec<SegmentRef>,
}

#[derive(Debug)]
struct Pipe {
    orientation: Orientation,
    coord: f64,
    tracks: Vec<Track>,
}

#[derive(Debug, Clone, Copy)]
struct RoutedSegment {
    edge_index: usize,
    segment_index: usize,
    orientation: Orientation,
    pipe_index: usize,
    track_index: usize,
    from: f64,
    to: f64,
}

#[derive(Debug, Clone, Copy)]
struct DestinationInfo {
    destination: f64,
    deviation: f64,
    delta: f64,
}

#[derive(Debug, Clone, Copy)]
struct RoutedLine {
    orientation: Orientation,
    coord: f64,
    from: f64,
    to: f64,
}

fn segments_overlap(left: SegmentRef, right: SegmentRef) -> bool {
    left.from < right.to && right.from < left.to
}

fn segment_ref(segment: RoutedSegment) -> SegmentRef {
    SegmentRef {
        edge_index: segment.edge_index,
        segment_index: segment.segment_index,
        from: segment.from,
        to: segment.to,
    }
}

fn point_on_line(line: RoutedLine, along: f64) -> LayoutPoint {
    match line.orientation {
        Orientation::Vertical => LayoutPoint {
            x: line.coord,
            y: along,
        },
        Orientation::Horizontal => LayoutPoint {
            x: along,
            y: line.coord,
        },
    }
}

fn shared_line_endpoint_coord(line: RoutedLine, next: RoutedLine) -> f64 {
    if (line.to - next.from).abs() < EPSILON || (line.to - next.to).abs() < EPSILON {
        line.to
    } else {
        line.from
    }
}

struct TrackAssignment<'a> {
    edges: &'a [WorkingEdge],
    pipes: Vec<Pipe>,
    segments: Vec<RoutedSegment>,
    by_edge: Vec<Vec<usize>>,
    destination_cache: HashMap<usize, DestinationInfo>,
}

impl<'a> TrackAssignment<'a> {
    fn new(edges: &'a [WorkingEdge], work_budget: &mut LayoutWorkBudget) -> Result<Self> {
        work_budget.charge(edges.len())?;
        Ok(Self {
            edges,
            pipes: Vec::new(),
            segments: Vec::new(),
            by_edge: vec![Vec::new(); edges.len()],
            destination_cache: HashMap::new(),
        })
    }

    fn pipe_for(
        &mut self,
        orientation: Orientation,
        coord: f64,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<usize> {
        work_budget.charge(self.pipes.len())?;
        if let Some(index) = self
            .pipes
            .iter()
            .position(|pipe| pipe.orientation == orientation && (pipe.coord - coord).abs() < 1.0)
        {
            return Ok(index);
        }
        work_budget.charge(2)?;
        self.pipes.push(Pipe {
            orientation,
            coord,
            tracks: vec![Track::default()],
        });
        Ok(self.pipes.len() - 1)
    }

    fn add_edge(&mut self, edge_index: usize, work_budget: &mut LayoutWorkBudget) -> Result<()> {
        let segment_count = self.edges[edge_index].points.len().saturating_sub(1);
        work_budget.charge(segment_count)?;
        for segment_index in 0..segment_count {
            let (dx, dy, vertical_coord, horizontal_coord, vertical_span, horizontal_span) = {
                let points = &self.edges[edge_index].points;
                let start = &points[segment_index];
                let end = &points[segment_index + 1];
                (
                    (start.x - end.x).abs(),
                    (start.y - end.y).abs(),
                    start.x,
                    start.y,
                    (start.y.min(end.y), start.y.max(end.y)),
                    (start.x.min(end.x), start.x.max(end.x)),
                )
            };
            if dx <= EPSILON && dy <= EPSILON {
                continue;
            }
            let orientation = if dx <= EPSILON {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            };
            let coord = match orientation {
                Orientation::Vertical => vertical_coord,
                Orientation::Horizontal => horizontal_coord,
            };
            let (from, to) = match orientation {
                Orientation::Vertical => vertical_span,
                Orientation::Horizontal => horizontal_span,
            };
            let pipe_index = self.pipe_for(orientation, coord, work_budget)?;
            let routed = RoutedSegment {
                edge_index,
                segment_index,
                orientation,
                pipe_index,
                track_index: 0,
                from,
                to,
            };
            let routed_index = self.segments.len();
            work_budget.charge(3)?;
            self.segments.push(routed);
            self.by_edge[edge_index].push(routed_index);
            self.pipes[pipe_index].tracks[0]
                .segments
                .push(segment_ref(routed));
        }
        Ok(())
    }

    fn adjacent_segments(
        &self,
        segment_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<Vec<usize>> {
        let segment = self.segments[segment_index];
        let indices = &self.by_edge[segment.edge_index];
        work_budget.charge(indices.len().saturating_add(2))?;
        let Some(position) = indices.iter().position(|index| *index == segment_index) else {
            return Ok(Vec::new());
        };
        let mut adjacent = Vec::with_capacity(2);
        if position > 0 {
            adjacent.push(indices[position - 1]);
        }
        if position + 1 < indices.len() {
            adjacent.push(indices[position + 1]);
        }
        Ok(adjacent)
    }

    fn segments_cross(&self, left: usize, right: usize) -> bool {
        let left = self.segments[left];
        let right = self.segments[right];
        if left.orientation == right.orientation {
            return false;
        }
        let (horizontal, vertical) = if left.orientation == Orientation::Horizontal {
            (left, right)
        } else {
            (right, left)
        };
        let horizontal_coord = self.pipes[horizontal.pipe_index].coord;
        let vertical_coord = self.pipes[vertical.pipe_index].coord;
        vertical_coord > horizontal.from
            && vertical_coord < horizontal.to
            && horizontal_coord > vertical.from
            && horizontal_coord < vertical.to
    }

    fn segments_conflict(
        &self,
        left_index: usize,
        right_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<bool> {
        let left = self.segments[left_index];
        let right = self.segments[right_index];
        if left.track_index == right.track_index {
            work_budget.charge(1)?;
            return Ok(segments_overlap(segment_ref(left), segment_ref(right)));
        }
        let left_adjacent = self.adjacent_segments(left_index, work_budget)?;
        let right_adjacent = self.adjacent_segments(right_index, work_budget)?;
        work_budget.charge(left_adjacent.len().saturating_mul(right_adjacent.len()))?;
        Ok(left_adjacent.iter().any(|left| {
            right_adjacent
                .iter()
                .any(|right| self.segments_cross(*left, *right))
        }))
    }

    fn remove_from_track(
        &mut self,
        segment_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<()> {
        let segment = self.segments[segment_index];
        work_budget.charge(
            self.pipes[segment.pipe_index].tracks[segment.track_index]
                .segments
                .len(),
        )?;
        self.pipes[segment.pipe_index].tracks[segment.track_index]
            .segments
            .retain(|entry| {
                entry.edge_index != segment.edge_index
                    || entry.segment_index != segment.segment_index
            });
        Ok(())
    }

    fn move_segment(
        &mut self,
        segment_index: usize,
        track_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<()> {
        self.remove_from_track(segment_index, work_budget)?;
        self.segments[segment_index].track_index = track_index;
        let segment = self.segments[segment_index];
        work_budget.charge(1)?;
        self.pipes[segment.pipe_index].tracks[track_index]
            .segments
            .push(segment_ref(segment));
        Ok(())
    }

    fn move_segment_chain(
        &mut self,
        segment_index: usize,
        track_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<()> {
        let segment = self.segments[segment_index];
        work_budget.charge(self.by_edge[segment.edge_index].len().saturating_mul(2))?;
        let chain: Vec<usize> = self.by_edge[segment.edge_index]
            .iter()
            .copied()
            .filter(|index| self.segments[*index].pipe_index == segment.pipe_index)
            .collect();
        for index in chain {
            self.move_segment(index, track_index, work_budget)?;
        }
        Ok(())
    }

    fn create_track(
        &mut self,
        pipe_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<usize> {
        let index = self.pipes[pipe_index].tracks.len();
        work_budget.charge(1)?;
        self.pipes[pipe_index].tracks.push(Track::default());
        Ok(index)
    }

    fn available_track(
        &self,
        segment_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<Option<usize>> {
        let segment = self.segments[segment_index];
        let tracks = &self.pipes[segment.pipe_index].tracks;
        work_budget.charge(tracks.len())?;
        let candidate_segments = tracks.iter().fold(0usize, |total, track| {
            total.saturating_add(track.segments.len())
        });
        work_budget.charge(candidate_segments)?;
        Ok(tracks
            .iter()
            .enumerate()
            .find(|(_, track)| {
                !track.segments.iter().any(|entry| {
                    (entry.edge_index != segment.edge_index
                        || entry.segment_index != segment.segment_index)
                        && segments_overlap(*entry, segment_ref(segment))
                })
            })
            .map(|(index, _)| index))
    }

    fn try_swap_tracks(
        &mut self,
        left_index: usize,
        right_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<bool> {
        let left = self.segments[left_index];
        let right = self.segments[right_index];
        let left_target = right.track_index;
        let right_target = left.track_index;
        work_budget.charge(
            self.pipes[left.pipe_index].tracks[left_target]
                .segments
                .len()
                .saturating_add(
                    self.pipes[right.pipe_index].tracks[right_target]
                        .segments
                        .len(),
                ),
        )?;
        let can_left_move = !self.pipes[left.pipe_index].tracks[left_target]
            .segments
            .iter()
            .any(|entry| {
                (entry.edge_index != right.edge_index || entry.segment_index != right.segment_index)
                    && segments_overlap(*entry, segment_ref(left))
            });
        let can_right_move = !self.pipes[right.pipe_index].tracks[right_target]
            .segments
            .iter()
            .any(|entry| {
                (entry.edge_index != left.edge_index || entry.segment_index != left.segment_index)
                    && segments_overlap(*entry, segment_ref(right))
            });
        if !can_left_move || !can_right_move {
            return Ok(false);
        }
        self.remove_from_track(left_index, work_budget)?;
        self.remove_from_track(right_index, work_budget)?;
        self.segments[left_index].track_index = left_target;
        self.segments[right_index].track_index = right_target;
        let left = self.segments[left_index];
        let right = self.segments[right_index];
        work_budget.charge(2)?;
        self.pipes[left.pipe_index].tracks[left.track_index]
            .segments
            .push(segment_ref(left));
        self.pipes[right.pipe_index].tracks[right.track_index]
            .segments
            .push(segment_ref(right));
        Ok(true)
    }

    fn resolve_conflict(
        &mut self,
        left: usize,
        right: usize,
        move_chain: bool,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<()> {
        if self.try_swap_tracks(left, right, work_budget)? {
            return Ok(());
        }
        let pipe_index = self.segments[left].pipe_index;
        let track = match self.available_track(right, work_budget)? {
            Some(track) => track,
            None => self.create_track(pipe_index, work_budget)?,
        };
        if move_chain {
            self.move_segment_chain(right, track, work_budget)?;
        } else {
            self.move_segment(right, track, work_budget)?;
        }
        Ok(())
    }

    fn resolve_handles(
        &mut self,
        handles: &[usize],
        move_chain: bool,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<usize> {
        work_budget.charge(unordered_pair_count(handles.len()))?;
        let mut conflicts = 0;
        for left in 0..handles.len() {
            for right in left + 1..handles.len() {
                let left_index = handles[left];
                let right_index = handles[right];
                if self.segments[left_index].pipe_index != self.segments[right_index].pipe_index {
                    continue;
                }
                if self.segments_conflict(left_index, right_index, work_budget)? {
                    conflicts += 1;
                    self.resolve_conflict(left_index, right_index, move_chain, work_budget)?;
                }
            }
        }
        Ok(conflicts)
    }

    fn destination_info(
        &mut self,
        edge_index: usize,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<DestinationInfo> {
        work_budget.charge(1)?;
        if let Some(info) = self.destination_cache.get(&edge_index) {
            return Ok(*info);
        }
        let indices = &self.by_edge[edge_index];
        work_budget.charge(indices.len())?;
        let info = if let Some(first_index) = indices.first() {
            let first = self.segments[*first_index];
            let base = self.pipes[first.pipe_index].coord;
            let mut destination = base;
            for segment_index in indices.iter().skip(1) {
                let segment = self.segments[*segment_index];
                if segment.orientation == Orientation::Horizontal {
                    destination = if (segment.from - base).abs() > (segment.to - base).abs() {
                        segment.from
                    } else {
                        segment.to
                    };
                    break;
                }
            }
            DestinationInfo {
                destination,
                deviation: (destination - base).abs(),
                delta: destination - base,
            }
        } else {
            DestinationInfo {
                destination: 0.0,
                deviation: 0.0,
                delta: 0.0,
            }
        };
        self.destination_cache.insert(edge_index, info);
        Ok(info)
    }

    fn fix_source_handles(&mut self, work_budget: &mut LayoutWorkBudget) -> Result<usize> {
        work_budget.charge(self.edges.len().saturating_mul(2))?;
        for index in 0..self.edges.len() {
            if !self.by_edge[index].is_empty() {
                self.destination_info(index, work_budget)?;
            }
        }
        let groups = {
            let mut groups: IndexMap<&str, Vec<usize>> = IndexMap::new();
            for (index, edge) in self.edges.iter().enumerate() {
                if !self.by_edge[index].is_empty() {
                    groups.entry(&edge.from).or_default().push(index);
                }
            }
            work_budget.charge(groups.len())?;
            groups.into_values().collect::<Vec<_>>()
        };
        let mut conflicts = 0;
        for mut group in groups {
            work_budget.charge(sorting_work_units(group.len()))?;
            group.sort_by(|left, right| {
                let left_info = self
                    .destination_cache
                    .get(left)
                    .expect("destination info was precomputed");
                let right_info = self
                    .destination_cache
                    .get(right)
                    .expect("destination info was precomputed");
                if (left_info.deviation - right_info.deviation).abs() > 1.0 {
                    return left_info.deviation.total_cmp(&right_info.deviation);
                }
                if (left_info.destination - right_info.destination).abs() > 1.0 {
                    return left_info.destination.total_cmp(&right_info.destination);
                }
                let distance = |edge_index: usize| {
                    let points = &self.edges[edge_index].points;
                    points
                        .first()
                        .zip(points.last())
                        .map_or(0.0, |(start, end)| {
                            (start.x - end.x).abs() + (start.y - end.y).abs()
                        })
                };
                let left_distance = distance(*left);
                let right_distance = distance(*right);
                if (left_distance - right_distance).abs() > 1.0 {
                    return right_distance.total_cmp(&left_distance);
                }
                let left_len = self.by_edge[*left].len();
                let right_len = self.by_edge[*right].len();
                if left_len != right_len {
                    return left_len.cmp(&right_len);
                }
                if left_len == 1 {
                    let left_segment = self.segments[self.by_edge[*left][0]];
                    let right_segment = self.segments[self.by_edge[*right][0]];
                    let left_span = left_segment.to - left_segment.from;
                    let right_span = right_segment.to - right_segment.from;
                    if (left_span - right_span).abs() > 1.0 {
                        return left_span.total_cmp(&right_span);
                    }
                }
                left.cmp(right)
            });
            work_budget.charge(group.len())?;
            let handles: Vec<usize> = group
                .into_iter()
                .filter_map(|edge| self.by_edge[edge].first().copied())
                .collect();
            conflicts += self.resolve_handles(&handles, true, work_budget)?;
        }
        Ok(conflicts)
    }

    fn fix_target_handles(&mut self, work_budget: &mut LayoutWorkBudget) -> Result<usize> {
        work_budget.charge(self.edges.len().saturating_mul(2))?;
        let groups = {
            let mut groups: IndexMap<&str, Vec<usize>> = IndexMap::new();
            for (index, edge) in self.edges.iter().enumerate() {
                if !self.by_edge[index].is_empty() {
                    groups.entry(&edge.to).or_default().push(index);
                }
            }
            work_budget.charge(groups.len())?;
            groups.into_values().collect::<Vec<_>>()
        };
        let mut conflicts = 0;
        for mut group in groups {
            work_budget.charge(sorting_work_units(group.len()))?;
            group.sort_by(|left, right| {
                let perpendicular_span = |edge_index: usize| {
                    let indices = &self.by_edge[edge_index];
                    if indices.len() < 2 {
                        0.0
                    } else {
                        let segment = self.segments[indices[indices.len() - 2]];
                        (segment.to - segment.from).abs()
                    }
                };
                perpendicular_span(*left)
                    .total_cmp(&perpendicular_span(*right))
                    .then_with(|| left.cmp(right))
            });
            work_budget.charge(group.len())?;
            let handles: Vec<usize> = group
                .into_iter()
                .filter_map(|edge| self.by_edge[edge].last().copied())
                .collect();
            conflicts += self.resolve_handles(&handles, true, work_budget)?;
        }
        Ok(conflicts)
    }

    fn fix_pipe_conflicts(&mut self, work_budget: &mut LayoutWorkBudget) -> Result<usize> {
        work_budget.charge(self.pipes.len())?;
        let mut conflicts = 0;
        for pipe_index in 0..self.pipes.len() {
            work_budget.charge(self.segments.len().saturating_mul(2))?;
            let mut segments: Vec<usize> = self
                .segments
                .iter()
                .enumerate()
                .filter(|(_, segment)| segment.pipe_index == pipe_index)
                .map(|(index, _)| index)
                .collect();
            work_budget.charge(sorting_work_units(segments.len()))?;
            segments.sort_by_key(|index| {
                let segment = self.segments[*index];
                (segment.edge_index, segment.segment_index)
            });
            work_budget.charge(unordered_pair_count(segments.len()))?;
            for left in 0..segments.len() {
                for right in left + 1..segments.len() {
                    let left_index = segments[left];
                    let right_index = segments[right];
                    if self.segments_conflict(left_index, right_index, work_budget)? {
                        conflicts += 1;
                        self.resolve_conflict(left_index, right_index, false, work_budget)?;
                    }
                }
            }
        }
        Ok(conflicts)
    }

    fn reduce_conflicts(&mut self, work_budget: &mut LayoutWorkBudget) -> Result<()> {
        for _ in 0..MAX_CONFLICT_REDUCTION_PASSES {
            work_budget.charge(1)?;
            let changed = self
                .fix_source_handles(work_budget)?
                .saturating_add(self.fix_target_handles(work_budget)?)
                .saturating_add(self.fix_pipe_conflicts(work_budget)?);
            if changed == 0 {
                break;
            }
        }
        Ok(())
    }

    fn segment_coordinates(
        &mut self,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<HashMap<(usize, usize), f64>> {
        work_budget.charge(self.pipes.len())?;
        let mut coordinates = HashMap::new();
        for pipe_index in 0..self.pipes.len() {
            let tracks = &self.pipes[pipe_index].tracks;
            work_budget.charge(tracks.len())?;
            let entry_count = tracks.iter().fold(0usize, |total, track| {
                total.saturating_add(track.segments.len())
            });
            work_budget.charge(entry_count.saturating_mul(2))?;
            let mut entries: Vec<(usize, SegmentRef)> = tracks
                .iter()
                .enumerate()
                .flat_map(|(track, value)| {
                    value
                        .segments
                        .iter()
                        .copied()
                        .map(move |segment| (track, segment))
                })
                .collect();
            work_budget.charge(sorting_work_units(entries.len()))?;
            entries.sort_by(|left, right| left.1.from.total_cmp(&right.1.from));
            work_budget.charge(entries.len())?;
            let mut clusters: Vec<Vec<(usize, SegmentRef)>> = Vec::new();
            for entry in entries {
                if let Some(cluster) = clusters.last_mut() {
                    work_budget.charge(cluster.len())?;
                    let end = cluster
                        .iter()
                        .map(|(_, segment)| segment.to)
                        .fold(f64::NEG_INFINITY, f64::max);
                    if entry.1.from < end {
                        work_budget.charge(1)?;
                        cluster.push(entry);
                        continue;
                    }
                }
                work_budget.charge(1)?;
                clusters.push(vec![entry]);
            }
            for cluster in clusters {
                // JavaScript Set preserves first-seen insertion order. That order is
                // observable below whenever scores tie, because Array.sort is stable.
                work_budget.charge(cluster.len().saturating_mul(2))?;
                let used: IndexSet<usize> = cluster.iter().map(|(track, _)| *track).collect();
                let mut scores: HashMap<usize, f64> = HashMap::new();
                for (track, segment) in &cluster {
                    let delta = self
                        .destination_info(segment.edge_index, work_budget)?
                        .delta;
                    *scores.entry(*track).or_default() += delta;
                }
                work_budget.charge(used.len().saturating_mul(6))?;
                let mut left: Vec<usize> = used
                    .iter()
                    .copied()
                    .filter(|track| scores.get(track).copied().unwrap_or(0.0) < -1.0)
                    .collect();
                let mut right: Vec<usize> = used
                    .iter()
                    .copied()
                    .filter(|track| scores.get(track).copied().unwrap_or(0.0) > 1.0)
                    .collect();
                let mut neutral: Vec<usize> = used
                    .iter()
                    .copied()
                    .filter(|track| scores.get(track).copied().unwrap_or(0.0).abs() <= 1.0)
                    .collect();
                work_budget.charge(sorting_work_units(left.len()))?;
                left.sort_by(|a, b| {
                    scores
                        .get(b)
                        .copied()
                        .unwrap_or(0.0)
                        .total_cmp(&scores.get(a).copied().unwrap_or(0.0))
                });
                work_budget.charge(sorting_work_units(right.len()))?;
                right.sort_by(|a, b| {
                    scores
                        .get(a)
                        .copied()
                        .unwrap_or(0.0)
                        .total_cmp(&scores.get(b).copied().unwrap_or(0.0))
                });
                if neutral.is_empty() && !used.is_empty() {
                    work_budget.charge(used.len().saturating_mul(2))?;
                    let mut closest: Vec<usize> = used.iter().copied().collect();
                    work_budget.charge(sorting_work_units(closest.len()))?;
                    closest.sort_by(|a, b| {
                        scores
                            .get(a)
                            .copied()
                            .unwrap_or(0.0)
                            .abs()
                            .total_cmp(&scores.get(b).copied().unwrap_or(0.0).abs())
                    });
                    let best = closest[0];
                    work_budget.charge(left.len().saturating_add(right.len()))?;
                    left.retain(|track| *track != best);
                    right.retain(|track| *track != best);
                    neutral.push(best);
                }
                work_budget.charge(
                    used.len()
                        .saturating_mul(cluster.len())
                        .saturating_add(cluster.len()),
                )?;
                let pipe_coord = self.pipes[pipe_index].coord;
                let mut assign = |track: usize, coord: f64| {
                    for (_, segment) in cluster.iter().filter(|(candidate, _)| *candidate == track)
                    {
                        coordinates.insert((segment.edge_index, segment.segment_index), coord);
                    }
                };
                for (index, track) in left.iter().enumerate() {
                    assign(*track, pipe_coord - (index + 1) as f64 * TRACK_SPACING);
                }
                for (index, track) in neutral.iter().enumerate() {
                    let coord = if index == 0 {
                        pipe_coord
                    } else {
                        let direction = if index % 2 == 1 { 1.0 } else { -1.0 };
                        let magnitude = index.div_ceil(2) as f64;
                        pipe_coord + direction * magnitude * TRACK_SPACING * 0.5
                    };
                    assign(*track, coord);
                }
                for (index, track) in right.iter().enumerate() {
                    assign(*track, pipe_coord + (index + 1) as f64 * TRACK_SPACING);
                }
            }
        }
        Ok(coordinates)
    }

    fn rebuild_edges(
        &self,
        edges: &mut [WorkingEdge],
        coordinates: &HashMap<(usize, usize), f64>,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<()> {
        work_budget.charge(edges.len())?;
        for (edge_index, edge) in edges.iter_mut().enumerate() {
            let indices = &self.by_edge[edge_index];
            if indices.is_empty() || edge.points.len() < 2 {
                continue;
            }
            work_budget.charge(indices.len().saturating_mul(2).saturating_add(2))?;
            let source_port = edge.points[0].clone();
            let target_port = edge.points.last().expect("target port").clone();
            let lines: Vec<RoutedLine> = indices
                .iter()
                .map(|index| {
                    let segment = self.segments[*index];
                    RoutedLine {
                        orientation: segment.orientation,
                        coord: coordinates
                            .get(&(segment.edge_index, segment.segment_index))
                            .copied()
                            .unwrap_or(self.pipes[segment.pipe_index].coord),
                        from: segment.from,
                        to: segment.to,
                    }
                })
                .collect();
            work_budget.charge(lines.len().saturating_mul(3).saturating_add(2))?;
            let mut points = vec![source_port];
            for (index, line) in lines.iter().copied().enumerate() {
                let previous = points.last().expect("route point");
                let previous_along = match line.orientation {
                    Orientation::Vertical => previous.y,
                    Orientation::Horizontal => previous.x,
                };
                let previous_coord = match line.orientation {
                    Orientation::Vertical => previous.x,
                    Orientation::Horizontal => previous.y,
                };
                if (previous_coord - line.coord).abs() > EPSILON {
                    points.push(point_on_line(line, previous_along));
                }
                let next = lines.get(index + 1).copied();
                match next {
                    Some(next) if next.orientation == line.orientation => {
                        if (line.coord - next.coord).abs() > EPSILON {
                            let junction = if line.orientation == Orientation::Vertical {
                                (previous_along + next.from) / 2.0
                            } else {
                                shared_line_endpoint_coord(line, next)
                            };
                            points.push(point_on_line(line, junction));
                            points.push(point_on_line(next, junction));
                        } else if index == 0 || index + 2 == lines.len() {
                            points
                                .push(point_on_line(line, shared_line_endpoint_coord(line, next)));
                        }
                    }
                    Some(next) => points.push(point_on_line(line, next.coord)),
                    None => {
                        let end_along = if (line.from - previous_along).abs()
                            < (line.to - previous_along).abs()
                        {
                            line.to
                        } else {
                            line.from
                        };
                        points.push(point_on_line(line, end_along));
                    }
                }
            }
            if points.last().is_none_or(|last| {
                (last.x - target_port.x).abs() > EPSILON || (last.y - target_port.y).abs() > EPSILON
            }) {
                points.push(target_port);
            }
            work_budget.charge(points.len())?;
            points.dedup_by(|left, right| {
                (left.x - right.x).abs() <= EPSILON && (left.y - right.y).abs() <= EPSILON
            });
            edge.points = points;
        }
        Ok(())
    }
}

pub(super) fn assign_tracks(
    edges: &mut [WorkingEdge],
    routing_order: &[usize],
    centered_straight_edges: &HashSet<usize>,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    let mut assignment = TrackAssignment::new(edges, work_budget)?;
    work_budget.charge(routing_order.len())?;
    for edge_index in routing_order.iter().copied() {
        if !centered_straight_edges.contains(&edge_index) {
            assignment.add_edge(edge_index, work_budget)?;
        }
    }
    assignment.reduce_conflicts(work_budget)?;
    let coordinates = assignment.segment_coordinates(work_budget)?;

    // TrackAssignment borrows the pre-rebuild edge geometry. Rebuild through a
    // temporary vector so source/target ports stay stable during materialization.
    work_budget.charge(edges.len())?;
    let clone_work = edges.iter().fold(edges.len(), |total, edge| {
        total.saturating_add(edge.points.len())
    });
    work_budget.charge(clone_work)?;
    let mut rebuilt = edges.to_vec();
    assignment.rebuild_edges(&mut rebuilt, &coordinates, work_budget)?;
    work_budget.charge(edges.len())?;
    for (edge, next) in edges.iter_mut().zip(rebuilt) {
        edge.points = next.points;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};

    fn edge(id: &str, y: f64) -> WorkingEdge {
        WorkingEdge {
            id: id.to_string(),
            from: "source".to_string(),
            to: "target".to_string(),
            reference_id: id.to_string(),
            label_node_id: None,
            reversed_for_layout: false,
            points: vec![
                LayoutPoint { x: 0.0, y },
                LayoutPoint { x: 50.0, y },
                LayoutPoint { x: 50.0, y: 100.0 },
            ],
        }
    }

    fn straight_edge(id: &str, y: f64) -> WorkingEdge {
        WorkingEdge {
            id: id.to_string(),
            from: "source".to_string(),
            to: "target".to_string(),
            reference_id: id.to_string(),
            label_node_id: None,
            reversed_for_layout: false,
            points: vec![LayoutPoint { x: 0.0, y }, LayoutPoint { x: 100.0, y }],
        }
    }

    fn policy(max: usize) -> RenderResourcePolicy {
        RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, max)
            .unwrap()
    }

    fn point_snapshot(edges: &[WorkingEdge]) -> Vec<Vec<(f64, f64)>> {
        edges
            .iter()
            .map(|edge| edge.points.iter().map(|point| (point.x, point.y)).collect())
            .collect()
    }

    fn measured_assignment_work(mut edges: Vec<WorkingEdge>) -> usize {
        let routing_order = (0..edges.len()).collect::<Vec<_>>();
        let mut budget = LayoutWorkBudget::unbounded_for_tests();
        assign_tracks(&mut edges, &routing_order, &HashSet::new(), &mut budget).unwrap();
        budget.used()
    }

    #[test]
    fn exact_track_assignment_budget_reaches_the_boundary() {
        let mut edges = vec![edge("first", 0.0), edge("second", 10.0)];
        let required = measured_assignment_work(edges.clone());
        let mut budget = LayoutWorkBudget::new(policy(required), 0).unwrap();

        assign_tracks(&mut edges, &[0, 1], &HashSet::new(), &mut budget).unwrap();

        assert_eq!(budget.used(), required);
    }

    #[test]
    fn rejected_track_assignment_preserves_input_geometry() {
        let mut edges = vec![edge("first", 0.0), edge("second", 10.0)];
        let before = point_snapshot(&edges);
        let required = measured_assignment_work(edges.clone());
        let mut budget = LayoutWorkBudget::new(policy(required - 1), 0).unwrap();

        let error = assign_tracks(&mut edges, &[0, 1], &HashSet::new(), &mut budget).unwrap_err();

        assert!(
            error.to_string().contains("max_layout_work_units"),
            "{error}"
        );
        assert_eq!(point_snapshot(&edges), before);
        assert!(budget.used() < required);
    }

    #[test]
    fn same_pipe_conflicts_charge_more_than_separate_pipes() {
        let dense: Vec<_> = (0..6)
            .map(|index| straight_edge(&format!("dense-{index}"), 0.0))
            .collect();
        let separated = (0..6)
            .map(|index| straight_edge(&format!("separate-{index}"), index as f64 * 10.0))
            .collect();

        let dense_work = measured_assignment_work(dense.clone());
        let separated_work = measured_assignment_work(separated);

        assert!(
            dense_work > separated_work,
            "same-pipe conflict scans and moves must be visible in the meter: dense={dense_work}, separated={separated_work}"
        );

        let mut rejected = dense;
        let before = point_snapshot(&rejected);
        let routing_order = (0..rejected.len()).collect::<Vec<_>>();
        let mut budget = LayoutWorkBudget::new(policy(dense_work - 1), 0).unwrap();
        assign_tracks(&mut rejected, &routing_order, &HashSet::new(), &mut budget).unwrap_err();
        assert_eq!(point_snapshot(&rejected), before);
    }

    #[test]
    fn pipe_lookup_charges_each_existing_pipe_before_allocation() {
        let edges = Vec::new();
        let mut budget = LayoutWorkBudget::new(policy(9), 0).unwrap();
        let mut assignment = TrackAssignment::new(&edges, &mut budget).unwrap();

        assignment
            .pipe_for(Orientation::Horizontal, 0.0, &mut budget)
            .unwrap();
        assignment
            .pipe_for(Orientation::Horizontal, 10.0, &mut budget)
            .unwrap();
        assignment
            .pipe_for(Orientation::Horizontal, 20.0, &mut budget)
            .unwrap();

        assert_eq!(budget.used(), 9);
        let error = assignment
            .pipe_for(Orientation::Horizontal, 30.0, &mut budget)
            .unwrap_err();
        assert!(
            error.to_string().contains("max_layout_work_units"),
            "{error}"
        );
        assert_eq!(assignment.pipes.len(), 3);
    }
}
