// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

#[cfg(feature = "text")]
use std::sync::Arc;

#[cfg(feature = "text")]
use crate::FontResolver;
use crate::{ImageHrefResolver, ImageRendering, ShapeRendering, Size, TextRendering};

/// Processing options.
#[derive(Debug)]
pub struct Options<'a> {
    pub(crate) source_kind: super::source_kind::SourceKind,
    /// Request-owned isolated font replay; no consumer-specific API.
    #[cfg(feature = "text")]
    pub font_replay: Option<Arc<dyn crate::FontReplay>>,
    /// Directory that will be used during relative paths resolving.
    ///
    /// Expected to be the same as the directory that contains the SVG file,
    /// but can be set to any.
    ///
    /// Default: `None`
    pub resources_dir: Option<std::path::PathBuf>,

    /// Active private request context; every public top-level parse replaces it with a fresh owner.
    pub processing: std::sync::Arc<crate::ProcessingContext>,

    /// Target DPI.
    ///
    /// Impacts units conversion.
    ///
    /// Default: 96.0
    pub dpi: f32,

    /// A default font family.
    ///
    /// Will be used when no `font-family` attribute is set in the SVG.
    ///
    /// Default: Times New Roman
    pub font_family: String,

    /// A default font size.
    ///
    /// Will be used when no `font-size` attribute is set in the SVG.
    ///
    /// Default: 12
    pub font_size: f32,

    /// A list of languages.
    ///
    /// Will be used to resolve a `systemLanguage` conditional attribute.
    ///
    /// Format: en, en-US.
    ///
    /// Default: `[en]`
    pub languages: Vec<String>,

    /// Specifies the default shape rendering method.
    ///
    /// Will be used when an SVG element's `shape-rendering` property is set to `auto`.
    ///
    /// Default: GeometricPrecision
    pub shape_rendering: ShapeRendering,

    /// Specifies the default text rendering method.
    ///
    /// Will be used when an SVG element's `text-rendering` property is set to `auto`.
    ///
    /// Default: OptimizeLegibility
    pub text_rendering: TextRendering,

    /// Specifies the default image rendering method.
    ///
    /// Will be used when an SVG element's `image-rendering` property is set to `auto`.
    ///
    /// Default: OptimizeQuality
    pub image_rendering: ImageRendering,

    /// Default viewport size to assume if there is no `viewBox` attribute and
    /// the `width` or `height` attributes are relative.
    ///
    /// Default: `(100, 100)`
    pub default_size: Size,

    /// Specifies the way `xlink:href` in `<image>` elements should be handled.
    ///
    /// Default: see type's documentation for details
    pub image_href_resolver: ImageHrefResolver<'a>,

    /// Specifies how fonts should be resolved and loaded.
    #[cfg(feature = "text")]
    pub font_resolver: FontResolver<'a>,

    /// A database of fonts usable by text.
    ///
    /// This is a base database. If a custom `font_resolver` is specified,
    /// additional fonts can be loaded during parsing. Those will be added to a
    /// copy of this database. The full database containing all fonts referenced
    /// in a `Tree` becomes available as [`Tree::fontdb`](crate::Tree::fontdb)
    /// after parsing. If no fonts were loaded dynamically, that database will
    /// be the same as this one.
    #[cfg(feature = "text")]
    pub fontdb: Arc<fontdb::Database>,
    /// A CSS stylesheet that should be injected into the SVG. Can be used to overwrite
    /// certain attributes.
    pub style_sheet: Option<String>,
    // Last: the owned font-source default strings physically retire first.
    pub(crate) _font_defaults_lease: Option<markdown_svg_budget::Lease>,
}

impl Default for Options<'_> {
    fn default() -> Options<'static> {
        Options {
            source_kind: super::source_kind::SourceKind::GeneralImage,
            resources_dir: None,
            processing: std::sync::Arc::new(crate::ProcessingContext::default()),
            dpi: 96.0,
            // Default font is user-agent dependent so we can use whichever we like.
            font_family: "Times New Roman".to_owned(),
            font_size: 12.0,
            languages: vec!["en".to_string()],
            shape_rendering: ShapeRendering::default(),
            text_rendering: TextRendering::default(),
            image_rendering: ImageRendering::default(),
            default_size: Size::from_wh(100.0, 100.0).unwrap(),
            image_href_resolver: ImageHrefResolver::default(),
            #[cfg(feature = "text")]
            font_resolver: FontResolver::default(),
            #[cfg(feature = "text")]
            font_replay: None,
            #[cfg(feature = "text")]
            fontdb: Arc::new(fontdb::Database::new()),
            style_sheet: None,
            _font_defaults_lease: None,
        }
    }
}

impl Options<'_> {
    /// Derive the actual image-subdocument profile without forwarding boxes.
    /// The aggregate lease precedes every fallible string/vector allocation;
    /// all of those physical owners precede the lease in Options' drop order.
    pub(crate) fn nested_image_options(
        &self,
    ) -> Result<Options<'_>, markdown_svg_budget::Error> {
        use crate::tree::owner;
        use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourceKind};
        let p = &self.processing;
        owner::check(p, 0)?;
        const FAMILY: &str = "Times New Roman";
        fn overflow(p: &crate::ProcessingContext) -> Error {
            p.record_resource(Error::new(FailureKind::Quota, LimitKind::Capacity))
        }
        fn copy_text(source: &str, p: &crate::ProcessingContext) -> Result<String, Error> {
            let mut result = String::new();
            result.try_reserve_exact(source.len()).map_err(|_| {
                p.record_resource(Error::new(FailureKind::Allocation, LimitKind::Capacity))
            })?;
            let mut rest = source;
            while !rest.is_empty() {
                let mut end = rest.len().min(256);
                while !rest.is_char_boundary(end) {
                    end -= 1;
                }
                owner::check(p, end as u64)?;
                result.push_str(&rest[..end]);
                rest = &rest[end..];
            }
            Ok(result)
        }
        let result = (|| {
            let mut bytes = self
                .languages
                .len()
                .checked_mul(std::mem::size_of::<String>())
                .and_then(|v| v.checked_add(FAMILY.len()))
                .ok_or_else(|| overflow(p))?;
            for language in &self.languages {
                owner::check(p, std::mem::size_of::<String>() as u64)?;
                bytes = bytes
                    .checked_add(language.len())
                    .ok_or_else(|| overflow(p))?;
            }
            // Declared before every owner, including on partial construction.
            let mut lease = p
                .resources()
                .map(|r| r.reserve(ResourceKind::Scratch, bytes))
                .transpose()
                .map_err(|e| p.record_resource(e))?;
            let font_family = if p.resources().is_some() {
                copy_text(FAMILY, p)?
            } else {
                FAMILY.to_owned()
            };
            let mut languages = Vec::new();
            if p.resources().is_some() {
                languages
                    .try_reserve_exact(self.languages.len())
                    .map_err(|_| {
                        p.record_resource(Error::new(FailureKind::Allocation, LimitKind::Capacity))
                    })?;
                for language in &self.languages {
                    owner::check(p, std::mem::size_of::<String>() as u64)?;
                    languages.push(copy_text(language, p)?);
                }
                let mut actual = font_family
                    .capacity()
                    .checked_add(
                        languages
                            .capacity()
                            .checked_mul(std::mem::size_of::<String>())
                            .ok_or_else(|| overflow(p))?,
                    )
                    .ok_or_else(|| overflow(p))?;
                for language in &languages {
                    owner::check(p, std::mem::size_of::<String>() as u64)?;
                    actual = actual
                        .checked_add(language.capacity())
                        .ok_or_else(|| overflow(p))?;
                }
                if actual != bytes {
                    return Err(
                        p.record_resource(Error::new(FailureKind::Invariant, LimitKind::Capacity))
                    );
                }
            } else {
                languages = self.languages.clone();
            }
            if let Some(lease) = &mut lease {
                lease
                    .allocation_committed()
                    .map_err(|e| p.record_resource(e))?;
            }
            owner::check(p, 0)?;
            Ok(Options {
                source_kind: super::source_kind::SourceKind::GeneralImage,
                resources_dir: None,
                processing: self.processing.clone(),
                dpi: self.dpi,
                font_family,
                font_size: self.font_size,
                languages,
                shape_rendering: self.shape_rendering,
                text_rendering: self.text_rendering,
                image_rendering: self.image_rendering,
                default_size: self.default_size,
                image_href_resolver: ImageHrefResolver {
                    resolve_data: self.image_href_resolver.resolve_data.borrowed(),
                    resolve_string: ImageHrefResolver::ignored_string_resolver(),
                },
                #[cfg(feature = "text")]
                fontdb: self.fontdb.clone(),
                #[cfg(feature = "text")]
                font_replay: self.font_replay.clone(),
                #[cfg(feature = "text")]
                font_resolver: crate::FontResolver {
                    select_font: self.font_resolver.select_font.borrowed(),
                    select_fallback: self.font_resolver.select_fallback.borrowed(),
                },
                style_sheet: None,
                _font_defaults_lease: lease,
            })
        })();
        owner::finish(p, result)
    }
    /// Construct the controlled image profile from already funded owners.
    ///
    /// No default ProcessingContext, font database, family or language storage
    /// is created and discarded. The caller funds these owners before creating
    /// them and keeps their allocation leases until their physical retirement.
    #[cfg(feature = "text")]
    pub fn with_resources_and_fontdb(
        processing: Arc<crate::ProcessingContext>,
        fontdb: Arc<fontdb::Database>,
        font_family: String,
        languages: Vec<String>,
    ) -> Options<'static> {
        Options {
            source_kind: super::source_kind::SourceKind::GeneralImage,
            resources_dir: None,
            processing,
            dpi: 96.0,
            font_family,
            font_size: 16.0,
            languages,
            shape_rendering: ShapeRendering::default(),
            text_rendering: TextRendering::default(),
            image_rendering: ImageRendering::default(),
            default_size: Size::from_wh(300.0, 150.0).unwrap(),
            image_href_resolver: ImageHrefResolver::default(),
            font_resolver: FontResolver::default(),
            font_replay: None,
            fontdb,
            style_sheet: None,
            _font_defaults_lease: None,
        }
    }

    /// Preserve the legacy font-source defaults without constructing a second
    /// context or font database. Pay the three actual default-string buffers
    /// before allocation when this is part of a controlled request.
    #[cfg(feature = "text")]
    pub(crate) fn font_source_options(
        processing: Arc<crate::ProcessingContext>,
        fontdb: Arc<fontdb::Database>,
        font_replay: Option<Arc<dyn crate::FontReplay>>,
    ) -> Result<Options<'static>, markdown_svg_budget::Error> {
        use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourceKind};
        const FAMILY: &str = "Times New Roman";
        const LANGUAGE: &str = "en";
        let bytes = FAMILY.len() + LANGUAGE.len() + std::mem::size_of::<String>();
        // Declared first so partially allocated storage also drops before its
        // pending lease if allocation or admission fails.
        let mut lease = processing
            .resources()
            .map(|r| r.reserve(ResourceKind::Scratch, bytes))
            .transpose()?;
        let allocation_error =
            || processing.record_resource(Error::new(FailureKind::Allocation, LimitKind::Capacity));
        let mut font_family = String::new();
        font_family
            .try_reserve_exact(FAMILY.len())
            .map_err(|_| allocation_error())?;
        font_family.push_str(FAMILY);
        let mut language = String::new();
        language
            .try_reserve_exact(LANGUAGE.len())
            .map_err(|_| allocation_error())?;
        language.push_str(LANGUAGE);
        let mut languages = Vec::new();
        languages
            .try_reserve_exact(1)
            .map_err(|_| allocation_error())?;
        languages.push(language);
        let actual = font_family.capacity()
            + languages[0].capacity()
            + languages.capacity() * std::mem::size_of::<String>();
        if actual != bytes {
            return Err(
                processing.record_resource(Error::new(FailureKind::Invariant, LimitKind::Capacity))
            );
        }
        if let Some(lease) = &mut lease {
            lease.allocation_committed()?;
        }
        Ok(Options {
            source_kind: super::source_kind::SourceKind::GeneralImage,
            resources_dir: None,
            processing,
            dpi: 96.0,
            font_family,
            font_size: 12.0,
            languages,
            shape_rendering: ShapeRendering::default(),
            text_rendering: TextRendering::default(),
            image_rendering: ImageRendering::default(),
            default_size: Size::from_wh(100.0, 100.0).unwrap(),
            image_href_resolver: ImageHrefResolver::default(),
            font_resolver: FontResolver::default(),
            font_replay,
            fontdb,
            style_sheet: None,
            _font_defaults_lease: lease,
        })
    }

    /// Derive a fresh top-level request. Resolver closures only borrow the configuration.
    pub(crate) fn fresh_request(&self) -> Options<'_> {
        Options {
            source_kind: self.source_kind,
            resources_dir: self.resources_dir.clone(),
            processing: std::sync::Arc::new(crate::ProcessingContext::default()),
            dpi: self.dpi,
            font_family: self.font_family.clone(),
            font_size: self.font_size,
            languages: self.languages.clone(),
            shape_rendering: self.shape_rendering,
            text_rendering: self.text_rendering,
            image_rendering: self.image_rendering,
            default_size: self.default_size,
            image_href_resolver: crate::ImageHrefResolver {
                resolve_data: self.image_href_resolver.resolve_data.borrowed(),
                resolve_string: self.image_href_resolver.resolve_string.borrowed(),
            },
            #[cfg(feature = "text")]
            fontdb: self.fontdb.clone(),
            #[cfg(feature = "text")]
            font_replay: self.font_replay.clone(),
            #[cfg(feature = "text")]
            font_resolver: crate::FontResolver {
                select_font: self.font_resolver.select_font.borrowed(),
                select_fallback: self.font_resolver.select_fallback.borrowed(),
            },
            style_sheet: self.style_sheet.clone(),
            _font_defaults_lease: None,
        }
    }

    /// Converts a relative path into absolute relative to the SVG file itself.
    ///
    /// If `Options::resources_dir` is not set, returns itself.
    pub fn get_abs_path(&self, rel_path: &std::path::Path) -> std::path::PathBuf {
        match self.resources_dir {
            Some(ref dir) => dir.join(rel_path),
            None => rel_path.into(),
        }
    }

    /// Explicitly acquires an unfunded compatibility database for mutation.
    ///
    /// Request-funded snapshots are immutable and rejected before allocation.
    /// Shared compatibility databases are cloned through the fallible API;
    /// controlled font replay constructs a new paid version instead.
    #[cfg(feature = "text")]
    pub fn try_fontdb_mut(&mut self) -> Result<&mut fontdb::Database, fontdb::CompatCloneError> {
        if self.fontdb.is_funded() {
            return Err(fontdb::CompatCloneError::ControlledReadOnly);
        }
        if Arc::get_mut(&mut self.fontdb).is_none() {
            let database = self.fontdb.try_clone_unfunded()?;
            self.fontdb = Arc::new(database);
        }
        // The existing Arc was exclusive or the new Arc has never escaped.
        Ok(Arc::get_mut(&mut self.fontdb).expect("exclusive unfunded database"))
    }
}

#[cfg(all(test, feature = "text"))]
mod funded_database_compat_tests {
    use super::*;

    #[test]
    fn explicit_compat_mutation_preserves_shared_database_snapshot() {
        let old = Arc::new(fontdb::Database::new());
        let mut options = Options {
            fontdb: old.clone(),
            ..Options::default()
        };
        let database = options.try_fontdb_mut().unwrap();
        database.push_face_info(fontdb::FaceInfo {
            id: fontdb::ID::dummy(),
            // Metadata-only compatibility clone test; bytes are never parsed.
            source: fontdb::Source::Binary(Arc::new([0u8; 4])),
            index: 0,
            families: vec![(
                "compat-family".to_owned(),
                fontdb::Language::English_UnitedStates,
            )],
            post_script_name: String::new(),
            style: fontdb::Style::Normal,
            weight: fontdb::Weight::NORMAL,
            stretch: fontdb::Stretch::Normal,
            monospaced: false,
        });
        assert!(old.is_empty());
        assert_eq!(options.fontdb.len(), 1);
        assert!(!Arc::ptr_eq(&old, &options.fontdb));
        let current = Arc::as_ptr(&options.fontdb);
        options.try_fontdb_mut().unwrap();
        assert_eq!(current, Arc::as_ptr(&options.fontdb));
    }
}
