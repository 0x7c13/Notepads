// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Per-call layout state. Kept separate from the public options so callers do
//! not retain caches or have to initialize implementation-specific fields.
use ratex_types::execution::{ExecutionControl, ExecutionError, Phase, Resource};
use ratex_types::storage::{StorageError, StorageFamily, StorageLedger};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ops::Deref,
    rc::Rc,
    sync::Arc,
};

use ratex_font::FontId;
use ratex_parser::ParseNode;
use ratex_types::{color::Color, math_style::MathStyle};

use crate::layout_options::{ChineseTextRun, ChineseTextStyle};
use crate::{FallbackGlyphMetrics, LayoutOptions};

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct DelimiterKey {
    content: String,
    style: MathStyle,
    color: [u32; 4],
    align_relation_spacing: Option<u64>,
    inter_glyph_kern_em: u64,
    explicit_size_multiplier: u64,
    brace_font: BraceFont,
    chinese_text_style: ChineseTextStyle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum BraceFont {
    #[default]
    Outside,
    Pending(FontId),
    Inside(Option<FontId>),
}

#[derive(Clone)]
pub(crate) struct ScopedOptions<'a> {
    pub(crate) execution: Option<&'a ExecutionControl<'a>>,
    pub(crate) storage: Option<&'a StorageLedger<'a>>,
    depth: Option<&'a Cell<u32>>,
    options: LayoutOptions,
    brace_font: BraceFont,
    pub(crate) chinese_text_runs: Option<&'a [ChineseTextRun<'a>]>,
    pub(crate) chinese_text_style: ChineseTextStyle,
    pub(crate) fallback_glyph_metrics: Option<Arc<HashMap<char, FallbackGlyphMetrics>>>,
    #[cfg(test)]
    pub(crate) disable_cache: bool,
    delimiters: DelimiterCache<'a>,
}

#[derive(Clone)]
enum DelimiterCache<'a> {
    Legacy(Rc<RefCell<HashMap<DelimiterKey, f64>>>),
    Bounded(&'a RefCell<Vec<(DelimiterKey, f64)>>),
}

impl Default for ScopedOptions<'_> {
    fn default() -> Self {
        Self {
            execution: None,
            storage: None,
            depth: None,
            options: LayoutOptions::default(),
            brace_font: BraceFont::Outside,
            chinese_text_runs: None,
            chinese_text_style: ChineseTextStyle::default(),
            fallback_glyph_metrics: None,
            #[cfg(test)]
            disable_cache: false,
            delimiters: DelimiterCache::Legacy(Rc::new(RefCell::new(HashMap::new()))),
        }
    }
}

impl Deref for ScopedOptions<'_> {
    type Target = LayoutOptions;
    fn deref(&self) -> &Self::Target {
        &self.options
    }
}

impl<'a> ScopedOptions<'a> {
    pub(crate) fn with_execution(mut self, control: &'a ExecutionControl<'a>) -> Self {
        self.execution = Some(control);
        self
    }
    pub(crate) fn checkpoint(&self) -> bool {
        if let Some(storage) = self.storage {
            return storage.check(Phase::Layout).is_ok();
        }
        self.execution
            .is_none_or(|control| control.checkpoint(Phase::Layout).is_ok())
    }

    pub(crate) fn new_bounded(
        options: LayoutOptions,
        storage: &'a StorageLedger<'a>,
        cache: &'a RefCell<Vec<(DelimiterKey, f64)>>,
        depth: &'a Cell<u32>,
    ) -> Self {
        Self {
            execution: Some(storage.control()),
            storage: Some(storage),
            depth: Some(depth),
            options,
            brace_font: BraceFont::Outside,
            chinese_text_runs: None,
            chinese_text_style: ChineseTextStyle::default(),
            fallback_glyph_metrics: None,
            #[cfg(test)]
            disable_cache: false,
            delimiters: DelimiterCache::Bounded(cache),
        }
    }

    pub(crate) fn unsupported(&self) {
        if let Some(control) = self.execution {
            control.fail(ExecutionError::Unsupported {
                phase: Phase::Layout,
            });
        }
    }

    pub(crate) fn enter_depth(&self) -> Option<LayoutDepth<'_>> {
        if !self.checkpoint() {
            return None;
        }
        if let Some(depth) = self.depth {
            let next = depth.get().checked_add(1)?;
            if next > 16 {
                self.storage
                    .expect("bounded depth has storage")
                    .fail(StorageError::DepthLimit {
                        phase: Phase::Layout,
                        limit: 16,
                        requested: next,
                    });
                return None;
            }
            depth.set(next);
        }
        Some(LayoutDepth { depth: self.depth })
    }

    pub(crate) fn push<T>(&self, family: StorageFamily, values: &mut Vec<T>, value: T) -> bool {
        if let Some(storage) = self.storage {
            if family == StorageFamily::Layout
                && storage
                    .control()
                    .consume(Phase::Layout, Resource::LayoutNodes, 1)
                    .is_err()
            {
                return false;
            }
            storage
                .try_push(Phase::Layout, family, values, value)
                .is_ok()
        } else {
            values.push(value);
            true
        }
    }

    pub(crate) fn boxed<T>(&self, value: T) -> Result<Box<T>, StorageError> {
        match self.storage {
            Some(storage) => {
                storage
                    .control()
                    .consume(Phase::Layout, Resource::LayoutNodes, 1)?;
                storage.try_box(Phase::Layout, StorageFamily::Layout, value)
            }
            None => Ok(Box::new(value)),
        }
    }

    pub(crate) fn boxed_optional<T>(
        &self,
        value: Option<T>,
    ) -> Result<Option<Box<T>>, StorageError> {
        value.map(|value| self.boxed(value)).transpose()
    }

    pub(crate) fn values<T, const N: usize>(
        &self,
        family: StorageFamily,
        values: [T; N],
    ) -> Result<Vec<T>, StorageError> {
        if let Some(storage) = self.storage {
            if family == StorageFamily::Layout {
                storage
                    .control()
                    .consume(Phase::Layout, Resource::LayoutNodes, N as u64)?;
            }
            let mut result = storage.try_vec_with_capacity(Phase::Layout, family, N)?;
            for value in values {
                result.push(value);
            }
            Ok(result)
        } else {
            Ok(Vec::from(values))
        }
    }

    pub(crate) fn copy_values<T: Copy>(
        &self,
        family: StorageFamily,
        values: &[T],
    ) -> Result<Vec<T>, StorageError> {
        if let Some(storage) = self.storage {
            let mut result = storage.try_vec_with_capacity(Phase::Layout, family, values.len())?;
            result.extend_from_slice(values);
            Ok(result)
        } else {
            Ok(values.to_vec())
        }
    }

    pub(crate) fn new(options: LayoutOptions) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    pub(crate) fn with_fallback_metrics(
        self,
        metrics: Arc<HashMap<char, FallbackGlyphMetrics>>,
    ) -> Self {
        Self {
            fallback_glyph_metrics: Some(metrics),
            ..self
        }
    }

    pub(crate) fn with_pending_brace_font(&self, font: Option<FontId>) -> Self {
        Self {
            brace_font: font.map_or(BraceFont::Outside, BraceFont::Pending),
            ..self.clone()
        }
    }

    pub(crate) fn with_brace_font(&self, font: Option<FontId>) -> Self {
        Self {
            brace_font: BraceFont::Inside(font),
            ..self.clone()
        }
    }

    /// An explicit TeX style declaration resets the current KaTeX math font.
    pub(crate) fn clear_brace_font(&mut self) {
        self.brace_font = if self.in_brace() {
            BraceFont::Inside(None)
        } else {
            BraceFont::Outside
        };
    }

    pub(crate) fn in_brace(&self) -> bool {
        matches!(self.brace_font, BraceFont::Inside(_))
    }

    pub(crate) fn brace_font(&self) -> Option<FontId> {
        match self.brace_font {
            BraceFont::Inside(font) => font,
            _ => None,
        }
    }

    pub(crate) fn enclosing_brace_font(&self) -> Option<FontId> {
        match self.brace_font {
            BraceFont::Pending(font) => Some(font),
            _ => self.brace_font(),
        }
    }

    pub(crate) fn with_chinese_text_style(&self, style: ChineseTextStyle) -> Self {
        Self {
            chinese_text_style: style,
            ..self.clone()
        }
    }

    pub(crate) fn with_style(&self, style: MathStyle) -> Self {
        Self {
            options: self.options.with_style(style),
            ..self.clone()
        }
    }

    pub(crate) fn with_color(&self, color: Color) -> Self {
        Self {
            options: self.options.with_color(color),
            ..self.clone()
        }
    }

    pub(crate) fn with_explicit_size_multiplier(&self, multiplier: f64) -> Self {
        Self {
            options: self.options.with_explicit_size_multiplier(multiplier),
            ..self.clone()
        }
    }

    pub(crate) fn with_middle_height(&self, height: Option<f64>) -> Self {
        let mut result = self.clone();
        result.options.leftright_delim_height = height;
        result
    }

    pub(crate) fn delimiter_key(
        &self,
        body: &[ParseNode],
        left: &str,
        right: &str,
    ) -> Result<DelimiterKey, StorageError> {
        // Content keys also work for temporary ASTs synthesized by HTML/text
        // and proof-tree layout; pointer keys could alias after those are freed.
        // A scope containing its own middle resets the inherited height. The
        // fallback metrics Arc is immutable and identical throughout this
        // per-call cache, so neither serialize nor hash its potentially huge map.
        // Keep this exhaustive so new public options require a cache-key decision.
        let LayoutOptions {
            style,
            color,
            align_relation_spacing,
            leftright_delim_height: _,
            inter_glyph_kern_em,
            explicit_size_multiplier,
        } = &self.options;
        let content = if let Some(storage) = self.storage {
            storage.check(Phase::Layout)?;
            // All reached AST fields use derive Serialize over finite owned nodes,
            // strings/primitives. No custom serializer or non-string map key is admitted.
            // The sticky writer cannot cause an IO error/boxed serde ErrorImpl allocation;
            // on failure it drains without appending, then the original error wins here.
            let mut writer = crate::bounded_writer::BoundedWriter::new(storage, 32 * 1024);
            let result = serde_json::to_writer(&mut writer, &(body, left, right));
            storage.check(Phase::Layout)?;
            if result.is_err() {
                return Err(storage.fail(StorageError::Execution(storage.control().fail(
                    ExecutionError::InternalInvariant {
                        phase: Phase::Layout,
                    },
                ))));
            }
            writer.finish()?
        } else {
            serde_json::to_string(&(body, left, right)).expect("serializable AST")
        };
        Ok(DelimiterKey {
            content,
            style: *style,
            color: [color.r, color.g, color.b, color.a].map(f32::to_bits),
            align_relation_spacing: align_relation_spacing.map(f64::to_bits),
            inter_glyph_kern_em: inter_glyph_kern_em.to_bits(),
            explicit_size_multiplier: explicit_size_multiplier.to_bits(),
            brace_font: self.brace_font,
            chinese_text_style: self.chinese_text_style,
        })
    }

    pub(crate) fn cached_delimiter_height(&self, key: &DelimiterKey) -> Option<f64> {
        #[cfg(test)]
        if self.disable_cache {
            return None;
        }
        match &self.delimiters {
            DelimiterCache::Legacy(cache) => cache.borrow().get(key).copied(),
            DelimiterCache::Bounded(cache) => cache
                .borrow()
                .iter()
                .find(|(entry, _)| entry == key)
                .map(|(_, value)| *value),
        }
    }

    pub(crate) fn cache_delimiter_height(&self, key: DelimiterKey, height: f64) {
        #[cfg(test)]
        if self.disable_cache {
            return;
        }
        // Retain measurements, not copies of public owned subtrees. Reusing
        // the first-pass result prevents exponential nested middle measurement
        // without retaining a large expanded tree at every enclosing scope.
        match &self.delimiters {
            DelimiterCache::Legacy(cache) => {
                cache.borrow_mut().insert(key, height);
            }
            DelimiterCache::Bounded(cache) => {
                let storage = self.storage.expect("bounded cache has explicit storage");
                if !self.checkpoint() {
                    return;
                }
                let mut entries = cache.borrow_mut();
                if let Some((_, value)) = entries.iter_mut().find(|(entry, _)| entry == &key) {
                    *value = height;
                    return;
                }
                let bytes = entries
                    .iter()
                    .try_fold(key.content.capacity(), |sum, (key, _)| {
                        sum.checked_add(key.content.capacity())
                    });
                if entries.len() >= 64 || bytes.is_none_or(|n| n > 2 * 1024 * 1024) {
                    storage.fail(StorageError::FamilyBytes {
                        family: StorageFamily::Strings,
                        limit: 2 * 1024 * 1024,
                        requested: bytes.unwrap_or(usize::MAX) as u64,
                    });
                    return;
                }
                let _ = storage.try_push(
                    Phase::Layout,
                    StorageFamily::Metadata,
                    &mut entries,
                    (key, height),
                );
            }
        }
    }
}

pub(crate) struct LayoutDepth<'a> {
    depth: Option<&'a Cell<u32>>,
}
impl Drop for LayoutDepth<'_> {
    fn drop(&mut self) {
        if let Some(depth) = self.depth {
            depth.set(depth.get() - 1);
        }
    }
}

#[cfg(test)]
mod fixup_key_tests {
    use super::*;
    use ratex_types::execution::{Limits, Observer};
    use std::sync::atomic::{AtomicBool, Ordering};
    #[test]
    fn fixup_key_quota_does_not_allocate_an_io_error_or_publish_an_empty_cache_key() {
        let ast = ratex_parser::parse(&"x".repeat(600)).unwrap();
        let cancel = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancel, None, Limits::default());
        let storage = StorageLedger::new(&control);
        let cache = RefCell::new(Vec::new());
        let depth = Cell::new(0);
        let options =
            ScopedOptions::new_bounded(LayoutOptions::default(), &storage, &cache, &depth);
        let result = options.delimiter_key(&ast, "(", ")");
        assert!(matches!(
            result,
            Err(StorageError::FamilyBytes {
                family: StorageFamily::Strings,
                limit: 32768,
                ..
            })
        ));
        assert!(cache.borrow().is_empty());
        assert!(matches!(
            storage.failure(),
            Some(StorageError::FamilyBytes { limit: 32768, .. })
        ));
    }
    #[test]
    fn fixup_cancellation_during_derive_serialization_retains_the_first_error_and_no_key() {
        struct CancelAt<'a>(&'a AtomicBool);
        impl Observer for CancelAt<'_> {
            fn checkpoint(&self, phase: Phase, ordinal: u64) {
                if phase == Phase::Layout && ordinal == 10 {
                    self.0.store(true, Ordering::Release);
                }
            }
        }
        let ast = ratex_parser::parse(&"x".repeat(600)).unwrap();
        let cancel = AtomicBool::new(false);
        let observer = CancelAt(&cancel);
        let control =
            ExecutionControl::new(&cancel, None, Limits::default()).with_observer(&observer);
        let storage = StorageLedger::new(&control);
        let cache = RefCell::new(Vec::new());
        let depth = Cell::new(0);
        let options =
            ScopedOptions::new_bounded(LayoutOptions::default(), &storage, &cache, &depth);
        let result = options.delimiter_key(&ast, "(", ")");
        assert!(matches!(
            result,
            Err(StorageError::Execution(ExecutionError::Cancelled {
                phase: Phase::Layout
            }))
        ));
        assert!(cache.borrow().is_empty());
        assert!(storage.used_bytes()[StorageFamily::Strings as usize] < 1024);
    }
}
