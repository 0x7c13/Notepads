// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Document-local origin. Font capabilities cannot leak into a later image.
use super::{Error, Options};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum SourceKind {
    #[default]
    GeneralImage,
    FontGlyph {
        glyph: u32,
        units_per_em: u16,
        current_color: svgtypes::Color,
    },
    GeneratedColorGlyph,
}
impl SourceKind {
    pub(crate) fn font(self) -> bool {
        matches!(self, Self::FontGlyph { .. })
    }
}
/// Fixed reasons; no untrusted source text is retained in a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// The controlled entry requires the active outer request capability.
    MissingRequestResources,
    /// Active content is rejected even in an ignored or unselected subtree.
    ActiveContent,
    /// External resources cannot reach a resolver.
    ExternalResource,
    /// This controlled profile cannot interpret the lexical CSS construct safely.
    UnsupportedCss,
    /// A restricted general-image element is not silently skipped.
    UnsupportedElement,
    /// The OpenType target does not exist.
    MissingGlyph,
    /// The target identifier occurs more than once.
    DuplicateGlyph,
    /// Actual font metadata cannot define a viewport.
    InvalidFontMetadata,
    /// Font compressed data is invalid or has non-member trailing data.
    InvalidFontGzip,
    /// Ordinary compressed SVG is outside this image profile.
    GeneralSvgzUnsupported,
    /// Source bytes are not strict UTF8.
    InvalidUtf8,
    /// Root name/namespace is not an SVG document.
    InvalidRootNamespace,
    /// A font glyph cannot reinterpret a nested SVG as a general image.
    FontSvgImage,
}
pub(crate) fn fail(opt: &Options, reason: SourceError) -> Error {
    use markdown_svg_budget::{Error as Failure, FailureKind, LimitKind};
    let kind = match reason {
        SourceError::ExternalResource => FailureKind::ExternalResource,
        SourceError::InvalidRootNamespace
        | SourceError::InvalidUtf8
        | SourceError::MissingGlyph
        | SourceError::DuplicateGlyph
        | SourceError::InvalidFontMetadata
        | SourceError::InvalidFontGzip => FailureKind::Decode,
        SourceError::MissingRequestResources => FailureKind::Invariant,
        _ => FailureKind::Unsupported,
    };
    opt.processing
        .record_resource(Failure::new(kind, LimitKind::None));
    Error::SourcePolicy(reason)
}
pub(crate) fn ignored_font_element(name: &str) -> bool {
    matches!(
        name,
        "text"
            | "tspan"
            | "textPath"
            | "tref"
            | "font"
            | "font-face"
            | "font-face-src"
            | "font-face-uri"
            | "font-face-format"
            | "font-face-name"
            | "glyph"
            | "missing-glyph"
            | "hkern"
            | "vkern"
            | "foreignObject"
            | "switch"
            | "a"
            | "view"
    )
}
fn active(name: &str) -> bool {
    matches!(
        name,
        "script"
            | "animate"
            | "animateColor"
            | "animateMotion"
            | "animateTransform"
            | "set"
            | "discard"
    )
}
/// Scan raw XML before the normalizer can skip unknown/hidden/ignored nodes.
/// CSS layout still belongs to upstream. Only proven font-name escapes receive
/// a lexical exception; resource-bearing and unknown CSS keeps its original gate.
pub(crate) fn validate(doc: &roxmltree::Document, opt: &Options) -> Result<(), Error> {
    let root = doc.root_element();
    if root.tag_name().name() != "svg"
        || !matches!(
            root.tag_name().namespace(),
            None | Some("http://www.w3.org/2000/svg")
        )
        || (opt.source_kind.font()
            && root.tag_name().namespace() != Some("http://www.w3.org/2000/svg"))
    {
        return Err(fail(opt, SourceError::InvalidRootNamespace));
    }
    let mut target_count = 0usize;
    let target = if let SourceKind::FontGlyph {
        glyph,
        units_per_em,
        ..
    } = opt.source_kind
    {
        if units_per_em == 0 {
            return Err(fail(opt, SourceError::InvalidFontMetadata));
        }
        Some(format!("glyph{glyph}"))
    } else {
        None
    };
    let mut work = ScanWork { opt, credit: 0 };
    for node in doc.descendants().filter(|n| n.is_element()) {
        work.charge(1)?;
        if let Some(resources) = opt.processing.resources() {
            // XML work was prepaid by the parser; this is a distinct raw pass.
            // Batch at the document boundary below rather than a Native call per node.
            if let Some(error) = resources.failure() {
                return Err(Error::Resource(error));
            }
        }
        if !opt.source_kind.font() && node.tag_name().name() == "foreignObject" {
            return Err(fail(opt, SourceError::UnsupportedElement));
        }
        if active(node.tag_name().name()) {
            return Err(fail(opt, SourceError::ActiveContent));
        }
        for attr in node.attributes() {
            let name = attr.name();
            work.charge(name.len() as u64 + 1)?;
            if name
                .as_bytes()
                .get(..2)
                .is_some_and(|p| p.eq_ignore_ascii_case(b"on"))
            {
                return Err(fail(opt, SourceError::ActiveContent));
            }
            if name == "href" {
                let value = attr.value().trim();
                if !value.starts_with('#') && !value.starts_with("data:") {
                    return Err(fail(opt, SourceError::ExternalResource));
                }
            }
            if name == "style" {
                css_urls(attr.value(), CssContext::InlineDeclarations, &mut work)?;
            } else if matches!(
                name,
                "fill"
                    | "stroke"
                    | "filter"
                    | "clip-path"
                    | "mask"
                    | "marker"
                    | "marker-start"
                    | "marker-mid"
                    | "marker-end"
                    | "cursor"
            ) {
                css_urls(attr.value(), CssContext::PaintValue, &mut work)?;
            }
            if name == "id" && target.as_deref() == Some(attr.value()) {
                target_count += 1;
            }
        }
        if node.tag_name().name() == "style" {
            css_urls(node.text().unwrap_or(""), CssContext::StyleSheet, &mut work)?;
        }
    }
    if target.is_some() && target_count != 1 {
        return Err(fail(
            opt,
            if target_count == 0 {
                SourceError::MissingGlyph
            } else {
                SourceError::DuplicateGlyph
            },
        ));
    }
    Ok(())
}
struct ScanWork<'a, 'b> {
    opt: &'a Options<'b>,
    credit: u64,
}
impl ScanWork<'_, '_> {
    fn charge(&mut self, mut amount: u64) -> Result<(), Error> {
        let Some(resources) = self.opt.processing.resources() else {
            return Ok(());
        };
        while amount != 0 {
            if self.credit == 0 {
                resources.checkpoint(4096).map_err(|e| {
                    self.opt.processing.record_resource(e);
                    Error::Resource(e)
                })?;
                self.credit = 4096;
            }
            let used = amount.min(self.credit);
            self.credit -= used;
            amount -= used;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum CssContext {
    PaintValue,
    InlineDeclarations,
    StyleSheet,
}

#[derive(Clone, Copy)]
enum FontValue {
    Family,
    Shorthand,
}

#[derive(Clone, Copy)]
enum DeclarationPosition {
    Start,
    Name(Option<FontValue>),
    Value(usize, FontValue),
    Other,
}

// This proves only a declaration position, not selector/property semantics.
// Unknown nested structures never create a font exception from their contents.
struct CssPosition {
    context: CssContext,
    blocks: usize,
    parens: usize,
    brackets: usize,
    selector_safe: bool,
    declaration_safe: bool,
    position: DeclarationPosition,
}

impl CssPosition {
    fn new(context: CssContext) -> Self {
        Self {
            context,
            blocks: 0,
            parens: 0,
            brackets: 0,
            selector_safe: true,
            declaration_safe: context == CssContext::InlineDeclarations,
            position: DeclarationPosition::Start,
        }
    }

    fn declarations(&self) -> bool {
        self.parens == 0
            && self.brackets == 0
            && self.declaration_safe
            && match self.context {
                CssContext::PaintValue => false,
                CssContext::InlineDeclarations => self.blocks == 0,
                CssContext::StyleSheet => self.blocks == 1,
            }
    }

    fn font_value(&self) -> Option<(usize, FontValue)> {
        if self.declarations() {
            if let DeclarationPosition::Value(start, kind) = self.position {
                return Some((start, kind));
            }
        }
        None
    }

    fn opaque(&mut self) {
        self.position = DeclarationPosition::Other;
    }

    fn token(&mut self, token: &[u8]) {
        if self.declarations() {
            self.position = match self.position {
                DeclarationPosition::Start => DeclarationPosition::Name(match token {
                    b"font-family" => Some(FontValue::Family),
                    b"font" => Some(FontValue::Shorthand),
                    _ => None,
                }),
                DeclarationPosition::Name(_) => DeclarationPosition::Other,
                position => position,
            };
        }
    }

    fn quoted(&mut self) {
        if self.context == CssContext::StyleSheet && self.blocks == 0 && self.brackets == 0 {
            self.selector_safe = false;
        }
        if !matches!(self.position, DeclarationPosition::Value(_, _)) {
            self.opaque();
        }
    }

    fn punct(&mut self, byte: u8, at: usize) {
        match byte {
            b'(' => {
                if self.brackets != 0 {
                    self.selector_safe = false;
                    self.declaration_safe = false;
                }
                self.parens += 1;
                self.opaque();
                if self.context == CssContext::StyleSheet && self.blocks == 0 {
                    self.selector_safe = false;
                }
            }
            b'[' => {
                if self.parens != 0 || self.brackets != 0 {
                    self.selector_safe = false;
                    self.declaration_safe = false;
                }
                self.brackets += 1;
                self.opaque();
            }
            b')' | b']' => {
                if (byte == b')' && self.brackets != 0) || (byte == b']' && self.parens != 0) {
                    self.selector_safe = false;
                    self.declaration_safe = false;
                }
                let depth = if byte == b')' {
                    &mut self.parens
                } else {
                    &mut self.brackets
                };
                if *depth == 0 {
                    self.selector_safe = false;
                    self.declaration_safe = false;
                } else {
                    *depth -= 1;
                }
                self.opaque();
            }
            b'{' | b'}' if self.parens != 0 || self.brackets != 0 => {
                self.selector_safe = false;
                self.declaration_safe = false;
                self.opaque();
            }
            b'{' => {
                if self.context == CssContext::StyleSheet {
                    self.declaration_safe = self.blocks == 0 && self.selector_safe;
                    self.blocks += 1;
                    self.position = DeclarationPosition::Start;
                } else {
                    self.blocks += 1;
                    self.declaration_safe = false;
                    self.opaque();
                }
            }
            b'}' => {
                if self.blocks == 0 {
                    self.selector_safe = false;
                    self.declaration_safe = false;
                } else {
                    self.blocks -= 1;
                    if self.blocks == 0 && self.context == CssContext::StyleSheet {
                        self.selector_safe = true;
                    }
                    self.declaration_safe = false;
                }
                self.opaque();
            }
            b';' if self.declarations() => self.position = DeclarationPosition::Start,
            b';' if self.context == CssContext::StyleSheet && self.blocks == 0 => {
                self.selector_safe = false;
                self.opaque();
            }
            b':' if self.declarations() => {
                self.position = match self.position {
                    DeclarationPosition::Name(Some(kind)) => {
                        DeclarationPosition::Value(at + 1, kind)
                    }
                    _ => DeclarationPosition::Other,
                };
            }
            b'!' => {
                if self.context == CssContext::StyleSheet && self.blocks == 0 {
                    self.selector_safe = false;
                }
                self.opaque();
            }
            byte if css_space(byte) => {}
            _ => {
                if matches!(
                    self.position,
                    DeclarationPosition::Start | DeclarationPosition::Name(_)
                ) {
                    self.opaque();
                }
            }
        }
    }
}

fn css_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0c)
}

fn css_byte(bytes: &[u8], at: usize, work: &mut ScanWork<'_, '_>) -> Result<Option<u8>, Error> {
    if at < bytes.len() {
        work.charge(1)?;
        Ok(Some(bytes[at]))
    } else {
        Ok(None)
    }
}

fn css_comment(bytes: &[u8], mut at: usize, work: &mut ScanWork<'_, '_>) -> Result<usize, Error> {
    while let Some(byte) = css_byte(bytes, at, work)? {
        at += 1;
        if byte == b'*' && css_byte(bytes, at, work)? == Some(b'/') {
            return Ok(at + 1);
        }
    }
    Ok(at)
}

// Return raw extent only. A lexical failure is distinct from a typed resource
// error; the latter must never become Unsupported or a successful prefix.
fn safe_font_extent(
    bytes: &[u8],
    mut at: usize,
    kind: FontValue,
    work: &mut ScanWork<'_, '_>,
) -> Result<Option<usize>, Error> {
    let mut quote = None;
    while let Some(byte) = css_byte(bytes, at, work)? {
        if byte == b'\\' {
            at += 1;
            let Some(byte) = css_byte(bytes, at, work)? else {
                return Ok(None);
            };
            if matches!(byte, b'\r' | b'\n' | 0x0c) {
                if quote.is_none() {
                    return Ok(None);
                }
                at += 1;
                if byte == b'\r' && css_byte(bytes, at, work)? == Some(b'\n') {
                    at += 1;
                }
            } else if byte.is_ascii_hexdigit() {
                for _ in 0..6 {
                    match css_byte(bytes, at, work)? {
                        Some(byte) if byte.is_ascii_hexdigit() => at += 1,
                        _ => break,
                    }
                }
                if let Some(space) = css_byte(bytes, at, work)?.filter(|byte| css_space(*byte)) {
                    at += 1;
                    if space == b'\r' && css_byte(bytes, at, work)? == Some(b'\n') {
                        at += 1;
                    }
                }
            } else {
                // Remaining UTF-8 continuation bytes cannot be ASCII delimiters.
                at += 1;
            }
            continue;
        }
        if let Some(mark) = quote {
            if byte == mark {
                quote = None;
            } else if matches!(byte, b'\r' | b'\n' | 0x0c) {
                return Ok(None);
            }
            at += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'/' if css_byte(bytes, at + 1, work)? == Some(b'*') => {
                at = css_comment(bytes, at + 2, work)?;
                continue;
            }
            b';' | b'}' | b'{' | b'!' => return Ok(Some(at)),
            b'(' | b')' | b'@' | b'[' | b']' => return Ok(None),
            byte if css_space(byte) || byte.is_ascii_alphanumeric() || byte >= 0x80 => {}
            b'-' | b'_' | b',' => {}
            b'.' | b'%' | b'/' if matches!(kind, FontValue::Shorthand) => {}
            _ => return Ok(None),
        }
        at += 1;
    }
    Ok(quote.is_none().then_some(at))
}

// The url(...) segment owns its own delimiters. In particular its ')' cannot
// accidentally pop an enclosing unknown function in CssPosition.
fn css_url(value: &str, mut at: usize, work: &mut ScanWork<'_, '_>) -> Result<usize, Error> {
    let bytes = value.as_bytes();
    while css_byte(bytes, at, work)?.is_some_and(|byte| byte.is_ascii_whitespace()) {
        at += 1;
    }
    let quote = css_byte(bytes, at, work)?.filter(|byte| matches!(byte, b'\'' | b'"'));
    if quote.is_some() {
        at += 1;
    }
    let mut local = None;
    while let Some(byte) = css_byte(bytes, at, work)? {
        if byte == b'\\' {
            return Err(fail(work.opt, SourceError::UnsupportedCss));
        }
        if Some(byte) == quote || (quote.is_none() && matches!(byte, b')' | b'\'' | b'"')) {
            break;
        }
        if local.is_none() {
            let width = if byte < 0x80 {
                1
            } else if byte < 0xe0 {
                2
            } else if byte < 0xf0 {
                3
            } else {
                4
            };
            work.charge(width as u64)?;
            let scalar = value[at..at + width].chars().next().unwrap();
            if !scalar.is_whitespace() {
                local = Some(scalar == '#');
            }
            at += width;
        } else {
            at += 1;
        }
    }
    if local != Some(true) {
        return Err(fail(work.opt, SourceError::ExternalResource));
    }
    if quote.is_some() && css_byte(bytes, at, work)? == quote {
        at += 1;
        while css_byte(bytes, at, work)?.is_some_and(|byte| byte.is_ascii_whitespace()) {
            at += 1;
        }
    }
    if css_byte(bytes, at, work)? == Some(b')') {
        at += 1;
    }
    Ok(at)
}

fn css_urls(value: &str, context: CssContext, work: &mut ScanWork<'_, '_>) -> Result<(), Error> {
    let opt = work.opt;
    let bytes = value.as_bytes();
    let mut at = 0;
    let mut position = CssPosition::new(context);
    'scan: while let Some(byte) = css_byte(bytes, at, work)? {
        if byte == b'\\' {
            if let Some((start, kind)) = position.font_value() {
                if let Some(end) = safe_font_extent(bytes, start, kind, work)? {
                    at = end;
                    position.opaque();
                    continue;
                }
            }
            return Err(fail(opt, SourceError::UnsupportedCss));
        }
        if byte == b'@' {
            return Err(fail(opt, SourceError::UnsupportedCss));
        }
        if byte == b'/' && css_byte(bytes, at + 1, work)? == Some(b'*') {
            at = css_comment(bytes, at + 2, work)?;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            position.quoted();
            let quote = byte;
            at += 1;
            while let Some(byte) = css_byte(bytes, at, work)? {
                if byte == quote {
                    at += 1;
                    continue 'scan;
                }
                if byte == b'\\' {
                    if let Some((start, kind)) = position.font_value() {
                        if let Some(end) = safe_font_extent(bytes, start, kind, work)? {
                            at = end;
                            position.opaque();
                            continue 'scan;
                        }
                    }
                    return Err(fail(opt, SourceError::UnsupportedCss));
                }
                at += 1;
            }
            continue;
        }
        let start = at;
        if byte.is_ascii_alphabetic() || byte == b'-' || byte == b'_' {
            at += 1;
            while let Some(byte) = css_byte(bytes, at, work)? {
                if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
                    at += 1;
                } else {
                    break;
                }
            }
            position.token(&bytes[start..at]);
            if at - start == 3 && bytes[start..at].eq_ignore_ascii_case(b"url") {
                while css_byte(bytes, at, work)?.is_some_and(|byte| byte.is_ascii_whitespace()) {
                    at += 1;
                }
                if css_byte(bytes, at, work)? == Some(b'(') {
                    position.opaque();
                    at = css_url(value, at + 1, work)?;
                }
            }
        } else {
            position.punct(byte, at);
            at += 1;
        }
    }
    Ok(())
}

#[cfg(feature = "font-svg-gzip")]
pub(crate) fn decode_font_gzip(
    data: &[u8],
    opt: &Options,
) -> Result<markdown_svg_budget::BudgetVec<u8>, Error> {
    let resources = opt
        .processing
        .resources()
        .ok_or_else(|| fail(opt, SourceError::MissingRequestResources))?;
    super::font_gzip::decode(data, resources).map_err(|error| {
        if error.kind == markdown_svg_budget::FailureKind::Decode {
            fail(opt, SourceError::InvalidFontGzip)
        } else {
            Error::Resource(opt.processing.record_resource(error))
        }
    })
}

pub(crate) fn decode_error(opt: &Options, error: Error) -> Error {
    opt.processing
        .record_resource(markdown_svg_budget::Error::new(
            markdown_svg_budget::FailureKind::Decode,
            markdown_svg_budget::LimitKind::None,
        ));
    error
}
pub(crate) fn utf8<'a>(data: &'a [u8], opt: &Options) -> Result<&'a str, Error> {
    std::str::from_utf8(data).map_err(|_| fail(opt, SourceError::InvalidUtf8))
}

pub(crate) fn ignored_font_node(
    node: roxmltree::Node<'_, '_>,
    resources: Option<&markdown_svg_budget::ResourcesRef>,
) -> Result<bool, markdown_svg_budget::Error> {
    if ignored_font_element(node.tag_name().name()) {
        return Ok(true);
    }
    if node.tag_name().name() == "image" {
        if let Some(href) = node
            .attribute("href")
            .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
        {
            if let Some(resources) = resources {
                match data_url::controlled::DataUrl::process(href, resources.clone()) {
                    Ok(url) => {
                        let ignored = url.mime_type().matches("image", "svg+xml");
                        drop(url);
                        resources.checkpoint(0)?;
                        return Ok(ignored);
                    }
                    Err(data_url::controlled::Error::Resource(error)) => return Err(error),
                    // Header/prefix classification remains nonfatal here. The
                    // real image converter handles the original body policy.
                    Err(_) => return Ok(false),
                }
            }
            if let Ok(url) = data_url::DataUrl::process(href) {
                let mime = url.mime_type();
                return Ok(mime.type_ == "image" && mime.subtype == "svg+xml");
            }
        }
    }
    Ok(false)
}

#[cfg(all(test, feature = "text"))]
mod font_profile_tests {
    use super::*;
    use crate::{FontFamily, Tree};
    use markdown_svg_budget::{
        Backend, Error as ResourceError, FailureKind, FirstFailureRef, LimitKind, Limits,
        ResourceKind, ResourcesRef, Ticket,
    };
    use std::sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    };

    struct Ledger {
        next: AtomicUsize,
        live: AtomicUsize,
        source_batches: AtomicUsize,
        reject_batch: usize,
        rejection: ResourceError,
        selected: AtomicUsize,
        paid_work: AtomicU64,
        sixth_batch_before_work: AtomicU64,
    }

    struct TestBackend(Arc<Ledger>);

    impl Backend for TestBackend {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
            let slot = self.0.next.fetch_add(1, Ordering::SeqCst) + 1;
            self.0.live.fetch_add(bytes, Ordering::SeqCst);
            Ticket::new(bytes as u64, slot as u32, 1)
        }

        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ResourceError> {
            Ok(())
        }

        fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
            self.0
                .live
                .fetch_sub(ticket.account_nonce() as usize, Ordering::SeqCst);
            Ok(())
        }

        fn checkpoint(&self, work: u64) -> Result<(), ResourceError> {
            let before = self.0.paid_work.fetch_add(work, Ordering::SeqCst);
            if work == 4096 {
                let batch = self.0.source_batches.fetch_add(1, Ordering::SeqCst) + 1;
                if batch == 6 {
                    self.0
                        .sixth_batch_before_work
                        .store(before, Ordering::SeqCst);
                }
                if batch == self.0.reject_batch {
                    return Err(self.0.rejection);
                }
            }
            Ok(())
        }
    }

    fn request(reject_batch: usize, rejection: ResourceError) -> (ResourcesRef, Arc<Ledger>) {
        request_with_limits(reject_batch, rejection, Limits::default())
    }

    fn request_with_limits(
        reject_batch: usize,
        rejection: ResourceError,
        limits: Limits,
    ) -> (ResourcesRef, Arc<Ledger>) {
        let ledger = Arc::new(Ledger {
            next: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
            source_batches: AtomicUsize::new(0),
            reject_batch,
            rejection,
            selected: AtomicUsize::new(0),
            paid_work: AtomicU64::new(0),
            sixth_batch_before_work: AtomicU64::new(0),
        });
        let resources = ResourcesRef::new_prepaid(
            TestBackend(ledger.clone()),
            FirstFailureRef::new_prepaid(),
            limits,
        );
        (resources, ledger)
    }

    // This original mock owns real glyf outlines and records the actual typed
    // Font seen after SourcePolicy/cascade. It is not a Windows selection oracle.
    fn profile_font() -> Vec<u8> {
        let original = crate::parser::glyph_coverage_sfnt(false, false, false);
        let fixture = include_bytes!("../../tests/fixtures/colr-foreground.ttf");
        let mut tables = Vec::new();
        for source in [original.as_slice(), fixture.as_slice()] {
            let count = u16::from_be_bytes(source[4..6].try_into().unwrap()) as usize;
            for index in 0..count {
                let at = 12 + index * 16;
                let tag: [u8; 4] = source[at..at + 4].try_into().unwrap();
                if (source.as_ptr() == original.as_ptr()) != (tag != *b"cmap") {
                    continue;
                }
                let offset =
                    u32::from_be_bytes(source[at + 8..at + 12].try_into().unwrap()) as usize;
                let length =
                    u32::from_be_bytes(source[at + 12..at + 16].try_into().unwrap()) as usize;
                tables.push((tag, &source[offset..offset + length]));
            }
        }
        assert!(tables.iter().any(|(tag, _)| *tag == *b"cmap"));
        tables.sort_by_key(|(tag, _)| *tag);
        // The old replay fixture supplies GIDs directly and needs no cmap.
        // Genuine ordinary shaping here needs the existing fixture's mapping.
        let mut output = original[..4].to_vec();
        output.extend((tables.len() as u16).to_be_bytes());
        output.resize(12, 0);
        let mut offset = 12 + tables.len() * 16;
        for (tag, data) in &tables {
            output.extend(tag);
            output.extend(0u32.to_be_bytes());
            output.extend((offset as u32).to_be_bytes());
            output.extend((data.len() as u32).to_be_bytes());
            offset += (data.len() + 3) & !3;
        }
        for (_, data) in tables {
            output.extend(data);
            while output.len() % 4 != 0 {
                output.push(0);
            }
        }
        output
    }

    fn profile_options(
        resources: &ResourcesRef,
        ledger: &Arc<Ledger>,
    ) -> (Options<'static>, Arc<Mutex<Vec<Vec<FontFamily>>>>) {
        let mut db = fontdb::Database::new();
        let id = db.push_face_info(fontdb::FaceInfo {
            id: fontdb::ID::dummy(),
            source: fontdb::Source::Binary(Arc::new(profile_font())),
            index: 0,
            families: vec![("Mock".into(), fontdb::Language::English_UnitedStates)],
            post_script_name: String::new(),
            style: fontdb::Style::Normal,
            weight: fontdb::Weight::NORMAL,
            stretch: fontdb::Stretch::Normal,
            monospaced: false,
        });
        let mut options = Options::with_resources_and_fontdb(
            Arc::new(crate::ProcessingContext::with_resources(resources.clone())),
            Arc::new(db),
            "Mock".into(),
            vec!["en".into()],
        );
        let observed = Arc::new(Mutex::new(Vec::new()));
        let capture = observed.clone();
        let ledger = ledger.clone();
        options.font_resolver.select_font = crate::FontSelectionFn::Owned(Box::new(move |font, _| {
            capture.lock().unwrap().push(
                font.families()
                    .iter()
                    .map(|f| match f {
                        FontFamily::Serif => FontFamily::Serif,
                        FontFamily::SansSerif => FontFamily::SansSerif,
                        FontFamily::Cursive => FontFamily::Cursive,
                        FontFamily::Fantasy => FontFamily::Fantasy,
                        FontFamily::Monospace => FontFamily::Monospace,
                        FontFamily::Named(s) => {
                            FontFamily::Named(crate::tree::owner::TreeString::plain(s.to_string()))
                        }
                    })
                    .collect(),
            );
            ledger.selected.fetch_add(1, Ordering::SeqCst);
            Some(id)
        }));
        (options, observed)
    }

    fn xml_attr(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('\r', "&#13;")
            .replace('\n', "&#10;")
    }

    fn inline(css: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><text id="t" x="10" y="50" style="{}">AB</text></svg>"#,
            xml_attr(css)
        )
    }

    fn stylesheet(css: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><style><![CDATA[{css}]]></style><text id="t" class="chosen" x="10" y="50">AB</text></svg>"#
        )
    }

    fn named(value: &str) -> FontFamily {
        FontFamily::Named(crate::tree::owner::TreeString::plain(value.to_owned()))
    }

    fn positive(source: &str, expected: &[FontFamily]) -> (u64, usize) {
        let (resources, ledger) = request(usize::MAX, ResourceError::native(-2174));
        let (options, observed) = profile_options(&resources, &ledger);
        let tree = Tree::from_data_controlled(source.as_bytes(), &options).unwrap();
        assert!(
            tree.has_text_nodes(),
            "the mock must actually reach text conversion: selected={} observed={:?}",
            ledger.selected.load(Ordering::SeqCst),
            observed.lock().unwrap()
        );
        assert!(tree.root().abs_bounding_box().width() > 0.0);
        let values = observed.lock().unwrap();
        assert!(!values.is_empty());
        assert!(
            values.iter().all(|families| families == expected),
            "{values:?}"
        );
        drop(values);
        let measured = (
            resources.statistics().work,
            ledger.source_batches.load(Ordering::SeqCst),
        );
        drop((tree, options, observed));
        assert_eq!(ledger.live.load(Ordering::SeqCst), 0);
        resources.validate_retired().unwrap();
        measured
    }

    fn negative(source: &str, expected: SourceError) {
        let (resources, ledger) = request(usize::MAX, ResourceError::native(-2174));
        let (options, observed) = profile_options(&resources, &ledger);
        let error = Tree::from_data_controlled(source.as_bytes(), &options).unwrap_err();
        assert!(
            matches!(error, Error::SourcePolicy(reason) if reason == expected),
            "{error:?}: {source}"
        );
        assert_eq!(ledger.selected.load(Ordering::SeqCst), 0);
        assert!(observed.lock().unwrap().is_empty());
        let expected_kind = if expected == SourceError::ExternalResource {
            FailureKind::ExternalResource
        } else {
            FailureKind::Unsupported
        };
        assert_eq!(resources.failure().unwrap().kind, expected_kind);
        drop((options, observed));
        assert_eq!(ledger.live.load(Ordering::SeqCst), 0);
        resources.validate_retired().unwrap();
    }

    #[test]
    fn source_profile_font_presentation_inline_stylesheet_and_shorthand_reach_same_typed_list() {
        let expected = [named("Mock"), FontFamily::Serif];
        let presentation = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><text x="10" y="50" font-family="Mo\63 k, \73 erif">AB</text></svg>"#;
        positive(presentation, &expected);
        positive(&inline(r"font-family:Mo\63 k, \73 erif"), &expected);
        positive(
            &stylesheet(r".chosen {font-family:Mo\63 k, \73 erif}"),
            &expected,
        );
        positive(&inline(r"font:12px Mo\63 k, \73 erif"), &expected);
        positive(
            &stylesheet(r".chosen {font:12px Mo\63 k, \73 erif}"),
            &expected,
        );
    }

    #[test]
    fn source_profile_font_literal_url_and_escaped_punctuation_are_only_names() {
        for (value, expected) in [
            (
                r#""url(https://invalid.invalid/x)", Mo\63 k"#,
                vec![named("url(https://invalid.invalid/x)"), named("Mock")],
            ),
            (
                r"Name\(Part\)\,\;\{\}\!\@, Mo\63 k",
                vec![named("Name(Part),;{}!@"), named("Mock")],
            ),
            (
                r#""A\\", "A\\\"B", Mo\63 k"#,
                vec![named("A\\"), named("A\\\"B"), named("Mock")],
            ),
            (
                "\"Mo\\\r\nck\", Mo\\000063k",
                vec![named("Mock"), named("Mock")],
            ),
            (
                r#""A/* ; } ! @ ( */B", Mo\63 k/* ; } ! @ ( \\ */"#,
                vec![named("A/* ; } ! @ ( */B"), named("Mock")],
            ),
            (
                "\u{80}Name, Mo\\63 k",
                vec![named("\u{80}Name"), named("Mock")],
            ),
        ] {
            positive(&inline(&format!("font-family:{value}")), &expected);
            positive(
                &stylesheet(&format!(".chosen{{font-family:{value}}}")),
                &expected,
            );
        }
    }

    #[test]
    fn source_profile_font_candidate_resumes_at_next_declaration_rule_and_important() {
        positive(
            &inline(r"font-family:Mo\63 k!important;fill:red;font-family:Mo\63 k"),
            &[named("Mock")],
        );
        positive(
            &stylesheet(r".chosen{font-family:Mo\63 k}.other{fill:red}.chosen{font:12px Mo\63 k}"),
            &[named("Mock")],
        );
        positive(
            &inline(r"fill:url('#paint') black;font-family:Mo\63 k"),
            &[named("Mock")],
        );
        positive(
            &inline(r"font-family:Mo\63 k/* comment to EOF ; } ! @ \"),
            &[named("Mock")],
        );
    }

    #[test]
    fn source_profile_font_exception_never_hides_external_tail() {
        for css in [
            r"font-family:Mo\63 k;fill:url(https://invalid.invalid/x)",
            r"font-family:Mo\63 k!important;fill:url(https://invalid.invalid/x)",
            r"font-family:Mo\63 k;/**/fill:url(https://invalid.invalid/x)",
            r"font:12px Mo\63 k;stroke:url('https://invalid.invalid/x')",
        ] {
            negative(&inline(css), SourceError::ExternalResource);
        }
        negative(
            &stylesheet(r".chosen{font-family:Mo\63 k}.other{fill:url(https://invalid.invalid/x)}"),
            SourceError::ExternalResource,
        );
    }

    #[test]
    fn source_profile_font_exception_does_not_apply_to_selectors_names_or_unknown_groups() {
        for css in [
            r#".chosen[title="font-family:Mo\63 k"]{fill:red}"#,
            r".chosen:font-family(Mo\63 k){fill:red}",
            r".chosen:foo(;font-family:Mo\63 k){fill:red}",
            r".chosen[foo;font-family:Mo\63 k]{fill:red}",
            r".chosen{unknown:foo(;font-family:Mo\63 k);fill:red}",
            r".chosen{unknown:[;font-family:Mo\63 k];fill:red}",
            r".chosen{unknown:foo(url(#paint);font-family:Mo\63 k);fill:red}",
            r".chosen{unknown:foo(url('#paint');font-family:Mo\63 k);fill:red}",
            r".chosen{unknown:{font-family:Mo\63 k};fill:red}",
            r".chosen{unknown:([)];font-family:Mo\63 k}",
            r".chosen{\66 ont-family:Mo\63 k}",
            r".chosen{*font-family:Mo\63 k}",
            r".chosen{font-family-suffix:Mo\63 k}",
            r".chosen{font-family:Mock!important Mo\63 k}",
            r".chosen:foo(){font-family:Mo\63 k}",
            r".chosen;{font-family:Mo\63 k}",
            r#".chosen"bad"{font-family:Mo\63 k}"#,
        ] {
            negative(&stylesheet(css), SourceError::UnsupportedCss);
        }
        for css in [
            r"unknown:foo(;font-family:Mo\63 k);fill:red",
            r"unknown:[;font-family:Mo\63 k];fill:red",
            r"unknown:{font-family:Mo\63 k};fill:red",
            r"unknown:foo(url(#paint);font-family:Mo\63 k);fill:red",
            r"fill:font-family:Mo\63 k",
            r"font-family:Mock!important Mo\63 k",
        ] {
            negative(&inline(css), SourceError::UnsupportedCss);
        }
        negative(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><path fill="font-family:Mo\63 k" d="M0 0h1v1z"/></svg>"#,
            SourceError::UnsupportedCss,
        );
    }

    #[test]
    fn source_profile_font_function_and_at_rule_escapes_keep_original_gate() {
        for css in [
            r"font-family:u\72l(https://invalid.invalid/x)",
            r"font:var(--x,Mo\63 k)",
            r"font-family:foo(bar),Mo\63 k;fill:url(https://invalid.invalid/x)",
            r"font-family:Mo\63 k @import 'https://invalid.invalid/x'",
            r"\40 import 'https://invalid.invalid/x';font-family:Mock",
        ] {
            negative(&inline(css), SourceError::UnsupportedCss);
        }
        for css in [
            r"@import 'https://invalid.invalid/x';.chosen{font-family:Mo\63 k}",
            r"@font-face{font-family:Mo\63 k;src:url(https://invalid.invalid/x)}",
            r"\40 import 'https://invalid.invalid/x';.chosen{font-family:Mock}",
        ] {
            negative(&stylesheet(css), SourceError::UnsupportedCss);
        }
        negative(
            &inline(r"font-family:foo(url(https://invalid.invalid/x)),Mo\63 k"),
            SourceError::ExternalResource,
        );
        negative(
            &inline(r"font-family:url(https://invalid.invalid/x),Mo\63 k"),
            SourceError::ExternalResource,
        );
    }

    #[test]
    fn source_profile_font_bad_string_and_invalid_continuation_cannot_hide_following_css() {
        for css in [
            "font-family:\"Mo\\63 k\n;fill:url(https://invalid.invalid/x)",
            "font-family:\"Mo\\63 k\r;fill:url(https://invalid.invalid/x)",
            "font-family:\"Mo\\63 k;fill:url(https://invalid.invalid/x)",
            "font-family:\"Mo\\63 k\\",
            "font-family:Mo\\\nck;fill:url(https://invalid.invalid/x)",
        ] {
            negative(
                &stylesheet(&format!(".chosen{{{css}}}")),
                SourceError::UnsupportedCss,
            );
        }
        // XML 1.0 rejects FF before SourcePolicy can receive this CSS byte.
        let source =
            stylesheet(".chosen{font-family:\"Mo\\63 k\u{c};fill:url(https://invalid.invalid/x)}");
        let (resources, ledger) = request(usize::MAX, ResourceError::native(-2174));
        let (options, observed) = profile_options(&resources, &ledger);
        assert!(Tree::from_data_controlled(source.as_bytes(), &options).is_err());
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Decode);
        assert_eq!(ledger.selected.load(Ordering::SeqCst), 0);
        drop((options, observed));
        resources.validate_retired().unwrap();
    }

    #[test]
    fn source_profile_font_exception_keeps_hidden_active_xml_and_external_href_gate() {
        for (tail, reason) in [
            (
                "<g display='none'><script/></g>",
                SourceError::ActiveContent,
            ),
            (
                "<g display='none' onclick='x()'/>",
                SourceError::ActiveContent,
            ),
            (
                "<image display='none' href='https://invalid.invalid/x'/>",
                SourceError::ExternalResource,
            ),
        ] {
            let source = inline(r"font-family:Mo\63 k").replace("</svg>", &format!("{tail}</svg>"));
            negative(&source, reason);
        }
    }

    #[test]
    fn source_profile_font_scan_work_is_linear_for_late_dense_and_many_escapes() {
        for mode in 0..4 {
            let mut measured = Vec::new();
            for count in [4096, 8192] {
                let value = match mode {
                    0 => format!("{}\\41", "A".repeat(count)),
                    1 => format!("\"{}\"", "\\41 ".repeat(count)),
                    2 => format!("/*{}*/Mo\\63 k", "中".repeat(count)),
                    _ => "Mo\\63 k".to_owned(),
                };
                let css = if mode == 3 {
                    format!("font-family:{value};").repeat(count / 32)
                } else {
                    format!("font-family:{value}")
                };
                let expected = match mode {
                    0 => named(&format!("{}A", "A".repeat(count))),
                    1 => named(&"A".repeat(count)),
                    _ => named("Mock"),
                };
                let source = inline(&css);
                let (work, _) = positive(&source, &[expected]);
                assert!(work <= source.len() as u64 * 64 + 1_048_576);
                measured.push(work);
            }
            assert!(
                measured[1] <= measured[0] * 3 + 65_536,
                "mode {mode}: {measured:?}"
            );
        }
        // No escape: the same genuine controlled pipeline still reaches Mock;
        // the admission scanner does not invoke the extra safe-extent pass.
        positive(&inline("font-family:Mock;fill:black"), &[named("Mock")]);
    }

    #[test]
    fn source_profile_font_rewind_cancellation_and_work_errors_preserve_first_fault_and_retire() {
        let source = inline(&format!("font-family:\"{}\\41\"", "A".repeat(16_384)));
        let (resources, ledger) = request(usize::MAX, ResourceError::native(-2174));
        let (options, observed) = profile_options(&resources, &ledger);
        let tree = Tree::from_data_controlled(source.as_bytes(), &options).unwrap();
        let before_sixth = ledger.sixth_batch_before_work.load(Ordering::SeqCst);
        assert!(before_sixth > 0);
        drop((tree, options, observed));
        resources.validate_retired().unwrap();

        // This is the real request Work limit, not only a backend fault with a
        // Quota label. Its rejected refill is in the same safety-rewind batch.
        let (resources, ledger) = request_with_limits(
            usize::MAX,
            ResourceError::native(-2174),
            Limits {
                work: before_sixth,
                ..Limits::default()
            },
        );
        let (options, observed) = profile_options(&resources, &ledger);
        let error = Tree::from_data_controlled(source.as_bytes(), &options).unwrap_err();
        let quota = ResourceError::new(FailureKind::Quota, LimitKind::Work);
        assert!(
            matches!(error, Error::Resource(record) if record == quota),
            "{error:?}"
        );
        assert_eq!(resources.failure(), Some(quota));
        assert_eq!(ledger.selected.load(Ordering::SeqCst), 0);
        drop((options, observed));
        resources.validate_retired().unwrap();
        for rejection in [
            ResourceError::native(-2174),
            ResourceError::new(FailureKind::Cancelled, LimitKind::Work),
            ResourceError::new(FailureKind::Quota, LimitKind::Work),
        ] {
            // Five source batches cover the ordinary prefix; these later
            // batches fail during the charged safety rewind, before typed Font.
            for reject_batch in [6, 7, 8] {
                let (resources, ledger) = request(reject_batch, rejection);
                let (options, observed) = profile_options(&resources, &ledger);
                let error = Tree::from_data_controlled(source.as_bytes(), &options).unwrap_err();
                assert!(
                    matches!(error, Error::Resource(record) if record == rejection),
                    "{error:?}"
                );
                assert_eq!(resources.failure(), Some(rejection));
                assert_eq!(options.processing.resource_failure(), Some(rejection));
                assert_eq!(ledger.selected.load(Ordering::SeqCst), 0);
                drop((options, observed));
                assert_eq!(ledger.live.load(Ordering::SeqCst), 0);
                resources.validate_retired().unwrap();
            }
        }
        let (resources, ledger) = request(usize::MAX, ResourceError::native(-2174));
        let first = ResourceError::native(-2718);
        resources.record(first);
        let (options, observed) = profile_options(&resources, &ledger);
        assert!(Tree::from_data_controlled(source.as_bytes(), &options).is_err());
        assert_eq!(resources.failure(), Some(first));
        assert_eq!(ledger.selected.load(Ordering::SeqCst), 0);
        drop((options, observed));
        resources.validate_retired().unwrap();
        positive(&inline(r"font-family:Mo\63 k"), &[named("Mock")]);
    }
}
