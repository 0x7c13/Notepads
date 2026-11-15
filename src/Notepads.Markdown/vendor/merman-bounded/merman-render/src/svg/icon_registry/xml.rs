use super::IconIdScope;
use super::error::{IconRegistryBuildError, IconRegistryBuildErrorKind};
use super::limits::{IconRegistryBuildLimits, IconRegistryResourceLimitId};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::{NamespaceResolver, ResolveResult};
use quick_xml::reader::NsReader;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
const XLINK_NAMESPACE: &str = "http://www.w3.org/1999/xlink";
const WRAPPER_PREFIX: &str =
    r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">"#;
const WRAPPER_SUFFIX: &str = "</svg>";
const SCOPED_ID_PREFIX: &str = "IconifyId";
const SCOPED_ID_HASH_HEX_LEN: usize = 16;
const DEFS_OPEN: &str = "<defs>";
const DEFS_CLOSE: &str = "</defs>";

#[derive(Debug)]
pub(super) struct ValidatedIconBody {
    source: Arc<str>,
    pack_index: usize,
    element_count: usize,
    uses_xlink: bool,
    edits: Box<[IdEdit]>,
    defs: Box<[DefsRange]>,
    scoped_len: usize,
    omitted_defs_scoped_len: usize,
    defs_content_scoped_len: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct XmlAdmissionUsage {
    pub(super) retained_plan_bytes: usize,
    pub(super) build_work_units: usize,
}

impl ValidatedIconBody {
    #[cfg(any(test, feature = "layout-cytoscape"))]
    pub(super) fn parse(
        body: String,
        pack_index: usize,
        limits: &IconRegistryBuildLimits,
    ) -> Result<Self, IconRegistryBuildError> {
        Self::parse_with_usage(body, pack_index, limits, 0, 0).map(|(body, _)| body)
    }

    pub(super) fn parse_with_usage(
        body: String,
        pack_index: usize,
        limits: &IconRegistryBuildLimits,
        retained_plan_bytes_used: usize,
        build_work_units_used: usize,
    ) -> Result<(Self, XmlAdmissionUsage), IconRegistryBuildError> {
        if body.len() > limits.max_body_bytes {
            return Err(limit_error(
                pack_index,
                IconRegistryResourceLimitId::MaxBodyBytes,
                body.len(),
                limits.max_body_bytes,
                "icon body exceeds the admitted byte limit",
            ));
        }

        let wrapped_len = WRAPPER_PREFIX
            .len()
            .checked_add(body.len())
            .and_then(|length| length.checked_add(WRAPPER_SUFFIX.len()))
            .ok_or_else(|| arithmetic_error(pack_index, "icon XML wrapper length overflowed"))?;
        let mut wrapped = String::with_capacity(wrapped_len);
        wrapped.push_str(WRAPPER_PREFIX);
        wrapped.push_str(&body);
        wrapped.push_str(WRAPPER_SUFFIX);

        let stats = validate_wrapped_fragment(&wrapped, pack_index, limits)?;
        let structure_work = stats.element_count.checked_add(1).ok_or_else(|| {
            arithmetic_error(pack_index, "icon XML structure work accounting overflowed")
        })?;
        ensure_build_work(
            build_work_units_used,
            structure_work,
            pack_index,
            limits,
            "icon XML structure exceeds the build work budget",
        )?;
        let planning_work_start = build_work_units_used
            .checked_add(structure_work)
            .ok_or_else(|| arithmetic_error(pack_index, "icon XML work accounting overflowed"))?;
        let (mut edits, defs) = plan_xml_rewrites(
            &body,
            &wrapped,
            pack_index,
            limits,
            retained_plan_bytes_used,
            planning_work_start,
        )?;
        let sort_work = checked_sort_work(edits.len(), pack_index)?;
        let pre_sort_work = edits
            .len()
            .checked_add(defs.len())
            .and_then(|work| work.checked_add(sort_work))
            .ok_or_else(|| arithmetic_error(pack_index, "icon XML plan work overflowed"))?;
        ensure_build_work(
            planning_work_start,
            pre_sort_work,
            pack_index,
            limits,
            "icon XML rewrite planning exceeds the build work budget",
        )?;
        edits.sort_unstable_by(|left, right| {
            (left.start, left.end, left.id_index).cmp(&(right.start, right.end, right.id_index))
        });
        validate_edit_order(&body, &edits, pack_index)?;
        let scoped_len = checked_scoped_len(body.len(), &edits, pack_index)?;
        let omitted_defs_scoped_len =
            checked_ranges_scoped_len(defs.iter().map(DefsRange::omission), &edits, pack_index)?;
        let defs_content_scoped_len =
            checked_ranges_scoped_len(defs.iter().map(DefsRange::content), &edits, pack_index)?;
        let retained_plan_bytes = checked_plan_bytes(edits.len(), defs.len(), pack_index)?;
        let plan_work = edits
            .len()
            .checked_add(defs.len())
            .and_then(|work| work.checked_add(sort_work))
            .ok_or_else(|| arithmetic_error(pack_index, "icon XML plan work overflowed"))?;

        Ok((
            Self {
                source: Arc::from(body),
                pack_index,
                element_count: stats.element_count,
                uses_xlink: stats.uses_xlink,
                edits: edits.into_boxed_slice(),
                defs: defs.into_boxed_slice(),
                scoped_len,
                omitted_defs_scoped_len,
                defs_content_scoped_len,
            },
            XmlAdmissionUsage {
                retained_plan_bytes,
                build_work_units: structure_work.checked_add(plan_work).ok_or_else(|| {
                    arithmetic_error(pack_index, "icon XML work accounting overflowed")
                })?,
            },
        ))
    }

    pub(super) fn source_len(&self) -> usize {
        self.source.len()
    }

    pub(super) const fn element_count(&self) -> usize {
        self.element_count
    }

    pub(super) const fn edit_count(&self) -> usize {
        self.edits.len()
    }

    pub(super) const fn uses_xlink(&self) -> bool {
        self.uses_xlink
    }

    /// Returns the exact output size for any scope.
    ///
    /// The scope contributes only a fixed-width FNV-1a digest. All checked arithmetic is performed
    /// while the body is admitted, so render-time callers can pre-charge this value directly.
    pub(super) const fn scoped_len(&self) -> usize {
        self.scoped_len
    }

    pub(super) fn scope(&self, scope: IconIdScope) -> Result<String, IconRegistryBuildError> {
        let mut output = String::with_capacity(self.scoped_len);
        let scope_hash = scope.hash();
        let mut source_offset = 0usize;

        for edit in &self.edits {
            let range = edit.range();
            output.push_str(&self.source[source_offset..range.start]);
            write!(
                output,
                "{SCOPED_ID_PREFIX}{scope_hash:016x}{}",
                edit.id_index()
            )
            .map_err(|_| arithmetic_error(self.pack_index, "icon ID scoping failed"))?;
            source_offset = range.end;
        }
        output.push_str(&self.source[source_offset..]);

        if output.len() != self.scoped_len {
            return Err(arithmetic_error(
                self.pack_index,
                "scoped icon body length did not match its validated plan",
            ));
        }
        Ok(output)
    }

    pub(super) fn transformed_scoped_len(
        &self,
        wrapper_start_len: usize,
        wrapper_end_len: usize,
    ) -> Result<usize, IconRegistryBuildError> {
        self.scoped_len
            .checked_sub(self.omitted_defs_scoped_len)
            .and_then(|length| length.checked_add(self.defs_content_scoped_len))
            .and_then(|length| length.checked_add(wrapper_start_len))
            .and_then(|length| length.checked_add(wrapper_end_len))
            .and_then(|length| {
                (self.defs_content_scoped_len > 0)
                    .then_some(DEFS_OPEN.len() + DEFS_CLOSE.len())
                    .map_or(Some(length), |defs_wrapper| {
                        length.checked_add(defs_wrapper)
                    })
            })
            .ok_or_else(|| arithmetic_error(self.pack_index, "transformed icon size overflowed"))
    }

    pub(super) fn scope_transformed(
        &self,
        scope: IconIdScope,
        wrapper_start: &str,
        wrapper_end: &str,
    ) -> Result<String, IconRegistryBuildError> {
        let projected = self.transformed_scoped_len(wrapper_start.len(), wrapper_end.len())?;
        let scope_hash = scope.hash();
        let mut output = String::with_capacity(projected);

        if self.defs_content_scoped_len > 0 {
            output.push_str(DEFS_OPEN);
            let mut edit_index = 0usize;
            for defs in &self.defs {
                write_scoped_range(
                    &self.source,
                    &self.edits,
                    defs.content(),
                    scope_hash,
                    &mut edit_index,
                    &mut output,
                    self.pack_index,
                )?;
            }
            output.push_str(DEFS_CLOSE);
        }

        output.push_str(wrapper_start);
        let mut source_offset = 0usize;
        let mut edit_index = 0usize;
        for defs in &self.defs {
            let omission = defs.omission();
            write_scoped_range(
                &self.source,
                &self.edits,
                source_offset..omission.start,
                scope_hash,
                &mut edit_index,
                &mut output,
                self.pack_index,
            )?;
            skip_edits_through(
                &self.edits,
                omission.clone(),
                &mut edit_index,
                self.pack_index,
            )?;
            source_offset = omission.end;
        }
        write_scoped_range(
            &self.source,
            &self.edits,
            source_offset..self.source.len(),
            scope_hash,
            &mut edit_index,
            &mut output,
            self.pack_index,
        )?;
        output.push_str(wrapper_end);

        if output.len() != projected {
            return Err(arithmetic_error(
                self.pack_index,
                "transformed icon length did not match its validated XML plan",
            ));
        }
        Ok(output)
    }
}

pub(super) fn validate_icon_body(
    body: String,
    pack_index: usize,
    limits: &IconRegistryBuildLimits,
    retained_plan_bytes_used: usize,
    build_work_units_used: usize,
) -> Result<(Arc<ValidatedIconBody>, XmlAdmissionUsage), IconRegistryBuildError> {
    let (body, usage) = ValidatedIconBody::parse_with_usage(
        body,
        pack_index,
        limits,
        retained_plan_bytes_used,
        build_work_units_used,
    )?;
    Ok((Arc::new(body), usage))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdEdit {
    start: u32,
    end: u32,
    id_index: u32,
}

impl IdEdit {
    fn new(
        range: Range<usize>,
        id_index: usize,
        pack_index: usize,
    ) -> Result<Self, IconRegistryBuildError> {
        Ok(Self {
            start: range
                .start
                .try_into()
                .map_err(|_| arithmetic_error(pack_index, "icon ID start offset overflowed"))?,
            end: range
                .end
                .try_into()
                .map_err(|_| arithmetic_error(pack_index, "icon ID end offset overflowed"))?,
            id_index: id_index
                .try_into()
                .map_err(|_| arithmetic_error(pack_index, "icon ID index overflowed"))?,
        })
    }

    fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    const fn id_index(self) -> usize {
        self.id_index as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DefsRange {
    omission_start: u32,
    omission_end: u32,
    content_start: u32,
    content_end: u32,
}

impl DefsRange {
    fn new(
        omission: Range<usize>,
        content: Range<usize>,
        pack_index: usize,
    ) -> Result<Self, IconRegistryBuildError> {
        Ok(Self {
            omission_start: omission
                .start
                .try_into()
                .map_err(|_| arithmetic_error(pack_index, "icon defs start offset overflowed"))?,
            omission_end: omission
                .end
                .try_into()
                .map_err(|_| arithmetic_error(pack_index, "icon defs end offset overflowed"))?,
            content_start: content
                .start
                .try_into()
                .map_err(|_| arithmetic_error(pack_index, "icon defs content start overflowed"))?,
            content_end: content
                .end
                .try_into()
                .map_err(|_| arithmetic_error(pack_index, "icon defs content end overflowed"))?,
        })
    }

    fn omission(&self) -> Range<usize> {
        self.omission_start as usize..self.omission_end as usize
    }

    fn content(&self) -> Range<usize> {
        self.content_start as usize..self.content_end as usize
    }
}

#[derive(Debug, Default)]
struct XmlStats {
    element_count: usize,
    uses_xlink: bool,
}

fn validate_wrapped_fragment(
    wrapped: &str,
    pack_index: usize,
    limits: &IconRegistryBuildLimits,
) -> Result<XmlStats, IconRegistryBuildError> {
    let mut reader = NsReader::from_str(wrapped);
    reader.config_mut().enable_all_checks(true);

    let mut stats = XmlStats::default();
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut root_closed = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|_| invalid_xml_error(pack_index, "icon body is not well-formed XML"))?;
        match event {
            Event::Start(element) => {
                let (namespace, local_name) = reader.resolver().resolve_element(element.name());
                if !root_seen {
                    if depth != 0 || root_closed {
                        return Err(invalid_xml_error(
                            pack_index,
                            "icon XML wrapper has an invalid root structure",
                        ));
                    }
                    let _ =
                        validate_element(&element, namespace, reader.resolver(), true, pack_index)?;
                    if local_name.as_ref() != "svg" {
                        return Err(invalid_xml_error(
                            pack_index,
                            "icon XML wrapper root is not SVG",
                        ));
                    }
                    root_seen = true;
                    depth = 1;
                    continue;
                }

                ensure_inside_wrapper(depth, root_closed, pack_index)?;
                stats.uses_xlink |=
                    validate_element(&element, namespace, reader.resolver(), false, pack_index)?;
                record_element(&mut stats, depth, pack_index, limits)?;
                depth = depth.checked_add(1).ok_or_else(|| {
                    arithmetic_error(pack_index, "icon XML nesting depth overflowed")
                })?;
            }
            Event::Empty(element) => {
                if !root_seen {
                    return Err(invalid_xml_error(
                        pack_index,
                        "icon XML wrapper root is unexpectedly empty",
                    ));
                }
                ensure_inside_wrapper(depth, root_closed, pack_index)?;
                let (namespace, _) = reader.resolver().resolve_element(element.name());
                stats.uses_xlink |=
                    validate_element(&element, namespace, reader.resolver(), false, pack_index)?;
                record_element(&mut stats, depth, pack_index, limits)?;
            }
            Event::End(_) => {
                if !root_seen || root_closed {
                    return Err(invalid_xml_error(
                        pack_index,
                        "icon body closes an element outside its XML wrapper",
                    ));
                }
                depth = depth.checked_sub(1).ok_or_else(|| {
                    invalid_xml_error(pack_index, "icon body contains an unmatched end tag")
                })?;
                if depth == 0 {
                    root_closed = true;
                }
            }
            Event::Text(text) => {
                ensure_inside_wrapper(depth, root_closed, pack_index)?;
                if text.contains("]]>") {
                    return Err(invalid_xml_error(
                        pack_index,
                        "icon body contains a forbidden XML text terminator",
                    ));
                }
                let content = text.xml10_content();
                validate_xml_chars(&content, pack_index, "icon body contains invalid XML text")?;
            }
            Event::CData(text) => {
                ensure_inside_wrapper(depth, root_closed, pack_index)?;
                let content = text.xml10_content();
                validate_xml_chars(&content, pack_index, "icon body contains invalid CDATA")?;
            }
            Event::GeneralRef(reference) => {
                ensure_inside_wrapper(depth, root_closed, pack_index)?;
                validate_general_reference(&reference, pack_index)?;
            }
            Event::Comment(comment) => {
                ensure_inside_wrapper(depth, root_closed, pack_index)?;
                let content = comment.xml10_content();
                validate_xml_chars(
                    &content,
                    pack_index,
                    "icon body contains an invalid XML comment",
                )?;
            }
            Event::Decl(_) => {
                return Err(invalid_xml_error(
                    pack_index,
                    "XML declarations are not allowed in icon bodies",
                ));
            }
            Event::PI(_) => {
                return Err(invalid_xml_error(
                    pack_index,
                    "processing instructions are not allowed in icon bodies",
                ));
            }
            Event::DocType(_) => {
                return Err(invalid_xml_error(
                    pack_index,
                    "document type and entity declarations are not allowed in icon bodies",
                ));
            }
            Event::Eof => break,
        }
    }

    if !root_seen || !root_closed || depth != 0 {
        return Err(invalid_xml_error(
            pack_index,
            "icon XML wrapper is not completely closed",
        ));
    }
    Ok(stats)
}

fn validate_element(
    element: &BytesStart<'_>,
    namespace: ResolveResult<'_>,
    resolver: &NamespaceResolver,
    is_wrapper: bool,
    pack_index: usize,
) -> Result<bool, IconRegistryBuildError> {
    let mut uses_xlink = matches!(
        &namespace,
        ResolveResult::Bound(namespace) if namespace.as_ref() == XLINK_NAMESPACE
    ) && !is_wrapper;
    match namespace {
        ResolveResult::Unknown(_) => {
            return Err(invalid_xml_error(
                pack_index,
                "icon body uses an undeclared namespace prefix",
            ));
        }
        ResolveResult::Bound(namespace) if is_wrapper && namespace.as_ref() != SVG_NAMESPACE => {
            return Err(invalid_xml_error(
                pack_index,
                "icon XML wrapper uses a non-SVG namespace",
            ));
        }
        ResolveResult::Unbound if is_wrapper => {
            return Err(invalid_xml_error(
                pack_index,
                "icon XML wrapper is missing the SVG namespace",
            ));
        }
        ResolveResult::Unbound | ResolveResult::Bound(_) => {}
    }

    validate_xml_qname(element.name().as_ref(), pack_index)?;
    let mut expanded_attributes = HashSet::new();

    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| {
            invalid_xml_error(pack_index, "icon body contains an invalid XML attribute")
        })?;
        validate_xml_qname(attribute.key.as_ref(), pack_index)?;
        if attribute.value.contains('<') {
            return Err(invalid_xml_error(
                pack_index,
                "icon body contains an invalid XML attribute value",
            ));
        }
        let normalized = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|_| {
                invalid_xml_error(
                    pack_index,
                    "icon body contains an invalid XML attribute value",
                )
            })?;
        validate_xml_chars(
            &normalized,
            pack_index,
            "icon body contains an invalid XML attribute value",
        )?;

        if attribute.key.as_namespace_binding().is_some() {
            continue;
        }

        let (attribute_namespace, local_name) = resolver.resolve_attribute(attribute.key);
        let attribute_namespace = match attribute_namespace {
            ResolveResult::Unknown(_) => {
                return Err(invalid_xml_error(
                    pack_index,
                    "icon body uses an undeclared attribute namespace prefix",
                ));
            }
            ResolveResult::Bound(namespace) => {
                uses_xlink |= namespace.as_ref() == XLINK_NAMESPACE;
                Some(namespace.into_inner())
            }
            ResolveResult::Unbound => None,
        };
        if !expanded_attributes.insert((attribute_namespace, local_name.into_inner())) {
            return Err(invalid_xml_error(
                pack_index,
                "icon body contains duplicate expanded XML attributes",
            ));
        }
    }

    Ok(uses_xlink)
}

fn record_element(
    stats: &mut XmlStats,
    fragment_depth: usize,
    pack_index: usize,
    limits: &IconRegistryBuildLimits,
) -> Result<(), IconRegistryBuildError> {
    let elements = stats
        .element_count
        .checked_add(1)
        .ok_or_else(|| arithmetic_error(pack_index, "icon XML element count overflowed"))?;
    if elements > limits.max_xml_elements_per_body {
        return Err(limit_error(
            pack_index,
            IconRegistryResourceLimitId::MaxXmlElementsPerBody,
            elements,
            limits.max_xml_elements_per_body,
            "icon body exceeds the XML element limit",
        ));
    }
    if fragment_depth > limits.max_xml_depth_per_body {
        return Err(limit_error(
            pack_index,
            IconRegistryResourceLimitId::MaxXmlDepthPerBody,
            fragment_depth,
            limits.max_xml_depth_per_body,
            "icon body exceeds the XML nesting-depth limit",
        ));
    }

    stats.element_count = elements;
    Ok(())
}

fn ensure_inside_wrapper(
    depth: usize,
    root_closed: bool,
    pack_index: usize,
) -> Result<(), IconRegistryBuildError> {
    if depth == 0 || root_closed {
        return Err(invalid_xml_error(
            pack_index,
            "icon body contains XML outside its fragment wrapper",
        ));
    }
    Ok(())
}

fn validate_xml_chars(
    content: &str,
    pack_index: usize,
    message: &'static str,
) -> Result<(), IconRegistryBuildError> {
    if content.chars().all(crate::xml::is_xml_1_0_char) {
        Ok(())
    } else {
        Err(invalid_xml_error(pack_index, message))
    }
}

fn validate_general_reference(
    reference: &BytesRef<'_>,
    pack_index: usize,
) -> Result<(), IconRegistryBuildError> {
    if let Some(value) = reference.resolve_char_ref().map_err(|_| {
        invalid_xml_error(
            pack_index,
            "icon body contains an invalid XML character reference",
        )
    })? {
        if crate::xml::is_xml_1_0_char(value) {
            return Ok(());
        }
        return Err(invalid_xml_error(
            pack_index,
            "icon body contains an XML 1.0-forbidden character reference",
        ));
    }

    if matches!(reference.as_ref(), "amp" | "apos" | "gt" | "lt" | "quot") {
        Ok(())
    } else {
        Err(invalid_xml_error(
            pack_index,
            "icon body contains an undeclared XML entity reference",
        ))
    }
}

fn validate_xml_qname(name: &str, pack_index: usize) -> Result<(), IconRegistryBuildError> {
    let mut components = name.split(':');
    let first = components.next().unwrap_or_default();
    let second = components.next();
    if components.next().is_some()
        || !is_valid_xml_ncname(first)
        || second.is_some_and(|component| !is_valid_xml_ncname(component))
    {
        return Err(invalid_xml_error(
            pack_index,
            "icon body contains an invalid XML qualified name",
        ));
    }
    Ok(())
}

fn is_valid_xml_ncname(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(is_xml_name_start_char)
        && chars.all(|character| character != ':' && is_xml_name_char(character))
}

fn is_xml_name_start_char(character: char) -> bool {
    matches!(
        character,
        'A'..='Z'
            | '_'
            | 'a'..='z'
            | '\u{c0}'..='\u{d6}'
            | '\u{d8}'..='\u{f6}'
            | '\u{f8}'..='\u{2ff}'
            | '\u{370}'..='\u{37d}'
            | '\u{37f}'..='\u{1fff}'
            | '\u{200c}'..='\u{200d}'
            | '\u{2070}'..='\u{218f}'
            | '\u{2c00}'..='\u{2fef}'
            | '\u{3001}'..='\u{d7ff}'
            | '\u{f900}'..='\u{fdcf}'
            | '\u{fdf0}'..='\u{fffd}'
            | '\u{10000}'..='\u{effff}'
    )
}

fn is_xml_name_char(character: char) -> bool {
    is_xml_name_start_char(character)
        || matches!(
            character,
            '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}'
        )
}

fn plan_xml_rewrites(
    body: &str,
    wrapped: &str,
    pack_index: usize,
    limits: &IconRegistryBuildLimits,
    retained_plan_bytes_used: usize,
    build_work_units_used: usize,
) -> Result<(Vec<IdEdit>, Vec<DefsRange>), IconRegistryBuildError> {
    let document = roxmltree::Document::parse(wrapped).map_err(|_| {
        invalid_xml_error(
            pack_index,
            "icon body could not be represented as strict XML",
        )
    })?;
    let defs = plan_defs_ranges(
        body,
        &document,
        pack_index,
        limits,
        retained_plan_bytes_used,
    )?;
    let mut id_indexes = HashMap::<String, usize>::new();
    let mut edits = Vec::new();

    for attribute in document
        .descendants()
        .filter(roxmltree::Node::is_element)
        .flat_map(|node| node.attributes())
        .filter(|attribute| attribute.namespace().is_none() && attribute.name() == "id")
    {
        let Some(value_range) = body_source_range(attribute.range_value(), body.len(), pack_index)?
        else {
            continue;
        };
        let id = attribute.value();
        let next_index = id_indexes.len();
        if id_indexes.insert(id.to_owned(), next_index).is_some() {
            return Err(invalid_xml_error(
                pack_index,
                "icon body contains duplicate unnamespaced id values",
            ));
        }
        let raw_value = &body[value_range.clone()];
        let semantic = [SemanticEdit {
            range: 0..id.len(),
            id_index: next_index,
        }];
        let mapped = decoded_ranges_to_raw(raw_value, id, &semantic, DecodeMode::Attribute)
            .map_err(|error| {
                decode_map_error(
                    error,
                    pack_index,
                    "icon body contains an unmappable XML id attribute",
                )
            })?;
        push_id_edit(
            &mut edits,
            offset_range(value_range.start, mapped[0].clone(), pack_index)?,
            next_index,
            defs.len(),
            retained_plan_bytes_used,
            build_work_units_used,
            limits,
            pack_index,
        )?;
    }

    if !id_indexes.is_empty() {
        for attribute in document
            .descendants()
            .filter(roxmltree::Node::is_element)
            .flat_map(|node| node.attributes())
        {
            if attribute.namespace().is_none() && attribute.name() == "id" {
                continue;
            }
            let Some(value_range) =
                body_source_range(attribute.range_value(), body.len(), pack_index)?
            else {
                continue;
            };
            let semantic_edits = attribute_reference_edits(
                attribute,
                &id_indexes,
                edits.len(),
                limits.max_id_rewrite_edits_per_body,
                pack_index,
            )?;
            if semantic_edits.is_empty() {
                continue;
            }
            let raw_value = &body[value_range.clone()];
            let mapped = decoded_ranges_to_raw(
                raw_value,
                attribute.value(),
                &semantic_edits,
                DecodeMode::Attribute,
            )
            .map_err(|error| {
                decode_map_error(
                    error,
                    pack_index,
                    "icon body contains an unmappable XML ID reference",
                )
            })?;
            for (semantic_edit, mapped) in semantic_edits.into_iter().zip(mapped) {
                push_id_edit(
                    &mut edits,
                    offset_range(value_range.start, mapped, pack_index)?,
                    semantic_edit.id_index,
                    defs.len(),
                    retained_plan_bytes_used,
                    build_work_units_used,
                    limits,
                    pack_index,
                )?;
            }
        }

        for node in document.descendants().filter(|node| {
            node.is_text()
                && node.parent().is_some_and(|parent| {
                    parent.is_element()
                        && parent.tag_name().namespace() == Some(SVG_NAMESPACE)
                        && parent.tag_name().name() == "style"
                })
        }) {
            let Some(node_range) = body_source_range(node.range(), body.len(), pack_index)? else {
                continue;
            };
            let (content_range, mode) = style_text_content_range(body, node_range, pack_index)?;
            let raw_text = &body[content_range.clone()];
            let text = node.text().unwrap_or_default();
            let semantic_edits = url_reference_edits(
                text,
                &id_indexes,
                edits.len(),
                limits.max_id_rewrite_edits_per_body,
                pack_index,
            )?;
            let mapped =
                decoded_ranges_to_raw(raw_text, text, &semantic_edits, mode).map_err(|error| {
                    decode_map_error(
                        error,
                        pack_index,
                        "icon body contains unmappable SVG style text",
                    )
                })?;
            for (semantic_edit, mapped) in semantic_edits.into_iter().zip(mapped) {
                push_id_edit(
                    &mut edits,
                    offset_range(content_range.start, mapped, pack_index)?,
                    semantic_edit.id_index,
                    defs.len(),
                    retained_plan_bytes_used,
                    build_work_units_used,
                    limits,
                    pack_index,
                )?;
            }
        }
    }

    Ok((edits, defs))
}

fn validate_edit_order(
    body: &str,
    edits: &[IdEdit],
    pack_index: usize,
) -> Result<(), IconRegistryBuildError> {
    let mut previous_end = 0usize;
    for edit in edits {
        let range = edit.range();
        if range.start < previous_end
            || range.end > body.len()
            || range.start > range.end
            || !body.is_char_boundary(range.start)
            || !body.is_char_boundary(range.end)
        {
            return Err(invalid_xml_error(
                pack_index,
                "icon body produced an invalid overlapping ID rewrite plan",
            ));
        }
        previous_end = range.end;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_id_edit(
    edits: &mut Vec<IdEdit>,
    range: Range<usize>,
    id_index: usize,
    defs_count: usize,
    retained_plan_bytes_used: usize,
    build_work_units_used: usize,
    limits: &IconRegistryBuildLimits,
    pack_index: usize,
) -> Result<(), IconRegistryBuildError> {
    let next_count = edits
        .len()
        .checked_add(1)
        .ok_or_else(|| arithmetic_error(pack_index, "icon ID edit count overflowed"))?;
    if next_count > limits.max_id_rewrite_edits_per_body {
        return Err(limit_error(
            pack_index,
            IconRegistryResourceLimitId::MaxIdRewriteEditsPerBody,
            next_count,
            limits.max_id_rewrite_edits_per_body,
            "icon body exceeds the ID rewrite edit limit",
        ));
    }
    ensure_retained_plan_bytes(
        retained_plan_bytes_used,
        next_count,
        defs_count,
        pack_index,
        limits,
    )?;
    ensure_build_work(
        build_work_units_used,
        next_count
            .checked_add(defs_count)
            .ok_or_else(|| arithmetic_error(pack_index, "icon XML plan work overflowed"))?,
        pack_index,
        limits,
        "icon XML rewrite planning exceeds the build work budget",
    )?;
    edits
        .try_reserve(1)
        .map_err(|_| allocation_error(pack_index, "icon ID rewrite plan allocation failed"))?;
    edits.push(IdEdit::new(range, id_index, pack_index)?);
    Ok(())
}

fn plan_defs_ranges(
    body: &str,
    document: &roxmltree::Document<'_>,
    pack_index: usize,
    limits: &IconRegistryBuildLimits,
    retained_plan_bytes_used: usize,
) -> Result<Vec<DefsRange>, IconRegistryBuildError> {
    let mut source_ranges = Vec::<(Range<usize>, Range<usize>)>::new();
    for node in document.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().namespace() == Some(SVG_NAMESPACE)
            && node.tag_name().name() == "defs"
            && !node.ancestors().skip(1).any(|ancestor| {
                ancestor.is_element()
                    && ancestor.tag_name().namespace() == Some(SVG_NAMESPACE)
                    && ancestor.tag_name().name() == "defs"
            })
    }) {
        let Some(full) = body_source_range(node.range(), body.len(), pack_index)? else {
            continue;
        };
        let content = defs_content_range(body, full.clone(), pack_index)?;
        let next_count = source_ranges
            .len()
            .checked_add(1)
            .ok_or_else(|| arithmetic_error(pack_index, "icon defs count overflowed"))?;
        ensure_retained_plan_bytes(retained_plan_bytes_used, 0, next_count, pack_index, limits)?;
        source_ranges.try_reserve(1).map_err(|_| {
            allocation_error(pack_index, "icon defs extraction plan allocation failed")
        })?;
        source_ranges.push((full, trim_whitespace_range(body, content)));
    }

    let mut defs = Vec::new();
    defs.try_reserve(source_ranges.len())
        .map_err(|_| allocation_error(pack_index, "icon defs extraction plan allocation failed"))?;
    for (index, (full, content)) in source_ranges.iter().enumerate() {
        let lower_bound = defs
            .last()
            .map_or(0, |previous: &DefsRange| previous.omission().end);
        let upper_bound = source_ranges
            .get(index + 1)
            .map_or(body.len(), |(next, _)| next.start);
        let omission = expand_whitespace_range(body, full.clone(), lower_bound, upper_bound);
        if defs
            .last()
            .is_some_and(|previous: &DefsRange| previous.omission().end > omission.start)
        {
            return Err(invalid_xml_error(
                pack_index,
                "icon defs extraction ranges overlap",
            ));
        }
        defs.push(DefsRange::new(omission, content.clone(), pack_index)?);
    }

    Ok(defs)
}

fn defs_content_range(
    body: &str,
    full: Range<usize>,
    pack_index: usize,
) -> Result<Range<usize>, IconRegistryBuildError> {
    let raw = &body[full.clone()];
    let opening_end = xml_opening_tag_end(raw)
        .ok_or_else(|| invalid_xml_error(pack_index, "icon defs opening tag is not readable"))?;
    let opening = &raw[..opening_end];
    if opening
        .trim_end_matches(char::is_whitespace)
        .ends_with("/>")
    {
        let point = full
            .start
            .checked_add(opening_end)
            .ok_or_else(|| arithmetic_error(pack_index, "icon defs range overflowed"))?;
        return Ok(point..point);
    }
    let closing_start = raw
        .rfind("</")
        .ok_or_else(|| invalid_xml_error(pack_index, "icon defs closing tag is not readable"))?;
    let start = full
        .start
        .checked_add(opening_end)
        .ok_or_else(|| arithmetic_error(pack_index, "icon defs content range overflowed"))?;
    let end = full
        .start
        .checked_add(closing_start)
        .ok_or_else(|| arithmetic_error(pack_index, "icon defs content range overflowed"))?;
    if start > end {
        return Err(invalid_xml_error(
            pack_index,
            "icon defs content range is invalid",
        ));
    }
    Ok(start..end)
}

fn xml_opening_tag_end(raw: &str) -> Option<usize> {
    let mut quote = None;
    for (index, byte) in raw.bytes().enumerate() {
        match (quote, byte) {
            (Some(expected), actual) if expected == actual => quote = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return index.checked_add(1),
            (None, _) => {}
        }
    }
    None
}

fn trim_whitespace_range(body: &str, mut range: Range<usize>) -> Range<usize> {
    while range.start < range.end {
        let Some(character) = body[range.start..range.end].chars().next() else {
            break;
        };
        if !character.is_whitespace() {
            break;
        }
        range.start += character.len_utf8();
    }
    while range.start < range.end {
        let Some(character) = body[range.start..range.end].chars().next_back() else {
            break;
        };
        if !character.is_whitespace() {
            break;
        }
        range.end -= character.len_utf8();
    }
    range
}

fn expand_whitespace_range(
    body: &str,
    mut range: Range<usize>,
    lower_bound: usize,
    upper_bound: usize,
) -> Range<usize> {
    while range.start > lower_bound {
        let Some(character) = body[lower_bound..range.start].chars().next_back() else {
            break;
        };
        if !character.is_whitespace() {
            break;
        }
        range.start -= character.len_utf8();
    }
    while range.end < upper_bound {
        let Some(character) = body[range.end..upper_bound].chars().next() else {
            break;
        };
        if !character.is_whitespace() {
            break;
        }
        range.end += character.len_utf8();
    }
    range
}

fn body_source_range(
    document_range: Range<usize>,
    body_len: usize,
    pack_index: usize,
) -> Result<Option<Range<usize>>, IconRegistryBuildError> {
    let body_start = WRAPPER_PREFIX.len();
    let body_end = body_start
        .checked_add(body_len)
        .ok_or_else(|| arithmetic_error(pack_index, "icon body source range overflowed"))?;
    if document_range.end <= body_start || document_range.start >= body_end {
        return Ok(None);
    }
    if document_range.start < body_start || document_range.end > body_end {
        return Err(invalid_xml_error(
            pack_index,
            "icon XML node crosses the fragment wrapper boundary",
        ));
    }
    Ok(Some(
        document_range.start - body_start..document_range.end - body_start,
    ))
}

fn style_text_content_range(
    body: &str,
    node_range: Range<usize>,
    pack_index: usize,
) -> Result<(Range<usize>, DecodeMode), IconRegistryBuildError> {
    let raw = &body[node_range.clone()];
    if raw.starts_with("<![CDATA[") {
        if !raw.ends_with("]]>") {
            return Err(invalid_xml_error(
                pack_index,
                "icon body contains malformed SVG style CDATA",
            ));
        }
        let start = node_range
            .start
            .checked_add(9)
            .ok_or_else(|| arithmetic_error(pack_index, "style CDATA range overflowed"))?;
        let end = node_range.end.checked_sub(3).ok_or_else(|| {
            invalid_xml_error(pack_index, "icon body contains malformed SVG style CDATA")
        })?;
        Ok((start..end, DecodeMode::CData))
    } else {
        Ok((node_range, DecodeMode::Text))
    }
}

#[derive(Debug)]
struct SemanticEdit {
    range: Range<usize>,
    id_index: usize,
}

fn attribute_reference_edits(
    attribute: roxmltree::Attribute<'_, '_>,
    id_indexes: &HashMap<String, usize>,
    base_count: usize,
    maximum: usize,
    pack_index: usize,
) -> Result<Vec<SemanticEdit>, IconRegistryBuildError> {
    if let Some(namespace) = attribute.namespace() {
        if namespace == XLINK_NAMESPACE && attribute.name() == "href" {
            return fragment_reference_edits(
                attribute.value(),
                id_indexes,
                base_count,
                maximum,
                pack_index,
            );
        }
        return Ok(Vec::new());
    }

    match attribute.name() {
        "href" => fragment_reference_edits(
            attribute.value(),
            id_indexes,
            base_count,
            maximum,
            pack_index,
        ),
        "begin" | "end" => smil_reference_edits(
            attribute.value(),
            id_indexes,
            base_count,
            maximum,
            pack_index,
        ),
        "aria-activedescendant"
        | "aria-controls"
        | "aria-describedby"
        | "aria-details"
        | "aria-errormessage"
        | "aria-flowto"
        | "aria-labelledby"
        | "aria-owns" => idref_list_edits(
            attribute.value(),
            id_indexes,
            base_count,
            maximum,
            pack_index,
        ),
        "clip-path" | "color-profile" | "cursor" | "fill" | "filter" | "marker" | "marker-end"
        | "marker-mid" | "marker-start" | "mask" | "stroke" | "style" => url_reference_edits(
            attribute.value(),
            id_indexes,
            base_count,
            maximum,
            pack_index,
        ),
        _ => Ok(Vec::new()),
    }
}

fn fragment_reference_edits(
    value: &str,
    id_indexes: &HashMap<String, usize>,
    base_count: usize,
    maximum: usize,
    pack_index: usize,
) -> Result<Vec<SemanticEdit>, IconRegistryBuildError> {
    let Some(id) = value.strip_prefix('#') else {
        return Ok(Vec::new());
    };
    let Some(&id_index) = id_indexes.get(id) else {
        return Ok(Vec::new());
    };
    let mut edits = Vec::new();
    push_semantic_edit(
        &mut edits,
        SemanticEdit {
            range: 1..value.len(),
            id_index,
        },
        base_count,
        maximum,
        pack_index,
    )?;
    Ok(edits)
}

fn idref_list_edits(
    value: &str,
    id_indexes: &HashMap<String, usize>,
    base_count: usize,
    maximum: usize,
    pack_index: usize,
) -> Result<Vec<SemanticEdit>, IconRegistryBuildError> {
    let mut edits = Vec::new();
    let mut token_start = None;
    for (index, character) in value
        .char_indices()
        .chain(std::iter::once((value.len(), ' ')))
    {
        if character.is_ascii_whitespace() {
            if let Some(start) = token_start.take()
                && let Some(&id_index) = id_indexes.get(&value[start..index])
            {
                push_semantic_edit(
                    &mut edits,
                    SemanticEdit {
                        range: start..index,
                        id_index,
                    },
                    base_count,
                    maximum,
                    pack_index,
                )?;
            }
        } else if token_start.is_none() {
            token_start = Some(index);
        }
    }
    Ok(edits)
}

fn smil_reference_edits(
    value: &str,
    id_indexes: &HashMap<String, usize>,
    base_count: usize,
    maximum: usize,
    pack_index: usize,
) -> Result<Vec<SemanticEdit>, IconRegistryBuildError> {
    let mut edits = Vec::new();
    let mut segment_start = 0usize;
    for segment in value.split_inclusive(';') {
        let content = segment.strip_suffix(';').unwrap_or(segment);
        let leading = content.len() - content.trim_start_matches(char::is_whitespace).len();
        let timing = &content[leading..];
        let mut selected = None;
        for (dot, _) in timing.match_indices('.') {
            if timing[dot + 1..]
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic())
                && let Some(&id_index) = id_indexes.get(&timing[..dot])
            {
                selected = Some((dot, id_index));
            }
        }
        if let Some((id_len, id_index)) = selected {
            let start = segment_start + leading;
            push_semantic_edit(
                &mut edits,
                SemanticEdit {
                    range: start..start + id_len,
                    id_index,
                },
                base_count,
                maximum,
                pack_index,
            )?;
        }
        segment_start += segment.len();
    }
    Ok(edits)
}

fn url_reference_edits(
    value: &str,
    id_indexes: &HashMap<String, usize>,
    base_count: usize,
    maximum: usize,
    pack_index: usize,
) -> Result<Vec<SemanticEdit>, IconRegistryBuildError> {
    let bytes = value.as_bytes();
    let mut edits = Vec::new();
    let mut scan = 0usize;

    while scan + 4 <= bytes.len() {
        let Some(relative) = bytes[scan..]
            .windows(4)
            .position(|window| window.eq_ignore_ascii_case(b"url("))
        else {
            break;
        };
        let function_start = scan + relative;
        let mut cursor = function_start + 4;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let quote = match bytes.get(cursor) {
            Some(b'\'' | b'"') => {
                let quote = bytes[cursor];
                cursor += 1;
                Some(quote)
            }
            _ => None,
        };
        if bytes.get(cursor) != Some(&b'#') {
            scan = function_start + 4;
            continue;
        }

        let id_start = cursor + 1;
        let Some((id_end, function_end)) = url_reference_end(bytes, id_start, quote) else {
            scan = function_start + 4;
            continue;
        };
        if let Some(&id_index) = id_indexes.get(&value[id_start..id_end]) {
            push_semantic_edit(
                &mut edits,
                SemanticEdit {
                    range: id_start..id_end,
                    id_index,
                },
                base_count,
                maximum,
                pack_index,
            )?;
        }
        scan = function_end;
    }

    Ok(edits)
}

fn push_semantic_edit(
    edits: &mut Vec<SemanticEdit>,
    edit: SemanticEdit,
    base_count: usize,
    maximum: usize,
    pack_index: usize,
) -> Result<(), IconRegistryBuildError> {
    let actual = base_count
        .checked_add(edits.len())
        .and_then(|count| count.checked_add(1))
        .ok_or_else(|| arithmetic_error(pack_index, "semantic ID edit count overflowed"))?;
    if actual > maximum {
        return Err(limit_error(
            pack_index,
            IconRegistryResourceLimitId::MaxIdRewriteEditsPerBody,
            actual,
            maximum,
            "icon body exceeds the ID rewrite edit limit",
        ));
    }
    edits
        .try_reserve(1)
        .map_err(|_| allocation_error(pack_index, "semantic ID edit allocation failed"))?;
    edits.push(edit);
    Ok(())
}

fn url_reference_end(bytes: &[u8], id_start: usize, quote: Option<u8>) -> Option<(usize, usize)> {
    if let Some(quote) = quote {
        let relative_end = bytes[id_start..].iter().position(|byte| *byte == quote)?;
        let id_end = id_start + relative_end;
        let mut cursor = id_end + 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        (bytes.get(cursor) == Some(&b')')).then_some((id_end, cursor + 1))
    } else {
        let relative_end = bytes[id_start..].iter().position(|byte| *byte == b')')?;
        let function_end = id_start + relative_end + 1;
        let mut id_end = function_end - 1;
        while id_end > id_start && bytes[id_end - 1].is_ascii_whitespace() {
            id_end -= 1;
        }
        Some((id_end, function_end))
    }
}

#[derive(Debug, Clone, Copy)]
enum DecodeMode {
    Attribute,
    Text,
    CData,
}

#[derive(Debug, Clone, Copy)]
enum DecodeMapError {
    InvalidMapping,
    AllocationFailed,
}

fn decoded_ranges_to_raw(
    raw: &str,
    decoded: &str,
    targets: &[SemanticEdit],
    mode: DecodeMode,
) -> Result<Vec<Range<usize>>, DecodeMapError> {
    let mut previous_end = 0usize;
    for target in targets {
        if target.range.start < previous_end
            || target.range.start > target.range.end
            || target.range.end > decoded.len()
            || !decoded.is_char_boundary(target.range.start)
            || !decoded.is_char_boundary(target.range.end)
        {
            return Err(DecodeMapError::InvalidMapping);
        }
        previous_end = target.range.end;
    }
    if raw == decoded {
        let mut mapped = Vec::new();
        mapped
            .try_reserve(targets.len())
            .map_err(|_| DecodeMapError::AllocationFailed)?;
        mapped.extend(targets.iter().map(|target| target.range.clone()));
        return Ok(mapped);
    }

    let mut raw_cursor = 0usize;
    let mut decoded_cursor = 0usize;
    let mut target_index = 0usize;
    let mut raw_start = None;
    let mut mapped = Vec::new();
    mapped
        .try_reserve(targets.len())
        .map_err(|_| DecodeMapError::AllocationFailed)?;

    let record_boundaries = |decoded_cursor: usize,
                             raw_cursor: usize,
                             target_index: &mut usize,
                             raw_start: &mut Option<usize>,
                             mapped: &mut Vec<Range<usize>>|
     -> Option<()> {
        loop {
            let Some(target) = targets.get(*target_index) else {
                return Some(());
            };
            if raw_start.is_none() {
                if target.range.start != decoded_cursor {
                    return Some(());
                }
                *raw_start = Some(raw_cursor);
            }
            if target.range.end != decoded_cursor {
                return Some(());
            }
            mapped.push(raw_start.take()?..raw_cursor);
            *target_index += 1;
        }
    };

    record_boundaries(
        decoded_cursor,
        raw_cursor,
        &mut target_index,
        &mut raw_start,
        &mut mapped,
    )
    .ok_or(DecodeMapError::InvalidMapping)?;

    while raw_cursor < raw.len() {
        let (character, chunk_end) =
            next_decoded_char(raw, raw_cursor, mode).ok_or(DecodeMapError::InvalidMapping)?;
        let mut encoded = [0u8; 4];
        let produced = character.encode_utf8(&mut encoded);
        let decoded_end = decoded_cursor
            .checked_add(produced.len())
            .ok_or(DecodeMapError::InvalidMapping)?;
        if decoded
            .get(decoded_cursor..decoded_end)
            .ok_or(DecodeMapError::InvalidMapping)?
            != produced
        {
            return Err(DecodeMapError::InvalidMapping);
        }
        raw_cursor = chunk_end;
        decoded_cursor = decoded_end;
        record_boundaries(
            decoded_cursor,
            raw_cursor,
            &mut target_index,
            &mut raw_start,
            &mut mapped,
        )
        .ok_or(DecodeMapError::InvalidMapping)?;
    }

    if decoded_cursor != decoded.len() || target_index != targets.len() || raw_start.is_some() {
        return Err(DecodeMapError::InvalidMapping);
    }
    Ok(mapped)
}

fn decode_map_error(
    error: DecodeMapError,
    pack_index: usize,
    invalid_message: &'static str,
) -> IconRegistryBuildError {
    match error {
        DecodeMapError::InvalidMapping => invalid_xml_error(pack_index, invalid_message),
        DecodeMapError::AllocationFailed => {
            allocation_error(pack_index, "decoded XML range-map allocation failed")
        }
    }
}

fn next_decoded_char(raw: &str, offset: usize, mode: DecodeMode) -> Option<(char, usize)> {
    let remaining = raw.get(offset..)?;
    if !matches!(mode, DecodeMode::CData) && remaining.starts_with('&') {
        return decode_xml_reference(raw, offset);
    }
    if remaining.starts_with("\r\n") {
        return Some((
            match mode {
                DecodeMode::Attribute => ' ',
                DecodeMode::Text | DecodeMode::CData => '\n',
            },
            offset + 2,
        ));
    }

    let character = remaining.chars().next()?;
    let decoded = match (mode, character) {
        (DecodeMode::Attribute, '\r' | '\n' | '\t') => ' ',
        (DecodeMode::Text | DecodeMode::CData, '\r') => '\n',
        _ => character,
    };
    Some((decoded, offset + character.len_utf8()))
}

fn decode_xml_reference(raw: &str, ampersand: usize) -> Option<(char, usize)> {
    let content_start = ampersand.checked_add(1)?;
    let relative_end = raw.get(content_start..)?.find(';')?;
    let content_end = content_start.checked_add(relative_end)?;
    let entity = raw.get(content_start..content_end)?;
    let character = match entity {
        "amp" => '&',
        "apos" => '\'',
        "gt" => '>',
        "lt" => '<',
        "quot" => '"',
        _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16)
            .ok()
            .and_then(char::from_u32)?,
        _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32)?,
        _ => return None,
    };
    if !crate::xml::is_xml_1_0_char(character) {
        return None;
    }
    Some((character, content_end.checked_add(1)?))
}

fn offset_range(
    base: usize,
    range: Range<usize>,
    pack_index: usize,
) -> Result<Range<usize>, IconRegistryBuildError> {
    let start = base
        .checked_add(range.start)
        .ok_or_else(|| arithmetic_error(pack_index, "icon ID source range overflowed"))?;
    let end = base
        .checked_add(range.end)
        .ok_or_else(|| arithmetic_error(pack_index, "icon ID source range overflowed"))?;
    Ok(start..end)
}

fn checked_scoped_len(
    source_len: usize,
    edits: &[IdEdit],
    pack_index: usize,
) -> Result<usize, IconRegistryBuildError> {
    let mut length = source_len;
    for edit in edits {
        let range = edit.range();
        length = length
            .checked_sub(range.len())
            .and_then(|value| value.checked_add(scoped_id_len(edit.id_index())))
            .ok_or_else(|| arithmetic_error(pack_index, "scoped icon body length overflowed"))?;
    }
    Ok(length)
}

fn checked_ranges_scoped_len(
    ranges: impl Iterator<Item = Range<usize>>,
    edits: &[IdEdit],
    pack_index: usize,
) -> Result<usize, IconRegistryBuildError> {
    let mut total = 0usize;
    for range in ranges {
        total = total
            .checked_add(scoped_range_len(range, edits, pack_index)?)
            .ok_or_else(|| arithmetic_error(pack_index, "scoped XML range length overflowed"))?;
    }
    Ok(total)
}

fn scoped_range_len(
    range: Range<usize>,
    edits: &[IdEdit],
    pack_index: usize,
) -> Result<usize, IconRegistryBuildError> {
    let mut length = range.len();
    for edit in edits {
        let edit_range = edit.range();
        if edit_range.end <= range.start {
            continue;
        }
        if edit_range.start >= range.end {
            break;
        }
        if edit_range.start < range.start || edit_range.end > range.end {
            return Err(invalid_xml_error(
                pack_index,
                "icon XML plan splits an ID rewrite range",
            ));
        }
        length = length
            .checked_sub(edit_range.len())
            .and_then(|value| value.checked_add(scoped_id_len(edit.id_index())))
            .ok_or_else(|| arithmetic_error(pack_index, "scoped XML range length overflowed"))?;
    }
    Ok(length)
}

fn checked_plan_bytes(
    edits: usize,
    defs: usize,
    pack_index: usize,
) -> Result<usize, IconRegistryBuildError> {
    edits
        .checked_mul(size_of::<IdEdit>())
        .and_then(|bytes| {
            defs.checked_mul(size_of::<DefsRange>())
                .and_then(|defs_bytes| bytes.checked_add(defs_bytes))
        })
        .ok_or_else(|| arithmetic_error(pack_index, "retained XML plan size overflowed"))
}

fn ensure_retained_plan_bytes(
    retained_plan_bytes_used: usize,
    edits: usize,
    defs: usize,
    pack_index: usize,
    limits: &IconRegistryBuildLimits,
) -> Result<(), IconRegistryBuildError> {
    let plan_bytes = checked_plan_bytes(edits, defs, pack_index)?;
    let actual = retained_plan_bytes_used
        .checked_add(plan_bytes)
        .ok_or_else(|| arithmetic_error(pack_index, "retained XML plan accounting overflowed"))?;
    if actual > limits.max_retained_xml_plan_bytes {
        return Err(limit_error(
            pack_index,
            IconRegistryResourceLimitId::MaxRetainedXmlPlanBytes,
            actual,
            limits.max_retained_xml_plan_bytes,
            "retained XML rewrite plans exceed the registry byte budget",
        ));
    }
    Ok(())
}

fn ensure_build_work(
    build_work_units_used: usize,
    additional: usize,
    pack_index: usize,
    limits: &IconRegistryBuildLimits,
    message: &'static str,
) -> Result<(), IconRegistryBuildError> {
    let actual = build_work_units_used
        .checked_add(additional)
        .ok_or_else(|| arithmetic_error(pack_index, "icon XML work accounting overflowed"))?;
    if actual > limits.max_build_work_units {
        return Err(limit_error(
            pack_index,
            IconRegistryResourceLimitId::MaxBuildWorkUnits,
            actual,
            limits.max_build_work_units,
            message,
        ));
    }
    Ok(())
}

fn checked_sort_work(count: usize, pack_index: usize) -> Result<usize, IconRegistryBuildError> {
    if count < 2 {
        return Ok(0);
    }
    let levels = usize::BITS as usize - (count - 1).leading_zeros() as usize;
    count
        .checked_mul(levels)
        .ok_or_else(|| arithmetic_error(pack_index, "icon XML sort work overflowed"))
}

#[allow(clippy::too_many_arguments)]
fn write_scoped_range(
    source: &str,
    edits: &[IdEdit],
    range: Range<usize>,
    scope_hash: u64,
    edit_index: &mut usize,
    output: &mut String,
    pack_index: usize,
) -> Result<(), IconRegistryBuildError> {
    while edits
        .get(*edit_index)
        .is_some_and(|edit| edit.range().end <= range.start)
    {
        *edit_index += 1;
    }

    let mut source_offset = range.start;
    while let Some(edit) = edits.get(*edit_index) {
        let edit_range = edit.range();
        if edit_range.start >= range.end {
            break;
        }
        if edit_range.start < range.start || edit_range.end > range.end {
            return Err(invalid_xml_error(
                pack_index,
                "icon XML plan splits an ID rewrite range",
            ));
        }
        output.push_str(&source[source_offset..edit_range.start]);
        write!(
            output,
            "{SCOPED_ID_PREFIX}{scope_hash:016x}{}",
            edit.id_index()
        )
        .map_err(|_| arithmetic_error(pack_index, "icon ID scoping failed"))?;
        source_offset = edit_range.end;
        *edit_index += 1;
    }
    output.push_str(&source[source_offset..range.end]);
    Ok(())
}

fn skip_edits_through(
    edits: &[IdEdit],
    omission: Range<usize>,
    edit_index: &mut usize,
    pack_index: usize,
) -> Result<(), IconRegistryBuildError> {
    while let Some(edit) = edits.get(*edit_index) {
        let range = edit.range();
        if range.start >= omission.end {
            break;
        }
        if range.start < omission.start || range.end > omission.end {
            return Err(invalid_xml_error(
                pack_index,
                "icon defs omission splits an ID rewrite range",
            ));
        }
        *edit_index += 1;
    }
    Ok(())
}

fn scoped_id_len(index: usize) -> usize {
    SCOPED_ID_PREFIX.len() + SCOPED_ID_HASH_HEX_LEN + decimal_digits(index)
}

fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1usize;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn invalid_xml_error(pack_index: usize, message: &'static str) -> IconRegistryBuildError {
    IconRegistryBuildError::new(
        IconRegistryBuildErrorKind::InvalidXml,
        Some(pack_index),
        message,
    )
}

fn arithmetic_error(pack_index: usize, message: &'static str) -> IconRegistryBuildError {
    IconRegistryBuildError::new(
        IconRegistryBuildErrorKind::ArithmeticOverflow,
        Some(pack_index),
        message,
    )
}

fn allocation_error(pack_index: usize, message: &'static str) -> IconRegistryBuildError {
    IconRegistryBuildError::new(
        IconRegistryBuildErrorKind::AllocationFailed,
        Some(pack_index),
        message,
    )
}

fn limit_error(
    pack_index: usize,
    limit: IconRegistryResourceLimitId,
    actual: usize,
    maximum: usize,
    message: &'static str,
) -> IconRegistryBuildError {
    IconRegistryBuildError::new(
        IconRegistryBuildErrorKind::ResourceLimitExceeded,
        Some(pack_index),
        message,
    )
    .with_limit(limit, actual, maximum)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(value: &str) -> IconIdScope {
        super::super::icon_id_scope_for_test(value)
    }

    fn parse(body: &str) -> Result<ValidatedIconBody, IconRegistryBuildError> {
        ValidatedIconBody::parse(body.to_owned(), 7, &IconRegistryBuildLimits::fixed())
    }

    fn scoped_id(scope: &str, index: usize) -> String {
        format!(
            "{SCOPED_ID_PREFIX}{:016x}{index}",
            self::scope(scope).hash()
        )
    }

    #[test]
    fn scoping_is_deterministic_and_scope_specific() {
        let body = parse(
            r##"<defs><clipPath id="clip"><path/></clipPath></defs><path clip-path="url(#clip)"/>"##,
        )
        .expect("valid icon body");

        let first = body.scope(scope("diagram-a")).expect("scope succeeds");
        let repeated = body.scope(scope("diagram-a")).expect("scope succeeds");
        let other = body.scope(scope("diagram-b")).expect("scope succeeds");

        assert_eq!(first, repeated);
        assert_ne!(first, other);
        assert_eq!(first.len(), body.scoped_len());
        assert_eq!(body.element_count(), 4);
        assert_eq!(body.edit_count(), 2);
    }

    #[test]
    fn transformed_defs_extraction_uses_the_xml_plan() {
        let body = parse(concat!(
            "<!-- <defs>not-an-element</defs> -->",
            "<defs><linearGradient id=\"paint\"/></defs>",
            "<path fill=\"url(#paint)\"/>"
        ))
        .expect("valid icon body");
        let transformed = body
            .scope_transformed(scope("diagram-a"), "<g>", "</g>")
            .expect("XML-planned transform assembly succeeds");
        let paint = scoped_id("diagram-a", 0);

        assert_eq!(
            transformed.len(),
            body.transformed_scoped_len(3, 4).unwrap()
        );
        assert_eq!(
            transformed,
            format!(
                "<defs><linearGradient id=\"{paint}\"/></defs><g><!-- <defs>not-an-element</defs> --><path fill=\"url(#{paint})\"/></g>"
            )
        );
    }

    #[test]
    fn nested_defs_remain_structural_content_of_the_outer_defs() {
        let body = parse("<defs><defs><path/></defs></defs><circle/>")
            .expect("nested SVG defs are valid XML");
        let transformed = body
            .scope_transformed(scope("diagram-a"), "<g>", "</g>")
            .expect("outermost defs are extracted once");

        assert_eq!(
            transformed,
            "<defs><defs><path/></defs></defs><g><circle/></g>"
        );
        assert_eq!(
            transformed.len(),
            body.transformed_scoped_len(3, 4).unwrap()
        );
    }

    #[test]
    fn many_defs_are_assembled_from_one_bounded_xml_plan() {
        let limits = IconRegistryBuildLimits::fixed();
        let defs_count = limits.max_xml_elements_per_body / 2;
        let source = "<defs><path/></defs>".repeat(defs_count);
        let body = ValidatedIconBody::parse(source, 7, &limits)
            .expect("the maximum element count remains transformable");
        let transformed = body
            .scope_transformed(scope("diagram-a"), "<g>", "</g>")
            .expect("many defs are assembled without repeated body copies");
        let expected = format!("<defs>{}</defs><g></g>", "<path/>".repeat(defs_count));

        assert_eq!(transformed, expected);
        assert_eq!(
            transformed.len(),
            body.transformed_scoped_len(3, 4).unwrap()
        );
    }

    #[test]
    fn id_rewrite_edit_limit_is_exact_and_structured() {
        let source = r#"<path id="shape" aria-controls="shape shape"/>"#;
        let mut exact = IconRegistryBuildLimits::fixed();
        exact.max_id_rewrite_edits_per_body = 3;
        let body = ValidatedIconBody::parse(source.to_owned(), 7, &exact)
            .expect("the exact edit limit is admitted");
        assert_eq!(body.edit_count(), 3);

        let mut plus_one = exact;
        plus_one.max_id_rewrite_edits_per_body = 2;
        let error = ValidatedIconBody::parse(source.to_owned(), 7, &plus_one)
            .expect_err("limit plus one must fail before publishing a plan");
        assert_eq!(
            error.limit_id(),
            Some(IconRegistryResourceLimitId::MaxIdRewriteEditsPerBody.stable_id())
        );
        assert_eq!(error.actual(), Some(3));
        assert_eq!(error.maximum(), Some(2));
    }

    #[test]
    fn retained_xml_plan_byte_limit_is_exact_and_structured() {
        let source = r#"<path id="shape"/>"#;
        let exact_bytes = size_of::<IdEdit>();
        let mut exact = IconRegistryBuildLimits::fixed();
        exact.max_retained_xml_plan_bytes = exact_bytes;
        let (_, usage) = ValidatedIconBody::parse_with_usage(source.to_owned(), 7, &exact, 0, 0)
            .expect("the exact retained plan limit is admitted");
        assert_eq!(usage.retained_plan_bytes, exact_bytes);

        let mut plus_one = exact;
        plus_one.max_retained_xml_plan_bytes = exact_bytes - 1;
        let error = ValidatedIconBody::parse(source.to_owned(), 7, &plus_one)
            .expect_err("plan bytes above the aggregate limit must fail");
        assert_eq!(
            error.limit_id(),
            Some(IconRegistryResourceLimitId::MaxRetainedXmlPlanBytes.stable_id())
        );
        assert_eq!(error.actual(), Some(exact_bytes as u64));
        assert_eq!(error.maximum(), Some((exact_bytes - 1) as u64));
    }

    #[test]
    fn scopes_every_supported_id_reference_shape() {
        let body = parse(
            r##"<defs><clipPath id="clip"/><filter id="paint"/><path id="shape"/><title id="label"/></defs><use href="#shape" xlink:href="#shape" aria-controls="shape label" aria-labelledby="label" clip-path="url(#clip)" style="filter: url( '#paint' )"/><animate begin="shape.end; shape.click" end="shape.end"/><style>.a{clip-path:url(#clip);filter:URL( "#paint" )}</style>"##,
        )
        .expect("valid icon body");
        assert!(body.uses_xlink());

        let scope_name = "diagram-node";
        let scoped = body.scope(scope(scope_name)).expect("scope succeeds");
        let clip = scoped_id(scope_name, 0);
        let paint = scoped_id(scope_name, 1);
        let shape = scoped_id(scope_name, 2);
        let label = scoped_id(scope_name, 3);

        assert!(scoped.contains(&format!(r#"id="{clip}""#)), "{scoped}");
        assert!(scoped.contains(&format!(r#"id="{paint}""#)), "{scoped}");
        assert!(scoped.contains(&format!(r#"id="{shape}""#)), "{scoped}");
        assert!(scoped.contains(&format!(r#"id="{label}""#)), "{scoped}");
        assert!(
            scoped.contains(&format!(r##"href="#{shape}""##)),
            "{scoped}"
        );
        assert!(
            scoped.contains(&format!(r##"xlink:href="#{shape}""##)),
            "{scoped}"
        );
        assert!(
            scoped.contains(&format!(r#"aria-controls="{shape} {label}""#)),
            "{scoped}"
        );
        assert!(
            scoped.contains(&format!(r#"aria-labelledby="{label}""#)),
            "{scoped}"
        );
        assert!(
            scoped.contains(&format!("begin=\"{shape}.end; {shape}.click\"")),
            "{scoped}"
        );
        assert!(scoped.contains(&format!("end=\"{shape}.end\"")), "{scoped}");
        assert!(scoped.contains(&format!("url(#{clip})")), "{scoped}");
        assert!(scoped.contains(&format!("url( '#{paint}' )")), "{scoped}");
        assert!(scoped.contains(&format!("URL( \"#{paint}\" )")), "{scoped}");
    }

    #[test]
    fn namespaced_attributes_are_preserved_verbatim() {
        let source = r##"<g xmlns:meta="urn:test" meta:id="shape" meta:href="#shape" meta:begin="shape.end" meta:aria-controls="shape" meta:fill="url(#shape)"><path id="shape"/><use href="#shape"/></g>"##;
        let body = parse(source).expect("declared metadata namespace is valid");
        let scoped = body.scope(scope("diagram-a")).expect("scope succeeds");

        assert!(scoped.contains(r#"meta:id="shape""#), "{scoped}");
        assert!(scoped.contains(r##"meta:href="#shape""##), "{scoped}");
        assert!(scoped.contains(r#"meta:begin="shape.end""#), "{scoped}");
        assert!(scoped.contains(r#"meta:aria-controls="shape""#), "{scoped}");
        assert!(scoped.contains(r#"meta:fill="url(#shape)""#), "{scoped}");
        assert!(!scoped.contains(r#"<path id="shape""#), "{scoped}");
        assert!(!scoped.contains(r##"<use href="#shape""##), "{scoped}");
    }

    #[test]
    fn duplicate_unnamespaced_ids_are_rejected() {
        let error = parse(r#"<path id="same"/><circle id="same"/>"#)
            .expect_err("duplicate ids must fail admission");

        assert_eq!(error.kind(), IconRegistryBuildErrorKind::InvalidXml);
        assert_eq!(error.pack_index(), Some(7));
        assert!(!error.to_string().contains(r#"id="same""#));
    }

    #[test]
    fn malformed_dtd_pi_declaration_entity_and_namespace_are_rejected_without_fallback() {
        for source in [
            "<path>",
            "<!DOCTYPE svg><path/>",
            "<?icon test?><path/>",
            "<?xml version=\"1.0\"?><path/>",
            "<path>&custom;</path>",
            "<meta:path/>",
        ] {
            let error = parse(source).expect_err("strict XML admission must fail closed");
            assert_eq!(error.kind(), IconRegistryBuildErrorKind::InvalidXml);
            assert!(!error.to_string().contains(source), "{error}");
        }
    }

    #[test]
    fn predefined_and_numeric_character_references_are_accepted_and_mapped() {
        let body = parse(
            r##"<path id="sh&#x61;pe" aria-labelledby="sh&#97;pe"/><use href="#sh&#x61;pe"/><text>A&amp;&#x20;&#32;&apos;&quot;&lt;&gt;</text>"##,
        )
        .expect("standard XML references are accepted");
        let id = scoped_id("diagram-a", 0);
        let scoped = body.scope(scope("diagram-a")).expect("scope succeeds");

        assert!(scoped.contains(&format!(r#"id="{id}""#)), "{scoped}");
        assert!(
            scoped.contains(&format!(r#"aria-labelledby="{id}""#)),
            "{scoped}"
        );
        assert!(scoped.contains(&format!(r##"href="#{id}""##)), "{scoped}");
        assert!(
            scoped.contains("A&amp;&#x20;&#32;&apos;&quot;&lt;&gt;"),
            "{scoped}"
        );
    }

    #[test]
    fn element_and_depth_limits_use_structured_resource_errors() {
        let mut limits = IconRegistryBuildLimits::fixed();
        limits.max_xml_elements_per_body = 1;
        let element_error = ValidatedIconBody::parse("<path/><path/>".into(), 2, &limits)
            .expect_err("element limit must be enforced");
        assert_eq!(
            element_error.kind(),
            IconRegistryBuildErrorKind::ResourceLimitExceeded
        );
        assert_eq!(
            element_error.limit_id(),
            Some(IconRegistryResourceLimitId::MaxXmlElementsPerBody.stable_id())
        );

        let mut limits = IconRegistryBuildLimits::fixed();
        limits.max_xml_depth_per_body = 1;
        let depth_error = ValidatedIconBody::parse("<g><path/></g>".into(), 3, &limits)
            .expect_err("depth limit must be enforced");
        assert_eq!(
            depth_error.kind(),
            IconRegistryBuildErrorKind::ResourceLimitExceeded
        );
        assert_eq!(
            depth_error.limit_id(),
            Some(IconRegistryResourceLimitId::MaxXmlDepthPerBody.stable_id())
        );
    }

    #[test]
    fn xlink_detection_uses_xml_namespaces_not_textual_substrings() {
        let plain = parse(r#"<text>xlink:href</text>"#).expect("valid text");
        assert!(!plain.uses_xlink());

        let linked = parse(r##"<use xlink:href="#target"/><path id="target"/>"##)
            .expect("wrapper supplies the standard xlink namespace");
        assert!(linked.uses_xlink());
    }
}
