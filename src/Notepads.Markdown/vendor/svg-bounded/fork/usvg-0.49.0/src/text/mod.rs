// Copyright 2024 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use std::sync::Arc;

use crate::FontFamily;
use fontdb::{Database, ID};

use self::layout::DatabaseExt;
use crate::{Cache, Font, FontStretch, FontStyle, Text};

mod coverage;
pub(crate) mod flatten;
#[cfg(test)]
mod font_consumer_tests;
mod font_instance;
pub(crate) mod representation;
mod transform;

mod colr;
mod colr_storage;
/// Provides access to the layout of a text node.
pub mod layout;
#[cfg(all(test, target_os = "windows"))]
mod system_colr_actual_tests;

/// The optical sizing variation axis tag.
pub(crate) const OPSZ: skrifa::Tag = skrifa::Tag::from_be_bytes(*b"opsz");

/// The ID of a glyph within a font
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GlyphId(pub u32);

impl From<GlyphId> for skrifa::raw::types::GlyphId {
    fn from(value: GlyphId) -> Self {
        Self::new(value.0)
    }
}

/// A shorthand for [FontResolver]'s font selection function.
///
/// This function receives a font specification (families + a style, weight,
/// stretch triple) and a font database and should return the ID of the font
/// that shall be used (if any).
///
/// In the basic case, the function will search the existing fonts in the
/// database to find a good match, e.g. via
/// [`Database::query`](fontdb::Database::query). This is what the [default
/// implementation](FontResolver::default_font_selector) does.
///
/// Users with more complex requirements can mutate the database to load
/// additional fonts dynamically. Compatibility callers must explicitly clone
/// a shared database through `Database::try_clone_unfunded` before mutation.
/// Controlled font replay instead publishes immutable request-funded versions;
/// those snapshots cannot use compatibility mutation. The database is only
/// mutated additively. Removing fonts or replacing the entire database will
/// break things.
/// One resolver owner or a borrowed view of the same callback.
pub enum FontSelectionFn<'a> {
    /// Callback owned by the external configuration.
    Owned(Box<dyn Fn(&Font, &mut Arc<Database>) -> Option<ID> + Send + Sync + 'a>),
    /// Callback borrowed without allocating a forwarding closure.
    Borrowed(&'a (dyn Fn(&Font, &mut Arc<Database>) -> Option<ID> + Send + Sync + 'a)),
}
impl<'a> std::ops::Deref for FontSelectionFn<'a> {
    type Target = dyn Fn(&Font, &mut Arc<Database>) -> Option<ID> + Send + Sync + 'a;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(v) => v,
            Self::Borrowed(v) => *v,
        }
    }
}
impl FontSelectionFn<'_> {
    pub(crate) fn borrowed(&self) -> FontSelectionFn<'_> {
        FontSelectionFn::Borrowed(&**self)
    }
}

/// A shorthand for [FontResolver]'s fallback selection function.
///
/// This function receives a specific character, a list of already used fonts,
/// and a font database. It should return the ID of a font that
/// - is not any of the already used fonts
/// - is as close as possible to the first already used font (if any)
/// - supports the given character
///
/// The function can search the existing database, but can also load additional
/// fonts dynamically. See the documentation of [`FontSelectionFn`] for more
/// details.
/// One resolver owner or a borrowed view of the same callback.
pub enum FallbackSelectionFn<'a> {
    /// The built-in selector; its identity survives borrowed nested options.
    Default,
    /// Callback owned by the external configuration.
    Owned(Box<dyn Fn(char, &[ID], &mut Arc<Database>) -> Option<ID> + Send + Sync + 'a>),
    /// Callback borrowed without allocating a forwarding closure.
    Borrowed(&'a (dyn Fn(char, &[ID], &mut Arc<Database>) -> Option<ID> + Send + Sync + 'a)),
}
impl<'a> std::ops::Deref for FallbackSelectionFn<'a> {
    type Target = dyn Fn(char, &[ID], &mut Arc<Database>) -> Option<ID> + Send + Sync + 'a;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Default => &ordinary_default_fallback,
            Self::Owned(v) => v,
            Self::Borrowed(v) => *v,
        }
    }
}
impl FallbackSelectionFn<'_> {
    pub(crate) fn borrowed(&self) -> FallbackSelectionFn<'_> {
        match self {
            Self::Default => FallbackSelectionFn::Default,
            _ => FallbackSelectionFn::Borrowed(&**self),
        }
    }
    pub(crate) fn try_select(
        &self,
        c: char,
        excluded: &[ID],
        db: &mut Arc<Database>,
        processing: &crate::ProcessingContext,
    ) -> Result<Option<ID>, markdown_svg_budget::Error> {
        crate::tree::owner::check(processing, 0)?;
        let result = match self {
            Self::Default => default_fallback(c, excluded, db, processing.resources())?,
            _ => (std::ops::Deref::deref(self))(c, excluded, db),
        };
        // External callbacks are opaque. Only their caller boundary is checked;
        // this does not claim to account storage/work inside arbitrary code.
        crate::tree::owner::check(processing, 0)?;
        Ok(result)
    }
}

fn fallback_work(
    resources: Option<&markdown_svg_budget::ResourcesRef>,
    work: u64,
) -> Result<(), markdown_svg_budget::Error> {
    if let Some(resources) = resources {
        resources.checkpoint(work)?;
    }
    Ok(())
}
fn fallback_family<'a>(
    families: &'a [(String, fontdb::Language)],
    resources: Option<&markdown_svg_budget::ResourcesRef>,
) -> Result<&'a (String, fontdb::Language), markdown_svg_budget::Error> {
    for family in families {
        fallback_work(resources, 1)?;
        if family.1 == fontdb::Language::English_UnitedStates {
            return Ok(family);
        }
    }
    Ok(&families[0])
}
fn ordinary_default_fallback(c: char, excluded: &[ID], db: &mut Arc<Database>) -> Option<ID> {
    // No resource account exists in this compatibility invocation. Font-domain
    // errors remain false/missing, as in the original selector.
    default_fallback(c, excluded, db, None).expect("ordinary selector has no resource failures")
}
fn default_fallback(
    c: char,
    excluded: &[ID],
    db: &mut Arc<Database>,
    resources: Option<&markdown_svg_budget::ResourcesRef>,
) -> Result<Option<ID>, markdown_svg_budget::Error> {
    fallback_work(resources, 0)?;
    let Some(base_face) = db.face(excluded[0]) else {
        return Ok(None);
    };
    for face in db.faces() {
        fallback_work(resources, 1)?;
        let mut used = false;
        for id in excluded {
            fallback_work(resources, 1)?;
            if *id == face.id {
                used = true;
                break;
            }
        }
        if used {
            continue;
        }
        if base_face.style != face.style
            && base_face.weight != face.weight
            && base_face.stretch != face.stretch
        {
            continue;
        }
        if !db.try_has_char(face.id, c, resources)? {
            continue;
        }
        let base_family = fallback_family(&base_face.families, resources)?;
        let new_family = fallback_family(&face.families, resources)?;
        log::warn!("Fallback from {} to {}.", base_family.0, new_family.0);
        return Ok(Some(face.id));
    }
    Ok(None)
}

/// A font resolver for `<text>` elements.
///
/// This type can be useful if you want to have an alternative font handling to
/// the default one. By default, only fonts specified upfront in
/// [`Options::fontdb`](crate::Options::fontdb) will be used. This type allows
/// you to load additional fonts on-demand and customize the font selection
/// process.
pub struct FontResolver<'a> {
    /// Resolver function that will be used when selecting a specific font
    /// for a generic [`Font`] specification.
    pub select_font: FontSelectionFn<'a>,

    /// Resolver function that will be used when selecting a fallback font for a
    /// character.
    pub select_fallback: FallbackSelectionFn<'a>,
}

impl Default for FontResolver<'_> {
    fn default() -> Self {
        FontResolver {
            select_font: FontResolver::default_font_selector(),
            select_fallback: FontResolver::default_fallback_selector(),
        }
    }
}

impl FontResolver<'_> {
    /// Creates a default font selection resolver.
    ///
    /// The default implementation forwards to
    /// [`query`](fontdb::Database::query) on the font database specified in the
    /// [`Options`](crate::Options).
    pub fn default_font_selector() -> FontSelectionFn<'static> {
        FontSelectionFn::Owned(Box::new(move |font, fontdb| {
            let mut name_list = Vec::new();
            for family in &font.families {
                name_list.push(match family {
                    FontFamily::Serif => fontdb::Family::Serif,
                    FontFamily::SansSerif => fontdb::Family::SansSerif,
                    FontFamily::Cursive => fontdb::Family::Cursive,
                    FontFamily::Fantasy => fontdb::Family::Fantasy,
                    FontFamily::Monospace => fontdb::Family::Monospace,
                    FontFamily::Named(s) => fontdb::Family::Name(s),
                });
            }

            // Use the default font as fallback.
            name_list.push(fontdb::Family::Serif);

            let stretch = match font.stretch {
                FontStretch::UltraCondensed => fontdb::Stretch::UltraCondensed,
                FontStretch::ExtraCondensed => fontdb::Stretch::ExtraCondensed,
                FontStretch::Condensed => fontdb::Stretch::Condensed,
                FontStretch::SemiCondensed => fontdb::Stretch::SemiCondensed,
                FontStretch::Normal => fontdb::Stretch::Normal,
                FontStretch::SemiExpanded => fontdb::Stretch::SemiExpanded,
                FontStretch::Expanded => fontdb::Stretch::Expanded,
                FontStretch::ExtraExpanded => fontdb::Stretch::ExtraExpanded,
                FontStretch::UltraExpanded => fontdb::Stretch::UltraExpanded,
            };

            let style = match font.style {
                FontStyle::Normal => fontdb::Style::Normal,
                FontStyle::Italic => fontdb::Style::Italic,
                FontStyle::Oblique => fontdb::Style::Oblique,
            };

            let query = fontdb::Query {
                families: &name_list,
                weight: fontdb::Weight(font.weight),
                stretch,
                style,
            };

            let id = fontdb.query(&query);
            if id.is_none() {
                log::warn!(
                    "No match for '{}' font-family.",
                    font.families
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

            id
        }))
    }

    /// Creates a default font fallback selection resolver.
    ///
    /// The default implementation searches through the entire `fontdb`
    /// to find a font that has the correct style and supports the character.
    pub fn default_fallback_selector() -> FallbackSelectionFn<'static> {
        FallbackSelectionFn::Default
    }
}

impl std::fmt::Debug for FontResolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FontResolver { .. }")
    }
}

/// Convert a text into its paths. This is done in two steps:
/// 1. We convert the text into glyphs and position them according to the rules specified
///    in the SVG specification. While doing so, we also calculate the text bbox (which
///    is not based on the outlines of a glyph, but instead the glyph metrics as well
///    as decoration spans).
/// 2. We convert all of the positioned glyphs into outlines.
pub(crate) fn convert(text: &mut Text, resolver: &FontResolver, cache: &mut Cache) -> Option<()> {
    if cache.processing.failed() {
        return None;
    }
    let owner = cache.font_replay.clone();
    let replay = if let Some(adapter) = owner.as_ref() {
        match adapter.prepare(text, &mut cache.fontdb) {
            Ok(value) => Some(value),
            Err(()) => {
                cache.processing.fail(crate::ProcessingFailure::FontReplay);
                return None;
            }
        }
    } else {
        None
    };
    let layout_result = if replay
        .as_ref()
        .is_some_and(|replay| replay.logical.is_some())
    {
        // Preselection needs the complete Cache, including the same request's
        // resource/first-error state. The local database avoids overlapping
        // mutable borrows; it must not overwrite nested glyph SVG additions.
        let mut layout_fontdb = cache.fontdb.clone();
        let processing = cache.processing.clone();
        let result = layout::layout_text(
            text,
            resolver,
            &mut layout_fontdb,
            replay.as_ref(),
            owner.as_deref(),
            &processing,
            Some(cache),
        );
        if let Some(owner) = owner.as_ref().filter(|_| result.is_some()) {
            cache.fontdb = match owner.fontdb() {
                Ok(db) => db,
                Err(()) => {
                    cache.processing.fail(crate::ProcessingFailure::FontReplay);
                    return None;
                }
            };
        }
        result
    } else {
        layout::layout_text(
            text,
            resolver,
            &mut cache.fontdb,
            replay.as_ref(),
            owner.as_deref(),
            &cache.processing,
            None,
        )
    };
    let (text_fragments, bbox) = match layout_result {
        Some(value) => value,
        None => {
            // A prepared replay is an actual requested text, never an optional
            // font miss that may be silently removed from a partial scene.
            if replay.is_some() {
                cache.processing.fail(crate::ProcessingFailure::FontReplay);
            }
            return None;
        }
    };
    text.layouted = text_fragments;
    text.bounding_box = bbox.to_rect();
    text.abs_bounding_box = bbox.transform(text.abs_transform)?.to_rect();

    let (group, stroke_bbox) = flatten::flatten(text, cache)?;
    // flatten_inner admitted this real root before constructing it. Moving
    // that same root into the final Box must not count its children twice.
    let flattened = crate::scene::moved_root(&cache.processing, group)
        .map_err(|e| cache.processing.record_resource(e))
        .ok()?;
    let old = std::mem::replace(&mut text.flattened, flattened);
    drop(old);
    crate::tree::owner::check(&cache.processing, 0).ok()?;
    text.stroke_bounding_box = stroke_bbox.to_rect();
    text.abs_stroke_bounding_box = stroke_bbox.transform(text.abs_transform)?.to_rect();
    if let Some(owner) = owner.as_ref() {
        match owner.fontdb() {
            Ok(db) => cache.fontdb = db,
            Err(()) => {
                cache.processing.fail(crate::ProcessingFailure::FontReplay);
                return None;
            }
        }
    }

    Some(())
}

impl std::fmt::Debug for FontSelectionFn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Owned(_) => "FontSelectionFn::Owned(..)",
            Self::Borrowed(_) => "FontSelectionFn::Borrowed(..)",
        })
    }
}

impl std::fmt::Debug for FallbackSelectionFn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Default => "FallbackSelectionFn::Default",
            Self::Owned(_) => "FallbackSelectionFn::Owned(..)",
            Self::Borrowed(_) => "FallbackSelectionFn::Borrowed(..)",
        })
    }
}
