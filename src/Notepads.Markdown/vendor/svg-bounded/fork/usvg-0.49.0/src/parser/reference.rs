// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Document-local reference paths. No request or dynamic owner is created here.
use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourcesRef};

#[derive(Clone, Copy, Debug)]
pub(super) struct RefPath {
    nodes: [u32; 65],
    depth: u8,
}
impl Default for RefPath {
    fn default() -> Self {
        Self {
            nodes: [0; 65],
            depth: 0,
        }
    }
}
impl RefPath {
    pub(super) fn paid_root(id: u32, r: Option<&ResourcesRef>) -> Result<Self, Error> {
        if let Some(r) = r {
            r.checkpoint(66)?;
        }
        Ok(Self::root(id))
    }
    pub(super) fn root(id: u32) -> Self {
        let mut result = Self::default();
        result.nodes[0] = id;
        result
    }
    pub(super) fn depth(&self) -> u8 {
        self.depth
    }
    pub(super) fn enter(&self, id: u32, r: Option<&ResourcesRef>) -> Result<Self, Error> {
        let Some(r) = r else {
            return Ok(*self);
        };
        // Include the entire fixed copy and all inspected identities before reading them.
        r.checkpoint(u64::from(self.depth) + 67)?;
        let mut repeated = false;
        for previous in &self.nodes[..=usize::from(self.depth)] {
            repeated |= *previous == id;
        }
        if repeated {
            return Err(r.record(Error::new(FailureKind::Decode, LimitKind::None)));
        }
        if self.depth == 64 {
            return Err(r.record(Error::new(FailureKind::Quota, LimitKind::Capacity)));
        }
        let mut result = *self;
        result.depth += 1;
        result.nodes[usize::from(result.depth)] = id;
        Ok(result)
    }
    pub(super) fn check_span(
        &self,
        root: u32,
        span: u8,
        r: Option<&ResourcesRef>,
    ) -> Result<u8, Error> {
        let Some(r) = r else {
            return Ok(0);
        };
        let entered = self.enter(root, Some(r))?;
        entered.check_height(span.saturating_sub(1), Some(r))
    }
    pub(super) fn check_height(&self, height: u8, r: Option<&ResourcesRef>) -> Result<u8, Error> {
        let Some(r) = r else {
            return Ok(0);
        };
        r.checkpoint(1)?;
        let peak = u16::from(self.depth) + u16::from(height);
        if peak > 64 {
            return Err(r.record(Error::new(FailureKind::Quota, LimitKind::Capacity)));
        }
        Ok(peak as u8)
    }
}

#[derive(Clone, Copy, Default)]
struct Frame {
    base: u8,
    peak: u8,
}
pub(super) struct RefFrames {
    frames: [Frame; 65],
    len: usize,
}
impl Default for RefFrames {
    fn default() -> Self {
        Self {
            frames: [Frame::default(); 65],
            len: 0,
        }
    }
}
impl RefFrames {
    pub(super) fn push(&mut self, base: u8, r: Option<&ResourcesRef>) -> Result<(), Error> {
        if let Some(r) = r {
            r.checkpoint(6)?;
        }
        if self.len == self.frames.len() {
            return Err(
                r.map_or(Error::new(FailureKind::Quota, LimitKind::Capacity), |r| {
                    r.record(Error::new(FailureKind::Quota, LimitKind::Capacity))
                }),
            );
        }
        self.frames[self.len] = Frame { base, peak: base };
        self.len += 1;
        Ok(())
    }
    pub(super) fn note(&mut self, peak: u8, r: Option<&ResourcesRef>) -> Result<(), Error> {
        if let Some(r) = r {
            r.checkpoint(1)?;
        }
        if let Some(frame) = self.frames[..self.len].last_mut() {
            frame.peak = frame.peak.max(peak);
        }
        Ok(())
    }
    pub(super) fn height(&self) -> u8 {
        self.frames[..self.len]
            .last()
            .map_or(0, |f| f.peak - f.base)
    }
    // Cleanup is unconditional; a sticky error must never retain an active frame.
    pub(super) fn pop(&mut self) {
        self.len -= 1;
        let peak = self.frames[self.len].peak;
        if let Some(parent) = self.frames[..self.len].last_mut() {
            parent.peak = parent.peak.max(peak);
        }
    }
}
#[derive(Debug)]
pub(super) struct Checked<T> {
    pub(super) value: T,
    pub(super) height: u8,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PaintProof {
    pub(super) root: u32,
    pub(super) span: u8,
}
