// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

mod clippath;
mod converter;
#[cfg(feature = "text")]
mod font_cache;
mod reference;
#[cfg(test)]
mod reference_cases;
#[cfg(test)]
mod reference_tests;
mod resource_cache;
#[cfg(all(test, feature = "text"))]
pub(crate) use converter::glyph_coverage_sfnt;
mod filter;
#[cfg(feature = "font-svg-gzip")]
mod font_gzip;
#[cfg(all(test, feature = "font-svg-gzip"))]
mod font_gzip_control_tests;
mod image;
mod marker;
mod mask;
#[cfg(all(test, feature = "text"))]
mod nested_options_tests;
mod numeric;
mod options;
pub(crate) mod source_kind;
pub use source_kind::SourceError;
mod paint_server;
mod shapes;
mod style;
mod svgtree;
mod switch;
mod units;
mod use_node;

#[cfg(feature = "text")]
mod text;
#[cfg(feature = "text")]
pub(crate) use converter::Cache;
pub use image::{ImageHrefDataResolverFn, ImageHrefResolver, ImageHrefStringResolverFn};
pub use options::Options;
#[cfg(feature = "writer")]
pub(crate) use svgtree::{AId, EId};

/// List of all errors.
#[derive(Debug)]
pub enum Error {
    /// Failure in the document origin policy.
    SourcePolicy(SourceError),
    /// The active request resource capability rejected work or allocation.
    Resource(markdown_svg_budget::Error),
    /// A fixed request failure from the controlled prototype route.
    ProcessingFailure(crate::ProcessingError),
    /// Only UTF-8 content are supported.
    NotAnUtf8Str,

    /// `svgz` feature is required to parse SVGZ data.
    SvgzFeatureNotEnabled,

    /// Compressed SVG must use the GZip algorithm.
    MalformedGZip,

    /// We do not allow SVG with more than 1_000_000 elements for security reasons.
    ElementsLimitReached,

    /// SVG doesn't have a valid size.
    ///
    /// Occurs when width and/or height are <= 0.
    ///
    /// Also occurs if width, height and viewBox are not set.
    InvalidSize,

    /// Failed to parse an SVG data.
    ParsingFailed(roxmltree::Error),
}

impl From<roxmltree::Error> for Error {
    fn from(e: roxmltree::Error) -> Self {
        Error::ParsingFailed(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Self::SourcePolicy(reason) => write!(f, "controlled SVG source: {reason:?}"),
            Self::Resource(error) => write!(f, "controlled SVG resource: {error:?}"),
            Error::ProcessingFailure(error) => {
                write!(f, "controlled processing failure: {error:?}")
            }
            Error::NotAnUtf8Str => {
                write!(f, "provided data has not an UTF-8 encoding")
            }
            Self::SvgzFeatureNotEnabled => {
                write!(f, "enable svgz cargo feature to decode SVGZ data")
            }
            Error::MalformedGZip => {
                write!(f, "provided data has a malformed GZip content")
            }
            Error::ElementsLimitReached => {
                write!(f, "the maximum number of SVG elements has been reached")
            }
            Error::InvalidSize => {
                write!(f, "SVG has an invalid size")
            }
            Error::ParsingFailed(ref e) => {
                write!(f, "SVG data parsing failed cause {}", e)
            }
        }
    }
}

impl std::error::Error for Error {}

pub(crate) trait OptionLog {
    fn log_none<F: FnOnce()>(self, f: F) -> Self;
}

impl<T> OptionLog for Option<T> {
    #[inline]
    fn log_none<F: FnOnce()>(self, f: F) -> Self {
        self.or_else(|| {
            f();
            None
        })
    }
}

#[cfg(test)]
thread_local! {static CLEANUP_PHASE:std::cell::Cell<u8>=const{std::cell::Cell::new(0)};}
#[cfg(test)]
pub(crate) fn cleanup_phase() -> u8 {
    CLEANUP_PHASE.with(|p| p.get())
}
#[cfg(test)]
struct CleanupPhase(u8);
#[cfg(test)]
impl CleanupPhase {
    fn enter(role: u8) -> Self {
        Self(CLEANUP_PHASE.with(|p| p.replace(role)))
    }
}
#[cfg(test)]
impl Drop for CleanupPhase {
    fn drop(&mut self) {
        CLEANUP_PHASE.with(|p| p.set(self.0));
    }
}

impl crate::Tree {
    /// Parse within the already established controlled request. No new owner is created.
    pub fn from_data_controlled(data: &[u8], opt: &Options) -> Result<Self, Error> {
        if opt.processing.resources().is_none() {
            return Err(source_kind::fail(opt, SourceError::MissingRequestResources));
        }
        Self::from_data_in_request(data, opt)
    }

    /// Parses `Tree` from an SVG data.
    ///
    /// Can contain an SVG string or a gzip compressed data.
    pub fn from_data(data: &[u8], opt: &Options) -> Result<Self, Error> {
        if data.starts_with(&[0x1f, 0x8b]) {
            #[cfg(feature = "svgz")]
            {
                let data = decompress_svgz(data)?;
                let text = std::str::from_utf8(&data).map_err(|_| Error::NotAnUtf8Str)?;
                Self::from_str(text, opt)
            }

            #[cfg(not(feature = "svgz"))]
            Err(Error::SvgzFeatureNotEnabled)
        } else {
            let text = std::str::from_utf8(data).map_err(|_| Error::NotAnUtf8Str)?;
            Self::from_str(text, opt)
        }
    }

    /// Similar to the `from_data` method, except that it ignores all `image` elements linking to
    /// external files, as required by the SVG specification when SVG files are loaded
    /// for `<image href="..." />` tags.
    pub(crate) fn from_data_nested(
        data: &crate::EncodedImageData,
        opt: &Options,
    ) -> Result<Self, Error> {
        // The actual parent capability, not MIME, determines whether a nested
        // SVG may reach GeneralImage. Explicit font SVG images are ignored in
        // the raw profile; disguised text/plain bytes must never gain text.
        if opt.source_kind.font() {
            return Err(source_kind::fail(opt, SourceError::FontSvgImage));
        }
        // Reject identity and depth before deriving any owned configuration.
        data.validate_policy(opt.processing.resources())
            .map_err(|e| Error::Resource(opt.processing.record_resource(e)))?;
        let scope = if let Some(resources) = opt.processing.resources() {
            Some(
                resources
                    .enter_svg_depth()
                    .map_err(|e| Error::Resource(opt.processing.record_resource(e)))?,
            )
        } else {
            None
        };
        let result = (|| {
            let nested_opt = opt.nested_image_options().map_err(Error::Resource)?;
            let result = Self::from_data_in_request(data, &nested_opt);
            drop(nested_opt);
            result
        })();
        drop(scope);
        Self::after_owned_cleanup(result, opt)
    }

    // Private nested entry: inherit the active request instead of resetting it.
    pub(crate) fn from_data_in_request(data: &[u8], opt: &Options) -> Result<Self, Error> {
        let resources = opt.processing.resources();
        let _scope = if !matches!(opt.source_kind, source_kind::SourceKind::GeneralImage) {
            resources
                .map(|r| r.enter_child())
                .transpose()
                .map_err(|e| {
                    opt.processing.record_resource(e);
                    Error::Resource(e)
                })?
        } else {
            None
        };
        if data.starts_with(&[0x1f, 0x8b]) && opt.source_kind.font() {
            #[cfg(feature = "font-svg-gzip")]
            {
                let decoded = source_kind::decode_font_gzip(data, opt)?;
                let text = source_kind::utf8(decoded.as_slice(), opt)?;
                let result = Self::from_str_in_request(text, opt);
                {
                    #[cfg(test)]
                    let _phase = CleanupPhase::enter(3);
                    drop(decoded);
                }
                return Self::after_owned_cleanup(result, opt);
            }
            #[cfg(not(feature = "font-svg-gzip"))]
            return Err(source_kind::fail(opt, SourceError::GeneralSvgzUnsupported));
        }
        if let Some(resources) = resources {
            if !matches!(opt.source_kind, source_kind::SourceKind::GeneralImage) {
                resources.add_child_encoded(data.len()).map_err(|e| {
                    opt.processing.record_resource(e);
                    Error::Resource(e)
                })?;
            }
        }
        if data.starts_with(&[0x1f, 0x8b]) {
            // A legacy optional feature cannot widen this controlled profile.
            return Err(source_kind::fail(opt, SourceError::GeneralSvgzUnsupported));
        }
        let text = source_kind::utf8(data, opt)?;
        Self::from_str_in_request(text, opt)
    }

    fn from_str_in_request(text: &str, opt: &Options) -> Result<Self, Error> {
        let doc = roxmltree::Document::parse_with_options(
            text,
            roxmltree::ParsingOptions {
                allow_dtd: false,
                nodes_limit: 65_536,
                resources: opt.processing.resources().cloned(),
                ..Default::default()
            },
        )
        .map_err(|e| source_kind::decode_error(opt, Error::ParsingFailed(e)))?;
        let result = Self::from_xmltree_in_request(&doc, opt);
        {
            #[cfg(test)]
            let _phase = CleanupPhase::enter(1);
            drop(doc);
        }
        Self::after_owned_cleanup(result, opt)
    }

    // Check only after this entry's owned paid temporaries physically retire.
    // Err was recorded before cleanup; preserve its original error variant.
    fn after_owned_cleanup(result: Result<Self, Error>, opt: &Options) -> Result<Self, Error> {
        match result {
            Ok(tree) => {
                opt.processing.checked().map_err(Error::ProcessingFailure)?;
                Ok(tree)
            }
            Err(error) => Err(error),
        }
    }

    /// Parses `Tree` from an SVG string.
    pub fn from_str(text: &str, opt: &Options) -> Result<Self, Error> {
        let xml_opt = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };

        let doc =
            roxmltree::Document::parse_with_options(text, xml_opt).map_err(Error::ParsingFailed)?;

        Self::from_xmltree(&doc, opt)
    }

    /// Parses `Tree` from `roxmltree::Document`.
    pub fn from_xmltree(doc: &roxmltree::Document, opt: &Options) -> Result<Self, Error> {
        let request = opt.fresh_request();
        Self::from_xmltree_in_request(doc, &request)
    }

    fn from_xmltree_in_request(doc: &roxmltree::Document, opt: &Options) -> Result<Self, Error> {
        opt.processing.checked().map_err(Error::ProcessingFailure)?;
        source_kind::validate(doc, opt)?;
        let doc = svgtree::Document::parse_tree_for_source(
            doc,
            opt.style_sheet.as_deref(),
            opt.source_kind,
            opt.processing.resources().cloned(),
        )
        .map_err(|e| source_kind::decode_error(opt, Error::ParsingFailed(e)))?;
        opt.processing.checked().map_err(Error::ProcessingFailure)?;
        let result =
            self::converter::convert_doc(&doc, opt).map_err(|e| source_kind::decode_error(opt, e));
        {
            #[cfg(test)]
            let _phase = CleanupPhase::enter(2);
            drop(doc);
        }
        Self::after_owned_cleanup(result, opt)
    }
}

/// Decompresses an SVGZ file.
#[cfg(feature = "svgz")]
pub fn decompress_svgz(data: &[u8]) -> Result<Vec<u8>, Error> {
    use std::io::Read;

    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut decoded = Vec::with_capacity(data.len() * 2);
    decoder
        .read_to_end(&mut decoded)
        .map_err(|_| Error::MalformedGZip)?;
    Ok(decoded)
}

#[inline]
pub(crate) fn f32_bound(min: f32, val: f32, max: f32) -> f32 {
    debug_assert!(min.is_finite());
    debug_assert!(max.is_finite());

    if val > max {
        max
    } else if val >= min {
        val
    } else {
        // Catches `val < min` as well as a NaN `val`.
        min
    }
}
