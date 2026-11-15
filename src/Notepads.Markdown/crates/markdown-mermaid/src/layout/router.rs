// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Finite rectilinear visibility channels. Direction is part of Dijkstra state.
use crate::{
    canonical::CanonicalError,
    scene::{Point, Rect},
};
use markdown_memory::{Family, FundedVec, MemoryScope};

const NONE: u32 = u32::MAX;
const MAX_STATES: usize = 65_536;
const CLEARANCE_EPSILON: f64 = 1e-7;

pub struct ChannelGraph {
    xs: FundedVec<f64>,
    ys: FundedVec<f64>,
    neighbors: FundedVec<[u32; 4]>,
    search: Option<Search>,
    scope: MemoryScope,
}

fn point_at(xs: &[f64], ys: &[f64], index: usize) -> Point {
    Point {
        x: xs[index % xs.len()],
        y: ys[index / xs.len()],
    }
}

fn unique(values: &mut FundedVec<f64>, scope: &MemoryScope) -> Result<(), CanonicalError> {
    // Fallible insertion sort: the authority is consulted before every real
    // comparison/move, rather than estimating std's private comparison count.
    for read in 1..values.len() {
        let mut position = read;
        while position > 0 {
            scope.charge_work(1)?;
            if !values[position - 1].total_cmp(&values[position]).is_gt() {
                break;
            }
            values.swap(position - 1, position);
            position -= 1;
        }
    }
    let mut count = 0;
    for read in 0..values.len() {
        scope.charge_work(1)?;
        if count == 0 || (values[read] - values[count - 1]).abs() > 1e-6 {
            values[count] = values[read];
            count += 1;
        }
    }
    values.truncate(count);
    Ok(())
}
impl ChannelGraph {
    /// Every endpoint/portal/lane must be planned before this reusable graph.
    pub fn new(
        scope: &MemoryScope,
        bounds: Rect,
        obstacles: &[Rect],
        anchors: &[Point],
        lanes: &[Point],
        scene_states: &mut usize,
    ) -> Result<Self, CanonicalError> {
        let cap = 2usize
            .checked_add(
                obstacles
                    .len()
                    .checked_mul(2)
                    .ok_or(CanonicalError::Limited)?,
            )
            .and_then(|n| n.checked_add(anchors.len()))
            .and_then(|n| n.checked_add(lanes.len()))
            .ok_or(CanonicalError::Limited)?;
        if cap > 1024 {
            return Err(CanonicalError::Limited);
        }
        let tx = scope.reserve_array::<f64>(cap, Family::Work)?;
        let ty = scope.reserve_array::<f64>(cap, Family::Work)?;
        let mut xs = FundedVec::try_with_prepared(tx, cap)?;
        let mut ys = FundedVec::try_with_prepared(ty, cap)?;
        xs.try_push(bounds.left)?;
        xs.try_push(bounds.right)?;
        ys.try_push(bounds.top)?;
        ys.try_push(bounds.bottom)?;
        for r in obstacles {
            scope.charge_work(1)?;
            xs.try_push(r.left)?;
            xs.try_push(r.right)?;
            ys.try_push(r.top)?;
            ys.try_push(r.bottom)?;
        }
        for p in anchors.iter().chain(lanes) {
            scope.charge_work(1)?;
            xs.try_push(p.x)?;
            ys.try_push(p.y)?;
        }
        unique(&mut xs, scope)?;
        unique(&mut ys, scope)?;
        if xs
            .iter()
            .chain(ys.iter())
            .any(|v| !v.is_finite() || v.abs() > 1.0e7)
        {
            return Err(CanonicalError::InvalidGeometry);
        }
        let points = xs
            .len()
            .checked_mul(ys.len())
            .ok_or(CanonicalError::Limited)?;
        let states = points.checked_mul(4).ok_or(CanonicalError::Limited)?;
        if states > MAX_STATES
            || scene_states
                .checked_add(states)
                .is_none_or(|n| n > MAX_STATES)
        {
            return Err(CanonicalError::Limited);
        }
        // Validate actual cartesian/state cardinalities before payload reserve.
        scope.charge_work(points)?;
        let mut neighbors = FundedVec::try_with_capacity(scope, Family::Work, points)?;
        neighbors.try_resize(points, [NONE; 4])?;
        let mut active = FundedVec::try_with_capacity(scope, Family::Work, obstacles.len())?;
        connect_scan_lines(
            &xs,
            &ys,
            true,
            obstacles,
            &mut active,
            &mut neighbors,
            scope,
        )?;
        connect_scan_lines(
            &xs,
            &ys,
            false,
            obstacles,
            &mut active,
            &mut neighbors,
            scope,
        )?;
        *scene_states += states;
        Ok(Self {
            xs,
            ys,
            neighbors,
            search: None,
            scope: scope.clone(),
        })
    }
    fn index(&self, p: Point) -> Result<usize, CanonicalError> {
        self.scope.charge_work(self.xs.len() + self.ys.len())?;
        let x = self
            .xs
            .iter()
            .position(|x| (*x - p.x).abs() <= 1e-6)
            .ok_or(CanonicalError::InvalidGeometry)?;
        let y = self
            .ys
            .iter()
            .position(|y| (*y - p.y).abs() <= 1e-6)
            .ok_or(CanonicalError::InvalidGeometry)?;
        Ok(y * self.xs.len() + x)
    }
    pub fn route(
        &mut self,
        from: Point,
        to: Point,
        turn_penalty: f64,
        _source_ordinal: usize,
    ) -> Result<FundedVec<Point>, CanonicalError> {
        if !turn_penalty.is_finite() || turn_penalty < 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        let start = self.index(from)?;
        let target = self.index(to)?;
        if self.search.is_none() {
            self.search = Some(Search::new(&self.scope, self.neighbors.len() * 4)?);
        }
        let search = self.search.as_mut().unwrap();
        let generation = search.restart()?;
        let mut queue = Queue {
            heap: &mut search.heap,
            states: &mut search.states,
            scope: &self.scope,
        };
        for direction in 0..4 {
            let state = start * 4 + direction;
            self.scope.charge_work(1)?;
            queue.states[state] = SearchState {
                generation,
                distance: 0.0,
                ..SearchState::EMPTY
            };
            queue.update(state as u32)?;
        }
        let mut terminal = None;
        while let Some(state) = queue.pop()? {
            let state = state as usize;
            self.scope.charge_work(1)?;
            let (point, incoming) = (state / 4, state % 4);
            if point == target {
                terminal = Some(state);
                break;
            }
            let a = point_at(&self.xs, &self.ys, point);
            for direction in 0..4 {
                self.scope.charge_work(1)?;
                let next = self.neighbors[point][direction];
                if next == NONE {
                    continue;
                }
                let successor = next as usize * 4 + direction;
                let current = queue.states[successor];
                let discovered = current.generation == generation;
                // Only settled states in this generation have left the queue.
                if discovered && current.position == NONE {
                    continue;
                }
                let b = point_at(&self.xs, &self.ys, next as usize);
                let cost = queue.states[state].distance
                    + (a.x - b.x).abs()
                    + (a.y - b.y).abs()
                    + if direction == incoming {
                        0.0
                    } else {
                        turn_penalty
                    };
                let previous = if discovered {
                    current.distance
                } else {
                    f64::INFINITY
                };
                if cost < previous {
                    self.scope.charge_work(1)?;
                    queue.states[successor] = SearchState {
                        distance: cost,
                        parent: state as u32,
                        position: if discovered { current.position } else { NONE },
                        generation,
                    };
                    queue.update(successor as u32)?;
                }
            }
        }
        let mut state = terminal.ok_or(CanonicalError::Limited)?;
        let mut path = FundedVec::try_with_capacity(&self.scope, Family::Graph, 257)?;
        loop {
            self.scope.charge_work(1)?;
            let p = point_at(&self.xs, &self.ys, state / 4);
            if path.last().is_none_or(|last: &Point| *last != p) {
                if path.len() >= 257 {
                    return Err(CanonicalError::Limited);
                }
                path.try_push(p)?;
            }
            let parent = queue.states[state].parent;
            if parent == NONE {
                break;
            }
            state = parent as usize;
        }
        path.reverse();
        let mut count = 0;
        for read in 0..path.len() {
            self.scope.charge_work(1)?;
            if count >= 2 {
                let (a, b, c) = (path[count - 2], path[count - 1], path[read]);
                if ((a.x - b.x).abs() < 1e-7 && (b.x - c.x).abs() < 1e-7)
                    || ((a.y - b.y).abs() < 1e-7 && (b.y - c.y).abs() < 1e-7)
                {
                    count -= 1;
                }
            }
            path[count] = path[read];
            count += 1;
        }
        path.truncate(count);
        Ok(path)
    }
}

fn connect_scan_lines(
    xs: &[f64],
    ys: &[f64],
    horizontal: bool,
    obstacles: &[Rect],
    active: &mut FundedVec<(f64, f64)>,
    neighbors: &mut [[u32; 4]],
    scope: &MemoryScope,
) -> Result<(), CanonicalError> {
    let (travel, cross, direction) = if horizontal { (xs, ys, 0) } else { (ys, xs, 2) };
    for (line, &offset) in cross.iter().enumerate() {
        scope.charge_work(1)?;
        active.clear();
        // Only an obstacle that contains the scan line can block a point or
        // disconnect a segment along it.
        for rect in obstacles {
            scope.charge_work(1)?;
            let (near, far, start, end) = if horizontal {
                (rect.top, rect.bottom, rect.left, rect.right)
            } else {
                (rect.left, rect.right, rect.top, rect.bottom)
            };
            if offset > near + CLEARANCE_EPSILON && offset < far - CLEARANCE_EPSILON {
                active.try_push((start + CLEARANCE_EPSILON, end - CLEARANCE_EPSILON))?;
            }
        }
        let mut previous: Option<(f64, usize)> = None;
        'point: for (step, &position) in travel.iter().enumerate() {
            scope.charge_work(1)?;
            let mut segment_clear = true;
            for &(start, end) in active.iter() {
                scope.charge_work(1)?;
                if position > start {
                    if position < end {
                        continue 'point;
                    }
                    if previous.is_some_and(|(p, _)| p < end) {
                        segment_clear = false;
                    }
                }
            }
            let index = if horizontal {
                line * xs.len() + step
            } else {
                step * xs.len() + line
            };
            if segment_clear && let Some((_, adjacent)) = previous {
                neighbors[index][direction] = adjacent as u32;
                neighbors[adjacent][direction + 1] = index as u32;
            }
            previous = Some((position, index));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct SearchState {
    distance: f64,
    parent: u32,
    position: u32,
    generation: u32,
}

impl SearchState {
    const EMPTY: Self = Self {
        distance: f64::INFINITY,
        parent: NONE,
        position: NONE,
        generation: 0,
    };
}

struct Search {
    heap: FundedVec<u32>,
    states: FundedVec<SearchState>,
    generation: u32,
}

impl Search {
    fn new(scope: &MemoryScope, count: usize) -> Result<Self, CanonicalError> {
        scope.charge_work(count)?;
        let mut states = FundedVec::try_with_capacity(scope, Family::Work, count)?;
        states.try_resize(count, SearchState::EMPTY)?;
        Ok(Self {
            heap: FundedVec::try_with_capacity(scope, Family::Work, count)?,
            states,
            generation: 0,
        })
    }

    fn restart(&mut self) -> Result<u32, CanonicalError> {
        // Stale positions can refer to the previous queue. Generation checks
        // let the next route initialize only the states it actually discovers.
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(CanonicalError::Limited)?;
        self.heap.clear();
        Ok(self.generation)
    }
}

struct Queue<'a> {
    heap: &'a mut FundedVec<u32>,
    states: &'a mut [SearchState],
    scope: &'a MemoryScope,
}
impl Queue<'_> {
    fn less(&self, a: u32, b: u32) -> Result<bool, CanonicalError> {
        self.scope.charge_work(1)?;
        Ok(self.states[a as usize]
            .distance
            .total_cmp(&self.states[b as usize].distance)
            .then(a.cmp(&b))
            .is_lt())
    }
    fn swap(&mut self, a: usize, b: usize) {
        self.heap.swap(a, b);
        self.states[self.heap[a] as usize].position = a as u32;
        self.states[self.heap[b] as usize].position = b as u32;
    }
    fn update(&mut self, state: u32) -> Result<(), CanonicalError> {
        let mut index = self.states[state as usize].position;
        if index == NONE {
            index = self.heap.len() as u32;
            self.heap
                .push_reserved(state)
                .map_err(|(_, e)| CanonicalError::Memory(e))?;
            self.states[state as usize].position = index;
        }
        let mut index = index as usize;
        while index > 0 {
            let parent = (index - 1) / 2;
            if !self.less(self.heap[index], self.heap[parent])? {
                break;
            }
            self.swap(index, parent);
            index = parent;
        }
        Ok(())
    }
    fn pop(&mut self) -> Result<Option<u32>, CanonicalError> {
        let Some(last) = self.heap.pop() else {
            return Ok(None);
        };
        if self.heap.is_empty() {
            self.states[last as usize].position = NONE;
            return Ok(Some(last));
        }
        let first = std::mem::replace(&mut self.heap[0], last);
        self.states[first as usize].position = NONE;
        self.states[last as usize].position = 0;
        let mut index = 0;
        loop {
            let left = index * 2 + 1;
            if left >= self.heap.len() {
                break;
            }
            let right = left + 1;
            let next = if right < self.heap.len() && self.less(self.heap[right], self.heap[left])? {
                right
            } else {
                left
            };
            if !self.less(self.heap[next], self.heap[index])? {
                break;
            }
            self.swap(index, next);
            index = next;
        }
        Ok(Some(first))
    }
}
