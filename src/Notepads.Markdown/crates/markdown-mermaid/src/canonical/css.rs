// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::{
    geometry::{length, number},
    xml::Tree,
    *,
};
use markdown_memory::FundedVec;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Paint {
    None,
    Color(Color),
    Current,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Style<'a> {
    pub fill: Paint,
    pub stroke: Paint,
    pub color: Color,
    pub fill_opacity: f64,
    pub stroke_opacity: f64,
    pub opacity: f64,
    pub stroke_width: f64,
    pub dash: &'a str,
    pub dash_offset: f64,
    pub linecap: &'a str,
    pub linejoin: &'a str,
    pub miter: f64,
    pub fill_rule: &'a str,
    pub clip_rule: &'a str,
    pub font: FontPlan,
    pub anchor: &'a str,
    pub baseline: &'a str,
    pub hidden: bool,
    pub display_none: bool,
    pub preserve: bool,
    pub clip: Option<&'a str>,
    pub marker_start: Option<&'a str>,
    pub marker_end: Option<&'a str>,
    pub decoration: &'a str,
    pub overflow_visible: bool,
}

impl Default for Style<'_> {
    fn default() -> Self {
        let black = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        Self {
            fill: Paint::Color(black),
            stroke: Paint::None,
            color: black,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            opacity: 1.0,
            stroke_width: 1.0,
            dash: "none",
            dash_offset: 0.0,
            linecap: "butt",
            linejoin: "miter",
            miter: 4.0,
            fill_rule: "nonzero",
            clip_rule: "nonzero",
            font: FontPlan::default(),
            anchor: "start",
            baseline: "alphabetic",
            hidden: false,
            display_none: false,
            preserve: false,
            clip: None,
            marker_start: None,
            marker_end: None,
            decoration: "none",
            overflow_visible: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Rule<'a> {
    selector: &'a str,
    declarations: &'a str,
    specificity: u32,
    order: u32,
}

pub(crate) struct Sheet<'a> {
    rules: FundedVec<Rule<'a>>,
}

impl<'a> Sheet<'a> {
    pub fn parse(
        tree: &Tree<'a>,
        scope: &MemoryScope,
        control: &OperationControl,
        limits: CanonicalLimits,
    ) -> Result<Self, CanonicalError> {
        let mut result = Self {
            rules: FundedVec::new_in(scope.clone(), Family::Canonical),
        };
        for (i, node) in tree.nodes.iter().enumerate() {
            if node.tag != "#text" || node.parent.is_none_or(|p| tree.nodes[p].tag != "style") {
                continue;
            }
            checkpoint(scope, control)?;
            let mut rest = node.text;
            while !rest.trim().is_empty() {
                rest = rest.trim_start();
                if rest.starts_with("/*") {
                    let end = rest.find("*/").ok_or(CanonicalError::UnsupportedCss)?;
                    rest = &rest[end + 2..];
                    continue;
                }
                let open = rest.find('{').ok_or(CanonicalError::UnsupportedCss)?;
                let selector = rest[..open].trim();
                let close = balanced(rest, open)?;
                let body = &rest[open + 1..close];
                rest = &rest[close + 1..];
                if selector.starts_with('@') {
                    // Only fixed generator keyframes are inert until selected by an animation declaration.
                    if !selector.starts_with("@keyframes ") {
                        return Err(CanonicalError::UnsupportedCss);
                    }
                    continue;
                }
                for selector in selector.split(',') {
                    if result.rules.len() >= limits.rules {
                        return Err(CanonicalError::Limited);
                    }
                    let selector = selector.trim();
                    result.rules.try_push(Rule {
                        selector,
                        declarations: body,
                        specificity: specificity(selector)?,
                        order: i as u32,
                    })?;
                }
            }
        }
        Ok(result)
    }

    pub fn style(
        &self,
        tree: &Tree<'a>,
        index: usize,
        parent: Option<Style<'a>>,
        outlines: &dyn OutlineProvider,
    ) -> Result<Style<'a>, CanonicalError> {
        let base = parent.unwrap_or_default();
        let mut out = base;
        out.opacity = 1.0;
        out.clip = None;
        out.display_none = parent.is_some_and(|p| p.display_none);
        out.overflow_visible = false;
        let mut ranks = [Rank::default(); 32];
        let mut values = [None; 32];
        let node = tree.nodes[index];
        for attribute in &tree.attributes.as_slice()
            [node.first_attribute..node.first_attribute + node.attributes]
        {
            if property(attribute.name).is_some() {
                choose(
                    &mut values,
                    &mut ranks,
                    attribute.name,
                    attribute.value,
                    Rank {
                        important: false,
                        specificity: 0,
                        order: 0,
                    },
                    "",
                    node.tag,
                )?;
            }
        }
        for rule in &self.rules {
            if matches_selector(tree, index, rule.selector)? {
                for (key, value, important) in Declarations::new(rule.declarations) {
                    choose(
                        &mut values,
                        &mut ranks,
                        key,
                        value,
                        Rank {
                            important,
                            specificity: rule.specificity,
                            order: rule.order + 1,
                        },
                        rule.selector,
                        node.tag,
                    )?;
                }
            }
        }
        if let Some(value) = tree.attr(index, "style") {
            for (key, value, important) in Declarations::new(value) {
                choose(
                    &mut values,
                    &mut ranks,
                    key,
                    value,
                    Rank {
                        important,
                        specificity: u32::MAX,
                        order: u32::MAX,
                    },
                    "",
                    node.tag,
                )?;
            }
        }
        let mut unused = [Rank::default(); 32];
        // Resolve font declarations once against the parent computed font, before
        // any em/ex-dependent geometry. A lower-priority font-size cannot alter
        // the base of a higher-priority relative size.
        for index in [14usize, 15, 16, 17] {
            if let Some(value) = values[index] {
                if value == "inherit" {
                    inherit(&mut out, base, index);
                    continue;
                }
                if index == 15 {
                    let ex = if value.trim_end().ends_with("ex") {
                        outlines
                            .measure(TextRun {
                                text: "",
                                font: base.font,
                            })?
                            .x_height
                    } else {
                        0.0
                    };
                    out.font.em_size = length(value, base.font.em_size, ex, base.font.em_size)?;
                    if !(1.0..=256.0).contains(&out.font.em_size) {
                        return Err(CanonicalError::Limited);
                    }
                } else {
                    apply(
                        &mut out,
                        &mut unused,
                        PROPERTIES[index],
                        value,
                        Rank {
                            important: true,
                            specificity: u32::MAX,
                            order: u32::MAX,
                        },
                        "",
                        node.tag,
                        0.0,
                    )?;
                }
            }
        }
        let needs_ex = values
            .iter()
            .enumerate()
            .any(|(i, v)| i != 15 && v.is_some_and(|v| v.trim_end().ends_with("ex")));
        let ex = if needs_ex {
            outlines
                .measure(TextRun {
                    text: "",
                    font: out.font,
                })?
                .x_height
        } else {
            0.0
        };
        for (index, value) in values.into_iter().enumerate().take(PROPERTIES.len()) {
            if (14..=17).contains(&index) {
                continue;
            }
            if let Some(value) = value {
                if value == "inherit" {
                    inherit(&mut out, base, index);
                } else {
                    apply(
                        &mut out,
                        &mut unused,
                        PROPERTIES[index],
                        value,
                        Rank {
                            important: true,
                            specificity: u32::MAX,
                            order: u32::MAX,
                        },
                        "",
                        node.tag,
                        ex,
                    )?;
                }
            }
        }
        if tree.attr(index, "xml:space") == Some("preserve") {
            out.preserve = true;
        }
        if parent.is_some_and(|p| p.display_none) {
            out.display_none = true;
        }
        Ok(out)
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Rank {
    important: bool,
    specificity: u32,
    order: u32,
}

const PROPERTIES: [&str; 29] = [
    "fill",
    "stroke",
    "color",
    "fill-opacity",
    "stroke-opacity",
    "opacity",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "fill-rule",
    "clip-rule",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-anchor",
    "dominant-baseline",
    "visibility",
    "display",
    "white-space",
    "clip-path",
    "marker-start",
    "marker-end",
    "text-decoration",
    "overflow",
    "alignment-baseline",
];
fn property(name: &str) -> Option<usize> {
    PROPERTIES.iter().position(|v| *v == name)
}
fn choose<'a>(
    values: &mut [Option<&'a str>; 32],
    ranks: &mut [Rank; 32],
    key: &str,
    value: &'a str,
    rank: Rank,
    selector: &str,
    tag: &str,
) -> Result<(), CanonicalError> {
    if key == "max-width" && tag == "svg" && selector.is_empty() {
        if length(value, 16.0, 8.0, 1.0)? <= 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        return Ok(());
    }
    let Some(index) = property(key) else {
        if ignorable(selector, tag, key) {
            return Ok(());
        }
        return Err(CanonicalError::UnsupportedProperty);
    };
    if rank >= ranks[index] {
        ranks[index] = rank;
        values[index] = Some(value);
    }
    Ok(())
}
fn inherit<'a>(out: &mut Style<'a>, base: Style<'a>, index: usize) {
    match index {
        0 => out.fill = base.fill,
        1 => out.stroke = base.stroke,
        2 => out.color = base.color,
        3 => out.fill_opacity = base.fill_opacity,
        4 => out.stroke_opacity = base.stroke_opacity,
        5 => out.opacity = base.opacity,
        6 => out.stroke_width = base.stroke_width,
        7 => out.dash = base.dash,
        8 => out.dash_offset = base.dash_offset,
        9 => out.linecap = base.linecap,
        10 => out.linejoin = base.linejoin,
        11 => out.miter = base.miter,
        12 => out.fill_rule = base.fill_rule,
        13 => out.clip_rule = base.clip_rule,
        14 => out.font.family = base.font.family,
        15 => out.font.em_size = base.font.em_size,
        16 => out.font.weight = base.font.weight,
        17 => out.font.italic = base.font.italic,
        18 => out.anchor = base.anchor,
        19 | 28 => out.baseline = base.baseline,
        20 => out.hidden = base.hidden,
        21 => out.display_none = base.display_none,
        22 => out.preserve = base.preserve,
        23 => out.clip = base.clip,
        24 => out.marker_start = base.marker_start,
        25 => out.marker_end = base.marker_end,
        26 => {
            out.decoration = base.decoration;
            out.font.decoration = base.font.decoration;
        }
        27 => out.overflow_visible = base.overflow_visible,
        _ => {}
    }
}

// Keep the comparator's declaration, specificity and selector context explicit.
#[allow(clippy::too_many_arguments)]
fn apply<'a>(
    out: &mut Style<'a>,
    ranks: &mut [Rank; 32],
    key: &'a str,
    value: &'a str,
    rank: Rank,
    selector: &str,
    tag: &str,
    ex: f64,
) -> Result<(), CanonicalError> {
    let value = value.trim();
    if key == "max-width" && tag == "svg" && selector.is_empty() {
        if length(value, out.font.em_size, ex, 1.0)? <= 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        return Ok(());
    }
    let Some(index) = property(key) else {
        if ignorable(selector, tag, key) {
            return Ok(());
        }
        return Err(CanonicalError::UnsupportedProperty);
    };
    if rank < ranks[index] {
        return Ok(());
    }
    ranks[index] = rank;
    if value == "inherit" {
        return Ok(());
    }
    match key {
        "fill" => out.fill = paint(value)?,
        "stroke" => out.stroke = paint(value)?,
        "color" => out.color = color(value)?,
        "fill-opacity" => out.fill_opacity = alpha(value)?,
        "stroke-opacity" => out.stroke_opacity = alpha(value)?,
        "opacity" => out.opacity = alpha(value)?,
        "stroke-width" => {
            out.stroke_width = length(value, out.font.em_size, ex, 1.0)?;
            if out.stroke_width < 0.0 {
                return Err(CanonicalError::InvalidGeometry);
            }
        }
        "stroke-dasharray" => {
            if value != "none" {
                let mut n = super::geometry::Numbers::new(value);
                while !n.done() {
                    if n.next()? < 0.0 {
                        return Err(CanonicalError::InvalidGeometry);
                    }
                }
            }
            out.dash = value;
        }
        "stroke-dashoffset" => out.dash_offset = length(value, out.font.em_size, ex, 1.0)?,
        "stroke-linecap" => out.linecap = one(value, &["butt", "round", "square"])?,
        "stroke-linejoin" => out.linejoin = one(value, &["miter", "round", "bevel"])?,
        "stroke-miterlimit" => {
            out.miter = number(value)?;
            if out.miter < 1.0 {
                return Err(CanonicalError::InvalidGeometry);
            }
        }
        "fill-rule" => out.fill_rule = one(value, &["nonzero", "evenodd"])?,
        "clip-rule" => out.clip_rule = one(value, &["nonzero", "evenodd"])?,
        "font-family" => {
            let family = value
                .split(',')
                .next()
                .ok_or(CanonicalError::FontUnavailable)?
                .trim();
            let family = family
                .strip_prefix("&quot;")
                .and_then(|v| v.strip_suffix("&quot;"))
                .unwrap_or(family);
            out.font.family = FontFamily::parse(family).ok_or(CanonicalError::FontUnavailable)?;
        }
        "font-size" => {
            out.font.em_size = length(value, out.font.em_size, ex, out.font.em_size)?;
            if !(1.0..=256.0).contains(&out.font.em_size) {
                return Err(CanonicalError::Limited);
            }
        }
        "font-weight" => {
            out.font.weight = match value {
                "normal" => 400,
                "bold" => 700,
                _ => value
                    .parse::<u16>()
                    .ok()
                    .filter(|v| (1..=999).contains(v))
                    .ok_or(CanonicalError::UnsupportedCss)?,
            }
        }
        "font-style" => {
            out.font.italic = match value {
                "normal" => false,
                "italic" | "oblique" => true,
                _ => return Err(CanonicalError::UnsupportedCss),
            }
        }
        "text-anchor" => out.anchor = one(value, &["start", "middle", "end"])?,
        "dominant-baseline" | "alignment-baseline" => {
            out.baseline = one(
                value,
                &[
                    "auto",
                    "alphabetic",
                    "middle",
                    "central",
                    "hanging",
                    "text-before-edge",
                    "text-after-edge",
                    "baseline",
                ],
            )?
        }
        "visibility" => {
            out.hidden = match value {
                "visible" => false,
                "hidden" | "collapse" => true,
                _ => return Err(CanonicalError::UnsupportedCss),
            }
        }
        "display" => {
            out.display_none = match value {
                "none" => true,
                "inline" | "block" => false,
                _ => return Err(CanonicalError::UnsupportedCss),
            }
        }
        "white-space" => {
            out.preserve = match value {
                "normal" | "nowrap" => false,
                "pre" | "pre-wrap" | "break-spaces" => true,
                _ => return Err(CanonicalError::UnsupportedCss),
            }
        }
        "clip-path" => out.clip = local_url(value)?,
        "marker-start" => out.marker_start = local_url(value)?,
        "marker-end" => out.marker_end = local_url(value)?,
        "text-decoration" => {
            out.decoration = one(value, &["none", "underline", "line-through"])?;
            out.font.decoration = match value {
                "underline" => 1,
                "line-through" => 2,
                _ => 0,
            };
        }
        "overflow" => {
            out.overflow_visible = match value {
                "visible" => true,
                "hidden" | "scroll" | "auto" => false,
                _ => return Err(CanonicalError::UnsupportedCss),
            }
        }
        _ => return Err(CanonicalError::UnsupportedProperty),
    }
    Ok(())
}

fn one<'a>(value: &'a str, values: &[&str]) -> Result<&'a str, CanonicalError> {
    if values.contains(&value) {
        Ok(value)
    } else {
        Err(CanonicalError::UnsupportedCss)
    }
}

pub(crate) fn local_url(value: &str) -> Result<Option<&str>, CanonicalError> {
    if value == "none" {
        return Ok(None);
    }
    let id = value
        .strip_prefix("url(")
        .and_then(|v| v.strip_suffix(')'))
        .map(str::trim)
        .map(|v| v.trim_matches(['\'', '"']))
        .and_then(|v| v.strip_prefix('#'))
        .filter(|v| !v.is_empty())
        .ok_or(CanonicalError::InvalidReference)?;
    Ok(Some(id))
}

fn ignorable(selector: &str, tag: &str, key: &str) -> bool {
    let selector = selector.trim();
    match (key, tag) {
        ("text-align", "g") => {
            selector.ends_with(" .node .label")
                || selector.ends_with(" .rough-node .label")
                || selector.ends_with(" .edgeLabel")
        }
        ("background-color", "g") => {
            selector.ends_with(" .edgeLabel") || selector.ends_with(" .labelBkg")
        }
        ("background-color", "rect") => {
            selector.ends_with(" .edgeLabel rect") || selector.ends_with(" .labelBkg")
        }
        ("cursor", "g") => selector.ends_with(" .node.clickable"),
        _ => false,
    }
}

struct Declarations<'a> {
    rest: &'a str,
}
impl<'a> Declarations<'a> {
    fn new(rest: &'a str) -> Self {
        Self { rest }
    }
}
impl<'a> Iterator for Declarations<'a> {
    type Item = (&'a str, &'a str, bool);
    fn next(&mut self) -> Option<Self::Item> {
        self.rest = self
            .rest
            .trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ';');
        if self.rest.is_empty() {
            return None;
        }
        let mut quote = 0u8;
        let mut depth = 0usize;
        let mut end = self.rest.len();
        for (i, b) in self.rest.bytes().enumerate() {
            if quote != 0 {
                if b == quote {
                    quote = 0;
                }
                continue;
            }
            if matches!(b, b'\'' | b'"') {
                quote = b;
            } else if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth = depth.saturating_sub(1);
            } else if b == b';' && depth == 0 {
                end = i;
                break;
            }
        }
        let part = &self.rest[..end];
        self.rest = if end < self.rest.len() {
            &self.rest[end + 1..]
        } else {
            ""
        };
        let Some((key, value)) = part.split_once(':') else {
            return Some((part.trim(), "", false));
        };
        let value = value.trim();
        let important = value.strip_suffix("!important");
        Some((
            key.trim(),
            important.unwrap_or(value).trim(),
            important.is_some(),
        ))
    }
}

fn balanced(value: &str, open: usize) -> Result<usize, CanonicalError> {
    let mut depth = 0;
    let mut quote = 0u8;
    for (i, b) in value.bytes().enumerate().skip(open) {
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            continue;
        }
        if matches!(b, b'\'' | b'"') {
            quote = b;
        } else if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                return Ok(i);
            }
        }
    }
    Err(CanonicalError::UnsupportedCss)
}

fn specificity(value: &str) -> Result<u32, CanonicalError> {
    let mut ids = 0u32;
    let mut classes = 0u32;
    let mut tags = 0u32;
    let mut prior = ' ';
    for (offset, c) in value.char_indices() {
        if c == '#' {
            ids += 1;
        } else if c == '.' || c == '[' || (c == ':' && !value[offset..].starts_with(":not(")) {
            classes += 1;
        } else if c.is_ascii_alphabetic() && matches!(prior, ' ' | '>') {
            tags += 1;
        }
        prior = c;
    }
    ids.checked_mul(65536)
        .and_then(|v| v.checked_add(classes * 256))
        .and_then(|v| v.checked_add(tags))
        .ok_or(CanonicalError::Limited)
}

#[derive(Clone, Copy)]
struct Compound<'a> {
    value: &'a str,
    child: bool,
}

fn matches_selector(tree: &Tree<'_>, index: usize, selector: &str) -> Result<bool, CanonicalError> {
    tree.work(selector.len().saturating_add(1))?;
    let mut parts = [Compound {
        value: "",
        child: false,
    }; 32];
    let mut count = 0;
    let mut start = 0;
    let mut bracket = 0;
    let mut quote = 0u8;
    let mut child = false;
    let bytes = selector.as_bytes();
    let mut cursor = 0;
    while cursor <= bytes.len() {
        let b = bytes.get(cursor).copied().unwrap_or(b' ');
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            cursor += 1;
            continue;
        }
        if matches!(b, b'\'' | b'"') {
            quote = b;
        } else if b == b'[' {
            bracket += 1;
        } else if b == b']' {
            bracket -= 1;
        }
        if bracket == 0 && (b.is_ascii_whitespace() || b == b'>' || cursor == bytes.len()) {
            let value = selector[start..cursor].trim();
            if !value.is_empty() {
                if count == 32 {
                    return Err(CanonicalError::Limited);
                }
                parts[count] = Compound { value, child };
                count += 1;
                child = false;
            }
            if b == b'>' {
                child = true;
            }
            start = cursor + 1;
        }
        cursor += 1;
    }
    if count == 0 {
        return Err(CanonicalError::UnsupportedCss);
    }
    let mut current = index;
    if !compound(tree, current, parts[count - 1].value)? {
        return Ok(false);
    }
    for part in (1..count).rev() {
        let Some(mut parent) = tree.nodes[current].parent else {
            return Ok(false);
        };
        if parts[part].child {
            if !compound(tree, parent, parts[part - 1].value)? {
                return Ok(false);
            }
        } else {
            while !compound(tree, parent, parts[part - 1].value)? {
                let Some(next) = tree.nodes[parent].parent else {
                    return Ok(false);
                };
                parent = next;
            }
        }
        current = parent;
    }
    Ok(true)
}

fn compound(tree: &Tree<'_>, index: usize, mut value: &str) -> Result<bool, CanonicalError> {
    tree.work(
        tree.nodes[index]
            .attributes
            .saturating_add(value.len())
            .saturating_add(1),
    )?;
    let tag_end = value.find(['#', '.', '[', ':']).unwrap_or(value.len());
    let tag = &value[..tag_end];
    if !tag.is_empty() && tag != "*" && tag != tree.nodes[index].tag {
        return Ok(false);
    }
    value = &value[tag_end..];
    while !value.is_empty() {
        let kind = value.as_bytes()[0];
        value = &value[1..];
        if kind == b'[' {
            let end = value.find(']').ok_or(CanonicalError::UnsupportedCss)?;
            let part = &value[..end];
            value = &value[end + 1..];
            let Some((key, expected)) = part.split_once('=') else {
                return Err(CanonicalError::UnsupportedCss);
            };
            if !matches!(key.trim(), "data-look" | "data-color-id") {
                return Err(CanonicalError::UnsupportedCss);
            }
            if tree.attr(index, key.trim()) != Some(expected.trim().trim_matches(['\'', '"'])) {
                return Ok(false);
            }
            continue;
        }
        if kind == b':' {
            if let Some(rest) = value.strip_prefix("not(") {
                let end = rest.find(')').ok_or(CanonicalError::UnsupportedCss)?;
                let class = rest[..end]
                    .trim()
                    .strip_prefix('.')
                    .filter(|v| {
                        !v.is_empty()
                            && v.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                    })
                    .ok_or(CanonicalError::UnsupportedCss)?;
                if tree.has_class(index, class) {
                    return Ok(false);
                }
                value = &rest[end + 1..];
                continue;
            }
            let typed_position = if let Some(rest) = value.strip_prefix("nth-of-type(") {
                let end = rest.find(')').ok_or(CanonicalError::UnsupportedCss)?;
                let position = rest[..end]
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0 && *n <= 16384)
                    .ok_or(CanonicalError::UnsupportedCss)?;
                value = &rest[end + 1..];
                Some(position)
            } else if let Some(rest) = value.strip_prefix("first-of-type") {
                value = rest;
                Some(1)
            } else {
                None
            };
            if let Some(expected) = typed_position {
                tree.work(index.saturating_add(1))?;
                let node = tree.nodes[index];
                let position = tree.nodes[..=index]
                    .iter()
                    .filter(|prior| prior.parent == node.parent && prior.tag == node.tag)
                    .count();
                if position != expected {
                    return Ok(false);
                }
                continue;
            }
        }
        let end = value.find(['#', '.', '[', ':']).unwrap_or(value.len());
        let name = &value[..end];
        value = &value[end..];
        match kind {
            b'#' => {
                if tree.attr(index, "id") != Some(name) {
                    return Ok(false);
                }
            }
            b'.' => {
                if !tree.has_class(index, name) {
                    return Ok(false);
                }
            }
            b':' => {
                if name != "root" {
                    return Err(CanonicalError::UnsupportedCss);
                } else if tree.nodes[index].parent.is_some() {
                    return Ok(false);
                }
            }
            _ => return Err(CanonicalError::UnsupportedCss),
        }
    }
    Ok(true)
}

fn alpha(value: &str) -> Result<f64, CanonicalError> {
    let v = if let Some(v) = value.strip_suffix('%') {
        number(v)? / 100.0
    } else {
        number(value)?
    };
    Ok(v.clamp(0.0, 1.0))
}
fn paint(value: &str) -> Result<Paint, CanonicalError> {
    match value {
        "none" => Ok(Paint::None),
        "currentColor" | "currentcolor" => Ok(Paint::Current),
        _ => color(value).map(Paint::Color),
    }
}

pub(super) fn color(value: &str) -> Result<Color, CanonicalError> {
    if value.eq_ignore_ascii_case("transparent") {
        return Ok(Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        });
    }
    let hex = value.strip_prefix('#').or_else(|| {
        NAMED.split_ascii_whitespace().find_map(|entry| {
            let (k, v) = entry.split_once('=')?;
            if k.eq_ignore_ascii_case(value) {
                Some(v)
            } else {
                None
            }
        })
    });
    if let Some(hex) = hex {
        let n = u32::from_str_radix(hex, 16).map_err(|_| CanonicalError::UnsupportedCss)?;
        let (r, g, b, a) = match hex.len() {
            3 => (
                ((n >> 8) & 15) * 17,
                ((n >> 4) & 15) * 17,
                (n & 15) * 17,
                255,
            ),
            4 => (
                ((n >> 12) & 15) * 17,
                ((n >> 8) & 15) * 17,
                ((n >> 4) & 15) * 17,
                (n & 15) * 17,
            ),
            6 => (n >> 16, (n >> 8) & 255, n & 255, 255),
            8 => (n >> 24, (n >> 16) & 255, (n >> 8) & 255, n & 255),
            _ => return Err(CanonicalError::UnsupportedCss),
        };
        return Ok(Color {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: a as f64 / 255.0,
        });
    }
    let open = value.find('(').ok_or(CanonicalError::UnsupportedCss)?;
    let function = &value[..open];
    let args = value[open + 1..]
        .strip_suffix(')')
        .ok_or(CanonicalError::UnsupportedCss)?;
    let mut values = [""; 4];
    let mut count = 0;
    for v in args
        .split(|c: char| c.is_ascii_whitespace() || c == ',' || c == '/')
        .filter(|v| !v.is_empty())
    {
        if count == 4 {
            return Err(CanonicalError::UnsupportedCss);
        }
        values[count] = v;
        count += 1;
    }
    if count < 3 {
        return Err(CanonicalError::UnsupportedCss);
    }
    let a = if count == 4 { alpha(values[3])? } else { 1.0 };
    if matches!(function, "rgb" | "rgba") {
        let channel = |v: &str| -> Result<f64, CanonicalError> {
            Ok(if let Some(v) = v.strip_suffix('%') {
                number(v)? / 100.0
            } else {
                number(v)? / 255.0
            }
            .clamp(0.0, 1.0))
        };
        return Ok(Color {
            r: channel(values[0])?,
            g: channel(values[1])?,
            b: channel(values[2])?,
            a,
        });
    }
    if matches!(function, "hsl" | "hsla") {
        let h = number(values[0].trim_end_matches("deg"))?.rem_euclid(360.0) / 60.0;
        let s = alpha(values[1])?;
        let l = alpha(values[2])?;
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u8 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        return Ok(Color {
            r: r + m,
            g: g + m,
            b: b + m,
            a,
        });
    }
    Err(CanonicalError::UnsupportedCss)
}

const NAMED: &str = "aliceblue=f0f8ff antiquewhite=faebd7 aqua=00ffff aquamarine=7fffd4 azure=f0ffff beige=f5f5dc bisque=ffe4c4 black=000000 blanchedalmond=ffebcd blue=0000ff blueviolet=8a2be2 brown=a52a2a burlywood=deb887 cadetblue=5f9ea0 chartreuse=7fff00 chocolate=d2691e coral=ff7f50 cornflowerblue=6495ed cornsilk=fff8dc crimson=dc143c cyan=00ffff darkblue=00008b darkcyan=008b8b darkgoldenrod=b8860b darkgray=a9a9a9 darkgrey=a9a9a9 darkgreen=006400 darkkhaki=bdb76b darkmagenta=8b008b darkolivegreen=556b2f darkorange=ff8c00 darkorchid=9932cc darkred=8b0000 darksalmon=e9967a darkseagreen=8fbc8f darkslateblue=483d8b darkslategray=2f4f4f darkslategrey=2f4f4f darkturquoise=00ced1 darkviolet=9400d3 deeppink=ff1493 deepskyblue=00bfff dimgray=696969 dimgrey=696969 dodgerblue=1e90ff firebrick=b22222 floralwhite=fffaf0 forestgreen=228b22 fuchsia=ff00ff gainsboro=dcdcdc ghostwhite=f8f8ff gold=ffd700 goldenrod=daa520 gray=808080 grey=808080 green=008000 greenyellow=adff2f honeydew=f0fff0 hotpink=ff69b4 indianred=cd5c5c indigo=4b0082 ivory=fffff0 khaki=f0e68c lavender=e6e6fa lavenderblush=fff0f5 lawngreen=7cfc00 lemonchiffon=fffacd lightblue=add8e6 lightcoral=f08080 lightcyan=e0ffff lightgoldenrodyellow=fafad2 lightgray=d3d3d3 lightgrey=d3d3d3 lightgreen=90ee90 lightpink=ffb6c1 lightsalmon=ffa07a lightseagreen=20b2aa lightskyblue=87cefa lightslategray=778899 lightslategrey=778899 lightsteelblue=b0c4de lightyellow=ffffe0 lime=00ff00 limegreen=32cd32 linen=faf0e6 magenta=ff00ff maroon=800000 mediumaquamarine=66cdaa mediumblue=0000cd mediumorchid=ba55d3 mediumpurple=9370db mediumseagreen=3cb371 mediumslateblue=7b68ee mediumspringgreen=00fa9a mediumturquoise=48d1cc mediumvioletred=c71585 midnightblue=191970 mintcream=f5fffa mistyrose=ffe4e1 moccasin=ffe4b5 navajowhite=ffdead navy=000080 oldlace=fdf5e6 olive=808000 olivedrab=6b8e23 orange=ffa500 orangered=ff4500 orchid=da70d6 palegoldenrod=eee8aa palegreen=98fb98 paleturquoise=afeeee palevioletred=db7093 papayawhip=ffefd5 peachpuff=ffdab9 peru=cd853f pink=ffc0cb plum=dda0dd powderblue=b0e0e6 purple=800080 rebeccapurple=663399 red=ff0000 rosybrown=bc8f8f royalblue=4169e1 saddlebrown=8b4513 salmon=fa8072 sandybrown=f4a460 seagreen=2e8b57 seashell=fff5ee sienna=a0522d silver=c0c0c0 skyblue=87ceeb slateblue=6a5acd slategray=708090 slategrey=708090 snow=fffafa springgreen=00ff7f steelblue=4682b4 tan=d2b48c teal=008080 thistle=d8bfd8 tomato=ff6347 turquoise=40e0d0 violet=ee82ee wheat=f5deb3 white=ffffff whitesmoke=f5f5f5 yellow=ffff00 yellowgreen=9acd32";
