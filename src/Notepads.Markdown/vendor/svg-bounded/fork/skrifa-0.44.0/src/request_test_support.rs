// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Test bootstrap/observations are fixture allocations, not product RSS claims.

use markdown_svg_budget::{
    Backend, Error, FirstFailureRef, Limits, ResourceKind, ResourcesRef, Ticket,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    Reserve(usize),
    Commit(usize),
    Release(usize),
    Work(usize),
}
#[derive(Default, Debug)]
pub(crate) struct Events {
    pub reserve: usize,
    pub commit: usize,
    pub release: usize,
    pub positive_work: usize,
    pub work: Vec<u64>,
    pub sizes: Vec<usize>,
    pub injected: usize,
    pub failed_release_bytes: usize,
}
pub(crate) const FAULT: Error = Error::native(-0x5E01);
struct Host {
    events: Arc<Mutex<Events>>,
    fault: Option<Fault>,
}
impl Backend for Host {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let mut events = self.events.lock().unwrap();
        events.reserve += 1;
        events.sizes.push(bytes);
        if self.fault == Some(Fault::Reserve(events.reserve)) {
            events.injected += 1;
            return Err(FAULT);
        }
        Ticket::new(0x5E01, events.reserve as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
        let mut events = self.events.lock().unwrap();
        events.commit += 1;
        if self.fault == Some(Fault::Commit(events.commit)) {
            events.injected += 1;
            Err(FAULT)
        } else {
            Ok(())
        }
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let mut events = self.events.lock().unwrap();
        events.release += 1;
        if self.fault == Some(Fault::Release(events.release)) {
            // B deliberately keeps failed refunds charged. The ticket names
            // the exact previously admitted block, not an inferred size.
            events.failed_release_bytes = events.sizes[ticket.slot() as usize - 1];
            events.injected += 1;
            Err(FAULT)
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        let mut events = self.events.lock().unwrap();
        events.work.push(work);
        if work != 0 {
            events.positive_work += 1;
            if self.fault == Some(Fault::Work(events.positive_work)) {
                events.injected += 1;
                return Err(FAULT);
            }
        }
        Ok(())
    }
}
pub(crate) fn resources(
    fault: Option<Fault>,
    limits: Limits,
) -> (ResourcesRef, Arc<Mutex<Events>>) {
    let events = Arc::new(Mutex::new(Events::default()));
    let resources = ResourcesRef::new_prepaid(
        Host {
            events: events.clone(),
            fault,
        },
        FirstFailureRef::new_prepaid(),
        limits,
    );
    (resources, events)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Move([u32; 2]),
    Line([u32; 2]),
    Quad([u32; 4]),
    Curve([u32; 6]),
    Close,
}
#[derive(Default, Debug)]
pub(crate) struct Pen(pub Vec<Event>);
impl crate::outline::OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(Event::Move([x, y].map(f32::to_bits)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(Event::Line([x, y].map(f32::to_bits)));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.push(Event::Quad([cx, cy, x, y].map(f32::to_bits)));
    }
    fn curve_to(&mut self, cx: f32, cy: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        self.0
            .push(Event::Curve([cx, cy, c1x, c1y, x, y].map(f32::to_bits)));
    }
    fn close(&mut self) {
        self.0.push(Event::Close);
    }
}
pub(crate) fn metrics(value: crate::outline::AdjustedMetrics) -> (bool, Option<u32>, Option<u32>) {
    (
        value.has_overlaps,
        value.lsb.map(f32::to_bits),
        value.advance_width.map(f32::to_bits),
    )
}

/// Minimal SFNT container for new local stress fixtures. Fixed upstream font
/// files and reference expectations are never rewritten by this helper.
pub(crate) fn sfnt(mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by_key(|(tag, _)| *tag);
    let count = tables.len();
    assert!(count > 0 && count <= u16::MAX as usize);
    let power = 1usize << ((usize::BITS - 1) - count.leading_zeros());
    let mut bytes = vec![0; 12 + count * 16];
    bytes[..4].copy_from_slice(&0x00010000u32.to_be_bytes());
    bytes[4..6].copy_from_slice(&(count as u16).to_be_bytes());
    bytes[6..8].copy_from_slice(&((power * 16) as u16).to_be_bytes());
    bytes[8..10].copy_from_slice(&(power.trailing_zeros() as u16).to_be_bytes());
    bytes[10..12].copy_from_slice(&((count * 16 - power * 16) as u16).to_be_bytes());
    for (index, (tag, data)) in tables.into_iter().enumerate() {
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        let offset = bytes.len() as u32;
        let len = data.len() as u32;
        let start = 12 + index * 16;
        bytes[start..start + 4].copy_from_slice(&tag);
        bytes[start + 8..start + 12].copy_from_slice(&offset.to_be_bytes());
        bytes[start + 12..start + 16].copy_from_slice(&len.to_be_bytes());
        bytes.extend(data);
    }
    bytes
}
