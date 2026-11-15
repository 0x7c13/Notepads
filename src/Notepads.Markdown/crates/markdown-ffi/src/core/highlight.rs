// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Immutable lexical metadata only. No shared layout Operation or third-party ABI.
use super::*;
use markdown_core::highlight::{
    self, BlockStatus, FailureReason, HighlightCache, HighlightSnapshot, Language, SemanticKind,
    SkipReason, SnapshotStatus,
};

pub const HIGHLIGHT_ABI_VERSION: u32 = 1;
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct HighlightHeader {
    pub abi_version: u32,
    pub struct_size: u32,
}
impl HighlightHeader {
    fn of<T>() -> Self {
        Self {
            abi_version: HIGHLIGHT_ABI_VERSION,
            struct_size: size_of::<T>() as u32,
        }
    }
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct HighlightBlockWire {
    pub node: u32,
    pub state: u32,
    pub reason: u32,
    pub language: u32,
    pub logical_start: u64,
    pub logical_end: u64,
    pub spans_start: u32,
    pub spans_count: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct HighlightSpanWire {
    pub start_utf16: u32,
    pub length_utf16: u32,
    pub role: u32,
    pub reserved: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct HighlightSummaryWire {
    pub header: HighlightHeader,
    pub model: WireIdentity,
    pub blocks: u32,
    pub spans: u32,
    pub complete_blocks: u32,
    pub skipped_blocks: u32,
    pub failed_blocks: u32,
    pub entry_limited: u32,
    pub participating_utf8: u64,
    pub external_events: u64,
}
const _: () = {
    assert!(size_of::<HighlightBlockWire>() == 40 && size_of::<HighlightSpanWire>() == 16);
    assert!(size_of::<HighlightSummaryWire>() == 88);
    assert!(std::mem::offset_of!(HighlightSummaryWire, external_events) == 80);
};
pub struct HighlightResult {
    snapshot: HighlightSnapshot,
    poisoned: bool,
}
pub struct HighlightCacheResult {
    cache: HighlightCache,
    poisoned: bool,
}
impl HighlightResult {
    fn operation(&self) -> Result<Operation, CoreError> {
        if self.poisoned {
            return Err(CoreError::InvalidState);
        }
        self.snapshot.model().budget().begin_cancellable(false)
    }
    fn summary(&self) -> HighlightSummaryWire {
        let value = self.snapshot.summary();
        HighlightSummaryWire {
            header: HighlightHeader::of::<HighlightSummaryWire>(),
            model: wire_identity(self.snapshot.identity()),
            blocks: self.snapshot.rows().len() as u32,
            spans: self.snapshot.spans().len() as u32,
            complete_blocks: value.complete_blocks,
            skipped_blocks: value.skipped_blocks,
            failed_blocks: value.failed_blocks,
            entry_limited: u32::from(value.status == SnapshotStatus::EntryLimit),
            participating_utf8: value.participating_utf8,
            external_events: value.external_events,
        }
    }
}
unsafe fn highlight_header<T>(pointer: *const T) -> Result<(), i32> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: caller supplies an aligned, readable eight-byte prefix.
    let prefix = unsafe { pointer.cast::<HighlightHeader>().read() };
    if prefix.abi_version != HIGHLIGHT_ABI_VERSION || prefix.struct_size as usize != size_of::<T>()
    {
        return Err(STATUS_ABI_MISMATCH);
    }
    Ok(())
}
fn block_wire(value: &highlight::HighlightBlock) -> HighlightBlockWire {
    let (state, reason) = match value.status {
        BlockStatus::Pending => (0, 0),
        BlockStatus::Complete => (1, 0),
        BlockStatus::Skipped(reason) => (
            2,
            match reason {
                SkipReason::UnknownLanguage => 1,
                SkipReason::BlockBytes => 2,
                SkipReason::BlockLines => 3,
                SkipReason::LineBytes => 4,
                SkipReason::DocumentBytes => 5,
                SkipReason::SpanCount => 6,
            },
        ),
        BlockStatus::Failed(reason) => (
            3,
            match reason {
                FailureReason::RegexCompile => 1,
                FailureReason::RegexSearch => 2,
                FailureReason::Grammar => 3,
                FailureReason::Scope => 4,
            },
        ),
    };
    HighlightBlockWire {
        node: value.node.value(),
        state,
        reason,
        language: match value.language {
            None => 0,
            Some(Language::Cpp) => 1,
            Some(Language::Rust) => 2,
            Some(Language::Json) => 3,
            Some(Language::JavaScript) => 4,
            Some(Language::Python) => 5,
            Some(Language::Yaml) => 6,
            Some(Language::Shell) => 7,
            Some(Language::CSharp) => 8,
            Some(Language::Html) => 9,
            Some(Language::Css) => 10,
            Some(Language::Sql) => 11,
            Some(Language::Diff) => 12,
            Some(Language::Go) => 13,
            Some(Language::Java) => 14,
            Some(Language::Ruby) => 15,
            Some(Language::Xml) => 16,
            Some(Language::Lua) => 17,
            Some(Language::Makefile) => 18,
            Some(Language::C) => 19,
            Some(Language::R) => 20,
            Some(Language::Scala) => 21,
            Some(Language::Markdown) => 22,
            Some(Language::Batchfile) => 23,
            Some(Language::Perl) => 24,
            Some(Language::Haskell) => 25,
            Some(Language::ObjectiveC) => 26,
            Some(Language::ObjectiveCpp) => 27,
            Some(Language::Php) => 28,
            Some(Language::HtmlPhp) => 29,
            Some(Language::Tex) => 30,
            Some(Language::Clojure) => 31,
            Some(Language::Erlang) => 32,
            Some(Language::Groovy) => 33,
            Some(Language::Ocaml) => 34,
            Some(Language::CommonLisp) => 35,
            Some(Language::Tcl) => 36,
            Some(Language::JavaProperties) => 37,
            Some(Language::Graphviz) => 38,
            Some(Language::Matlab) => 39,
            Some(Language::Pascal) => 40,
            Some(Language::RestructuredText) => 41,
            Some(Language::AppleScript) => 42,
            Some(Language::D) => 43,
            Some(Language::Haml) => 44,
            Some(Language::HtmlErb) => 45,
            Some(Language::TypeScript) => 46,
            Some(Language::TypeScriptReact) => 47,
            Some(Language::PowerShell) => 48,
            Some(Language::Toml) => 49,
            Some(Language::Dockerfile) => 50,
        },
        logical_start: value.logical.start(),
        logical_end: value.logical.end(),
        spans_start: value.spans.start,
        spans_count: value.spans.len,
    }
}
fn span_wire(value: &highlight::TokenSpan) -> HighlightSpanWire {
    HighlightSpanWire {
        start_utf16: value.start_utf16,
        length_utf16: value.length_utf16,
        role: match value.kind {
            SemanticKind::Plain => 0,
            SemanticKind::Comment => 1,
            SemanticKind::String => 2,
            SemanticKind::Number => 3,
            SemanticKind::Keyword => 4,
            SemanticKind::Type => 5,
            SemanticKind::Function => 6,
            SemanticKind::Added => 7,
            SemanticKind::Removed => 8,
        },
        reserved: 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_highlight_abi_version() -> u32 {
    HIGHLIGHT_ABI_VERSION
}
/// # Safety
/// Live shared model/cancel, independent writable handle and ABI3 error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_highlight_cache_create(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    output: *mut *mut Owned<HighlightCacheResult>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model) && valid_handle(cancel));
    // SAFETY: preflighted live handles and writable output; action checks error prefix.
    unsafe {
        action(error, || {
            let model = &(*model).value.model;
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            let mut op = cancel.budget.begin(cancel.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            let cache = HighlightCache::new(model);
            op.boundary_work().checkpoint_now()?;
            output.write(slot.commit(HighlightCacheResult {
                cache,
                poisoned: false,
            }));
            Ok(())
        })
    }
}
/// # Safety
/// Shared live model/cancel and exclusive cache; sorted node array and all output
/// regions are aligned, independent and valid for their advertised sizes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_highlight_build_selected(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    nodes: *const u32,
    count: u32,
    cache: *mut Owned<HighlightCacheResult>,
    output: *mut *mut Owned<HighlightResult>,
    reused: *mut u32,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model) && valid_handle(cancel) && valid_handle(cache));
    preflight!((1..=highlight::MAX_BLOCKS as u32).contains(&count));
    preflight!(!nodes.is_null() && (nodes as usize).is_multiple_of(align_of::<u32>()));
    preflight!(!reused.is_null() && (reused as usize).is_multiple_of(align_of::<u32>()));
    // SAFETY: validated shapes; caller supplies live handles, array and independent
    // full writable outputs. Core checks node order/kind and the cache's budget root.
    let status = unsafe {
        action(error, || {
            let cache = &mut (*cache).value;
            if cache.poisoned {
                return Err(CoreError::InvalidState);
            }
            let model = (*model).value.model.clone();
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            let mut op = cancel.budget.begin(cancel.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            let nodes = std::slice::from_raw_parts(nodes, count as usize);
            let snapshot =
                HighlightSnapshot::build_selected(model, nodes, &mut cache.cache, &mut op)?;
            if let SnapshotStatus::Aborted(error) = snapshot.summary().status {
                return Err(error);
            }
            let reused_blocks = snapshot.summary().reused_blocks;
            op.external_write(size_of::<u32>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(slot.commit(HighlightResult {
                snapshot,
                poisoned: false,
            }));
            reused.write(reused_blocks);
            Ok(())
        })
    };
    if status == STATUS_PANIC {
        // SAFETY: cache stays exclusively borrowed through the complete call.
        unsafe {
            (*cache).value.poisoned = true;
        }
    }
    status
}
/// # Safety
/// Live shared model/cancel, independent writable handle and ABI3 error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_highlight_build(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    output: *mut *mut Owned<HighlightResult>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model) && valid_handle(cancel));
    // SAFETY: preflighted live handles and writable output; action checks error prefix.
    unsafe {
        action(error, || {
            let model = (*model).value.model.clone();
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            // Dedicated highlighter operation. Its first fault never enters layout.
            let mut op = cancel.budget.begin(cancel.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            let snapshot = HighlightSnapshot::build(model, &mut op)?;
            // The ABI is narrower than the core diagnostic representation: an
            // aborted source build exports no handle and no earlier partial cache.
            if let SnapshotStatus::Aborted(error) = snapshot.summary().status {
                return Err(error);
            }
            op.boundary_work().checkpoint_now()?;
            output.write(slot.commit(HighlightResult {
                snapshot,
                poisoned: false,
            }));
            Ok(())
        })
    }
}
/// # Safety
/// Exclusive live result, prefix-initialized full summary/error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_highlight_get_summary(
    handle: *mut Owned<HighlightResult>,
    output: *mut HighlightSummaryWire,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    if let Err(code) = unsafe { highlight_header(output) } {
        return code;
    }
    // SAFETY: exclusive valid result and nonoverlapping full output regions.
    let status = unsafe {
        action(error, || {
            let value = &(*handle).value;
            let mut op = value.operation()?;
            op.external_write(size_of::<HighlightSummaryWire>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(value.summary());
            Ok(())
        })
    };
    if status == STATUS_PANIC {
        unsafe {
            (*handle).value.poisoned = true;
        }
    }
    status
}
/// # Safety
/// Exclusive result and independent aligned output arrays with stated capacities.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_highlight_copy(
    handle: *mut Owned<HighlightResult>,
    blocks: *mut HighlightBlockWire,
    block_capacity: u32,
    block_size: u32,
    spans: *mut HighlightSpanWire,
    span_capacity: u32,
    span_size: u32,
    output: *mut HighlightSummaryWire,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    if let Err(code) = unsafe { highlight_header(output) } {
        return code;
    }
    if block_size as usize != size_of::<HighlightBlockWire>()
        || span_size as usize != size_of::<HighlightSpanWire>()
    {
        return STATUS_ABI_MISMATCH;
    }
    preflight!(
        block_capacity as usize <= highlight::MAX_BLOCKS
            && span_capacity as usize <= highlight::MAX_SPANS
    );
    preflight!(
        block_capacity == 0
            || (!blocks.is_null()
                && (blocks as usize).is_multiple_of(align_of::<HighlightBlockWire>()))
    );
    preflight!(
        span_capacity == 0
            || (!spans.is_null()
                && (spans as usize).is_multiple_of(align_of::<HighlightSpanWire>()))
    );
    // SAFETY: validated array shapes. Caller guarantees capacities, lifetime and exclusivity.
    let status = unsafe {
        action(error, || {
            let value = &(*handle).value;
            let mut op = value.operation()?;
            let rows = value.snapshot.rows();
            let tokens = value.snapshot.spans();
            for (required, limit) in [
                (rows.len(), block_capacity as usize),
                (tokens.len(), span_capacity as usize),
            ] {
                if required > limit {
                    return Err(CoreError::ResourceLimit {
                        kind: LimitKind::QueryCapacity,
                        requested: required as u64,
                        limit: limit as u64,
                    });
                }
            }
            let bytes = rows.len() * size_of::<HighlightBlockWire>()
                + tokens.len() * size_of::<HighlightSpanWire>();
            for start in (0..bytes).step_by(65_536) {
                op.external_read((bytes - start).min(65_536) as u64)?;
            }
            let writes = bytes + size_of::<HighlightSummaryWire>();
            for start in (0..writes).step_by(65_536) {
                op.external_write((writes - start).min(65_536) as u64)?;
            }
            for (index, row) in rows.iter().enumerate() {
                op.boundary_work().checkpoint_now()?;
                blocks.add(index).write(block_wire(row));
            }
            for (index, span) in tokens.iter().enumerate() {
                if index % 256 == 0 {
                    op.boundary_work().checkpoint_now()?;
                }
                spans.add(index).write(span_wire(span));
            }
            op.boundary_work().checkpoint_now()?;
            output.write(value.summary());
            Ok(())
        })
    };
    if status == STATUS_PANIC {
        unsafe {
            (*handle).value.poisoned = true;
        }
    }
    status
}
destroy!(notepads_markdown_highlight_destroy, HighlightResult);
destroy!(
    notepads_markdown_highlight_cache_destroy,
    HighlightCacheResult
);

#[cfg(test)]
mod tests;
