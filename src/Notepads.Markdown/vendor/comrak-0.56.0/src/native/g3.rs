// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Sparse, native-owned G3 acceptance records. No AST or source text is retained.

use super::rows::{NativeRows, charge_host};
use super::{BufferId, Host, NativeKind, NativeNodeId, NativeViolation, RSpan, WorkDelta};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fact {
    Table {
        node: NativeNodeId,
        columns: u32,
        delimiter: RSpan,
    },
    Alignment {
        table: NativeNodeId,
        column: u32,
        value: Alignment,
        delimiter_cell: RSpan,
    },
    Row {
        node: NativeNodeId,
        table: NativeNodeId,
        index: u32,
        header: bool,
        raw: RSpan,
    },
    Cell {
        node: NativeNodeId,
        row: NativeNodeId,
        column: u32,
        missing: bool,
        raw: RSpan,
        content: BufferId,
    },
    Task {
        node: NativeNodeId,
        item: NativeNodeId,
        paragraph: NativeNodeId,
        checked: bool,
        marker: RSpan,
        spacer: RSpan,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Record {
    Table {
        node: NativeNodeId,
        columns: u32,
        delimiter: RSpan,
        alignment_start: usize,
        alignment_count: u32,
        rows: u32,
    },
    Row {
        node: NativeNodeId,
        table: NativeNodeId,
        index: u32,
        header: bool,
        raw: RSpan,
        cells: u32,
    },
    Cell {
        node: NativeNodeId,
        row: NativeNodeId,
        column: u32,
        missing: bool,
        raw: RSpan,
        content: BufferId,
    },
    Task {
        node: NativeNodeId,
        item: NativeNodeId,
        paragraph: NativeNodeId,
        checked: bool,
        marker: RSpan,
        spacer: RSpan,
    },
}

impl Record {
    pub(super) fn node(self) -> NativeNodeId {
        match self {
            Self::Table { node, .. }
            | Self::Row { node, .. }
            | Self::Cell { node, .. }
            | Self::Task { node, .. } => node,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AlignmentRecord {
    pub(super) value: Alignment,
    pub(super) delimiter_cell: RSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct KindRecord {
    pub(super) node: NativeNodeId,
    pub(super) kind: NativeKind,
}

#[derive(Debug)]
pub(super) struct Storage<L> {
    kinds: NativeRows<KindRecord, L>,
    records: NativeRows<Record, L>,
    alignments: NativeRows<AlignmentRecord, L>,
}

impl<L> Default for Storage<L> {
    fn default() -> Self {
        Self {
            kinds: NativeRows::default(),
            records: NativeRows::default(),
            alignments: NativeRows::default(),
        }
    }
}

fn contains(outer: RSpan, inner: RSpan) -> bool {
    outer.buffer == inner.buffer
        && outer.start <= inner.start
        && inner.start <= inner.end
        && inner.end <= outer.end
}

impl<L> Storage<L> {
    pub(super) fn mark_retained(&self) {
        let Self {
            kinds: _,
            records: _,
            alignments: _,
        } = self;
        fn flat<T: Copy>() {}
        flat::<KindRecord>();
        flat::<Record>();
        flat::<AlignmentRecord>();
    }
    pub(super) fn kind<E>(
        &self,
        node: NativeNodeId,
        mut charge: impl FnMut(usize) -> Result<(), E>,
    ) -> Result<Option<NativeKind>, E> {
        let mut low = 0;
        let mut high = self.kinds.len();
        while low < high {
            let middle = low + (high - low) / 2;
            let Some(row) = self.kinds.get(middle, &mut charge)?.copied() else {
                return Ok(None);
            };
            charge(std::mem::size_of::<KindRecord>())?;
            match row.node.get().cmp(&node.get()) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Ok(Some(row.kind)),
            }
        }
        Ok(None)
    }

    pub(super) fn note_kind<H: Host<Lease = L>>(
        &mut self,
        node: NativeNodeId,
        kind: NativeKind,
        host: &mut H,
    ) -> Result<(), H::Error> {
        if !matches!(
            kind,
            NativeKind::Table
                | NativeKind::TableRow
                | NativeKind::TableCell
                | NativeKind::TaskCheckbox
        ) {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        if self.kinds.len() != 0 {
            let previous = self
                .kinds
                .get(self.kinds.len() - 1, |n| charge_host(host, n))?
                .copied()
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            host.work(WorkDelta::Scan(std::mem::size_of::<KindRecord>() as u64))?;
            if previous.node.get() >= node.get() {
                return Err(host.reject(NativeViolation::InvalidProvenance));
            }
        }
        host.work(WorkDelta::Copy(std::mem::size_of::<KindRecord>() as u64))?;
        self.kinds.push(KindRecord { node, kind }, host)
    }

    pub(super) fn get<E>(
        &self,
        index: usize,
        mut charge: impl FnMut(usize) -> Result<(), E>,
    ) -> Result<Option<Record>, E> {
        let row = self.records.get(index, &mut charge)?.copied();
        if row.is_some() {
            charge(std::mem::size_of::<Record>())?;
        }
        Ok(row)
    }

    pub(super) fn find<E>(
        &self,
        node: NativeNodeId,
        mut charge: impl FnMut(usize) -> Result<(), E>,
    ) -> Result<Option<(usize, Record)>, E> {
        let mut low = 0;
        let mut high = self.records.len();
        while low < high {
            let middle = low + (high - low) / 2;
            let Some(row) = self.get(middle, &mut charge)? else {
                return Ok(None);
            };
            match row.node().get().cmp(&node.get()) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Ok(Some((middle, row))),
            }
        }
        Ok(None)
    }

    pub(super) fn alignment<E>(
        &self,
        table_index: usize,
        column: usize,
        mut charge: impl FnMut(usize) -> Result<(), E>,
    ) -> Result<Option<AlignmentRecord>, E> {
        let Some(Record::Table {
            alignment_start,
            alignment_count,
            ..
        }) = self.get(table_index, &mut charge)?
        else {
            return Ok(None);
        };
        if column >= alignment_count as usize {
            return Ok(None);
        }
        let Some(index) = alignment_start.checked_add(column) else {
            return Ok(None);
        };
        let row = self.alignments.get(index, &mut charge)?.copied();
        if row.is_some() {
            charge(std::mem::size_of::<AlignmentRecord>())?;
        }
        Ok(row)
    }

    fn required<H: Host<Lease = L>>(
        &self,
        node: NativeNodeId,
        host: &mut H,
    ) -> Result<(usize, Record), H::Error> {
        self.find(node, |n| charge_host(host, n))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))
    }

    fn replace<H: Host<Lease = L>>(
        &mut self,
        index: usize,
        row: Record,
        host: &mut H,
    ) -> Result<(), H::Error> {
        host.work(WorkDelta::Copy(std::mem::size_of::<Record>() as u64))?;
        *self
            .records
            .get_mut(index, |n| charge_host(host, n))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))? = row;
        Ok(())
    }

    fn insert<H: Host<Lease = L>>(&mut self, row: Record, host: &mut H) -> Result<(), H::Error> {
        if self.records.len() != 0 {
            let previous = self
                .get(self.records.len() - 1, |n| charge_host(host, n))?
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            if previous.node().get() >= row.node().get() {
                return Err(host.reject(NativeViolation::InvalidProvenance));
            }
        }
        host.work(WorkDelta::Copy(std::mem::size_of::<Record>() as u64))?;
        self.records.push(row, host)
    }

    /// Caller first validates real node kinds/ancestry and buffer ownership.
    /// This checks the flat relation/order invariants without borrowing Context.
    pub(super) fn accept<H: Host<Lease = L>>(
        &mut self,
        fact: Fact,
        host: &mut H,
    ) -> Result<(), H::Error> {
        let (node, expected) = match fact {
            Fact::Table { node, .. } | Fact::Alignment { table: node, .. } => {
                (node, NativeKind::Table)
            }
            Fact::Row { node, .. } => (node, NativeKind::TableRow),
            Fact::Cell { node, .. } => (node, NativeKind::TableCell),
            Fact::Task { node, .. } => (node, NativeKind::TaskCheckbox),
        };
        if self.kind(node, |n| charge_host(host, n))? != Some(expected) {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        match fact {
            Fact::Table {
                node,
                columns,
                delimiter,
            } => {
                if columns == 0 || delimiter.start >= delimiter.end {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                }
                self.insert(
                    Record::Table {
                        node,
                        columns,
                        delimiter,
                        alignment_start: self.alignments.len(),
                        alignment_count: 0,
                        rows: 0,
                    },
                    host,
                )
            }
            Fact::Alignment {
                table,
                column,
                value,
                delimiter_cell,
            } => {
                let (index, row) = self.required(table, host)?;
                let Record::Table {
                    node,
                    columns,
                    delimiter,
                    alignment_start,
                    alignment_count,
                    rows,
                } = row
                else {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                };
                let expected = alignment_start
                    .checked_add(alignment_count as usize)
                    .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
                if rows != 0
                    || column != alignment_count
                    || column >= columns
                    || self.alignments.len() != expected
                    || !contains(delimiter, delimiter_cell)
                    || delimiter_cell.start == delimiter_cell.end
                {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                }
                if column != 0 {
                    let previous = self
                        .alignment(index, column as usize - 1, |n| charge_host(host, n))?
                        .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
                    if previous.delimiter_cell.end >= delimiter_cell.start {
                        return Err(host.reject(NativeViolation::InvalidProvenance));
                    }
                }
                host.work(WorkDelta::Copy(
                    std::mem::size_of::<AlignmentRecord>() as u64
                ))?;
                self.alignments.push(
                    AlignmentRecord {
                        value,
                        delimiter_cell,
                    },
                    host,
                )?;
                self.replace(
                    index,
                    Record::Table {
                        node,
                        columns,
                        delimiter,
                        alignment_start,
                        alignment_count: alignment_count + 1,
                        rows,
                    },
                    host,
                )
            }
            Fact::Row {
                node,
                table,
                index,
                header,
                raw,
            } => {
                let (table_index, record) = self.required(table, host)?;
                let Record::Table {
                    node: table_node,
                    columns,
                    delimiter,
                    alignment_start,
                    alignment_count,
                    rows,
                } = record
                else {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                };
                if alignment_count != columns
                    || index != rows
                    || header != (index == 0)
                    || raw.start > raw.end
                {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                }
                let next = rows
                    .checked_add(1)
                    .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
                self.insert(
                    Record::Row {
                        node,
                        table,
                        index,
                        header,
                        raw,
                        cells: 0,
                    },
                    host,
                )?;
                self.replace(
                    table_index,
                    Record::Table {
                        node: table_node,
                        columns,
                        delimiter,
                        alignment_start,
                        alignment_count,
                        rows: next,
                    },
                    host,
                )
            }
            Fact::Cell {
                node,
                row,
                column,
                missing,
                raw,
                content,
            } => {
                let (row_index, record) = self.required(row, host)?;
                let Record::Row {
                    node: row_node,
                    table,
                    index,
                    header,
                    raw: row_raw,
                    cells,
                } = record
                else {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                };
                let (_, table_record) = self.required(table, host)?;
                let Record::Table { columns, .. } = table_record else {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                };
                if column != cells
                    || column >= columns
                    || !contains(row_raw, raw)
                    || content == raw.buffer
                    || (missing && (raw.start != raw.end || header))
                {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                }
                self.insert(
                    Record::Cell {
                        node,
                        row,
                        column,
                        missing,
                        raw,
                        content,
                    },
                    host,
                )?;
                self.replace(
                    row_index,
                    Record::Row {
                        node: row_node,
                        table,
                        index,
                        header,
                        raw: row_raw,
                        cells: cells + 1,
                    },
                    host,
                )
            }
            Fact::Task {
                node,
                item,
                paragraph,
                checked,
                marker,
                spacer,
            } => {
                if node == item
                    || node == paragraph
                    || item == paragraph
                    || marker.start >= marker.end
                    || spacer.start >= spacer.end
                    || marker.buffer != spacer.buffer
                    || marker.end != spacer.start
                {
                    return Err(host.reject(NativeViolation::InvalidProvenance));
                }
                self.insert(
                    Record::Task {
                        node,
                        item,
                        paragraph,
                        checked,
                        marker,
                        spacer,
                    },
                    host,
                )
            }
        }
    }

    /// Run before the final provenance view can publish. Missing columns never
    /// turn into defaults, and an unfinished row is not a valid accepted table.
    pub(super) fn validate<H: Host<Lease = L>>(&self, host: &mut H) -> Result<(), H::Error> {
        self.kinds.visit(host, |row, host| {
            host.work(WorkDelta::Scan(std::mem::size_of::<KindRecord>() as u64))?;
            if self.find(row.node, |n| charge_host(host, n))?.is_none() {
                return Err(host.reject(NativeViolation::InvalidProvenance));
            }
            Ok(())
        })?;
        self.records.visit(host, |row, host| {
            host.work(WorkDelta::Scan(std::mem::size_of::<Record>() as u64))?;
            match *row {
                Record::Table {
                    columns,
                    alignment_count,
                    rows,
                    ..
                } if columns != alignment_count || rows == 0 => {
                    Err(host.reject(NativeViolation::InvalidProvenance))
                }
                Record::Row { table, cells, .. } => {
                    let (_, record) = self.required(table, host)?;
                    if !matches!(record, Record::Table { columns, .. } if columns == cells) {
                        return Err(host.reject(NativeViolation::InvalidProvenance));
                    }
                    Ok(())
                }
                _ => Ok(()),
            }
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::native::{AcceptedFact, NativeAdmission};
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum Error {
        Violation(NativeViolation),
        Cancelled,
    }

    pub(crate) struct Lease {
        live: Arc<AtomicU64>,
        bytes: u64,
    }
    impl Drop for Lease {
        fn drop(&mut self) {
            self.live.fetch_sub(self.bytes, Ordering::SeqCst);
        }
    }
    pub(crate) struct TestHost {
        pub(crate) live: Arc<AtomicU64>,
        pub(crate) first: Option<Error>,
        pub(crate) facts: usize,
        calls: usize,
        stop_at: Option<usize>,
    }
    impl TestHost {
        pub(crate) fn new() -> Self {
            Self {
                live: Arc::new(AtomicU64::new(0)),
                first: None,
                facts: 0,
                calls: 0,
                stop_at: None,
            }
        }
        fn checkpoint(&mut self) -> Result<(), Error> {
            if let Some(error) = self.first {
                return Err(error);
            }
            self.calls += 1;
            if self.stop_at == Some(self.calls) {
                self.first = Some(Error::Cancelled);
                return Err(Error::Cancelled);
            }
            Ok(())
        }
    }
    impl Host for TestHost {
        type Error = Error;
        type Lease = Lease;
        fn work(&mut self, _: WorkDelta) -> Result<(), Error> {
            self.checkpoint()
        }
        fn admit(&mut self, _: NativeAdmission) -> Result<(), Error> {
            self.checkpoint()
        }
        fn reserve(&mut self, bytes: u64) -> Result<Lease, Error> {
            self.checkpoint()?;
            self.live.fetch_add(bytes, Ordering::SeqCst);
            Ok(Lease {
                live: self.live.clone(),
                bytes,
            })
        }
        fn admit_actual(&mut self, lease: &mut Lease, bytes: u64) -> Result<(), Error> {
            self.checkpoint()?;
            if bytes >= lease.bytes {
                self.live.fetch_add(bytes - lease.bytes, Ordering::SeqCst);
            } else {
                self.live.fetch_sub(lease.bytes - bytes, Ordering::SeqCst);
            }
            lease.bytes = bytes;
            Ok(())
        }
        fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Error> {
            self.checkpoint()?;
            self.facts += 1;
            Ok(())
        }
        fn reject(&mut self, violation: NativeViolation) -> Error {
            *self.first.get_or_insert(Error::Violation(violation))
        }
    }

    fn table(host: &mut TestHost) -> Storage<Lease> {
        let mut store = Storage::default();
        let node = NativeNodeId::from_raw(1).unwrap();
        store.note_kind(node, NativeKind::Table, host).unwrap();
        store
            .accept(
                Fact::Table {
                    node,
                    columns: 2,
                    delimiter: RSpan {
                        buffer: BufferId::from_raw(1).unwrap(),
                        start: 17,
                        end: 27,
                    },
                },
                host,
            )
            .unwrap();
        store
    }

    fn first() -> Fact {
        Fact::Alignment {
            table: NativeNodeId::from_raw(1).unwrap(),
            column: 0,
            value: Alignment::Left,
            delimiter_cell: RSpan {
                buffer: BufferId::from_raw(1).unwrap(),
                start: 17,
                end: 20,
            },
        }
    }

    #[test]
    fn g3_alignment_witnesses_keep_nonzero_base_and_column_order() {
        let mut host = TestHost::new();
        let mut store = table(&mut host);
        store.accept(first(), &mut host).unwrap();
        store
            .accept(
                Fact::Alignment {
                    table: NativeNodeId::from_raw(1).unwrap(),
                    column: 1,
                    value: Alignment::Right,
                    delimiter_cell: RSpan {
                        buffer: BufferId::from_raw(1).unwrap(),
                        start: 23,
                        end: 26,
                    },
                },
                &mut host,
            )
            .unwrap();
        let first = store
            .alignment(0, 0, |n| charge_host(&mut host, n))
            .unwrap()
            .unwrap();
        let second = store
            .alignment(0, 1, |n| charge_host(&mut host, n))
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                first.value,
                first.delimiter_cell.start,
                first.delimiter_cell.end
            ),
            (Alignment::Left, 17, 20)
        );
        assert_eq!(
            (
                second.value,
                second.delimiter_cell.start,
                second.delimiter_cell.end
            ),
            (Alignment::Right, 23, 26)
        );
        assert!(
            store
                .alignment(0, 2, |n| charge_host(&mut host, n))
                .unwrap()
                .is_none()
        );
        // An accepted table without its mandatory header row cannot finalize.
        assert_eq!(
            store.validate(&mut host),
            Err(Error::Violation(NativeViolation::InvalidProvenance))
        );
        drop(store);
        assert_eq!(host.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn g3_alignment_rejects_duplicate_skipped_column_and_wrong_buffer() {
        for bad in [
            first(),
            Fact::Alignment {
                table: NativeNodeId::from_raw(1).unwrap(),
                column: 2,
                value: Alignment::Right,
                delimiter_cell: RSpan {
                    buffer: BufferId::from_raw(1).unwrap(),
                    start: 23,
                    end: 26,
                },
            },
            Fact::Alignment {
                table: NativeNodeId::from_raw(1).unwrap(),
                column: 1,
                value: Alignment::Right,
                delimiter_cell: RSpan {
                    buffer: BufferId::from_raw(2).unwrap(),
                    start: 23,
                    end: 26,
                },
            },
            Fact::Alignment {
                table: NativeNodeId::from_raw(1).unwrap(),
                column: 1,
                value: Alignment::Right,
                delimiter_cell: RSpan {
                    buffer: BufferId::from_raw(1).unwrap(),
                    start: 24,
                    end: 28,
                },
            },
        ] {
            let mut host = TestHost::new();
            let mut store = table(&mut host);
            store.accept(first(), &mut host).unwrap();
            assert_eq!(
                store.accept(bad, &mut host),
                Err(Error::Violation(NativeViolation::InvalidProvenance))
            );
            assert_eq!(
                host.first,
                Some(Error::Violation(NativeViolation::InvalidProvenance))
            );
            drop(store);
            assert_eq!(host.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn g3_alignment_growth_cancellation_releases_real_page_leases() {
        let mut failures = 0;
        for offset in 1..=128 {
            let mut host = TestHost::new();
            let mut store = table(&mut host);
            host.stop_at = Some(host.calls + offset);
            let result = store.accept(first(), &mut host);
            if let Err(error) = result {
                assert_eq!(error, Error::Cancelled);
                assert_eq!(
                    host.reject(NativeViolation::InternalInvariant),
                    Error::Cancelled
                );
                failures += 1;
            }
            drop(store);
            assert_eq!(host.live.load(Ordering::SeqCst), 0);
            if result.is_ok() {
                break;
            }
        }
        assert!(
            failures > 5,
            "cover lookup, page/directory growth and parent update"
        );
    }
}
