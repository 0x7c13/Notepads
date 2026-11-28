// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Temporary presence index; parameter owners and their source order stay put.
use crate::storage::{Fault, List, Storage, Text, Work};
use core::cmp::Ordering;

const NONE: usize = usize::MAX;
const PATH: usize = 2 * usize::BITS as usize;
#[derive(Clone, Copy)]
#[repr(C)]
struct Node {
    parameter: usize,
    left: usize,
    right: usize,
    height: u8,
}
pub(super) struct ParameterIndex<S: Storage> {
    nodes: S::List<Node>,
    root: usize,
}
impl<S: Storage> ParameterIndex<S> {
    pub(super) fn new(work: &Work) -> Self {
        Self {
            nodes: S::list(work),
            root: NONE,
        }
    }
    fn compare(left: &str, right: &str, work: &Work) -> Result<Ordering, Fault> {
        for (a, b) in left.bytes().zip(right.bytes()) {
            work.charge(1)?;
            if a != b {
                return Ok(a.cmp(&b));
            }
        }
        Ok(left.len().cmp(&right.len()))
    }
    pub(super) fn contains(
        &self,
        name: &str,
        parameters: &[(S::Text, S::Text)],
        work: &Work,
    ) -> Result<bool, Fault> {
        let mut at = self.root;
        while at != NONE {
            work.charge(1)?;
            let node = self.nodes.as_slice()[at];
            match Self::compare(name, parameters[node.parameter].0.as_str(), work)? {
                Ordering::Equal => return Ok(true),
                Ordering::Less => at = node.left,
                Ordering::Greater => at = node.right,
            }
        }
        Ok(false)
    }
    fn height(&self, at: usize) -> u8 {
        if at == NONE {
            0
        } else {
            self.nodes.as_slice()[at].height
        }
    }
    fn update(&mut self, at: usize, work: &Work) -> Result<(), Fault> {
        let node = self.nodes.as_slice()[at];
        let height = self
            .height(node.left)
            .max(self.height(node.right))
            .checked_add(1)
            .ok_or_else(|| work.invariant())?;
        self.nodes.as_mut_slice()[at].height = height;
        Ok(())
    }
    fn rotate_left(&mut self, root: usize, work: &Work) -> Result<usize, Fault> {
        work.charge(1)?;
        let next = self.nodes.as_slice()[root].right;
        if next == NONE {
            return Err(work.invariant());
        }
        self.nodes.as_mut_slice()[root].right = self.nodes.as_slice()[next].left;
        self.nodes.as_mut_slice()[next].left = root;
        self.update(root, work)?;
        self.update(next, work)?;
        Ok(next)
    }
    fn rotate_right(&mut self, root: usize, work: &Work) -> Result<usize, Fault> {
        work.charge(1)?;
        let next = self.nodes.as_slice()[root].left;
        if next == NONE {
            return Err(work.invariant());
        }
        self.nodes.as_mut_slice()[root].left = self.nodes.as_slice()[next].right;
        self.nodes.as_mut_slice()[next].right = root;
        self.update(root, work)?;
        self.update(next, work)?;
        Ok(next)
    }
    fn balance(&mut self, root: usize, work: &Work) -> Result<usize, Fault> {
        work.charge(1)?;
        self.update(root, work)?;
        let node = self.nodes.as_slice()[root];
        let left_height = self.height(node.left);
        let right_height = self.height(node.right);
        if left_height > right_height.saturating_add(1) {
            let left = self.nodes.as_slice()[node.left];
            if self.height(left.left) < self.height(left.right) {
                let next = self.rotate_left(node.left, work)?;
                self.nodes.as_mut_slice()[root].left = next;
            }
            return self.rotate_right(root, work);
        }
        if right_height > left_height.saturating_add(1) {
            let right = self.nodes.as_slice()[node.right];
            if self.height(right.right) < self.height(right.left) {
                let next = self.rotate_right(node.right, work)?;
                self.nodes.as_mut_slice()[root].right = next;
            }
            return self.rotate_left(root, work);
        }
        Ok(root)
    }
    pub(super) fn insert(
        &mut self,
        parameter: usize,
        parameters: &[(S::Text, S::Text)],
        work: &Work,
    ) -> Result<(), Fault> {
        let name = parameters[parameter].0.as_str();
        let mut path = [NONE; PATH];
        let mut to_left = [false; PATH];
        let mut depth = 0usize;
        let mut at = self.root;
        while at != NONE {
            work.charge(1)?;
            if depth == PATH {
                return Err(work.invariant());
            }
            let node = self.nodes.as_slice()[at];
            let direction = match Self::compare(name, parameters[node.parameter].0.as_str(), work)?
            {
                // A later uppercase input can legally produce the same stored
                // lowercase key. Keep its parameter, but index the first slot.
                Ordering::Equal => return Ok(()),
                Ordering::Less => true,
                Ordering::Greater => false,
            };
            path[depth] = at;
            to_left[depth] = direction;
            depth += 1;
            at = if direction { node.left } else { node.right };
        }
        let inserted = self.nodes.as_slice().len();
        self.nodes.push(Node {
            parameter,
            left: NONE,
            right: NONE,
            height: 1,
        })?;
        if depth == 0 {
            self.root = inserted;
            return Ok(());
        }
        let parent = path[depth - 1];
        if to_left[depth - 1] {
            self.nodes.as_mut_slice()[parent].left = inserted;
        } else {
            self.nodes.as_mut_slice()[parent].right = inserted;
        }
        while depth != 0 {
            depth -= 1;
            let balanced = self.balance(path[depth], work)?;
            if depth == 0 {
                self.root = balanced;
            } else {
                let parent = path[depth - 1];
                if to_left[depth - 1] {
                    self.nodes.as_mut_slice()[parent].left = balanced;
                } else {
                    self.nodes.as_mut_slice()[parent].right = balanced;
                }
            }
        }
        Ok(())
    }
}
