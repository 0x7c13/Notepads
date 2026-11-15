// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::{
    css::{Paint, Style},
    geometry::{Numbers, length, number, path_info, transform},
    xml::Tree,
    *,
};
use markdown_memory::FundedVec;
use std::fmt::{self, Write};

struct Output {
    bytes: FundedString,
    limit: usize,
    error: Option<CanonicalError>,
}
impl Output {
    fn new(scope: &MemoryScope, limit: usize) -> Result<Self, CanonicalError> {
        Ok(Self {
            bytes: string(scope, "")?,
            limit,
            error: None,
        })
    }
    fn put(&mut self, value: &str) -> Result<(), CanonicalError> {
        let len = self
            .bytes
            .as_str()
            .len()
            .checked_add(value.len())
            .ok_or(CanonicalError::Limited)?;
        if len > self.limit {
            self.error = Some(CanonicalError::Limited);
            return Err(CanonicalError::Limited);
        }
        self.bytes.try_push_str(value).map_err(|error| {
            let error = CanonicalError::Memory(error);
            self.error = Some(error);
            error
        })
    }
    fn failure(&self) -> CanonicalError {
        self.error
            .unwrap_or(CanonicalError::Memory(MemoryError::InvalidState))
    }
    fn escaped(&mut self, value: &str) -> Result<(), CanonicalError> {
        for ch in value.chars() {
            match ch {
                '&' => self.put("&amp;")?,
                '<' => self.put("&lt;")?,
                '>' => self.put("&gt;")?,
                '"' => self.put("&quot;")?,
                _ => {
                    let mut bytes = [0; 4];
                    self.put(ch.encode_utf8(&mut bytes))?;
                }
            }
        }
        Ok(())
    }
}
impl Write for Output {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.put(value).map_err(|_| fmt::Error)
    }
}

macro_rules! emit {($out:expr,$($arg:tt)*)=>{write!($out,$($arg)*).map_err(|_|$out.failure())?};}

pub(super) struct Compiler<'a, 's> {
    tree: &'a Tree<'s>,
    sheet: &'a css::Sheet<'s>,
    scope: &'a MemoryScope,
    control: &'a OperationControl,
    outlines: &'a dyn OutlineProvider,
    styles: FundedVec<Style<'s>>,
    out: Output,
    limits: CanonicalLimits,
    width: f64,
    height: f64,
    view: [f64; 4],
    commands: usize,
    text_runs: usize,
    expansions: usize,
    clip_serial: usize,
}

impl<'a, 's> Compiler<'a, 's> {
    pub fn new(
        tree: &'a Tree<'s>,
        sheet: &'a css::Sheet<'s>,
        scope: &'a MemoryScope,
        control: &'a OperationControl,
        outlines: &'a dyn OutlineProvider,
        limits: CanonicalLimits,
    ) -> Result<Self, CanonicalError> {
        let mut styles = FundedVec::try_with_capacity(scope, Family::Canonical, tree.nodes.len())?;
        for (index, node) in tree.nodes.iter().enumerate() {
            checkpoint(scope, control)?;
            let parent = node.parent.map(|p| styles[p]);
            styles.try_push(sheet.style(tree, index, parent, outlines)?)?;
        }
        let view = if let Some(value) = tree.attr(0, "viewBox") {
            let mut n = Numbers::new(value);
            let v = [n.next()?, n.next()?, n.next()?, n.next()?];
            if !n.done() {
                return Err(CanonicalError::InvalidGeometry);
            }
            v
        } else {
            [
                0.0,
                0.0,
                dimension(tree, 0, "width", 1.0)?,
                dimension(tree, 0, "height", 1.0)?,
            ]
        };
        if view[2] <= 0.0 || view[3] <= 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        let width = view[2];
        let height = view[3];
        Ok(Self {
            tree,
            sheet,
            scope,
            control,
            outlines,
            styles,
            out: Output::new(scope, limits.output_bytes)?,
            limits,
            width,
            height,
            view,
            commands: 0,
            text_runs: 0,
            expansions: 0,
            clip_serial: 0,
        })
    }

    pub fn run(mut self) -> Result<CanonicalDocument, CanonicalError> {
        self.validate_inputs()?;
        emit!(
            self.out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"xMidYMid meet\">",
            self.width,
            self.height,
            self.view[0],
            self.view[1],
            self.view[2],
            self.view[3]
        );
        self.out.put("<defs>")?;
        for (index, node) in self.tree.nodes.iter().enumerate() {
            if node.tag == "clipPath" {
                let mut chain = [usize::MAX; 64];
                self.check_clip(index, 0, &mut chain)?;
                let units = self
                    .tree
                    .attr(index, "clipPathUnits")
                    .unwrap_or("userSpaceOnUse");
                if !matches!(units, "userSpaceOnUse" | "objectBoundingBox") {
                    return Err(CanonicalError::UnsupportedProperty);
                }
                emit!(
                    self.out,
                    "<clipPath id=\"c{}\" clipPathUnits=\"{}\"",
                    index,
                    units
                );
                self.transform_attribute(index)?;
                self.out.put(">")?;
                self.children(index, 1, true, &mut chain)?;
                self.out.put("</clipPath>")?;
            }
        }
        self.out.put("</defs>")?;
        let mut chain = [usize::MAX; 64];
        self.children(0, 0, false, &mut chain)?;
        self.out.put("</svg>")?;
        checkpoint(self.scope, self.control)?;
        self.scope.charge_work(self.out.bytes.len())?;
        validate_canonical(self.out.bytes.as_str(), self.limits)?;
        Ok(CanonicalDocument {
            bytes: self.out.bytes,
            width: self.width,
            height: self.height,
            path_commands: self.commands,
            text_runs: self.text_runs,
        })
    }

    fn validate_inputs(&self) -> Result<(), CanonicalError> {
        self.tree.work(self.tree.attributes.len())?;
        for (index, node) in self.tree.nodes.iter().enumerate() {
            checkpoint(self.scope, self.control)?;
            if self.inert_generator_filter(index)? {
                continue;
            }
            if !matches!(
                node.tag,
                "svg"
                    | "g"
                    | "path"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "defs"
                    | "clipPath"
                    | "marker"
                    | "use"
                    | "text"
                    | "tspan"
                    | "style"
                    | "title"
                    | "desc"
                    | "#text"
            ) {
                return Err(CanonicalError::UnsupportedElement);
            }
            if node.tag == "svg" && index != 0 {
                return Err(CanonicalError::UnsupportedElement);
            }
            let n = *node;
            for attr in &self.tree.attributes.as_slice()
                [n.first_attribute..n.first_attribute + n.attributes]
            {
                if attr.name.starts_with("data-") || attr.name.starts_with("aria-") {
                    continue;
                }
                if !matches!(
                    attr.name,
                    "id" | "class"
                        | "role"
                        | "tabindex"
                        | "focusable"
                        | "xmlns"
                        | "xmlns:xlink"
                        | "xml:space"
                        | "style"
                        | "transform"
                        | "x"
                        | "y"
                        | "x1"
                        | "x2"
                        | "y1"
                        | "y2"
                        | "cx"
                        | "cy"
                        | "r"
                        | "rx"
                        | "ry"
                        | "width"
                        | "height"
                        | "viewBox"
                        | "preserveAspectRatio"
                        | "d"
                        | "points"
                        | "dx"
                        | "dy"
                        | "font-family"
                        | "font-size"
                        | "font-weight"
                        | "font-style"
                        | "text-anchor"
                        | "dominant-baseline"
                        | "alignment-baseline"
                        | "fill"
                        | "stroke"
                        | "color"
                        | "fill-opacity"
                        | "stroke-opacity"
                        | "opacity"
                        | "fill-rule"
                        | "clip-rule"
                        | "stroke-width"
                        | "stroke-linecap"
                        | "stroke-linejoin"
                        | "stroke-miterlimit"
                        | "stroke-dasharray"
                        | "stroke-dashoffset"
                        | "clip-path"
                        | "clipPathUnits"
                        | "visibility"
                        | "display"
                        | "marker-start"
                        | "marker-end"
                        | "markerWidth"
                        | "markerHeight"
                        | "markerUnits"
                        | "orient"
                        | "refX"
                        | "refY"
                        | "overflow"
                        | "href"
                        | "xlink:href"
                        | "pointer-events"
                ) {
                    return Err(CanonicalError::UnsupportedProperty);
                }
                if attr.name == "xmlns" && attr.value != "http://www.w3.org/2000/svg" {
                    return Err(CanonicalError::UnsupportedElement);
                }
                if attr.name == "xmlns:xlink" && attr.value != "http://www.w3.org/1999/xlink" {
                    return Err(CanonicalError::UnsupportedElement);
                }
                if matches!(attr.name, "href" | "xlink:href") && !attr.value.starts_with('#') {
                    return Err(CanonicalError::InvalidReference);
                }
            }
            if let Some(id) = self.tree.attr(index, "id")
                && self.tree.find_id(id)? != index
            {
                return Err(CanonicalError::InvalidReference);
            }
        }
        Ok(())
    }

    fn inert_generator_filter(&self, index: usize) -> Result<bool, CanonicalError> {
        let node = self.tree.nodes[index];
        let filter = match node.tag {
            "filter" => index,
            "feDropShadow" => match node.parent {
                Some(parent) if self.tree.nodes[parent].tag == "filter" => parent,
                _ => return Ok(false),
            },
            _ => return Ok(false),
        };
        if self.tree.nodes[filter]
            .parent
            .is_none_or(|p| self.tree.nodes[p].tag != "defs")
        {
            return Ok(false);
        }
        let Some(root_id) = self.tree.attr(0, "id") else {
            return Ok(false);
        };
        let Some(id) = self.tree.attr(filter, "id") else {
            return Ok(false);
        };
        let small = id.strip_suffix("-drop-shadow-small") == Some(root_id);
        if !small && id.strip_suffix("-drop-shadow") != Some(root_id) {
            return Ok(false);
        }
        self.tree.find_id(id)?;
        let extent = if small { "150%" } else { "130%" };
        let offset = if small { "2" } else { "4" };
        if self.tree.nodes[filter].attributes != 3
            || self.tree.attr(filter, "height") != Some(extent)
            || self.tree.attr(filter, "width") != Some(extent)
        {
            return Ok(false);
        }
        self.tree.work(self.tree.nodes.len())?;
        let mut children = 0;
        for (child, n) in self
            .tree
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent == Some(filter))
        {
            children += 1;
            if n.tag != "feDropShadow"
                || n.attributes != 5
                || self.tree.attr(child, "dx") != Some(offset)
                || self.tree.attr(child, "dy") != Some(offset)
                || self.tree.attr(child, "stdDeviation") != Some("0")
                || self.tree.attr(child, "flood-opacity") != Some("0.06")
                || self.tree.attr(child, "flood-color") != Some("#000000")
            {
                return Ok(false);
            }
        }
        // Any actual filter presentation attribute remains unsupported below;
        // any selected CSS filter already failed during style resolution.
        // A use/clip reference to this node cannot be expanded as a supported
        // geometry target. Only the unreferenced fixed generator definition is
        // absent from the emitted scene, as SVG requires.
        Ok(children == 1)
    }

    fn children(
        &mut self,
        parent: usize,
        depth: usize,
        clip: bool,
        chain: &mut [usize; 64],
    ) -> Result<(), CanonicalError> {
        self.tree.work(self.tree.nodes.len())?;
        for index in 0..self.tree.nodes.len() {
            if self.tree.nodes[index].parent == Some(parent) {
                self.node(index, depth + 1, clip, chain)?;
            }
        }
        Ok(())
    }

    fn node(
        &mut self,
        index: usize,
        depth: usize,
        clip: bool,
        chain: &mut [usize; 64],
    ) -> Result<(), CanonicalError> {
        checkpoint(self.scope, self.control)?;
        if depth >= self.limits.depth || depth >= 64 {
            return Err(CanonicalError::Limited);
        }
        self.expansions += 1;
        if self.expansions > self.limits.expansions {
            return Err(CanonicalError::Limited);
        }
        let node = self.tree.nodes[index];
        let style = self.styles[index];
        if style.display_none {
            return Ok(());
        }
        match node.tag {
            "style" | "defs" | "marker" | "clipPath" | "title" | "desc" | "#text" => return Ok(()),
            "g" => {
                self.group_open(index, style)?;
                self.children(index, depth, clip, chain)?;
                self.out.put("</g>")?;
            }
            "use" => {
                let reference = self
                    .tree
                    .attr(index, "href")
                    .or_else(|| self.tree.attr(index, "xlink:href"))
                    .and_then(|v| v.strip_prefix('#'))
                    .ok_or(CanonicalError::InvalidReference)?;
                let target = self.tree.find_id(reference)?;
                if chain[..depth].contains(&target) {
                    return Err(CanonicalError::InvalidReference);
                }
                chain[depth] = target;
                self.group_open(index, style)?;
                let x = dimension(self.tree, index, "x", 0.0)?;
                let y = dimension(self.tree, index, "y", 0.0)?;
                emit!(self.out, "<g transform=\"translate({} {})\">", x, y);
                let mut restore = FundedVec::new_in(self.scope.clone(), Family::Canonical);
                for child in 0..self.tree.nodes.len() {
                    if child == target || self.tree.descendant(child, target)? {
                        restore.try_push((child, self.styles[child]))?;
                        let parent = if child == target {
                            Some(style)
                        } else {
                            self.tree.nodes[child].parent.map(|p| self.styles[p])
                        };
                        self.styles[child] =
                            self.sheet.style(self.tree, child, parent, self.outlines)?;
                    }
                }
                self.node(target, depth + 1, clip, chain)?;
                for (child, old) in &restore {
                    self.styles[*child] = *old;
                }
                self.out.put("</g></g>")?;
            }
            "text" => {
                if !style.hidden {
                    self.text(index, depth)?;
                }
            }
            "tspan" => return Err(CanonicalError::MalformedXml),
            _ => {
                if !style.hidden {
                    self.shape(index, style, clip, depth, chain)?;
                }
            }
        }
        Ok(())
    }

    fn group_open(&mut self, index: usize, style: Style<'_>) -> Result<(), CanonicalError> {
        self.out.put("<g")?;
        self.transform_attribute(index)?;
        if style.opacity != 1.0 {
            emit!(self.out, " opacity=\"{}\"", style.opacity);
        }
        if let Some(id) = style.clip {
            let target = self.tree.find_id(id)?;
            if self.tree.nodes[target].tag != "clipPath" {
                return Err(CanonicalError::InvalidReference);
            }
            emit!(self.out, " clip-path=\"url(#c{})\"", target);
        }
        self.out.put(">")
    }

    fn transform_attribute(&mut self, index: usize) -> Result<(), CanonicalError> {
        if let Some(value) = self.tree.attr(index, "transform") {
            let m = transform(value)?;
            emit!(
                self.out,
                " transform=\"matrix({} {} {} {} {} {})\"",
                m.a,
                m.b,
                m.c,
                m.d,
                m.e,
                m.f
            );
        }
        Ok(())
    }

    fn shape(
        &mut self,
        index: usize,
        style: Style<'_>,
        clip: bool,
        depth: usize,
        chain: &mut [usize; 64],
    ) -> Result<(), CanonicalError> {
        let tag = self.tree.nodes[index].tag;
        self.group_open(index, style)?;
        emit!(self.out, "<{}", tag);
        match tag {
            "path" => {
                let d = self.tree.attr(index, "d").unwrap_or("");
                let info = path_info(d, self.limits.commands.saturating_sub(self.commands))?;
                self.commands += info.commands;
                self.out.put(" d=\"")?;
                self.out.escaped(d)?;
                self.out.put("\"")?;
            }
            "rect" => {
                for key in ["x", "y", "width", "height", "rx", "ry"] {
                    self.geometry_attribute(index, key, 0.0)?;
                }
                self.commands += 5;
            }
            "circle" => {
                for key in ["cx", "cy", "r"] {
                    self.geometry_attribute(index, key, 0.0)?;
                }
                self.commands += 4;
            }
            "ellipse" => {
                for key in ["cx", "cy", "rx", "ry"] {
                    self.geometry_attribute(index, key, 0.0)?;
                }
                self.commands += 4;
            }
            "line" => {
                for key in ["x1", "y1", "x2", "y2"] {
                    self.geometry_attribute(index, key, 0.0)?;
                }
                self.commands += 2;
            }
            "polyline" | "polygon" => {
                let value = self
                    .tree
                    .attr(index, "points")
                    .ok_or(CanonicalError::InvalidGeometry)?;
                let mut numbers = Numbers::new(value);
                let mut count = 0;
                while !numbers.done() {
                    numbers.next()?;
                    count += 1;
                }
                if count % 2 != 0 {
                    return Err(CanonicalError::InvalidGeometry);
                }
                self.commands += count / 2;
                self.out.put(" points=\"")?;
                self.out.escaped(value)?;
                self.out.put("\"")?;
            }
            _ => return Err(CanonicalError::UnsupportedElement),
        }
        if self.commands > self.limits.commands {
            return Err(CanonicalError::Limited);
        }
        self.paint(style, clip)?;
        self.out.put("/>")?;
        if tag == "path" && !clip {
            let info = path_info(
                self.tree.attr(index, "d").unwrap_or(""),
                self.limits.commands,
            )?;
            if let Some(marker) = style.marker_start {
                self.marker(
                    marker,
                    info.start,
                    info.start_tangent,
                    true,
                    style.stroke_width,
                    depth,
                    chain,
                )?;
            }
            if let Some(marker) = style.marker_end {
                self.marker(
                    marker,
                    info.end,
                    info.end_tangent,
                    false,
                    style.stroke_width,
                    depth,
                    chain,
                )?;
            }
        }
        self.out.put("</g>")
    }

    fn geometry_attribute(
        &mut self,
        index: usize,
        key: &str,
        default: f64,
    ) -> Result<(), CanonicalError> {
        let value = dimension(self.tree, index, key, default)?;
        if matches!(key, "width" | "height" | "r" | "rx" | "ry") && value < 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        emit!(self.out, " {}=\"{}\"", key, value);
        Ok(())
    }

    fn paint(&mut self, style: Style<'_>, clip: bool) -> Result<(), CanonicalError> {
        if clip {
            emit!(
                self.out,
                " fill=\"#ffffff\" stroke=\"none\" fill-rule=\"{}\" clip-rule=\"{}\"",
                style.clip_rule,
                style.clip_rule
            );
            return Ok(());
        }
        self.paint_value("fill", style.fill, style)?;
        self.paint_value("stroke", style.stroke, style)?;
        emit!(
            self.out,
            " fill-opacity=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\" stroke-miterlimit=\"{}\" fill-rule=\"{}\"",
            style.fill_opacity,
            style.stroke_opacity,
            style.stroke_width,
            style.linecap,
            style.linejoin,
            style.miter,
            style.fill_rule
        );
        if style.dash != "none" {
            self.out.put(" stroke-dasharray=\"")?;
            self.out.escaped(style.dash)?;
            self.out.put("\"")?;
        }
        if style.dash_offset != 0.0 {
            emit!(self.out, " stroke-dashoffset=\"{}\"", style.dash_offset);
        }
        Ok(())
    }

    fn paint_value(
        &mut self,
        key: &str,
        paint: Paint,
        style: Style<'_>,
    ) -> Result<(), CanonicalError> {
        let value = match paint {
            Paint::None => {
                emit!(self.out, " {}=\"none\"", key);
                return Ok(());
            }
            Paint::Color(v) => v,
            Paint::Current => style.color,
        };
        emit!(
            self.out,
            " {}=\"rgba({},{},{},{})\"",
            key,
            (value.r * 255.0).round() as u8,
            (value.g * 255.0).round() as u8,
            (value.b * 255.0).round() as u8,
            value.a
        );
        Ok(())
    }

    // This development comparator passes the complete geometric marker state
    // and the explicit bounded recursion guard together at each call site.
    #[allow(clippy::too_many_arguments)]
    fn marker(
        &mut self,
        id: &str,
        point: (f64, f64),
        tangent: (f64, f64),
        start: bool,
        stroke: f64,
        depth: usize,
        chain: &mut [usize; 64],
    ) -> Result<(), CanonicalError> {
        let index = self.tree.find_id(id)?;
        if self.tree.nodes[index].tag != "marker" || chain[..depth].contains(&index) {
            return Err(CanonicalError::InvalidReference);
        }
        chain[depth] = index;
        let width = dimension(self.tree, index, "markerWidth", 3.0)?;
        let height = dimension(self.tree, index, "markerHeight", 3.0)?;
        if width <= 0.0 || height <= 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        let view = if let Some(value) = self.tree.attr(index, "viewBox") {
            let mut n = Numbers::new(value);
            let v = [n.next()?, n.next()?, n.next()?, n.next()?];
            if !n.done() {
                return Err(CanonicalError::InvalidGeometry);
            }
            v
        } else {
            [0.0, 0.0, width, height]
        };
        if view[2] <= 0.0 || view[3] <= 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        let refx = dimension(self.tree, index, "refX", 0.0)?;
        let refy = dimension(self.tree, index, "refY", 0.0)?;
        let units = match self
            .tree
            .attr(index, "markerUnits")
            .unwrap_or("strokeWidth")
        {
            "strokeWidth" => stroke,
            "userSpaceOnUse" => 1.0,
            _ => return Err(CanonicalError::UnsupportedProperty),
        };
        let angle = match self.tree.attr(index, "orient").unwrap_or("0") {
            "auto" => tangent.1.atan2(tangent.0),
            "auto-start-reverse" => {
                tangent.1.atan2(tangent.0) + if start { std::f64::consts::PI } else { 0.0 }
            }
            v => number(v.trim_end_matches("deg"))?.to_radians(),
        };
        let preserve = self
            .tree
            .attr(index, "preserveAspectRatio")
            .unwrap_or("xMidYMid meet");
        let (sx, sy, tx, ty) = viewport(view, width, height, preserve)?;
        let reference = (sx * refx + tx, sy * refy + ty);
        let matrix = Matrix::translate(point.0, point.1)
            .then(Matrix::rotate(angle))?
            .then(Matrix::scale(units, units))?
            .then(Matrix::translate(-reference.0, -reference.1))?;
        emit!(
            self.out,
            "<g transform=\"matrix({} {} {} {} {} {})\">",
            matrix.a,
            matrix.b,
            matrix.c,
            matrix.d,
            matrix.e,
            matrix.f
        );
        let clip = !self.styles[index].overflow_visible;
        if clip {
            let serial = self.clip_serial;
            self.clip_serial += 1;
            emit!(
                self.out,
                "<defs><clipPath id=\"m{}\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#m{})\">",
                serial,
                width,
                height,
                serial
            );
        }
        emit!(
            self.out,
            "<g transform=\"matrix({} 0 0 {} {} {})\">",
            sx,
            sy,
            tx,
            ty
        );
        self.children(index, depth + 1, false, chain)?;
        self.out.put("</g>")?;
        if clip {
            self.out.put("</g>")?;
        }
        self.out.put("</g>")
    }

    fn check_clip(
        &self,
        index: usize,
        depth: usize,
        chain: &mut [usize; 64],
    ) -> Result<(), CanonicalError> {
        if depth >= 64 || chain[..depth].contains(&index) {
            return Err(CanonicalError::InvalidReference);
        }
        chain[depth] = index;
        for child in 0..self.tree.nodes.len() {
            if self.tree.descendant(child, index)?
                && let Some(id) = self.styles[child].clip
            {
                let target = self.tree.find_id(id)?;
                if self.tree.nodes[target].tag != "clipPath" {
                    return Err(CanonicalError::InvalidReference);
                }
                self.check_clip(target, depth + 1, chain)?;
            }
        }
        Ok(())
    }

    fn text(&mut self, index: usize, depth: usize) -> Result<(), CanonicalError> {
        let mut runs = FundedVec::new_in(self.scope.clone(), Family::Text);
        let mut chunks = FundedVec::new_in(self.scope.clone(), Family::Text);
        chunks.try_push(Chunk {
            width: 0.0,
            anchor: self.styles[index].anchor,
        })?;
        let mut cursor = Cursor {
            x: 0.0,
            y: 0.0,
            chunk: 0,
            space: true,
        };
        self.collect_text(index, depth, &mut cursor, &mut runs, &mut chunks)?;
        self.group_open(index, self.styles[index])?;
        for run in &runs {
            checkpoint(self.scope, self.control)?;
            if run.style.hidden || run.style.display_none {
                continue;
            }
            let width = chunks[run.chunk].width;
            let shift = match chunks[run.chunk].anchor {
                "start" => 0.0,
                "middle" => -width / 2.0,
                "end" => -width,
                _ => return Err(CanonicalError::UnsupportedProperty),
            };
            let baseline = run.y
                + self
                    .outlines
                    .baseline_offset(run.style.font, run.style.baseline)?;
            emit!(
                self.out,
                "<g transform=\"translate({} {})\"><path d=\"",
                run.x + shift,
                baseline
            );
            {
                let mut sink = GlyphSink {
                    out: &mut self.out,
                    count: &mut self.commands,
                    limit: self.limits.commands,
                };
                self.outlines.outline(
                    TextRun {
                        text: run.text.as_str(),
                        font: run.style.font,
                    },
                    &mut sink,
                )?;
            }
            self.out.put("\"")?;
            self.paint(run.style, false)?;
            self.out.put("/></g>")?;
            self.text_runs += 1;
        }
        self.out.put("</g>")
    }

    fn collect_text(
        &self,
        index: usize,
        depth: usize,
        cursor: &mut Cursor,
        runs: &mut FundedVec<TextPiece<'s>>,
        chunks: &mut FundedVec<Chunk<'s>>,
    ) -> Result<(), CanonicalError> {
        checkpoint(self.scope, self.control)?;
        self.tree.work(self.tree.nodes.len())?;
        if depth >= 64 {
            return Err(CanonicalError::Limited);
        }
        let node = self.tree.nodes[index];
        let style = self.styles[index];
        if node.tag == "#text" {
            let decoded = xml::decode(node.text, self.scope)?;
            let mut text = string(self.scope, "")?;
            for ch in decoded.as_str().chars() {
                if !style.preserve && ch.is_ascii_whitespace() {
                    if !cursor.space {
                        text.try_push(' ')?;
                        cursor.space = true;
                    }
                } else {
                    text.try_push(ch)?;
                    cursor.space = false;
                }
            }
            if text.as_str().is_empty() {
                return Ok(());
            }
            let metrics = self.outlines.measure(TextRun {
                text: text.as_str(),
                font: style.font,
            })?;
            runs.try_push(TextPiece {
                text,
                style,
                x: cursor.x,
                y: cursor.y,
                chunk: cursor.chunk,
            })?;
            cursor.x += metrics.advance;
            chunks[cursor.chunk].width += metrics.advance;
            return Ok(());
        }
        if !matches!(node.tag, "text" | "tspan") {
            return Err(CanonicalError::UnsupportedElement);
        }
        let needs_ex = ["x", "y", "dx", "dy"].into_iter().any(|key| {
            self.tree
                .attr(index, key)
                .is_some_and(|v| v.trim_end().ends_with("ex"))
        });
        let ex = if needs_ex {
            self.outlines
                .measure(TextRun {
                    text: "",
                    font: style.font,
                })?
                .x_height
        } else {
            0.0
        };
        if self.tree.attr(index, "x").is_some() || self.tree.attr(index, "y").is_some() {
            if let Some(value) = self.tree.attr(index, "x") {
                cursor.x = length(value, style.font.em_size, ex, self.width)?;
            }
            if let Some(value) = self.tree.attr(index, "y") {
                cursor.y = length(value, style.font.em_size, ex, self.height)?;
            }
            chunks.try_push(Chunk {
                width: 0.0,
                anchor: style.anchor,
            })?;
            cursor.chunk = chunks.len() - 1;
        }
        if let Some(value) = self.tree.attr(index, "dx") {
            let dx = length(value, style.font.em_size, ex, self.width)?;
            cursor.x += dx;
            chunks[cursor.chunk].width += dx;
        }
        if let Some(value) = self.tree.attr(index, "dy") {
            cursor.y += length(value, style.font.em_size, ex, self.height)?;
        }
        for child in 0..self.tree.nodes.len() {
            if self.tree.nodes[child].parent == Some(index) {
                self.collect_text(child, depth + 1, cursor, runs, chunks)?;
            }
        }
        Ok(())
    }
}

struct TextPiece<'a> {
    text: FundedString,
    style: Style<'a>,
    x: f64,
    y: f64,
    chunk: usize,
}
struct Chunk<'a> {
    width: f64,
    anchor: &'a str,
}
struct Cursor {
    x: f64,
    y: f64,
    chunk: usize,
    space: bool,
}

struct GlyphSink<'a> {
    out: &'a mut Output,
    count: &'a mut usize,
    limit: usize,
}
impl PathSink for GlyphSink<'_> {
    fn push(&mut self, c: PathCommand) -> Result<(), CanonicalError> {
        *self.count += 1;
        if *self.count > self.limit {
            return Err(CanonicalError::Limited);
        }
        if ![c.x1, c.y1, c.x2, c.y2, c.x3, c.y3]
            .into_iter()
            .all(super::geometry::valid)
        {
            return Err(CanonicalError::InvalidGeometry);
        }
        match c.verb {
            PathVerb::Move => {
                emit!(self.out, "M{} {} ", c.x1, c.y1);
            }
            PathVerb::Line => {
                emit!(self.out, "L{} {} ", c.x1, c.y1);
            }
            PathVerb::Quad => {
                emit!(self.out, "Q{} {} {} {} ", c.x1, c.y1, c.x2, c.y2);
            }
            PathVerb::Cubic => {
                emit!(
                    self.out,
                    "C{} {} {} {} {} {} ",
                    c.x1,
                    c.y1,
                    c.x2,
                    c.y2,
                    c.x3,
                    c.y3
                );
            }
            PathVerb::Close => self.out.put("Z ")?,
        }
        Ok(())
    }
}

fn dimension(tree: &Tree<'_>, node: usize, key: &str, default: f64) -> Result<f64, CanonicalError> {
    tree.attr(node, key)
        .map_or(Ok(default), |v| length(v, 16.0, 8.0, 1.0))
}

fn viewport(
    v: [f64; 4],
    width: f64,
    height: f64,
    preserve: &str,
) -> Result<(f64, f64, f64, f64), CanonicalError> {
    let mut parts = preserve.split_ascii_whitespace();
    let align = parts.next().unwrap_or("xMidYMid");
    let mode = parts.next().unwrap_or("meet");
    if parts.next().is_some() {
        return Err(CanonicalError::UnsupportedProperty);
    }
    let sx = width / v[2];
    let sy = height / v[3];
    if align == "none" {
        return Ok((sx, sy, -v[0] * sx, -v[1] * sy));
    }
    if !matches!(mode, "meet" | "slice") {
        return Err(CanonicalError::UnsupportedProperty);
    }
    let s = if mode == "meet" {
        sx.min(sy)
    } else {
        sx.max(sy)
    };
    let x = if align.starts_with("xMin") {
        0.0
    } else if align.starts_with("xMid") {
        0.5
    } else if align.starts_with("xMax") {
        1.0
    } else {
        return Err(CanonicalError::UnsupportedProperty);
    };
    let y = if align.ends_with("YMin") {
        0.0
    } else if align.ends_with("YMid") {
        0.5
    } else if align.ends_with("YMax") {
        1.0
    } else {
        return Err(CanonicalError::UnsupportedProperty);
    };
    Ok((
        s,
        s,
        -v[0] * s + (width - v[2] * s) * x,
        -v[1] * s + (height - v[3] * s) * y,
    ))
}

/// Independent streaming shape of the *output* vocabulary. No second owned XML tree.
fn validate_canonical(value: &str, limits: CanonicalLimits) -> Result<(), CanonicalError> {
    let mut cursor = 0usize;
    let mut count = 0usize;
    let mut stack = [""; 64];
    let mut transforms = [Matrix::default(); 64];
    let mut depth = 0usize;
    let bytes = value.as_bytes();
    while cursor < bytes.len() {
        if bytes[cursor] != b'<' {
            return Err(CanonicalError::MalformedXml);
        }
        cursor += 1;
        let closing = bytes.get(cursor) == Some(&b'/');
        if closing {
            cursor += 1;
        }
        let start = cursor;
        while bytes.get(cursor).is_some_and(|b| b.is_ascii_alphanumeric()) {
            cursor += 1;
        }
        let tag = &value[start..cursor];
        if !matches!(
            tag,
            "svg"
                | "g"
                | "path"
                | "rect"
                | "circle"
                | "ellipse"
                | "line"
                | "polyline"
                | "polygon"
                | "defs"
                | "clipPath"
        ) {
            return Err(CanonicalError::UnsupportedElement);
        }
        if closing {
            if depth == 0 || stack[depth - 1] != tag || bytes.get(cursor) != Some(&b'>') {
                return Err(CanonicalError::MalformedXml);
            }
            depth -= 1;
            cursor += 1;
            continue;
        }
        count += 1;
        if count > limits.expansions.saturating_mul(4) {
            return Err(CanonicalError::Limited);
        }
        let mut attributes = [""; 32];
        let mut attributes_count = 0;
        let mut local_transform = Matrix::default();
        let closed = loop {
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'>') {
                cursor += 1;
                break false;
            }
            if bytes.get(cursor) == Some(&b'/') && bytes.get(cursor + 1) == Some(&b'>') {
                cursor += 2;
                break true;
            }
            let start = cursor;
            while bytes
                .get(cursor)
                .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b':'))
            {
                cursor += 1;
            }
            if start == cursor {
                return Err(CanonicalError::MalformedXml);
            }
            let key = &value[start..cursor];
            if attributes_count == attributes.len() || attributes[..attributes_count].contains(&key)
            {
                return Err(CanonicalError::MalformedXml);
            }
            attributes[attributes_count] = key;
            attributes_count += 1;
            let paint = matches!(
                key,
                "fill"
                    | "stroke"
                    | "fill-opacity"
                    | "stroke-opacity"
                    | "stroke-width"
                    | "stroke-linecap"
                    | "stroke-linejoin"
                    | "stroke-miterlimit"
                    | "fill-rule"
                    | "clip-rule"
                    | "stroke-dasharray"
                    | "stroke-dashoffset"
            );
            let allowed = match tag {
                "svg" => matches!(
                    key,
                    "xmlns" | "width" | "height" | "viewBox" | "preserveAspectRatio"
                ),
                "g" => matches!(key, "transform" | "opacity" | "clip-path"),
                "defs" => false,
                "clipPath" => matches!(key, "id" | "clipPathUnits" | "transform"),
                "path" => key == "d" || paint,
                "rect" => matches!(key, "x" | "y" | "width" | "height" | "rx" | "ry") || paint,
                "circle" => matches!(key, "cx" | "cy" | "r") || paint,
                "ellipse" => matches!(key, "cx" | "cy" | "rx" | "ry") || paint,
                "line" => matches!(key, "x1" | "x2" | "y1" | "y2") || paint,
                "polyline" | "polygon" => key == "points" || paint,
                _ => false,
            };
            if !allowed {
                return Err(CanonicalError::UnsupportedProperty);
            }
            if bytes.get(cursor) != Some(&b'=') || bytes.get(cursor + 1) != Some(&b'"') {
                return Err(CanonicalError::MalformedXml);
            }
            cursor += 2;
            let start = cursor;
            while let Some(b) = bytes.get(cursor) {
                if *b == b'"' {
                    break;
                }
                if *b == b'<' {
                    return Err(CanonicalError::MalformedXml);
                }
                cursor += 1;
            }
            if bytes.get(cursor) != Some(&b'"') {
                return Err(CanonicalError::MalformedXml);
            }
            if key == "transform" {
                local_transform = transform(&value[start..cursor])?;
            }
            cursor += 1;
        };
        let inherited = if depth == 0 {
            Matrix::default()
        } else {
            transforms[depth - 1]
        };
        let combined = inherited.then(local_transform)?;
        if !closed {
            if depth >= stack.len() {
                return Err(CanonicalError::Limited);
            }
            stack[depth] = tag;
            transforms[depth] = combined;
            depth += 1;
        }
    }
    if depth == 0 {
        Ok(())
    } else {
        Err(CanonicalError::MalformedXml)
    }
}
