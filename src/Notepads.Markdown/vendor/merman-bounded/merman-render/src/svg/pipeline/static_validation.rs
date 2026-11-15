use crate::resources::{RenderResourcePolicy, ResourceLimitId};
use crate::{Error, Result};
use cssparser::{
    AtRuleParser, BasicParseErrorKind, CowRcStr, ParseError, ParseErrorKind, Parser, ParserState,
    QualifiedRuleParser, StyleSheetParser, Token,
};
use std::collections::{BTreeSet, HashMap};
use svgtypes::{Length, LengthUnit, NumberListParser};

use super::builtin::attr_sanitize::{is_safe_data_image_url, matches_active_svg_element};
use super::builtin::css_sanitize::{
    CssValidationError, matches_external_image_function,
    validate_resvg_css_declaration_list_with_checkpoints,
    validate_resvg_css_stylesheet_with_checkpoints,
};
use super::final_validation::{
    ReferenceDependencyGraph, ReferencePlanningError,
    plan_svg_reference_dependencies_with_checkpoints, validate_well_formed_svg_with_execution,
};
use super::{
    SvgPostprocessExecution, checkpoint_loop, is_css_value_attribute, is_svg_idref_attribute,
};
use crate::svg::parity::{C4_EXTERNAL_PERSON_IMG, C4_PERSON_IMG};
use std::cell::{Cell, RefCell};

const STATIC_VALIDATION_PASS: &str = "validate-static-inline-svg";
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
const XHTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";
const XLINK_NAMESPACE: &str = "http://www.w3.org/1999/xlink";
const CSS_NESTING_HARD_LIMIT: u8 = 64;
const ROOT_DIMENSION_HARD_LIMIT_PX: f64 = 1_000_000.0;
const SVG_BYTES_PER_ROOT_PIXEL: usize = 16;

pub(super) fn validate_rustdoc_static_svg(
    svg: &str,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    admit_svg_bytes(svg, execution)?;
    validate_well_formed_svg_with_execution(svg, execution)?;
    validate(svg, ForeignObjectPolicy::Reject, CssStage::Final, execution)
}

pub(super) fn validate_rustdoc_admission_svg(
    svg: &str,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    admit_svg_bytes(svg, execution)?;
    validate_well_formed_svg_with_execution(svg, execution)?;
    validate(
        svg,
        ForeignObjectPolicy::AllowSafeXhtml,
        CssStage::Admission,
        execution,
    )
}

fn admit_svg_bytes(svg: &str, execution: SvgPostprocessExecution<'_>) -> Result<()> {
    execution.checkpoint()?;
    execution.preflight_svg_byte_count(svg.len())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ForeignObjectPolicy {
    Reject,
    AllowSafeXhtml,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CssStage {
    Admission,
    Final,
}

fn validate(
    svg: &str,
    foreign_objects: ForeignObjectPolicy,
    css_stage: CssStage,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    let limits = execution.resource_policy();
    let mut checkpoint = || execution.checkpoint();
    checkpoint()?;
    let document = roxmltree::Document::parse(svg).map_err(|error| {
        static_validation_error(format!("rendered SVG is not valid XML: {error}"))
    })?;
    checkpoint()?;
    let root = document.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("svg") {
        return Err(static_validation_error(
            "rendered document root is not <svg>",
        ));
    }
    let root_id = unnamespaced_attribute(root, "id");
    let root_dimension_limit = root_dimension_limit(limits);

    let mut ids = BTreeSet::new();
    for (iteration, node) in document
        .descendants()
        .filter(roxmltree::Node::is_element)
        .enumerate()
    {
        checkpoint_loop(iteration, &mut checkpoint)?;
        if let Some(id) = unnamespaced_attribute(node, "id") {
            if id.is_empty() {
                return Err(static_validation_error("rendered SVG contains an empty id"));
            }
            if !ids.insert(id.to_string()) {
                return Err(static_validation_error(format!(
                    "rendered SVG contains duplicate id {id:?}"
                )));
            }
        }
    }

    let mut attribute_iteration = 0usize;
    for (node_iteration, node) in document
        .descendants()
        .filter(roxmltree::Node::is_element)
        .enumerate()
    {
        checkpoint_loop(node_iteration, &mut checkpoint)?;
        let element = node.tag_name().name();
        let namespace = node.tag_name().namespace();
        let inside_foreign_object = is_inside_svg_foreign_object(node, execution)?;
        let safe_xhtml = foreign_objects == ForeignObjectPolicy::AllowSafeXhtml
            && namespace == Some(XHTML_NAMESPACE)
            && inside_foreign_object;
        if !safe_xhtml
            && let Some(namespace) = namespace
            && namespace != SVG_NAMESPACE
        {
            return Err(static_validation_error(format!(
                "rendered SVG contains non-SVG element <{element}> in namespace {namespace:?}"
            )));
        }
        let allowed_foreign_object = foreign_objects == ForeignObjectPolicy::AllowSafeXhtml
            && element.eq_ignore_ascii_case("foreignObject")
            && node
                .tag_name()
                .namespace()
                .is_none_or(|namespace| namespace == SVG_NAMESPACE);
        if element.eq_ignore_ascii_case("base")
            || (matches_active_svg_element(element) && !allowed_foreign_object)
            || (inside_foreign_object && is_forbidden_xhtml_element(element))
        {
            return Err(static_validation_error(format!(
                "rendered SVG contains forbidden <{element}> content"
            )));
        }
        if safe_xhtml {
            if !is_allowed_static_xhtml_element(element) {
                return Err(static_validation_error(format!(
                    "rendered SVG contains forbidden XHTML element <{element}>"
                )));
            }
        } else if !is_allowed_static_svg_element(element) {
            return Err(static_validation_error(format!(
                "rendered SVG contains unsupported SVG element <{element}>"
            )));
        }

        for attribute in node.attributes() {
            checkpoint_loop(attribute_iteration, &mut checkpoint)?;
            attribute_iteration = attribute_iteration.saturating_add(1);
            let name = attribute.name();
            let value = attribute.value();
            if node == root && attribute.namespace().is_none() {
                if matches!(name.to_ascii_lowercase().as_str(), "width" | "height") {
                    validate_root_layout_value_with_checkpoint(
                        || validate_root_dimension(name, value, root_dimension_limit),
                        &mut checkpoint,
                    )?;
                } else if name.eq_ignore_ascii_case("viewBox") {
                    validate_root_layout_value_with_checkpoint(
                        || validate_root_view_box(value, root_dimension_limit),
                        &mut checkpoint,
                    )?;
                }
            }
            if name.eq_ignore_ascii_case("base") {
                return Err(static_validation_error(format!(
                    "rendered SVG contains forbidden base attribute {name:?}"
                )));
            }
            if is_event_attribute(name) {
                return Err(static_validation_error(format!(
                    "rendered SVG contains event attribute {name:?}"
                )));
            }
            if is_forbidden_embedding_attribute(name)
                || (inside_foreign_object
                    && is_forbidden_xhtml_attribute(name)
                    && !(safe_xhtml
                        && element.eq_ignore_ascii_case("a")
                        && name.eq_ignore_ascii_case("href")))
            {
                return Err(static_validation_error(format!(
                    "rendered SVG contains forbidden embedding attribute {name:?}"
                )));
            }
            if name.eq_ignore_ascii_case("href") {
                if let Some(namespace) = attribute.namespace()
                    && namespace != XLINK_NAMESPACE
                {
                    return Err(static_validation_error(format!(
                        "rendered SVG contains href in unsupported namespace {namespace:?}"
                    )));
                }
                validate_href(element, value, &ids, execution)?;
            }
            if is_svg_idref_attribute(name) {
                validate_idrefs(name, value, &ids, execution)?;
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "src" | "data" | "poster"
            ) && !value.trim().is_empty()
            {
                return Err(static_validation_error(format!(
                    "rendered SVG contains external resource attribute {name:?}"
                )));
            }
            if is_css_value_attribute(name) {
                validate_css(value, &ids, false, execution)?;
                if node == root && name.eq_ignore_ascii_case("style") {
                    validate_root_layout_declaration_text(value, root_dimension_limit, execution)?;
                }
                if css_stage == CssStage::Final && name.eq_ignore_ascii_case("style") {
                    validate_resvg_css_declaration_list_with_checkpoints(value, &mut checkpoint)
                        .map_err(|error| {
                            map_controlled_css_error(error, |message| {
                                format!(
                                    "rendered SVG contains non-static CSS declarations: {message}"
                                )
                            })
                        })?;
                }
            }
        }

        if element.eq_ignore_ascii_case("style") {
            let mut css = String::new();
            for (child_iteration, child) in node.children().enumerate() {
                checkpoint_loop(child_iteration, &mut checkpoint)?;
                if let Some(text) = child.text() {
                    checkpoint()?;
                    css.push_str(text);
                    checkpoint()?;
                }
            }
            validate_css(&css, &ids, true, execution)?;
            let root_id = root_id.ok_or_else(|| {
                static_validation_error("rendered SVG with embedded CSS must have a root id")
            })?;
            validate_stylesheet_scope(
                &css,
                root_id,
                css_stage == CssStage::Admission,
                root_dimension_limit,
                execution,
            )?;
            if css_stage == CssStage::Final {
                validate_resvg_css_stylesheet_with_checkpoints(&css, &mut checkpoint).map_err(
                    |error| {
                        map_controlled_css_error(error, |message| {
                            format!("rendered SVG contains non-static CSS: {message}")
                        })
                    },
                )?;
            }
        }
    }
    validate_browser_reference_expansion(&document, execution)?;
    checkpoint()
}

fn validate_browser_reference_expansion(
    document: &roxmltree::Document<'_>,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    let mut checkpoint = || execution.checkpoint();
    checkpoint()?;
    let mut elements = Vec::new();
    for (iteration, node) in document
        .descendants()
        .filter(roxmltree::Node::is_element)
        .enumerate()
    {
        checkpoint_loop(iteration, &mut checkpoint)?;
        elements.push(node);
    }
    checkpoint()?;
    let mut indices = HashMap::with_capacity(elements.len());
    let mut ids = HashMap::new();
    let mut dependencies = vec![Vec::new(); elements.len()];

    for (index, node) in elements.iter().copied().enumerate() {
        checkpoint_loop(index, &mut checkpoint)?;
        indices.insert(node.id(), index);
        if let Some(id) = unnamespaced_attribute(node, "id") {
            ids.insert(id, index);
        }
        if let Some(parent) = node.parent_element()
            && let Some(&parent_index) = indices.get(&parent.id())
        {
            dependencies[parent_index].push((index, 1));
        }
    }

    for (index, node) in elements.iter().copied().enumerate() {
        checkpoint_loop(index, &mut checkpoint)?;
        if !node.tag_name().name().eq_ignore_ascii_case("use")
            || node
                .tag_name()
                .namespace()
                .is_some_and(|namespace| namespace != SVG_NAMESPACE)
        {
            continue;
        }
        let href = unnamespaced_attribute(node, "href")
            .or_else(|| node.attribute((XLINK_NAMESPACE, "href")));
        let Some(target) = href.and_then(|value| value.trim().strip_prefix('#')) else {
            continue;
        };
        if let Some(&target_index) = ids.get(target) {
            dependencies[index].push((target_index, 1));
        }
    }

    let graph = ReferenceDependencyGraph::new(dependencies, elements.len());
    let plan = match plan_svg_reference_dependencies_with_checkpoints(&graph, &mut checkpoint) {
        Ok(plan) => plan,
        Err(ReferencePlanningError::Invalid(error)) => {
            return Err(static_validation_error(error));
        }
        Err(ReferencePlanningError::Checkpoint(error)) => return Err(error),
    };
    checkpoint()?;
    execution.preflight_svg_structure(plan.expanded_elements(), plan.max_tree_depth())?;
    checkpoint()
}

fn unnamespaced_attribute<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.namespace().is_none() && attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn is_inside_svg_foreign_object(
    node: roxmltree::Node<'_, '_>,
    execution: SvgPostprocessExecution<'_>,
) -> Result<bool> {
    let mut checkpoint = || execution.checkpoint();
    for (iteration, ancestor) in node.ancestors().enumerate() {
        checkpoint_loop(iteration, &mut checkpoint)?;
        if ancestor.is_element()
            && ancestor
                .tag_name()
                .name()
                .eq_ignore_ascii_case("foreignObject")
            && ancestor
                .tag_name()
                .namespace()
                .is_none_or(|namespace| namespace == SVG_NAMESPACE)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_event_attribute(name: &str) -> bool {
    name.get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("on"))
}

fn is_allowed_static_svg_element(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "a" | "circle"
            | "clippath"
            | "defs"
            | "desc"
            | "ellipse"
            | "feblend"
            | "fecolormatrix"
            | "fecomponenttransfer"
            | "fecomposite"
            | "feconvolvematrix"
            | "fediffuselighting"
            | "fedisplacementmap"
            | "fedistantlight"
            | "fedropshadow"
            | "feflood"
            | "fefunca"
            | "fefuncb"
            | "fefuncg"
            | "fefuncr"
            | "fegaussianblur"
            | "feimage"
            | "femerge"
            | "femergenode"
            | "femorphology"
            | "feoffset"
            | "fepointlight"
            | "fespecularlighting"
            | "fespotlight"
            | "fetile"
            | "feturbulence"
            | "filter"
            | "foreignobject"
            | "g"
            | "image"
            | "line"
            | "lineargradient"
            | "marker"
            | "mask"
            | "metadata"
            | "path"
            | "pattern"
            | "polygon"
            | "polyline"
            | "radialgradient"
            | "rect"
            | "stop"
            | "style"
            | "svg"
            | "switch"
            | "symbol"
            | "text"
            | "textpath"
            | "title"
            | "tspan"
            | "use"
            | "view"
    )
}

fn is_allowed_static_xhtml_element(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "blockquote"
            | "br"
            | "center"
            | "cite"
            | "code"
            | "dd"
            | "del"
            | "div"
            | "dl"
            | "dt"
            | "em"
            | "font"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "i"
            | "ins"
            | "kbd"
            | "li"
            | "mark"
            | "menu"
            | "ol"
            | "p"
            | "pre"
            | "q"
            | "rp"
            | "rt"
            | "ruby"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strike"
            | "strong"
            | "sub"
            | "sup"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
            | "tt"
            | "u"
            | "ul"
            | "var"
            | "wbr"
    )
}

fn is_forbidden_xhtml_element(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "audio"
            | "button"
            | "canvas"
            | "embed"
            | "form"
            | "iframe"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "object"
            | "picture"
            | "script"
            | "select"
            | "source"
            | "style"
            | "textarea"
            | "track"
            | "video"
    )
}

fn is_forbidden_xhtml_attribute(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "action"
            | "attributionsrc"
            | "autofocus"
            | "contenteditable"
            | "download"
            | "formaction"
            | "href"
            | "ping"
            | "poster"
            | "src"
            | "tabindex"
            | "target"
    )
}

fn is_forbidden_embedding_attribute(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "attributionsrc" | "download" | "ping"
    )
}

fn validate_href(
    element: &str,
    value: &str,
    ids: &BTreeSet<String>,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    let mut checkpoint = || execution.checkpoint();
    checkpoint()?;
    let value = value.trim();
    if let Some(target) = value.strip_prefix('#') {
        return require_local_target(target, ids, "href").map_err(static_validation_error);
    }

    if matches!(element.to_ascii_lowercase().as_str(), "image" | "feimage")
        && value
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
    {
        checkpoint()?;
        let result = validate_inline_raster(value).map_err(static_validation_error);
        checkpoint()?;
        return result;
    }

    if !element.eq_ignore_ascii_case("a") {
        return Err(static_validation_error(format!(
            "rendered <{element}> references a non-local resource {value:?}"
        )));
    }

    let mut compact = String::with_capacity(value.len());
    for (iteration, character) in value.chars().enumerate() {
        checkpoint_loop(iteration, &mut checkpoint)?;
        if !character.is_ascii_control() && !character.is_ascii_whitespace() {
            compact.extend(character.to_lowercase());
        }
    }
    checkpoint()?;
    if compact.starts_with("javascript:")
        || compact.starts_with("vbscript:")
        || compact.starts_with("data:")
        || compact.starts_with("file:")
        || compact.starts_with("//")
    {
        return Err(static_validation_error(format!(
            "rendered SVG contains unsafe navigation href {value:?}"
        )));
    }
    Ok(())
}

fn validate_inline_raster(value: &str) -> std::result::Result<(), String> {
    if !is_safe_data_image_url(value) {
        return Err("rendered SVG contains an invalid inline raster data URL".to_string());
    }
    if matches!(value, C4_PERSON_IMG | C4_EXTERNAL_PERSON_IMG) {
        Ok(())
    } else {
        Err("rendered SVG contains an unrecognized inline raster asset".to_string())
    }
}

fn require_local_target(
    target: &str,
    ids: &BTreeSet<String>,
    context: &str,
) -> std::result::Result<(), String> {
    if target.is_empty() || !ids.contains(target) {
        return Err(format!(
            "rendered SVG {context} references missing local id {target:?}"
        ));
    }
    Ok(())
}

fn validate_idrefs(
    attribute: &str,
    value: &str,
    ids: &BTreeSet<String>,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    let mut checkpoint = || execution.checkpoint();
    checkpoint()?;
    let mut references = value.split_ascii_whitespace().peekable();
    if references.peek().is_none() {
        return Err(static_validation_error(format!(
            "rendered SVG contains an empty {attribute} reference"
        )));
    }
    for (iteration, reference) in references.enumerate() {
        checkpoint_loop(iteration, &mut checkpoint)?;
        require_local_target(reference, ids, attribute).map_err(static_validation_error)?;
    }
    checkpoint()
}

fn validate_css(
    css: &str,
    ids: &BTreeSet<String>,
    stylesheet: bool,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    run_css_validation(execution, |control| {
        let mut parser = Parser::new(css);
        validate_css_parser(&mut parser, ids, stylesheet, 0, control)
    })
}

fn validate_stylesheet_scope(
    css: &str,
    root_id: &str,
    allow_discardable_animation: bool,
    root_dimension_limit: f64,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    run_css_validation(execution, |control| {
        let mut input = Parser::new(css);
        validate_scoped_rule_list(
            &mut input,
            root_id,
            0,
            allow_discardable_animation,
            root_dimension_limit,
            control,
        )
        .map_err(|error| match error.kind {
            ParseErrorKind::Custom(message) => message,
            ParseErrorKind::Basic(error) => {
                format!("rendered SVG contains invalid scoped CSS: {error}")
            }
        })
    })
}

struct StaticCssControl<'a> {
    execution: SvgPostprocessExecution<'a>,
    iterations: Cell<usize>,
    error: RefCell<Option<Error>>,
}

impl<'a> StaticCssControl<'a> {
    fn new(execution: SvgPostprocessExecution<'a>) -> Self {
        Self {
            execution,
            iterations: Cell::new(0),
            error: RefCell::new(None),
        }
    }

    fn observe(&self) -> bool {
        if self.error.borrow().is_some() {
            return false;
        }
        let iteration = self.iterations.get();
        self.iterations.set(iteration.saturating_add(1));
        if iteration & 63 != 0 {
            return true;
        }
        match self.execution.checkpoint() {
            Ok(()) => true,
            Err(error) => {
                self.error.replace(Some(error));
                false
            }
        }
    }

    fn take_error(&self) -> Option<Error> {
        self.error.borrow_mut().take()
    }
}

fn run_css_validation<T>(
    execution: SvgPostprocessExecution<'_>,
    validate: impl FnOnce(&StaticCssControl<'_>) -> std::result::Result<T, String>,
) -> Result<T> {
    execution.checkpoint()?;
    let control = StaticCssControl::new(execution);
    let result = validate(&control);
    if let Some(error) = control.take_error() {
        return Err(error);
    }
    execution.checkpoint()?;
    result.map_err(static_validation_error)
}

fn css_checkpoint(control: &StaticCssControl<'_>) -> std::result::Result<(), String> {
    control
        .observe()
        .then_some(())
        .ok_or_else(|| "static SVG CSS validation was interrupted".to_string())
}

fn css_parse_checkpoint(
    control: &StaticCssControl<'_>,
) -> std::result::Result<(), ParseError<String>> {
    css_checkpoint(control).map_err(ParseError::custom)
}

#[derive(Clone, Copy)]
enum ScopedAtRule {
    Group,
    DiscardableAnimation,
}

struct ScopedRuleParser<'root, 'control, 'execution> {
    root_id: &'root str,
    nesting: u8,
    allow_discardable_animation: bool,
    root_dimension_limit: f64,
    control: &'control StaticCssControl<'execution>,
}

impl<'i> AtRuleParser<'i> for ScopedRuleParser<'_, '_, '_> {
    type Prelude = ScopedAtRule;
    type AtRule = ();
    type Error = String;

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::Prelude, ParseError<Self::Error>> {
        consume_scoped_css_tokens(input, self.nesting, self.control)?;
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "container" | "document" | "layer" | "media" | "scope" | "supports"
        ) {
            Ok(ScopedAtRule::Group)
        } else if self.allow_discardable_animation
            && matches!(
                name.to_ascii_lowercase().as_str(),
                "keyframes" | "-webkit-keyframes"
            )
        {
            Ok(ScopedAtRule::DiscardableAnimation)
        } else {
            Err(ParseError::custom(format!(
                "rendered SVG contains forbidden CSS @{name} rule"
            )))
        }
    }

    fn rule_without_block(
        &mut self,
        _prelude: Self::Prelude,
        _start: &ParserState,
    ) -> std::result::Result<Self::AtRule, ()> {
        Err(())
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::AtRule, ParseError<Self::Error>> {
        match prelude {
            ScopedAtRule::Group => validate_scoped_rule_list(
                input,
                self.root_id,
                self.nesting + 1,
                self.allow_discardable_animation,
                self.root_dimension_limit,
                self.control,
            ),
            ScopedAtRule::DiscardableAnimation => {
                consume_scoped_css_tokens(input, self.nesting + 1, self.control)
            }
        }
    }
}

impl<'i> QualifiedRuleParser<'i> for ScopedRuleParser<'_, '_, '_> {
    type Prelude = SelectorScope;
    type QualifiedRule = ();
    type Error = String;

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::Prelude, ParseError<Self::Error>> {
        validate_selector_list_scope(input, self.root_id, self.nesting, self.control)
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::QualifiedRule, ParseError<Self::Error>> {
        if prelude.targets_root {
            validate_root_layout_declarations(
                input,
                self.nesting,
                self.root_dimension_limit,
                self.control,
            )
        } else {
            consume_scoped_css_tokens(input, self.nesting, self.control)
        }
    }
}

#[derive(Clone, Copy)]
struct SelectorScope {
    targets_root: bool,
}

fn validate_scoped_rule_list(
    input: &mut Parser<'_>,
    root_id: &str,
    nesting: u8,
    allow_discardable_animation: bool,
    root_dimension_limit: f64,
    control: &StaticCssControl<'_>,
) -> std::result::Result<(), ParseError<String>> {
    css_parse_checkpoint(control)?;
    if nesting >= CSS_NESTING_HARD_LIMIT {
        return Err(ParseError::custom(
            "rendered SVG CSS exceeds the nesting limit".to_string(),
        ));
    }
    let mut parser = ScopedRuleParser {
        root_id,
        nesting,
        allow_discardable_animation,
        root_dimension_limit,
        control,
    };
    for rule in StyleSheetParser::new(input, &mut parser) {
        rule.map_err(|(error, _, _)| error)?;
    }
    Ok(())
}

fn validate_selector_list_scope(
    input: &mut Parser<'_>,
    root_id: &str,
    nesting: u8,
    control: &StaticCssControl<'_>,
) -> std::result::Result<SelectorScope, ParseError<String>> {
    let mut expects_root = true;
    let mut saw_root = false;
    let mut current_targets_root = false;
    let mut any_targets_root = false;
    let mut pending_descendant = false;
    loop {
        css_parse_checkpoint(control)?;
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                if saw_root && !expects_root {
                    return Ok(SelectorScope {
                        targets_root: any_targets_root || current_targets_root,
                    });
                }
                return Err(ParseError::custom(
                    "rendered SVG contains an empty CSS selector".to_string(),
                ));
            }
            Err(error) => return Err(error.into()),
        };
        match token {
            Token::WhiteSpace(_) if saw_root => pending_descendant = true,
            Token::WhiteSpace(_) | Token::Comment(_) => {}
            Token::Comma if !expects_root => {
                any_targets_root |= current_targets_root;
                expects_root = true;
                saw_root = false;
                current_targets_root = false;
                pending_descendant = false;
            }
            Token::IDHash(id) | Token::Hash(id) if expects_root && id.as_ref() == root_id => {
                expects_root = false;
                saw_root = true;
                current_targets_root = true;
                pending_descendant = false;
            }
            Token::Delim('+') | Token::Delim('~') | Token::Delim('|') => {
                return Err(ParseError::custom(
                    "rendered SVG CSS selector can escape the SVG root".to_string(),
                ));
            }
            Token::Delim('>') if !expects_root => {
                current_targets_root = false;
                pending_descendant = false;
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock
                if !expects_root =>
            {
                if pending_descendant {
                    current_targets_root = false;
                    pending_descendant = false;
                }
                input.parse_nested_block(|nested| {
                    consume_scoped_css_tokens(nested, nesting + 1, control)
                })?;
            }
            _ if expects_root => {
                return Err(ParseError::custom(format!(
                    "rendered SVG CSS selector must start with #{root_id}"
                )));
            }
            _ => {
                if pending_descendant {
                    current_targets_root = false;
                    pending_descendant = false;
                }
            }
        }
    }
}

fn validate_root_layout_declaration_text(
    css: &str,
    root_dimension_limit: f64,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    run_css_validation(execution, |control| {
        let mut input = Parser::new(css);
        validate_root_layout_declarations(&mut input, 0, root_dimension_limit, control).map_err(
            |error| match error.kind {
                ParseErrorKind::Custom(message) => message,
                ParseErrorKind::Basic(error) => {
                    format!("rendered SVG contains invalid root CSS: {error}")
                }
            },
        )
    })
}

fn map_controlled_css_error(
    error: CssValidationError<Error>,
    describe: impl FnOnce(String) -> String,
) -> Error {
    match error {
        CssValidationError::Invalid(message) => static_validation_error(describe(message)),
        CssValidationError::Checkpoint(error) => error,
    }
}

fn validate_root_layout_declarations(
    input: &mut Parser<'_>,
    nesting: u8,
    root_dimension_limit: f64,
    control: &StaticCssControl<'_>,
) -> std::result::Result<(), ParseError<String>> {
    css_parse_checkpoint(control)?;
    if nesting >= CSS_NESTING_HARD_LIMIT {
        return Err(ParseError::custom(
            "rendered SVG CSS exceeds the nesting limit".to_string(),
        ));
    }
    while !input.is_exhausted() {
        css_parse_checkpoint(control)?;
        input.parse_until_after(cssparser::Delimiter::Semicolon, |declaration| {
            css_parse_checkpoint(control)?;
            let property = declaration.expect_ident_cloned()?;
            declaration.expect_colon()?;
            let normalized = property.to_ascii_lowercase();
            if is_root_size_property(&normalized) {
                let value_start = declaration.position();
                consume_scoped_css_tokens(declaration, nesting + 1, control)?;
                css_parse_checkpoint(control)?;
                let value = declaration.slice_from(value_start).trim().to_string();
                css_parse_checkpoint(control)?;
                validate_root_dimension(&property, &value, root_dimension_limit)
                    .map_err(ParseError::custom)?;
            } else if let Some(font_property) = usecase_root_font_property(&property) {
                validate_usecase_root_font_value(
                    declaration,
                    font_property,
                    root_dimension_limit,
                    control,
                )?;
            } else if is_allowed_root_presentation_property(&normalized) {
                consume_scoped_css_tokens(declaration, nesting + 1, control)?;
            } else {
                return Err(ParseError::custom(format!(
                    "rendered SVG root layout cannot set {property}"
                )));
            }
            Ok(())
        })?;
    }
    Ok(())
}

// The Usecase renderer emits only these six literal font variables on the root.
// Do not admit arbitrary custom properties or deferred values such as var()/url().
fn usecase_root_font_property(property: &str) -> Option<&'static str> {
    match property {
        "--mermaid-usecase-actor-font-size" | "--mermaid-usecase-font-size" => Some("font-size"),
        "--mermaid-usecase-actor-font-family" | "--mermaid-usecase-font-family" => {
            Some("font-family")
        }
        "--mermaid-usecase-actor-font-weight" | "--mermaid-usecase-font-weight" => {
            Some("font-weight")
        }
        _ => None,
    }
}

fn validate_usecase_root_font_value(
    input: &mut Parser<'_>,
    property: &str,
    size_limit: f64,
    control: &StaticCssControl<'_>,
) -> std::result::Result<(), ParseError<String>> {
    let invalid = || format!("rendered SVG root layout has invalid Usecase {property}");
    if property == "font-family" {
        let mut has_family = false;
        let mut quoted_family = false;
        while !input.is_exhausted() {
            css_parse_checkpoint(control)?;
            match input.next()? {
                Token::Ident(name)
                    if !quoted_family
                        && !["inherit", "initial", "unset", "revert", "revert-layer"]
                            .iter()
                            .any(|keyword| name.eq_ignore_ascii_case(keyword)) =>
                {
                    has_family = true;
                }
                Token::QuotedString(name) if !has_family && !name.is_empty() => {
                    has_family = true;
                    quoted_family = true;
                }
                Token::Comma if has_family => {
                    has_family = false;
                    quoted_family = false;
                }
                _ => return Err(ParseError::custom(invalid())),
            }
        }
        if !has_family {
            return Err(ParseError::custom(invalid()));
        }
    } else {
        css_parse_checkpoint(control)?;
        let valid = match (property, input.next()?) {
            ("font-size", Token::Dimension { value, unit, .. }) => {
                value.is_finite()
                    && *value >= 0.0
                    && f64::from(*value) <= size_limit
                    && unit.eq_ignore_ascii_case("px")
            }
            ("font-weight", Token::Number { value, .. }) => {
                value.is_finite() && (1.0..=1000.0).contains(value)
            }
            ("font-weight", Token::Ident(value)) => ["normal", "bold", "bolder", "lighter"]
                .iter()
                .any(|allowed| value.eq_ignore_ascii_case(allowed)),
            _ => false,
        };
        if !valid {
            return Err(ParseError::custom(invalid()));
        }
        input.expect_exhausted()?;
    }
    Ok(())
}

fn is_allowed_root_presentation_property(property: &str) -> bool {
    matches!(
        property,
        "background-color"
            | "color"
            | "cursor"
            | "direction"
            | "fill"
            | "fill-opacity"
            | "fill-rule"
            | "font"
            | "font-family"
            | "font-size"
            | "font-stretch"
            | "font-style"
            | "font-variant"
            | "font-weight"
            | "letter-spacing"
            | "opacity"
            | "paint-order"
            | "pointer-events"
            | "shape-rendering"
            | "stroke"
            | "stroke-dasharray"
            | "stroke-dashoffset"
            | "stroke-linecap"
            | "stroke-linejoin"
            | "stroke-miterlimit"
            | "stroke-opacity"
            | "stroke-width"
            | "text-anchor"
            | "text-decoration"
            | "text-rendering"
            | "unicode-bidi"
            | "visibility"
            | "white-space"
            | "word-spacing"
    )
}

fn is_root_size_property(property: &str) -> bool {
    matches!(
        property,
        "width"
            | "height"
            | "min-width"
            | "min-height"
            | "max-width"
            | "max-height"
            | "inline-size"
            | "block-size"
            | "min-inline-size"
            | "min-block-size"
            | "max-inline-size"
            | "max-block-size"
    )
}

fn root_dimension_limit(limits: RenderResourcePolicy) -> f64 {
    limits
        .value(ResourceLimitId::MaxSvgBytes)
        .map(|bytes| bytes / SVG_BYTES_PER_ROOT_PIXEL)
        .unwrap_or(ROOT_DIMENSION_HARD_LIMIT_PX as usize)
        .clamp(1, ROOT_DIMENSION_HARD_LIMIT_PX as usize) as f64
}

fn validate_root_layout_value_with_checkpoint(
    validate: impl FnOnce() -> std::result::Result<(), String>,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    let result = validate();
    checkpoint()?;
    result.map_err(static_validation_error)
}

fn validate_root_dimension(
    property: &str,
    value: &str,
    limit: f64,
) -> std::result::Result<(), String> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") {
        return Ok(());
    }
    let length = value
        .parse::<Length>()
        .map_err(|_| format!("rendered SVG root layout has invalid {property} value {value:?}"))?;
    if !length.number.is_finite() || length.number <= 0.0 {
        return Err(format!(
            "rendered SVG root layout requires positive finite {property}"
        ));
    }
    match length.unit {
        LengthUnit::None | LengthUnit::Px if length.number <= limit => Ok(()),
        LengthUnit::Percent if length.number <= 100.0 => Ok(()),
        LengthUnit::None | LengthUnit::Px | LengthUnit::Percent => Err(format!(
            "rendered SVG root layout {property} exceeds its bounded size"
        )),
        _ => Err(format!(
            "rendered SVG root layout has unsupported {property} unit"
        )),
    }
}

fn validate_root_view_box(value: &str, limit: f64) -> std::result::Result<(), String> {
    let mut values = NumberListParser::from(value);
    let mut parsed = [0.0; 4];
    for slot in &mut parsed {
        *slot = values
            .next()
            .ok_or_else(|| "rendered SVG root layout has an invalid viewBox".to_string())?
            .map_err(|_| "rendered SVG root layout has an invalid viewBox".to_string())?;
    }
    if values.next().is_some()
        || parsed.iter().any(|value| !value.is_finite())
        || parsed[2] <= 0.0
        || parsed[3] <= 0.0
        || parsed[0].abs() > limit
        || parsed[1].abs() > limit
        || parsed[2] > limit
        || parsed[3] > limit
    {
        return Err("rendered SVG root layout has an unbounded viewBox".to_string());
    }
    Ok(())
}

fn consume_scoped_css_tokens(
    input: &mut Parser<'_>,
    nesting: u8,
    control: &StaticCssControl<'_>,
) -> std::result::Result<(), ParseError<String>> {
    css_parse_checkpoint(control)?;
    if nesting >= CSS_NESTING_HARD_LIMIT {
        return Err(ParseError::custom(
            "rendered SVG CSS exceeds the nesting limit".to_string(),
        ));
    }
    loop {
        css_parse_checkpoint(control)?;
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if matches!(
            token,
            Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock
        ) {
            input.parse_nested_block(|nested| {
                consume_scoped_css_tokens(nested, nesting + 1, control)
            })?;
        }
    }
}

fn validate_css_parser(
    input: &mut Parser<'_>,
    ids: &BTreeSet<String>,
    stylesheet: bool,
    nesting: u8,
    control: &StaticCssControl<'_>,
) -> std::result::Result<(), String> {
    css_checkpoint(control)?;
    if nesting >= CSS_NESTING_HARD_LIMIT {
        return Err("rendered SVG CSS exceeds the nesting limit".to_string());
    }
    loop {
        css_checkpoint(control)?;
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => return Ok(()),
            Err(error) => return Err(format!("rendered SVG contains invalid CSS: {error:?}")),
        };
        match token {
            Token::AtKeyword(name) if stylesheet && name.eq_ignore_ascii_case("import") => {
                return Err("rendered SVG contains a forbidden CSS @import".to_string());
            }
            Token::UnquotedUrl(value) => validate_css_url(&value, ids, !stylesheet)?,
            Token::Function(name) => {
                if matches_external_image_function(&name) {
                    return Err(
                        "rendered SVG contains a forbidden CSS image-set function".to_string()
                    );
                }
                let is_url = name.eq_ignore_ascii_case("url");
                input
                    .parse_nested_block(|nested| {
                        if is_url {
                            validate_quoted_css_url(nested, ids, !stylesheet, control)
                        } else {
                            validate_css_parser(nested, ids, stylesheet, nesting + 1, control)
                        }
                        .map_err(ParseError::<String>::custom)
                    })
                    .map_err(|error| nested_css_error(error, "function"))?;
            }
            Token::ParenthesisBlock | Token::SquareBracketBlock | Token::CurlyBracketBlock => {
                input
                    .parse_nested_block(|nested| {
                        validate_css_parser(nested, ids, stylesheet, nesting + 1, control)
                            .map_err(ParseError::<String>::custom)
                    })
                    .map_err(|error| nested_css_error(error, "block"))?;
            }
            Token::BadUrl(_) | Token::BadString(_) => {
                return Err("rendered SVG contains an invalid CSS token".to_string());
            }
            _ => {}
        }
    }
}

fn nested_css_error(error: ParseError<String>, context: &str) -> String {
    match error.kind {
        ParseErrorKind::Custom(message) => message,
        ParseErrorKind::Basic(error) => {
            format!("rendered SVG contains an invalid CSS {context}: {error}")
        }
    }
}

fn validate_quoted_css_url(
    input: &mut Parser<'_>,
    ids: &BTreeSet<String>,
    require_target: bool,
    control: &StaticCssControl<'_>,
) -> std::result::Result<(), String> {
    let mut payload = None::<Token<'_>>;
    loop {
        css_checkpoint(control)?;
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => break,
            Err(error) => {
                return Err(format!(
                    "rendered SVG contains an invalid CSS URL: {error:?}"
                ));
            }
        };
        match token {
            Token::WhiteSpace(_) | Token::Comment(_) => {}
            Token::BadUrl(_) | Token::BadString(_) => {
                return Err("rendered SVG contains an invalid CSS URL token".to_string());
            }
            token if payload.is_none() => payload = Some(token),
            _ => return Err("rendered SVG contains a malformed CSS URL".to_string()),
        }
    }

    match payload {
        Some(Token::QuotedString(value)) => validate_css_url(&value, ids, require_target),
        Some(Token::IDHash(value)) => validate_local_css_target(&value, ids, require_target),
        Some(_) => Err("rendered SVG contains a malformed CSS URL".to_string()),
        None => Err("rendered SVG contains an empty CSS URL".to_string()),
    }
}

fn validate_css_url(
    value: &str,
    ids: &BTreeSet<String>,
    require_target: bool,
) -> std::result::Result<(), String> {
    let Some(target) = value.trim().strip_prefix('#') else {
        return Err(format!(
            "rendered SVG contains a non-local CSS URL {value:?}"
        ));
    };
    validate_local_css_target(target, ids, require_target)
}

fn validate_local_css_target(
    target: &str,
    ids: &BTreeSet<String>,
    require_target: bool,
) -> std::result::Result<(), String> {
    if target.is_empty() {
        return Err("rendered SVG contains an empty local CSS URL".to_string());
    }
    if require_target {
        require_local_target(target, ids, "CSS URL")
    } else {
        Ok(())
    }
}

fn static_validation_error(message: impl Into<String>) -> Error {
    Error::SvgPostprocess {
        pass: STATIC_VALIDATION_PASS.to_string(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use merman_core::{CancelReason, OperationControl, OperationPhase};

    fn validate_static_with_limits(svg: &str, limits: RenderResourcePolicy) -> Result<()> {
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(limits)
            .begin_session_with_control(OperationControl::new())
            .unwrap();
        validate_rustdoc_static_svg(svg, SvgPostprocessExecution::new(&session))
    }

    fn validate_admission_with_limits(svg: &str, limits: RenderResourcePolicy) -> Result<()> {
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(limits)
            .begin_session_with_control(OperationControl::new())
            .unwrap();
        validate_rustdoc_admission_svg(svg, SvgPostprocessExecution::new(&session))
    }

    fn validate(svg: &str) -> Result<()> {
        validate_static_with_limits(svg, RenderResourcePolicy::trusted_native())
    }

    #[test]
    fn static_css_control_preserves_cancellation_at_token_cadence() {
        let control = OperationControl::new();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(RenderResourcePolicy::trusted_native())
            .begin_session_with_control(control.clone())
            .unwrap();
        let css_control = StaticCssControl::new(SvgPostprocessExecution::new(&session));

        assert!(css_control.observe());
        control.cancel();
        for _ in 1..64 {
            assert!(css_control.observe());
        }
        assert!(!css_control.observe());

        let Some(Error::Cancelled(cancelled)) = css_control.take_error() else {
            panic!("expected structured cancellation");
        };
        assert_eq!(cancelled.phase, OperationPhase::Postprocess);
        assert_eq!(cancelled.reason, CancelReason::Requested);
    }

    #[test]
    fn static_validation_admits_svg_bytes_before_parsing_xml() {
        let limits = RenderResourcePolicy::trusted_native()
            .with_limit(ResourceLimitId::MaxSvgBytes, 1)
            .unwrap();

        let error = validate_static_with_limits("<svg>", limits)
            .expect_err("the byte limit must reject before XML parsing");

        assert!(matches!(error, Error::ResourceLimitExceeded(_)));
    }

    #[test]
    fn root_layout_parse_error_yields_to_sticky_cancellation() {
        let mut checkpoints = 0usize;
        let error = validate_root_layout_value_with_checkpoint(
            || validate_root_dimension("width", "not-a-length", 100.0),
            &mut || {
                checkpoints = checkpoints.saturating_add(1);
                if checkpoints == 2 {
                    Err(Error::Cancelled(merman_core::OperationCancelled {
                        phase: OperationPhase::Postprocess,
                        reason: CancelReason::Requested,
                    }))
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("cancellation after parsing must win over the parse error");

        let Error::Cancelled(cancelled) = error else {
            panic!("expected structured cancellation");
        };
        assert_eq!(cancelled.phase, OperationPhase::Postprocess);
        assert_eq!(cancelled.reason, CancelReason::Requested);
    }

    #[test]
    fn accepts_rebased_static_svg_and_safe_navigation() {
        validate(
            r##"<svg id="root" aria-label="prose url(https://example.test/not-a-resource)"><defs><path id="shape"/><linearGradient id="paint"/></defs><style>#root #shape{fill:url(#paint)}#root .optional{fill:url(#unused-theme-paint)}</style><use href="#shape"/><a href="https://example.test/docs"><text>Docs</text></a></svg>"##,
        )
        .unwrap();
    }

    #[test]
    fn accepts_quoted_local_css_urls_with_whitespace_and_comments() {
        validate(
            r##"<svg><defs><linearGradient id="paint"/></defs><path style='fill:url( "#paint" /* trailing */ )'/></svg>"##,
        )
        .unwrap();
    }

    #[test]
    fn rejects_quoted_css_urls_with_multiple_payload_tokens() {
        let error = validate(
            r##"<svg><defs><linearGradient id="paint"/></defs><path style='fill:url("#paint" fallback)'/></svg>"##,
        )
        .expect_err("a CSS URL must contain exactly one payload token");

        assert!(error.to_string().contains("malformed CSS URL"), "{error}");
    }

    #[test]
    fn rejects_active_and_external_svg_content() {
        for (svg, expected) in [
            (r#"<svg><script/></svg>"#, "forbidden <script>"),
            (r#"<svg><g onclick="x"/></svg>"#, "event attribute"),
            (
                r#"<svg><image href="https://example.test/a.png"/></svg>"#,
                "non-local resource",
            ),
            (
                r#"<svg><linearGradient href="https://example.test/p.svg#g"/></svg>"#,
                "non-local resource",
            ),
            (
                r#"<svg xml:base="https://example.test/"><path/></svg>"#,
                "base attribute",
            ),
            (
                r#"<svg><style>@import url(https://example.test/a.css)</style></svg>"#,
                "@import",
            ),
            (
                r#"<svg><path style="fill:url(#missing)"/></svg>"#,
                "missing local id",
            ),
            (
                r#"<svg id="root" aria-controls="missing"><path/></svg>"#,
                "missing local id",
            ),
            (
                r#"<svg><style>.x{fill:u\72l(https://example.test/a.svg)}</style></svg>"#,
                "non-local CSS URL",
            ),
            (
                r#"<svg><path background="url(https://tracker.test/a.png)"/></svg>"#,
                "non-local CSS URL",
            ),
            (
                r#"<svg><path background-image="url(https://tracker.test/a.png)"/></svg>"#,
                "non-local CSS URL",
            ),
            (
                r#"<svg><style>@\69mport 'https://example.test/a.css';</style></svg>"#,
                "@import",
            ),
            (
                r#"<svg><foreignObject><div xmlns="http://www.w3.org/1999/xhtml">label</div></foreignObject></svg>"#,
                "forbidden <foreignObject>",
            ),
            (
                r#"<svg><animate attributeName="x"/></svg>"#,
                "forbidden <animate>",
            ),
            (
                r#"<svg><style>.x{background-image:im\61ge-set(\"https://example.test/x.png\" 1x)}</style></svg>"#,
                "image-set",
            ),
            (
                r#"<svg><image href="data:image/svg+xml;base64,PHN2Zy8+"/></svg>"#,
                "invalid inline raster",
            ),
            (
                r#"<svg><image href="data:image/jpeg;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB"/></svg>"#,
                "unrecognized inline raster",
            ),
            (
                r#"<svg><a href="https://example.test" ping="https://tracker.test"/></svg>"#,
                "embedding attribute",
            ),
            (
                r#"<svg><foreignObject><input xmlns="http://www.w3.org/1999/xhtml" autofocus="autofocus"/></foreignObject></svg>"#,
                "forbidden <foreignObject>",
            ),
        ] {
            let error = validate(svg).expect_err("unsafe SVG");
            assert!(error.to_string().contains(expected), "{expected}: {error}");
        }
    }

    #[test]
    fn admission_allows_safe_xhtml_navigation_but_not_active_links() {
        for href in [
            "https://example.test/docs",
            "/docs",
            "mailto:docs@example.test",
        ] {
            let svg = format!(
                r#"<svg><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><a href="{href}">Docs</a></div></foreignObject></svg>"#
            );
            validate_admission_with_limits(&svg, RenderResourcePolicy::trusted_native()).unwrap();
        }
        for attributes in [
            r#"href="javascript:alert(1)""#,
            r#"href="data:text/html,hello""#,
            r#"href="https://example.test" ping="https://tracker.test""#,
            r#"href="https://example.test" onclick="run()""#,
        ] {
            let svg = format!(
                r#"<svg><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><a {attributes}>Docs</a></div></foreignObject></svg>"#
            );
            assert!(
                validate_admission_with_limits(&svg, RenderResourcePolicy::trusted_native())
                    .is_err(),
                "{svg}"
            );
        }
    }

    #[test]
    fn admission_rejects_interactive_foreign_object_content() {
        let error = validate_admission_with_limits(
            r#"<svg><foreignObject><input xmlns="http://www.w3.org/1999/xhtml" autofocus="autofocus"/></foreignObject></svg>"#,
            RenderResourcePolicy::trusted_native(),
        )
        .expect_err("interactive foreignObject content");

        assert!(error.to_string().contains("forbidden <input>"), "{error}");
    }

    #[test]
    fn rejects_html_foreign_content_breakout_elements_in_svg_namespace() {
        let validators: [fn(&str, RenderResourcePolicy) -> Result<()>; 2] =
            [validate_admission_with_limits, validate_static_with_limits];
        for element in ["meta", "div", "img", "input", "button", "p", "span"] {
            let svg = format!(r#"<svg xmlns="{SVG_NAMESPACE}"><{element}/></svg>"#);
            for validator in validators {
                let error = validator(&svg, RenderResourcePolicy::trusted_native())
                    .expect_err("HTML parser breakout element");
                assert!(
                    error
                        .to_string()
                        .contains(&format!("unsupported SVG element <{element}>")),
                    "{svg}: {error}"
                );
            }
        }
    }

    #[test]
    fn admission_allows_only_pinned_xhtml_below_foreign_object() {
        validate_admission_with_limits(
            r#"<svg><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><span>label</span></div></foreignObject></svg>"#,
            RenderResourcePolicy::trusted_native(),
        )
        .unwrap();

        let error = validate_admission_with_limits(
            r#"<svg><foreignObject><marquee xmlns="http://www.w3.org/1999/xhtml">label</marquee></foreignObject></svg>"#,
            RenderResourcePolicy::trusted_native(),
        )
        .expect_err("unapproved XHTML element");
        assert!(
            error
                .to_string()
                .contains("forbidden XHTML element <marquee>"),
            "{error}"
        );
    }

    #[test]
    fn admission_rejects_external_background_resource_attributes() {
        for attribute in ["background", "background-image"] {
            let svg =
                format!(r#"<svg><path {attribute}="url(https://tracker.test/a.png)"/></svg>"#);
            let error =
                validate_admission_with_limits(&svg, RenderResourcePolicy::trusted_native())
                    .expect_err("external background resource");

            assert!(error.to_string().contains("non-local CSS URL"), "{error}");
        }
    }

    #[test]
    fn admission_allows_animation_only_for_the_staticization_stage() {
        let svg = r#"<svg id="root"><style>@keyframes dash{to{opacity:0}}#root .edge{animation:dash 1s}</style><path class="edge"/></svg>"#;

        validate_admission_with_limits(svg, RenderResourcePolicy::trusted_native()).unwrap();
        let error = validate(svg).expect_err("final animated CSS");
        assert!(error.to_string().contains("forbidden CSS"), "{error}");
    }

    #[test]
    fn rejects_css_that_can_affect_the_rustdoc_host() {
        for css in [
            "body{display:none}",
            "html{filter:blur(20px)}",
            "#root, body{display:none}",
            "#root + body{display:none}",
            "@font-face{font-family:host;src:url(#font)}",
        ] {
            let svg = format!(r#"<svg id="root"><style>{css}</style></svg>"#);
            let error = validate(&svg).expect_err("host-affecting CSS");
            assert!(
                error.to_string().contains("CSS selector")
                    || error.to_string().contains("forbidden CSS"),
                "{css}: {error}"
            );
        }

        validate(
            r#"<svg id="root"><style>@media (prefers-color-scheme: dark){#root{fill:black}#root .node{stroke:white}}</style><g class="node"/></svg>"#,
        )
        .unwrap();
    }

    #[test]
    fn rejects_layout_escape_css_that_can_target_the_svg_root() {
        let validators: [fn(&str, RenderResourcePolicy) -> Result<()>; 2] =
            [validate_admission_with_limits, validate_static_with_limits];
        for svg in [
            r#"<svg id="root" style="position:fixed;inset:0;z-index:2147483647;pointer-events:auto"/>"#,
            r#"<svg id="root"><style>#root{position:fixed!important;inset:0;z-index:2147483647}</style></svg>"#,
            r#"<svg id="root"><style>@media (min-width:1px){#root{width:100vw;height:100vh}}</style></svg>"#,
            r#"<svg id="root"><style>#root.diagram{margin:-100px;transform:scale(100)}</style></svg>"#,
            r#"<svg id="root" width="100vw" height="100vh" style="pointer-events:auto"/>"#,
            r#"<svg id="root" width="999999999" height="999999999"/>"#,
            r#"<svg id="root" viewBox="0 0 999999999 1"/>"#,
            r#"<svg id="root" style="width:999999999px;height:999999999px"/>"#,
            r#"<svg id="root"><style>#root{width:calc(100% + 999999999px);height:999999999px;pointer-events:auto}</style></svg>"#,
            r#"<svg id="root" style="width:100%;padding:999999999px"/>"#,
            r#"<svg id="root"><style>#root{border:999999999px solid;aspect-ratio:1/999999999}</style></svg>"#,
        ] {
            for validator in validators {
                let error = validator(svg, RenderResourcePolicy::trusted_native())
                    .expect_err("root layout escape");
                assert!(
                    error.to_string().contains("SVG root layout"),
                    "{svg}: {error}"
                );
            }
        }
    }

    #[test]
    fn allows_only_literal_usecase_font_variables_on_the_svg_root() {
        let validators: [fn(&str, RenderResourcePolicy) -> Result<()>; 2] =
            [validate_admission_with_limits, validate_static_with_limits];
        for declarations in [
            "--mermaid-usecase-actor-font-size:14px;--mermaid-usecase-font-size:12px;\
             --mermaid-usecase-actor-font-family:'Open Sans', sans-serif;\
             --mermaid-usecase-font-family:Arial, sans-serif;\
             --mermaid-usecase-actor-font-weight:normal;--mermaid-usecase-font-weight:700",
            "--mermaid-usecase-actor-font-size:0px;--mermaid-usecase-font-weight:bold",
            "--mermaid-usecase-font-family:'inherit', sans-serif",
        ] {
            for svg in [
                format!(r#"<svg id="root" style="{declarations}"/>"#),
                format!(r#"<svg id="root"><style>#root{{{declarations}}}</style></svg>"#),
            ] {
                for validator in validators {
                    validator(&svg, RenderResourcePolicy::trusted_native()).unwrap();
                }
            }
        }
    }

    #[test]
    fn rejects_dynamic_or_unbounded_usecase_root_font_variables() {
        let validators: [fn(&str, RenderResourcePolicy) -> Result<()>; 2] =
            [validate_admission_with_limits, validate_static_with_limits];
        for declaration in [
            "--mermaid-usecase-other:fixed",
            "--mermaid-usecase-font-size:var(--host-size)",
            "--mermaid-usecase-font-size:calc(100vw)",
            "--mermaid-usecase-font-size:10vw",
            "--mermaid-usecase-font-size:-1px",
            "--mermaid-usecase-font-size:999999999px",
            "--mermaid-usecase-font-size:12px!important",
            "--mermaid-usecase-font-family:url(https://tracker.test/font)",
            "--mermaid-usecase-font-family:var(--host-font)",
            "--mermaid-usecase-font-family:inherit",
            "--mermaid-usecase-font-family:InHeRiT",
            "--mermaid-usecase-font-family:initial",
            "--mermaid-usecase-font-family:unset",
            "--mermaid-usecase-font-family:revert",
            "--mermaid-usecase-font-family:revert-layer",
            r"--mermaid-usecase-font-family:\69nherit",
            "--mermaid-usecase-font-family:Arial,",
            "--mermaid-usecase-font-family:",
            "--mermaid-usecase-font-weight:var(--host-weight)",
            "--mermaid-usecase-font-weight:1001",
            "--mermaid-usecase-font-weight:normal bold",
            "--mermaid-usecase-font-weight:normal;position:fixed",
        ] {
            for svg in [
                format!(r#"<svg id="root" style="{declaration}"/>"#),
                format!(r#"<svg id="root"><style>#root{{{declaration}}}</style></svg>"#),
            ] {
                for validator in validators {
                    assert!(
                        validator(&svg, RenderResourcePolicy::trusted_native()).is_err(),
                        "accepted {svg}"
                    );
                }
            }
        }
    }

    #[test]
    fn allows_layout_css_scoped_to_svg_descendants() {
        validate(
            r#"<svg id="root"><style>#root .node{position:absolute;z-index:2;transform:matrix(1,0,0,1,2,3)}</style><g class="node"/></svg>"#,
        )
        .unwrap();
        validate(r#"<svg id="root" width="100%" viewBox="0 0 681 400" style="max-width:681px"/>"#)
            .unwrap();
    }

    #[test]
    fn rejects_recursive_or_amplified_browser_use_expansion() {
        let validators: [fn(&str, RenderResourcePolicy) -> Result<()>; 2] =
            [validate_admission_with_limits, validate_static_with_limits];
        for svg in [
            r##"<svg><g id="loop"><use href="#loop"/></g></svg>"##,
            r##"<svg><g id="left"><use href="#right"/></g><g id="right"><use href="#left"/></g></svg>"##,
            r##"<svg xmlns:xlink="http://www.w3.org/1999/xlink"><path id="safe"/><g id="loop"><use xlink:href="#safe" href="#loop"/></g></svg>"##,
        ] {
            for validator in validators {
                let error = validator(svg, RenderResourcePolicy::trusted_native())
                    .expect_err("recursive browser use expansion");
                assert!(error.to_string().contains("cycle"), "{svg}: {error}");
            }
        }

        let mut amplified = String::from(r#"<svg><defs><g id="level-0"><path/></g>"#);
        for level in 1..=6 {
            amplified.push_str(&format!(
                r##"<g id="level-{level}"><use href="#level-{}"/><use href="#level-{}"/></g>"##,
                level - 1,
                level - 1
            ));
        }
        amplified.push_str(r##"</defs><use href="#level-6"/></svg>"##);
        let limits = RenderResourcePolicy::trusted_native()
            .with_limit(ResourceLimitId::MaxSvgElements, 64)
            .unwrap();
        for validator in validators {
            let error = validator(&amplified, limits).expect_err("amplified use expansion");
            assert!(error.to_string().contains("max_svg_elements"), "{error}");
        }

        for validator in validators {
            validator(
                r##"<svg><defs><path id="shared"/></defs><use href="#shared"/><use href="#shared"/></svg>"##,
                RenderResourcePolicy::trusted_native(),
            )
            .unwrap();
        }
    }
}
