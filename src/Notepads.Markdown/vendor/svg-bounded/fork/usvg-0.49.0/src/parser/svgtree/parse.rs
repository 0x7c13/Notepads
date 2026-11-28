// Copyright 2021 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use super::super::reference::RefPath;
use super::storage::{Storage, Work, try_sort};
use core::cmp::Ordering;
use markdown_svg_budget::{
    Error as ResourceError, FailureKind, LimitKind, PaidUtf8, ResourceKind, ResourcesRef,
};

use roxmltree::Error;
use simplecss::Declaration;
use svgtypes::FontShorthand;

use super::{AId, Attribute, Document, EId, NodeData, NodeId, NodeKind, ShortRange};

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const XML_NAMESPACE_NS: &str = "http://www.w3.org/XML/1998/namespace";
// The frozen table has 264 keys; its longest key is 28 bytes. Keep a
// conservative supported-key envelope, guarded against the actual table below.
// Unknown input names need not be hashed with one input-sized SipHasher call.
const MAX_LOOKUP_NAME_BYTES: usize = 64;

fn attribute_name(name: &str, work: &Work) -> Result<Option<AId>, Error> {
    if name.len() > MAX_LOOKUP_NAME_BYTES {
        return Ok(None);
    }
    work.bytes(name.len())?;
    Ok(AId::from_str(name))
}

impl<'input> Document<'input> {
    pub(crate) fn parse_tree_for_source(
        xml: &roxmltree::Document<'input>,
        stylesheet: Option<&'input str>,
        kind: super::super::source_kind::SourceKind,
        resources: Option<ResourcesRef>,
    ) -> Result<Self, Error> {
        let result = parse(xml, stylesheet, kind, resources.clone());
        if let Err(error) = &result {
            if let Some(r) = &resources {
                let typed = match error {
                    Error::Resource(e) => *e,
                    _ => ResourceError::new(FailureKind::Decode, LimitKind::None),
                };
                r.record(typed);
            }
        }
        result
    }

    pub(crate) fn append(&mut self, parent_id: NodeId, kind: NodeKind) -> Result<NodeId, Error> {
        self.work.event()?;
        let new_child_id = NodeId::from(self.nodes.len());
        self.nodes.try_push(NodeData {
            source_id: u32::MAX,
            text_reference: u32::MAX,
            parent: Some(parent_id),
            next_sibling: None,
            children: None,
            kind,
        })?;

        let last_child_id = self.nodes[parent_id.get_usize()].children.map(|(_, id)| id);

        if let Some(id) = last_child_id {
            self.nodes[id.get_usize()].next_sibling = Some(new_child_id);
        }

        self.nodes[parent_id.get_usize()].children = Some(
            if let Some((first_child_id, _)) = self.nodes[parent_id.get_usize()].children {
                (first_child_id, new_child_id)
            } else {
                (new_child_id, new_child_id)
            },
        );

        Ok(new_child_id)
    }

    fn append_attribute(
        &mut self,
        name: AId,
        value: roxmltree::StringStorage<'input>,
        important: bool,
    ) -> Result<(), Error> {
        self.work.event()?;
        if self.attrs.len() >= u32::MAX as usize {
            return Err(self.work.semantic(Error::NodesLimitReached));
        }
        self.attrs.try_push(Attribute {
            name,
            value,
            important,
        })
    }
    pub(super) fn rebuild_links(&mut self) -> Result<(), Error> {
        // Store only node/attribute slots and the actual normalized traversal order.
        let mut links = Storage::new(self.work.resources());
        for (ordinal, node) in self.descendants().enumerate() {
            self.work.event()?;
            if let Some(attribute) = node.attribute_id(AId::Id) {
                links.try_push(super::LinkEntry {
                    node: node.id,
                    attribute,
                    ordinal,
                })?;
            }
        }
        self.work.checked()?;
        try_sort(&mut links, |a, b| {
            Ok(self
                .work
                .compare(
                    self.attrs[a.attribute].value.as_str(),
                    self.attrs[b.attribute].value.as_str(),
                )?
                .then(a.ordinal.cmp(&b.ordinal)))
        })?;
        self.links = links;
        Ok(())
    }
    fn owned_string(&self, text: &str) -> Result<roxmltree::StringStorage<'input>, Error> {
        match self.work.resources() {
            Some(r) => Ok(roxmltree::StringStorage::Paid(
                PaidUtf8::try_from_str(r.clone(), ResourceKind::Scratch, text)
                    .map_err(Error::Resource)?
                    .into_shared()
                    .map_err(Error::Resource)?,
            )),
            None => Ok(roxmltree::StringStorage::new_owned(text)),
        }
    }
}

#[derive(Clone, Copy)]
struct XmlId<'a, 'input> {
    key: &'a str,
    node: roxmltree::Node<'a, 'input>,
    ordinal: usize,
}
pub(super) struct XmlIds<'a, 'input> {
    entries: Storage<XmlId<'a, 'input>>,
    work: Work,
}
impl<'a, 'input> XmlIds<'a, 'input> {
    fn build(
        xml: &'a roxmltree::Document<'input>,
        resources: Option<&ResourcesRef>,
    ) -> Result<Self, Error> {
        let work = Work::new(resources.cloned());
        let mut entries = Storage::new(resources);
        for (ordinal, node) in xml.descendants().enumerate() {
            work.event()?;
            for attr in node.attributes() {
                work.event()?;
                if attr.namespace().is_none() && work.compare(attr.name(), "id")? == Ordering::Equal
                {
                    entries.try_push(XmlId {
                        key: attr.value(),
                        node,
                        ordinal,
                    })?;
                    break;
                }
            }
        }
        try_sort(&mut entries, |a, b| {
            Ok(work.compare(a.key, b.key)?.then(a.ordinal.cmp(&b.ordinal)))
        })?;
        Ok(Self { entries, work })
    }
    pub(super) fn get(&self, key: &str) -> Result<Option<roxmltree::Node<'a, 'input>>, Error> {
        let mut first = 0;
        let mut end = self.entries.len();
        while first < end {
            let mid = first + (end - first) / 2;
            if self.work.compare(self.entries[mid].key, key)? == Ordering::Less {
                first = mid + 1;
            } else {
                end = mid;
            }
        }
        let Some(entry) = self.entries.get(first) else {
            return Ok(None);
        };
        Ok((self.work.compare(entry.key, key)? == Ordering::Equal).then_some(entry.node))
    }
}

fn parse<'input>(
    xml: &roxmltree::Document<'input>,
    injected_stylesheet: Option<&'input str>,
    source_kind: super::super::source_kind::SourceKind,
    resources: Option<ResourcesRef>,
) -> Result<Document<'input>, Error> {
    let mut doc = Document {
        nodes: Storage::new(resources.as_ref()),
        attrs: Storage::new(resources.as_ref()),
        links: Storage::new(resources.as_ref()),
        work: Work::new(resources),
        font_document: source_kind.font(),
        expand_uses: true,
        selection_wrapper: None,
    };

    let id_map = XmlIds::build(xml, doc.work.resources())?;

    // Add a root node.
    doc.nodes.try_push(NodeData {
        source_id: xml.root().id().get(),
        text_reference: u32::MAX,
        parent: None,
        next_sibling: None,
        children: None,
        kind: NodeKind::Root,
    })?;

    let style_sheet = resolve_css(xml, injected_stylesheet, &doc.work)?;

    if let super::super::source_kind::SourceKind::FontGlyph {
        glyph,
        current_color,
        ..
    } = source_kind
    {
        // SourceKind carries u32 even though real glyph IDs are u16.
        // Fixed storage covers every representable private source value.
        let mut target_bytes = [0u8; 15];
        target_bytes[..5].copy_from_slice(b"glyph");
        let mut digits = [0u8; 10];
        let mut n = glyph;
        let mut used = 0;
        loop {
            digits[used] = b'0' + (n % 10) as u8;
            used += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        for i in 0..used {
            target_bytes[5 + i] = digits[used - 1 - i];
        }
        let target_name = core::str::from_utf8(&target_bytes[..5 + used]).unwrap();
        let target = id_map
            .get(target_name)?
            .ok_or_else(|| doc.work.semantic(Error::NoRootNode))?;
        let raw_root = xml.root_element();
        if target == raw_root {
            // Root target is the document itself: viewport is applied once.
            parse_xml_node(
                raw_root,
                xml.root(),
                doc.root().id,
                &style_sheet,
                false,
                0,
                &RefPath::paid_root(xml.root_element().id().get(), doc.work.resources())
                    .map_err(Error::Resource)?,
                &mut doc,
                &id_map,
            )?;
        } else {
            // Preserve the source tree for CSS matching and definition lookup.
            // Non-selected siblings are normalized as definitions, never converted.
            let root_id = parse_svg_element(
                raw_root,
                doc.root().id,
                EId::Svg,
                &style_sheet,
                false,
                &mut doc,
            )?;
            let defs = doc.append(
                root_id,
                NodeKind::Element {
                    tag_name: EId::Defs,
                    attributes: ShortRange::new(0, 0),
                },
            )?;
            doc.expand_uses = false;
            parse_xml_node_children(
                raw_root,
                xml.root(),
                defs,
                &style_sheet,
                false,
                1,
                &RefPath::paid_root(target.id().get(), doc.work.resources())
                    .map_err(Error::Resource)?,
                &mut doc,
                &id_map,
            )?;
            doc.expand_uses = true;
            let use_id = doc.append(
                root_id,
                NodeKind::Element {
                    tag_name: EId::Use,
                    attributes: ShortRange::new(0, 0),
                },
            )?;
            doc.selection_wrapper = Some(use_id);
            parse_use_target(
                target,
                xml.root(),
                use_id,
                &style_sheet,
                1,
                &RefPath::paid_root(target.id().get(), doc.work.resources())
                    .map_err(Error::Resource)?,
                &mut doc,
                &id_map,
            )?;
        }
        // Initial font currentColor is inherited only when the font document has
        // no explicit/CSS color. All normal paint readers use the same cascade.
        let root_id = doc
            .root()
            .first_element_child()
            .ok_or_else(|| doc.work.semantic(Error::NoRootNode))?
            .id;
        let (start, end) = match &doc.nodes[root_id.get_usize()].kind {
            NodeKind::Element { attributes, .. } => (attributes.start, attributes.end),
            _ => unreachable!(),
        };
        if !doc.attrs[start as usize..end as usize]
            .iter()
            .any(|a| a.name == AId::Color)
        {
            // Append at the end, then move this root's range as a block so later
            // child attributes remain unchanged. No raw XML/string wrapper exists.
            let mut color = [b'#'; 9];
            const HEX: &[u8; 16] = b"0123456789abcdef";
            for (i, byte) in [
                current_color.red,
                current_color.green,
                current_color.blue,
                current_color.alpha,
            ]
            .iter()
            .enumerate()
            {
                color[1 + i * 2] = HEX[(byte >> 4) as usize];
                color[2 + i * 2] = HEX[(byte & 15) as usize];
            }
            let color = doc.owned_string(core::str::from_utf8(&color).unwrap())?;
            let begin = doc.attrs.len();
            // Charge the complete copied range before cloning any shared handles.
            let count = (end - start) as usize;
            doc.attrs.try_reserve(count + 1)?;
            for idx in start as usize..end as usize {
                doc.work.event()?;
                let attr = doc.attrs[idx].clone();
                doc.attrs.try_push(attr)?;
            }
            doc.attrs.try_push(Attribute {
                name: AId::Color,
                value: color,
                important: false,
            })?;
            if let NodeKind::Element { attributes, .. } = &mut doc.nodes[root_id.get_usize()].kind {
                *attributes = ShortRange::new(begin as u32, doc.attrs.len() as u32);
            }
        }
    } else {
        parse_xml_node_children(
            xml.root(),
            xml.root(),
            doc.root().id,
            &style_sheet,
            false,
            0,
            &RefPath::paid_root(xml.root_element().id().get(), doc.work.resources())
                .map_err(Error::Resource)?,
            &mut doc,
            &id_map,
        )?;
    }

    // Check that the root element is `svg`.
    match doc.root().first_element_child() {
        Some(child) => {
            if child.tag_name() != Some(EId::Svg) {
                return Err(doc.work.semantic(roxmltree::Error::NoRootNode));
            }
        }
        None => return Err(doc.work.semantic(roxmltree::Error::NoRootNode)),
    }

    doc.rebuild_links()?;

    if doc.work.resources().is_none() {
        fix_recursive_patterns(&mut doc)?;
        fix_recursive_links(EId::ClipPath, AId::ClipPath, &mut doc)?;
        fix_recursive_links(EId::Mask, AId::Mask, &mut doc)?;
        fix_recursive_links(EId::Filter, AId::Filter, &mut doc)?;
        fix_recursive_fe_image(&mut doc)?;
    }
    drop(style_sheet);
    doc.work.checked()?;
    Ok(doc)
}

pub(super) fn parse_tag_name(node: roxmltree::Node, work: &Work) -> Result<Option<EId>, Error> {
    if !node.is_element() {
        return Ok(None);
    }
    if let Some(namespace) = node.tag_name().namespace() {
        if work.compare(namespace, SVG_NS)? != Ordering::Equal {
            return Ok(None);
        }
    }
    // The generated lookup hashes every name byte before its fixed-key probe.
    if node.tag_name().name().len() > MAX_LOOKUP_NAME_BYTES {
        return Ok(None);
    }
    work.bytes(node.tag_name().name().len())?;
    Ok(EId::from_str(node.tag_name().name()))
}

fn parse_xml_node_children<'input>(
    parent: roxmltree::Node<'_, 'input>,
    origin: roxmltree::Node,
    parent_id: NodeId,
    style_sheet: &super::css::StyleSheet,
    ignore_ids: bool,
    depth: u32,
    refs: &RefPath,
    doc: &mut Document<'input>,
    id_map: &XmlIds<'_, 'input>,
) -> Result<(), Error> {
    for node in parent.children() {
        doc.work.event()?;
        parse_xml_node(
            node,
            origin,
            parent_id,
            style_sheet,
            ignore_ids,
            depth,
            refs,
            doc,
            id_map,
        )?;
    }

    Ok(())
}

fn parse_xml_node<'input>(
    node: roxmltree::Node<'_, 'input>,
    origin: roxmltree::Node,
    parent_id: NodeId,
    style_sheet: &super::css::StyleSheet,
    ignore_ids: bool,
    depth: u32,
    refs: &RefPath,
    doc: &mut Document<'input>,
    id_map: &XmlIds<'_, 'input>,
) -> Result<(), Error> {
    doc.work.event()?;
    if depth > 1024 {
        return Err(doc.work.semantic(Error::NodesLimitReached));
    }

    if doc.font_document
        && node.is_element()
        && super::super::source_kind::ignored_font_node(node, doc.work.resources())
            .map_err(Error::Resource)?
    {
        return Ok(());
    }
    let mut tag_name = match parse_tag_name(node, &doc.work)? {
        Some(id) => id,
        None => return Ok(()),
    };

    if tag_name == EId::Style {
        return Ok(());
    }

    // TODO: remove?
    // Treat links as groups.
    if tag_name == EId::A {
        tag_name = EId::G;
    }

    let node_id = parse_svg_element(node, parent_id, tag_name, style_sheet, ignore_ids, doc)?;
    if tag_name == EId::Text {
        super::text::parse_svg_text_element(node, node_id, style_sheet, doc, id_map, refs)?;
    } else if tag_name == EId::Use && doc.expand_uses {
        parse_svg_use_element(
            node,
            origin,
            node_id,
            style_sheet,
            depth + 1,
            refs,
            doc,
            id_map,
        )?;
    } else if tag_name != EId::Use {
        parse_xml_node_children(
            node,
            origin,
            node_id,
            style_sheet,
            ignore_ids,
            depth + 1,
            refs,
            doc,
            id_map,
        )?;
    }

    Ok(())
}

pub(crate) fn parse_svg_element<'input>(
    xml_node: roxmltree::Node<'_, 'input>,
    parent_id: NodeId,
    tag_name: EId,
    style_sheet: &super::css::StyleSheet,
    ignore_ids: bool,
    doc: &mut Document<'input>,
) -> Result<NodeId, Error> {
    doc.work.event()?;
    let attrs_start_idx = doc.attrs.len();
    let mut href_idx: Option<usize> = None;

    // Copy presentational attributes first.
    for attr in xml_node.attributes() {
        doc.work.event()?;
        // xml:lang is a namespaced inherited text property. Preserve its actual
        // XML value in this same CSS/SVG tree; no external fixture metadata.
        if attr.namespace() == Some(XML_NAMESPACE_NS) && attr.name() == "lang" {
            doc.append_attribute(AId::XmlLang, attr.value_storage().clone(), false)?;
            continue;
        }
        match attr.namespace() {
            None | Some(SVG_NS) | Some(XLINK_NS) | Some(XML_NAMESPACE_NS) => {}
            _ => continue,
        }

        let aid = match attribute_name(attr.name(), &doc.work)? {
            Some(v) => v,
            None => continue,
        };

        if doc.font_document
            && xml_node == xml_node.document().root_element()
            && matches!(aid, AId::Clip | AId::Overflow)
        {
            continue;
        }

        // During a `use` resolving, all `id` attributes must be ignored.
        // Otherwise we will get elements with duplicated id's.
        if ignore_ids && aid == AId::Id {
            continue;
        }

        // For some reason those properties are allowed only inside a `style` attribute and CSS.
        if matches!(aid, AId::MixBlendMode | AId::Isolation | AId::FontKerning) {
            continue;
        } else if aid == AId::ImageRendering
            && matches!(
                attr.value(),
                "smooth" | "high-quality" | "crisp-edges" | "pixelated"
            )
        {
            continue;
        }

        // SVG 2: when both `href` and `xlink:href` are present, the unprefixed
        // `href` takes precedence and the XLink one must be ignored, regardless
        // of their order in the source document.
        // https://www.w3.org/TR/SVG2/linking.html#XLinkRefAttrs
        if aid == AId::Href {
            let is_unprefixed = attr.namespace().is_none();
            let is_xlink = attr.namespace() == Some(XLINK_NS);
            if !is_unprefixed && !is_xlink {
                continue;
            }

            if let Some(idx) = href_idx {
                // An `href` was already stored. Only an unprefixed `href` is
                // allowed to override it; an `xlink:href` is ignored.
                if is_unprefixed {
                    doc.attrs[idx].value = attr.value_storage().clone();
                }
                continue;
            }
        }

        let added = append_attribute(
            parent_id,
            tag_name,
            aid,
            attr.value_storage().clone(),
            false,
            doc,
        )?;
        if added && aid == AId::Href {
            href_idx = Some(doc.attrs.len() - 1);
        }
    }

    let ignore_root_clip = doc.font_document && xml_node == xml_node.document().root_element();
    let work = Work::new(doc.work.resources().cloned());
    let mut insert_attribute = |aid, value: &str, important: bool| -> Result<(), Error> {
        work.event()?;
        if ignore_root_clip && matches!(aid, AId::Clip | AId::Overflow) {
            return Ok(());
        }
        // Check that attribute already exists.
        let mut idx = None;
        for (i, attr) in doc.attrs[attrs_start_idx..].iter().enumerate() {
            work.event()?;
            if attr.name == aid {
                idx = Some(i);
                break;
            }
        }
        let owned = doc.owned_string(value)?;

        // Append an attribute as usual.
        let added = append_attribute(parent_id, tag_name, aid, owned, important, doc)?;

        // Check that attribute was actually added, because it could be skipped.
        if added {
            if let Some(idx) = idx {
                let last_idx = doc.attrs.len() - 1;
                let existing_idx = attrs_start_idx + idx;

                // See https://developer.mozilla.org/en-US/docs/Web/CSS/important
                // When a declaration is important, the order of precedence is reversed.
                // Declarations marked as important in the user-agent style sheets override
                // all important declarations in the user style sheets. Similarly, all important
                // declarations in the user style sheets override all important declarations in the
                // author's style sheets. Finally, all important declarations take precedence over
                // all animations.
                //
                // Which means:
                // 1) Existing is not important, new is not important -> swap
                // 2) Existing is important, new is not important -> don't swap
                // 3) Existing is not important, new is important -> swap
                // 4) Existing is important, new is important -> don't swap (since the order
                // is reversed, so existing important attributes take precedence over new
                // important attributes)
                let has_precedence = !doc.attrs[existing_idx].important;

                if has_precedence {
                    doc.attrs.swap(existing_idx, last_idx);
                }

                // Remove last.
                doc.attrs.pop();
            }
        }
        Ok(())
    };

    let mut write_declaration = |declaration: &Declaration| -> Result<(), Error> {
        work.event()?;
        work.bytes(declaration.name.len())?;
        work.bytes(declaration.value.len())?;
        // TODO: perform XML attribute normalization
        let imp = declaration.important;
        let val = declaration.value;

        if declaration.name == "marker" {
            insert_attribute(AId::MarkerStart, val, imp)?;
            insert_attribute(AId::MarkerMid, val, imp)?;
            insert_attribute(AId::MarkerEnd, val, imp)?;
        } else if declaration.name == "font" {
            if let Ok(shorthand) = FontShorthand::from_str(val) {
                // First we need to reset all values to their default.
                insert_attribute(AId::FontStyle, "normal", imp)?;
                insert_attribute(AId::FontVariant, "normal", imp)?;
                insert_attribute(AId::FontWeight, "normal", imp)?;
                insert_attribute(AId::FontStretch, "normal", imp)?;
                insert_attribute(AId::LineHeight, "normal", imp)?;
                insert_attribute(AId::FontSizeAdjust, "none", imp)?;
                insert_attribute(AId::FontKerning, "auto", imp)?;
                insert_attribute(AId::FontVariantCaps, "normal", imp)?;
                insert_attribute(AId::FontVariantLigatures, "normal", imp)?;
                insert_attribute(AId::FontVariantNumeric, "normal", imp)?;
                insert_attribute(AId::FontVariantEastAsian, "normal", imp)?;
                insert_attribute(AId::FontVariantPosition, "normal", imp)?;

                // Then, we set the properties that have been declared.
                shorthand
                    .font_stretch
                    .map(|s| insert_attribute(AId::FontStretch, s, imp))
                    .transpose()?;
                shorthand
                    .font_weight
                    .map(|s| insert_attribute(AId::FontWeight, s, imp))
                    .transpose()?;
                shorthand
                    .font_variant
                    .map(|s| insert_attribute(AId::FontVariant, s, imp))
                    .transpose()?;
                shorthand
                    .font_style
                    .map(|s| insert_attribute(AId::FontStyle, s, imp))
                    .transpose()?;
                insert_attribute(AId::FontSize, shorthand.font_size, imp)?;
                insert_attribute(AId::FontFamily, shorthand.font_family, imp)?;
            } else {
                log::warn!(
                    "Failed to parse {} value: '{}'",
                    AId::Font,
                    declaration.value
                );
            }
        } else if let Some(aid) = attribute_name(declaration.name, &work)? {
            // Parse only the presentation attributes.
            if aid.is_presentation() {
                insert_attribute(aid, val, imp)?;
            }
        }
        Ok(())
    };

    // Both facades use the same CSS grammar and matcher. A paid failure is
    // never converted into an ordinary non-matching/empty declaration list.
    style_sheet.apply(
        &XmlNode {
            node: xml_node,
            work: &work,
        },
        &work,
        &mut write_declaration,
    )?;
    if let Some(value) = xml_node.attribute("style") {
        super::css::inline(value, &work, &mut write_declaration)?;
    }

    if doc.nodes.len() > 1_000_000 {
        return Err(doc.work.semantic(Error::NodesLimitReached));
    }

    let node_id = doc.append(
        parent_id,
        NodeKind::Element {
            tag_name,
            attributes: ShortRange::new(attrs_start_idx as u32, doc.attrs.len() as u32),
        },
    )?;

    doc.nodes[node_id.get_usize()].source_id = xml_node.id().get();
    Ok(node_id)
}

fn append_attribute<'input>(
    parent_id: NodeId,
    tag_name: EId,
    aid: AId,
    value: roxmltree::StringStorage<'input>,
    important: bool,
    doc: &mut Document<'input>,
) -> Result<bool, Error> {
    doc.work.event()?;
    match aid {
        // The `style` attribute will be split into attributes, so we don't need it.
        AId::Style |
        // No need to copy a `class` attribute since CSS were already resolved.
        AId::Class => return Ok(false),
        _ => {}
    }

    // Ignore `xlink:href` on `tspan` (which was originally `tref` or `a`),
    // because we will convert `tref` into `tspan` anyway.
    if tag_name == EId::Tspan && aid == AId::Href {
        return Ok(false);
    }

    if aid.allows_inherit_value() && &*value == "inherit" {
        return resolve_inherit(parent_id, aid, doc);
    }

    doc.append_attribute(aid, value, important)?;
    Ok(true)
}

fn resolve_inherit(parent_id: NodeId, aid: AId, doc: &mut Document) -> Result<bool, Error> {
    let node_id = if aid.is_inheritable() {
        doc.get(parent_id)
            .ancestors()
            .find(|n| n.has_attribute(aid))
            .map(|n| n.id)
    } else {
        Some(parent_id)
    };
    doc.work.checked()?;
    if let Some(node_id) = node_id {
        let slot = doc.get(node_id).attribute_id(aid);
        doc.work.checked()?;
        if let Some(slot) = slot {
            let attr = doc.attrs[slot].clone();
            doc.append_attribute(aid, attr.value, attr.important)?;
            return Ok(true);
        }
    }

    // Fallback to a default value if possible.
    let value = match aid {
        AId::ImageRendering | AId::ShapeRendering | AId::TextRendering => "auto",

        AId::ClipPath
        | AId::Filter
        | AId::MarkerEnd
        | AId::MarkerMid
        | AId::MarkerStart
        | AId::Mask
        | AId::Stroke
        | AId::StrokeDasharray
        | AId::TextDecoration => "none",

        AId::FontStretch
        | AId::FontStyle
        | AId::FontVariant
        | AId::FontWeight
        | AId::LetterSpacing
        | AId::WordSpacing => "normal",

        AId::Fill | AId::FloodColor | AId::StopColor => "black",

        AId::FillOpacity
        | AId::FloodOpacity
        | AId::Opacity
        | AId::StopOpacity
        | AId::StrokeOpacity => "1",

        AId::ClipRule | AId::FillRule => "nonzero",

        AId::BaselineShift => "baseline",
        AId::ColorInterpolationFilters => "linearRGB",
        AId::Direction => "ltr",
        AId::Display => "inline",
        AId::FontSize => "medium",
        AId::Overflow => "visible",
        AId::StrokeDashoffset => "0",
        AId::StrokeLinecap => "butt",
        AId::StrokeLinejoin => "miter",
        AId::StrokeMiterlimit => "4",
        AId::StrokeWidth => "1",
        AId::TextAnchor => "start",
        AId::Visibility => "visible",
        AId::WritingMode => "lr-tb",
        _ => return Ok(false),
    };

    doc.append_attribute(aid, roxmltree::StringStorage::Borrowed(value), false)?;
    Ok(true)
}

fn resolve_href<'a, 'input: 'a>(
    node: roxmltree::Node<'a, 'input>,
    id_map: &XmlIds<'a, 'input>,
) -> Result<Option<roxmltree::Node<'a, 'input>>, Error> {
    // See the comment in `parse_svg_element` about `href` precedence.
    //
    // Note: `roxmltree::Node::attribute("href")` matches by local name only and
    // would return whichever `href`/`xlink:href` comes first, so we have to
    // filter by namespace explicitly.
    let mut unprefixed = None;
    let mut xlink = None;
    for attr in node.attributes() {
        id_map.work.event()?;
        if id_map.work.compare(attr.name(), "href")? == Ordering::Equal {
            if attr.namespace().is_none() {
                unprefixed = Some(attr.value());
                break;
            }
            if attr.namespace() == Some(XLINK_NS) {
                xlink = Some(attr.value());
            }
        }
    }
    let link_value = unprefixed.or(xlink);
    let Some(link_value) = link_value else {
        return Ok(None);
    };
    id_map.work.bytes(link_value.len())?;
    let Ok(link_id) = svgtypes::IRI::from_str(link_value) else {
        return Ok(None);
    };
    id_map.get(link_id.0)
}

fn parse_svg_use_element<'input>(
    node: roxmltree::Node<'_, 'input>,
    origin: roxmltree::Node,
    parent_id: NodeId,
    style_sheet: &super::css::StyleSheet,
    depth: u32,
    refs: &RefPath,
    doc: &mut Document<'input>,
    id_map: &XmlIds<'_, 'input>,
) -> Result<(), Error> {
    let link = match resolve_href(node, id_map)? {
        Some(v) => v,
        None => return Ok(()),
    };

    if doc.work.resources().is_some() && link == node {
        return Err(super::storage::failure(
            doc.work.resources(),
            ResourceError::new(FailureKind::Decode, LimitKind::None),
        ));
    }
    if doc.work.resources().is_none() {
        if link == node || link == origin {
            log::warn!(
                "Recursive 'use' detected. '{}' will be skipped.",
                node.attribute((SVG_NS, "id")).unwrap_or_default()
            );
            return Ok(());
        }

        // Make sure we're linked to an SVG element.
        if parse_tag_name(link, &doc.work)?.is_none() {
            return Ok(());
        }

        // Check that none of the linked node's children reference current `use` node
        // via other `use` node.
        //
        // Example:
        // <g id="g1">
        //     <use xlink:href="#use1" id="use2"/>
        // </g>
        // <use xlink:href="#g1" id="use1"/>
        //
        // `use2` should be removed.
        //
        // Also, child should not reference its parent:
        // <g id="g1">
        //     <use xlink:href="#g1" id="use1"/>
        // </g>
        //
        // `use1` should be removed.
        let mut is_recursive = false;
        for link_child in link.descendants().skip(1) {
            doc.work.event()?;
            if !link_child.has_tag_name((SVG_NS, "use")) {
                continue;
            }
            if let Some(link2) = resolve_href(link_child, id_map)? {
                if link2 == node || link2 == link {
                    is_recursive = true;
                    break;
                }
            }
        }

        if is_recursive {
            log::warn!(
                "Recursive 'use' detected. '{}' will be skipped.",
                node.attribute((SVG_NS, "id")).unwrap_or_default()
            );
            return Ok(());
        }
    }
    parse_use_target(link, node, parent_id, style_sheet, depth, refs, doc, id_map)
}

// One shared expansion body for authored use and the OpenType selected target.
fn parse_use_target<'input>(
    link: roxmltree::Node<'_, 'input>,
    origin: roxmltree::Node,
    parent_id: NodeId,
    style_sheet: &super::css::StyleSheet,
    depth: u32,
    refs: &RefPath,
    doc: &mut Document<'input>,
    id_map: &XmlIds<'_, 'input>,
) -> Result<(), Error> {
    let target_refs;
    let refs = if doc.selection_wrapper == Some(parent_id) {
        refs
    } else {
        target_refs = refs
            .enter(link.id().get(), doc.work.resources())
            .map_err(Error::Resource)?;
        &target_refs
    };
    parse_xml_node(
        link,
        origin,
        parent_id,
        style_sheet,
        true,
        depth + 1,
        refs,
        doc,
        id_map,
    )
}

fn resolve_css<'a>(
    xml: &'a roxmltree::Document<'a>,
    style_sheet: Option<&'a str>,
    work: &Work,
) -> Result<super::css::StyleSheet<'a>, Error> {
    let mut sheet = super::css::StyleSheet::new(work.resources());

    // Injected style sheets do not override internal ones (we mimic the logic of rsvg-convert),
    // so we need to parse it first.
    if let Some(style_sheet) = style_sheet {
        sheet.try_parse_more(style_sheet)?;
    }

    for node in xml.descendants() {
        work.event()?;
        if !node.has_tag_name("style") {
            continue;
        }
        match node.attribute("type") {
            Some("text/css") => {}
            Some(_) => continue,
            None => {}
        }

        let text = match node.text() {
            Some(v) => v,
            None => continue,
        };

        sheet.try_parse_more(text)?;
    }

    Ok(sheet)
}

struct XmlNode<'a, 'input, 'work> {
    node: roxmltree::Node<'a, 'input>,
    work: &'work Work,
}
impl simplecss::Element for XmlNode<'_, '_, '_> {
    fn parent_element(&self) -> Option<Self> {
        let mut node = self.node.parent();
        while let Some(candidate) = node {
            self.work.event().ok()?;
            if candidate.is_element() {
                return Some(Self {
                    node: candidate,
                    work: self.work,
                });
            }
            node = candidate.parent();
        }
        None
    }
    fn prev_sibling_element(&self) -> Option<Self> {
        let mut node = self.node.prev_sibling();
        while let Some(candidate) = node {
            self.work.event().ok()?;
            if candidate.is_element() {
                return Some(Self {
                    node: candidate,
                    work: self.work,
                });
            }
            node = candidate.prev_sibling();
        }
        None
    }
    fn has_local_name(&self, name: &str) -> bool {
        self.work
            .compare(self.node.tag_name().name(), name)
            .is_ok_and(|v| v == Ordering::Equal)
    }
    fn attribute_matches(&self, name: &str, operator: simplecss::AttributeOperator) -> bool {
        let result = (|| -> Result<bool, Error> {
            for attr in self.node.attributes() {
                self.work.event()?;
                if attr.namespace().is_none()
                    && self.work.compare(attr.name(), name)? == Ordering::Equal
                {
                    return attribute_operator_matches(self.work, operator, attr.value());
                }
            }
            Ok(false)
        })();
        result.unwrap_or(false)
    }
    fn pseudo_class_matches(&self, class: simplecss::PseudoClass) -> bool {
        if self.work.event().is_err() {
            return false;
        }
        match class {
            simplecss::PseudoClass::FirstChild => self.prev_sibling_element().is_none(),
            _ => false,
        }
    }
}
fn attribute_operator_matches(
    work: &Work,
    operator: simplecss::AttributeOperator,
    value: &str,
) -> Result<bool, Error> {
    use simplecss::AttributeOperator::*;
    match operator {
        Exists => Ok(true),
        Matches(v) => Ok(work.compare(value, v)? == Ordering::Equal),
        StartsWith(v) => {
            if work.compare(value, v)? == Ordering::Equal {
                return Ok(true);
            }
            let Some(prefix) = value.get(..v.len()) else {
                return Ok(false);
            };
            Ok(work.compare(prefix, v)? == Ordering::Equal
                && value.as_bytes().get(v.len()) == Some(&b'-'))
        }
        Contains(v) => {
            let bytes = value.as_bytes();
            let mut start = 0;
            let mut examined = 0;
            for (offset, byte) in bytes.iter().copied().enumerate() {
                examined += 1;
                if examined == 4096 {
                    work.bytes(examined)?;
                    examined = 0;
                }
                if byte == b' ' {
                    work.bytes(examined)?;
                    examined = 0;
                    if work.compare(&value[start..offset], v)? == Ordering::Equal {
                        return Ok(true);
                    }
                    start = offset + 1;
                }
            }
            work.bytes(examined)?;
            Ok(work.compare(&value[start..], v)? == Ordering::Equal)
        }
    }
}

fn fix_recursive_patterns(doc: &mut Document) -> Result<(), Error> {
    while let Some(node_id) = find_recursive_pattern(AId::Fill, doc)? {
        let idx = doc
            .get(node_id)
            .attribute_id(AId::Fill)
            .ok_or_else(|| doc.work.semantic(Error::NoRootNode))?;
        doc.attrs[idx].value = roxmltree::StringStorage::Borrowed("none");
    }

    while let Some(node_id) = find_recursive_pattern(AId::Stroke, doc)? {
        let idx = doc
            .get(node_id)
            .attribute_id(AId::Stroke)
            .ok_or_else(|| doc.work.semantic(Error::NoRootNode))?;
        doc.attrs[idx].value = roxmltree::StringStorage::Borrowed("none");
    }
    doc.work.checked()
}

fn find_recursive_pattern(aid: AId, doc: &Document) -> Result<Option<NodeId>, Error> {
    for pattern_node in doc
        .root()
        .descendants()
        .filter(|n| n.tag_name() == Some(EId::Pattern))
    {
        for node in pattern_node.descendants() {
            let value = match node.attribute::<&str>(aid) {
                Some(v) => v,
                None => continue,
            };

            doc.work.bytes(value.len())?;
            if let Ok(svgtypes::Paint::FuncIRI(link_id, _)) = svgtypes::Paint::from_str(value) {
                if doc.work.compare(link_id, pattern_node.element_id())? == Ordering::Equal {
                    // If a pattern child has a link to the pattern itself
                    // then we have to replace it with `none`.
                    // Otherwise we will get endless loop/recursion and stack overflow.
                    return Ok(Some(node.id));
                } else {
                    // Check that linked node children doesn't link this pattern.
                    if let Some(linked_node) = doc.element_by_id(link_id) {
                        for node2 in linked_node.descendants() {
                            let value2 = match node2.attribute::<&str>(aid) {
                                Some(v) => v,
                                None => continue,
                            };

                            doc.work.bytes(value2.len())?;
                            if let Ok(svgtypes::Paint::FuncIRI(link_id2, _)) =
                                svgtypes::Paint::from_str(value2)
                            {
                                if doc.work.compare(link_id2, pattern_node.element_id())?
                                    == Ordering::Equal
                                {
                                    return Ok(Some(node2.id));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    doc.work.checked()?;
    Ok(None)
}

fn fix_recursive_links(eid: EId, aid: AId, doc: &mut Document) -> Result<(), Error> {
    while let Some(node_id) = find_recursive_link(eid, aid, doc)? {
        let idx = doc
            .get(node_id)
            .attribute_id(aid)
            .ok_or_else(|| doc.work.semantic(Error::NoRootNode))?;
        doc.attrs[idx].value = roxmltree::StringStorage::Borrowed("none");
    }
    doc.work.checked()
}

fn find_recursive_link(eid: EId, aid: AId, doc: &Document) -> Result<Option<NodeId>, Error> {
    for node in doc
        .root()
        .descendants()
        .filter(|n| n.tag_name() == Some(eid))
    {
        for child in node.descendants() {
            if let Some(link) = child.node_attribute(aid) {
                if link == node {
                    // If an element child has a link to the element itself
                    // then we have to replace it with `none`.
                    // Otherwise we will get endless loop/recursion and stack overflow.
                    return Ok(Some(child.id));
                } else {
                    // Check that linked node children doesn't link this element.
                    for node2 in link.descendants() {
                        if let Some(link2) = node2.node_attribute(aid) {
                            if link2 == node {
                                return Ok(Some(node2.id));
                            }
                        }
                    }
                }
            }
        }
    }

    doc.work.checked()?;
    Ok(None)
}

/// Detects cases like:
///
/// ```xml
/// <filter id="filter1">
///   <feImage xlink:href="#rect1"/>
/// </filter>
/// <rect id="rect1" x="36" y="36" width="120" height="120" fill="green" filter="url(#filter1)"/>
/// ```
fn fix_recursive_fe_image(doc: &mut Document) -> Result<(), Error> {
    let mut ids = Storage::new(doc.work.resources());
    for fe_node in doc
        .root()
        .descendants()
        .filter(|n| n.tag_name() == Some(EId::FeImage))
    {
        if let Some(link) = fe_node.node_attribute(AId::Href) {
            if let Some(filter_uri) = link.attribute::<&str>(AId::Filter) {
                let filter_id = fe_node.parent().unwrap().element_id();
                doc.work.bytes(filter_uri.len())?;
                for func in svgtypes::FilterValueListParser::from(filter_uri).flatten() {
                    doc.work.event()?;
                    if let svgtypes::FilterValue::Url(url) = func {
                        if doc.work.compare(url, filter_id)? == Ordering::Equal {
                            ids.try_push(link.id)?;
                        }
                    }
                }
            }
        }
    }

    doc.work.checked()?;
    for id in ids.iter().copied() {
        doc.work.event()?;
        let idx = doc
            .get(id)
            .attribute_id(AId::Filter)
            .ok_or_else(|| doc.work.semantic(Error::NoRootNode))?;
        doc.attrs[idx].value = roxmltree::StringStorage::Borrowed("none");
    }
    doc.work.checked()
}

#[cfg(test)]
mod callback_work_tests {
    use super::*;
    use simplecss::Element;
    struct Backend;
    impl markdown_svg_budget::Backend for Backend {
        fn reserve(
            &self,
            _: ResourceKind,
            _: usize,
        ) -> Result<markdown_svg_budget::Ticket, ResourceError> {
            panic!("selector callback must not allocate")
        }
        fn allocation_committed(
            &self,
            _: &markdown_svg_budget::Ticket,
            _: usize,
        ) -> Result<(), ResourceError> {
            Ok(())
        }
        fn release_after_free(
            &self,
            _: markdown_svg_budget::Ticket,
        ) -> Result<(), ResourceError> {
            panic!("selector callback must not own a ticket")
        }
        fn checkpoint(&self, _: u64) -> Result<(), ResourceError> {
            Ok(())
        }
    }
    #[test]
    fn supported_name_envelope_contains_every_frozen_key_and_skips_long_unknown_hashes() {
        let mut count = 0;
        let mut maximum = 0;
        for line in include_str!("names.rs").lines() {
            let Some(rest) = line.trim_start().strip_prefix("(\"") else {
                continue;
            };
            let Some((key, tail)) = rest.split_once('"') else {
                continue;
            };
            if tail.starts_with(", EId::") || tail.starts_with(", AId::") {
                assert!(key.len() <= MAX_LOOKUP_NAME_BYTES);
                count += 1;
                maximum = maximum.max(key.len());
            }
        }
        assert_eq!(count, 264);
        assert_eq!(maximum, 28);
        let work = Work::new(None);
        let unknown = "a".repeat(100000);
        let source = format!("<{unknown}/>");
        let xml = roxmltree::Document::parse(&source).unwrap();
        assert!(parse_tag_name(xml.root_element(), &work).unwrap().is_none());
        assert!(attribute_name(&unknown, &work).unwrap().is_none());
    }
    #[test]
    fn selector_previous_sibling_charges_skipped_non_elements_and_records_quota() {
        let source = format!("<svg><rect/>{}<circle/></svg>", "<!-- gap -->".repeat(600));
        let xml = roxmltree::Document::parse(&source).unwrap();
        let circle = xml
            .descendants()
            .find(|n| n.has_tag_name("circle"))
            .unwrap();
        let work = Work::new(None);
        let node = XmlNode {
            node: circle,
            work: &work,
        };
        assert_eq!(
            node.prev_sibling_element().unwrap().node.tag_name().name(),
            "rect"
        );
        let resources = ResourcesRef::new_prepaid(
            Backend,
            markdown_svg_budget::FirstFailureRef::new_prepaid(),
            markdown_svg_budget::Limits {
                work: 256,
                ..Default::default()
            },
        );
        let work = Work::new(Some(resources.clone()));
        let node = XmlNode {
            node: circle,
            work: &work,
        };
        assert!(node.prev_sibling_element().is_none());
        assert_eq!(
            resources.failure(),
            Some(ResourceError::new(FailureKind::Quota, LimitKind::Work))
        );
        drop(work);
        resources.validate_retired().unwrap();
    }
}
