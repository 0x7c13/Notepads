// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Explicit POD wire layouts; no Rust enums or internal pointers.

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireHeader {
    pub abi_version: u32,
    pub struct_size: u32,
}
impl WireHeader {
    pub fn of<T>() -> Self {
        Self {
            abi_version: super::CORE_ABI_VERSION,
            struct_size: std::mem::size_of::<T>() as u32,
        }
    }
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireSource {
    pub document: [u8; 16],
    pub revision: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireIdentity {
    pub document: [u8; 16],
    pub revision: u64,
    pub generation: u64,
    pub profile: u32,
    pub extensions: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireError {
    pub header: WireHeader,
    pub status: i32,
    pub limit_kind: u32,
    pub requested: u64,
    pub limit: u64,
    pub expected: u64,
    pub actual: u64,
    pub offset: u64,
    pub edit_index: u64,
}
impl Default for WireError {
    fn default() -> Self {
        Self {
            header: WireHeader::of::<Self>(),
            status: 0,
            limit_kind: 0,
            requested: 0,
            limit: 0,
            expected: 0,
            actual: 0,
            offset: 0,
            edit_index: 0,
        }
    }
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireSnapshot {
    pub header: WireHeader,
    pub source: WireSource,
    pub profile: u32,
    pub reserved: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireSubmitResult {
    pub header: WireHeader,
    pub source: WireSource,
    pub state_valid: u32,
    pub has_source: u32,
    pub needs_resync: u32,
    pub source_accepted: u32,
    pub has_ready: u32,
    pub reserved: u32,
    pub ready: WireIdentity,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireEditBatch {
    pub header: WireHeader,
    pub before: WireSource,
    pub after: WireSource,
    pub count: u64,
    pub row_size: u32,
    pub reserved: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireEditRow {
    pub start: u64,
    pub end: u64,
    pub replacement_offset: u64,
    pub replacement_length: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireModelSummary {
    pub header: WireHeader,
    pub identity: WireIdentity,
    pub main_flow_id: u32,
    pub has_footnotes: u32,
    pub nodes: u64,
    pub flows: u64,
    pub objects: u64,
    pub payloads: u64,
    pub atoms: u64,
    pub main_utf16: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireReadResult {
    pub header: WireHeader,
    pub written: u64,
    pub next: u64,
    pub done: u32,
    pub reserved: u32,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireQuery {
    pub header: WireHeader,
    pub identity: WireIdentity,
    pub domain: u32,
    pub target_kind: u32,
    pub target_id: u32,
    pub affinity: u32,
    pub start: u64,
    pub end: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireRows {
    pub start: u32,
    pub count: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireSpan {
    pub domain: u32,
    pub reserved: u32,
    pub start: u64,
    pub end: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WirePosition {
    pub domain: u32,
    pub reserved: u32,
    pub value: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireRelated {
    pub spans: WireRows,
    pub anchors: WireRows,
    pub has_envelope: u32,
    pub reserved: u32,
    pub envelope: WireSpan,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WirePoint {
    pub target_kind: u32,
    pub target_id: u32,
    pub direction: u32,
    pub effects: u32,
    pub chosen: WirePosition,
    pub related: WireRelated,
    pub correspondence: u32,
    pub reserved: u32,
    pub contributors: WireRows,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireRange {
    pub kind: u32,
    pub reserved: u32,
    pub rows: WireRows,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireSelection {
    pub target_kind: u32,
    pub target_id: u32,
    pub direction: u32,
    pub effects: u32,
    pub expanded: u32,
    pub reserved: u32,
    pub start: WirePoint,
    pub end: WirePoint,
    pub related: WireRelated,
    pub contributions: WireRows,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireContributor {
    pub atom: u32,
    pub source_part: u32,
    pub kind: u32,
    pub correspondence: u32,
    pub source_start: u64,
    pub source_end: u64,
    pub target_start: u64,
    pub target_end: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireContribution {
    pub atom: u32,
    pub source_part: u32,
    pub kind: u32,
    pub expanded: u32,
    pub source_start: u64,
    pub source_end: u64,
    pub target_start: u64,
    pub target_end: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireQuerySummary {
    pub header: WireHeader,
    pub identity: WireIdentity,
    pub kind: u32,
    pub reserved: u32,
    pub points: u64,
    pub ranges: u64,
    pub selections: u64,
    pub spans: u64,
    pub anchors: u64,
    pub contributors: u64,
    pub contributions: u64,
}

macro_rules! layouts { ($($ty:ty=>$size:expr),* $(,)?) => { $(const _:()={assert!(std::mem::size_of::<$ty>()==$size);assert!(std::mem::align_of::<$ty>()==8);};)* }; }
layouts!(WireHeader=>8,WireSource=>24,WireIdentity=>40,WireError=>64,WireSnapshot=>40,WireModelSummary=>104,
    WireSubmitResult=>96,WireEditBatch=>72,WireEditRow=>32,WireReadResult=>32,WireQuery=>80,WireRows=>8,WireSpan=>24,WirePosition=>16,WireRelated=>48,
    WirePoint=>96,WireRange=>16,WireSelection=>272,WireContributor=>48,WireContribution=>48,WireQuerySummary=>112);

macro_rules! offsets { ($($ty:ty,$field:ident=>$offset:expr);* $(;)?) => { $(const _:()=assert!(std::mem::offset_of!($ty,$field)==$offset);)* }; }
offsets!(WireHeader,struct_size=>4;WireSource,revision=>16;WireIdentity,profile=>32;WireError,edit_index=>56;
    WireSnapshot,profile=>32;WireSubmitResult,source=>8;WireSubmitResult,state_valid=>32;WireSubmitResult,has_source=>36;
    WireSubmitResult,needs_resync=>40;WireSubmitResult,source_accepted=>44;WireSubmitResult,has_ready=>48;
    WireSubmitResult,reserved=>52;WireSubmitResult,ready=>56;WireEditBatch,before=>8;WireEditBatch,after=>32;
    WireEditBatch,count=>56;WireEditBatch,row_size=>64;WireEditBatch,reserved=>68;WireEditRow,start=>0;WireEditRow,end=>8;
    WireEditRow,replacement_offset=>16;WireEditRow,replacement_length=>24;
    WireModelSummary,main_utf16=>96;WireReadResult,done=>24;WireQuery,start=>64;
    WireRows,count=>4;WireSpan,end=>16;WirePosition,value=>8;WireRelated,envelope=>24;WirePoint,contributors=>88;
    WireRange,rows=>8;WireSelection,contributions=>264;WireContributor,target_end=>40;WireContribution,target_end=>40;
    WireQuerySummary,contributions=>104);
