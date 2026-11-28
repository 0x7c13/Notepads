use super::*;
use crate::family::RenderFamilyKind;
use std::ops::Range;

const VIEW_BOX_PLACEHOLDER: &str = "__MERMAN_ROOT_VIEW_BOX__";
const MAX_WIDTH_PLACEHOLDER: &str = "__MERMAN_ROOT_MAX_WIDTH__";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DiagramBounds {
    pub(super) min_x: f64,
    pub(super) min_y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
}

impl DiagramBounds {
    pub(super) fn from_view_box(min_x: f64, min_y: f64, width: f64, height: f64) -> Self {
        Self {
            min_x,
            min_y,
            width,
            height,
        }
    }

    pub(super) fn from_extents(
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        padding: f64,
    ) -> Self {
        let padding = if padding.is_finite() {
            padding.max(0.0)
        } else {
            padding
        };
        Self::from_view_box(
            min_x - padding,
            min_y - padding,
            max_x - min_x + 2.0 * padding,
            max_y - min_y + 2.0 * padding,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ViewBox {
    pub(super) min_x: f64,
    pub(super) min_y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
}

impl ViewBox {
    pub(super) fn new(min_x: f64, min_y: f64, width: f64, height: f64) -> Self {
        Self {
            min_x,
            min_y,
            width,
            height,
        }
    }

    fn from_bounds(bounds: DiagramBounds) -> Result<Self> {
        Ok(Self::new(
            checked_svg_coordinate(bounds.min_x, "viewBox min-x")?,
            checked_svg_coordinate(bounds.min_y, "viewBox min-y")?,
            checked_viewport_dimension(bounds.width, "viewBox width")?,
            checked_viewport_dimension(bounds.height, "viewBox height")?,
        ))
    }

    pub(super) fn attr(self) -> String {
        format!(
            "{} {} {} {}",
            fmt(self.min_x),
            fmt(self.min_y),
            fmt(self.width),
            fmt(self.height)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum RootMaxWidth {
    ViewBox,
    SvgNumber(f64),
    CssSixSignificant(f64),
    Precision { value: f64, significant_digits: u8 },
}

impl RootMaxWidth {
    fn format(self, view_box: Option<ViewBox>) -> Result<String> {
        let value = match self {
            Self::ViewBox => {
                view_box
                    .ok_or_else(|| Error::InvalidModel {
                        message: "root max-width requested without a viewBox".to_string(),
                    })?
                    .width
            }
            Self::SvgNumber(value)
            | Self::CssSixSignificant(value)
            | Self::Precision { value, .. } => value,
        };
        let value = checked_viewport_dimension(value, "root max-width")?;
        Ok(match self {
            Self::ViewBox | Self::SvgNumber(_) => fmt_string(value),
            Self::CssSixSignificant(_) => format_css_max_width(value),
            Self::Precision {
                significant_digits, ..
            } => format_precision_fixed(value, significant_digits),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum RootSizing {
    Responsive,
    Mermaid {
        use_max_width: bool,
    },
    #[cfg(feature = "layout-cytoscape")]
    MermaidOrIntrinsic {
        use_max_width: bool,
    },
    MermaidWithResponsiveHeight {
        use_max_width: bool,
        height: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RootViewportSpec {
    view_box: Option<DiagramBounds>,
    max_width: RootMaxWidth,
    sizing: RootSizing,
    fixed_size: Option<(f64, f64)>,
}

impl RootViewportSpec {
    pub(super) fn responsive(bounds: DiagramBounds) -> Self {
        Self {
            view_box: Some(bounds),
            max_width: RootMaxWidth::ViewBox,
            sizing: RootSizing::Responsive,
            fixed_size: None,
        }
    }

    pub(super) fn responsive_without_view_box(max_width: f64) -> Self {
        Self {
            view_box: None,
            max_width: RootMaxWidth::SvgNumber(max_width),
            sizing: RootSizing::Responsive,
            fixed_size: None,
        }
    }

    pub(super) fn mermaid(bounds: DiagramBounds, use_max_width: bool) -> Self {
        Self {
            sizing: RootSizing::Mermaid { use_max_width },
            ..Self::responsive(bounds)
        }
    }

    #[cfg(feature = "layout-cytoscape")]
    pub(super) fn mermaid_or_intrinsic(bounds: DiagramBounds, use_max_width: bool) -> Self {
        Self {
            sizing: RootSizing::MermaidOrIntrinsic { use_max_width },
            ..Self::responsive(bounds)
        }
    }

    pub(super) fn with_max_width(mut self, max_width: RootMaxWidth) -> Self {
        self.max_width = max_width;
        self
    }

    pub(super) fn with_mermaid_responsive_height(
        mut self,
        use_max_width: bool,
        height: f64,
    ) -> Self {
        self.sizing = RootSizing::MermaidWithResponsiveHeight {
            use_max_width,
            height,
        };
        self
    }

    pub(super) fn with_fixed_size(mut self, width: f64, height: f64) -> Self {
        self.fixed_size = Some((width, height));
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RootStylePlacement {
    Viewport,
    AfterRoleDescription,
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RootResponsiveHeightPlacement {
    BeforeExtraAttrs,
    AfterExtraAttrs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RootDomProfile {
    pub(super) style_viewbox_order: SvgRootStyleViewBoxOrder,
    pub(super) fixed_height_placement: SvgRootFixedHeightPlacement,
    pub(super) aria_attr_order: SvgRootAriaAttrOrder,
    pub(super) responsive_style_placement: RootStylePlacement,
    pub(super) fixed_style_placement: RootStylePlacement,
    pub(super) responsive_height_placement: RootResponsiveHeightPlacement,
    pub(super) trailing_newline: bool,
}

impl Default for RootDomProfile {
    fn default() -> Self {
        Self {
            style_viewbox_order: SvgRootStyleViewBoxOrder::StyleThenViewBox,
            fixed_height_placement: SvgRootFixedHeightPlacement::BeforeXmlns,
            aria_attr_order: SvgRootAriaAttrOrder::DescribedbyThenLabelledby,
            responsive_style_placement: RootStylePlacement::Viewport,
            fixed_style_placement: RootStylePlacement::Viewport,
            responsive_height_placement: RootResponsiveHeightPlacement::BeforeExtraAttrs,
            trailing_newline: true,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum RootDiagramId<'a> {
    Projected(SvgDiagramId<'a>),
    Raw(&'a str),
}

impl<'a> RootDiagramId<'a> {
    fn as_str(self) -> &'a str {
        match self {
            Self::Projected(diagram_id) => diagram_id.semantic_str(),
            Self::Raw(diagram_id) => diagram_id,
        }
    }

    fn write_escaped(self, out: &mut String) {
        match self {
            Self::Projected(diagram_id) => {
                let _ = write!(out, "{diagram_id}");
            }
            Self::Raw(diagram_id) => escape_attr_into(out, diagram_id),
        }
    }
}

impl std::fmt::Debug for RootDiagramId<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("RootDiagramId")
            .field(&self.as_str())
            .finish()
    }
}

impl std::fmt::Display for RootDiagramId<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl PartialEq for RootDiagramId<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for RootDiagramId<'_> {}

impl<'a> From<SvgDiagramId<'a>> for RootDiagramId<'a> {
    fn from(diagram_id: SvgDiagramId<'a>) -> Self {
        Self::Projected(diagram_id)
    }
}

impl<'a> From<&'a str> for RootDiagramId<'a> {
    fn from(diagram_id: &'a str) -> Self {
        Self::Raw(diagram_id)
    }
}

pub(super) struct RootChrome<'a> {
    pub(super) diagram_id: RootDiagramId<'a>,
    pub(super) class: Option<&'a str>,
    pub(super) extra_attrs: &'a [(&'a str, &'a str)],
    pub(super) aria_roledescription: &'a str,
    pub(super) aria_labelledby: Option<&'a str>,
    pub(super) aria_describedby: Option<&'a str>,
    pub(super) after_roledescription_attrs: &'a [(&'a str, &'a str)],
    pub(super) tail_attrs: &'a [(&'a str, &'a str)],
    pub(super) custom_properties: &'a [(&'a str, &'a str)],
    pub(super) dom: RootDomProfile,
}

impl<'a> RootChrome<'a> {
    pub(super) fn new(
        diagram_id: impl Into<RootDiagramId<'a>>,
        aria_roledescription: &'a str,
    ) -> Self {
        Self {
            diagram_id: diagram_id.into(),
            class: None,
            extra_attrs: &[],
            aria_roledescription,
            aria_labelledby: None,
            aria_describedby: None,
            after_roledescription_attrs: &[],
            tail_attrs: &[],
            custom_properties: &[],
            dom: RootDomProfile::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DeferredRootSpec {
    sizing: RootSizing,
}

impl DeferredRootSpec {
    pub(super) fn responsive() -> Self {
        Self {
            sizing: RootSizing::Responsive,
        }
    }

    #[cfg(feature = "layout-cytoscape")]
    pub(super) fn mermaid_or_intrinsic(use_max_width: bool) -> Self {
        Self {
            sizing: RootSizing::MermaidOrIntrinsic { use_max_width },
        }
    }
}

#[derive(Debug)]
enum RootDocumentState {
    Deferred {
        view_box_range: Range<usize>,
        max_width_range: Option<Range<usize>>,
        responsive: bool,
        root_open_snapshot: String,
        root_open_end: usize,
    },
    Ready {
        root_open: String,
    },
}

#[derive(Debug)]
pub(super) struct RootDocument {
    family: RenderFamilyKind,
    diagram_id: String,
    state: RootDocumentState,
}

/// A complete built-in SVG whose root was emitted and finalized by this module.
///
/// The field is intentionally private: sibling family modules may return this type, but only the
/// Root Viewport protocol can construct or unwrap it.
#[derive(Debug)]
pub(super) struct RootedSvg {
    svg: String,
    family: RenderFamilyKind,
    diagram_id: String,
}

#[derive(Debug, Clone)]
pub(super) struct RootViewportContext<'a> {
    family: RenderFamilyKind,
    diagram_id: RootDiagramId<'a>,
}

impl<'a> RootViewportContext<'a> {
    pub(super) fn new(family: RenderFamilyKind, diagram_id: impl Into<RootDiagramId<'a>>) -> Self {
        Self {
            family,
            diagram_id: diagram_id.into(),
        }
    }

    pub(super) fn begin_document(
        &self,
        out: &mut String,
        spec: DeferredRootSpec,
        chrome: RootChrome<'_>,
    ) -> Result<RootDocument> {
        self.require_empty_document(out)?;
        if chrome.diagram_id != self.diagram_id {
            return Err(Error::InvalidModel {
                message: "deferred root chrome belongs to a different render context".to_string(),
            });
        }
        let responsive = match spec.sizing {
            RootSizing::Responsive
            | RootSizing::Mermaid {
                use_max_width: true,
            } => true,
            #[cfg(feature = "layout-cytoscape")]
            RootSizing::MermaidOrIntrinsic {
                use_max_width: true,
            } => true,
            #[cfg(feature = "layout-cytoscape")]
            RootSizing::MermaidOrIntrinsic {
                use_max_width: false,
            } => false,
            RootSizing::Mermaid {
                use_max_width: false,
            }
            | RootSizing::MermaidWithResponsiveHeight { .. } => {
                return Err(Error::InvalidModel {
                    message: "unsupported deferred root sizing mode".to_string(),
                });
            }
        };
        let fixed_style = if responsive {
            None
        } else {
            with_custom_properties(None, chrome.custom_properties)?
        };
        let deferred_style_suffix =
            with_custom_properties(Some("px;".into()), chrome.custom_properties)?
                .unwrap_or_default();
        let style_placement = if responsive {
            chrome.dom.responsive_style_placement
        } else {
            chrome.dom.fixed_style_placement
        };
        let tracked_ranges = push_svg_root_open(
            out,
            SvgRootAttrs {
                diagram_id: chrome.diagram_id,
                class: chrome.class,
                width: if responsive {
                    SvgRootWidth::Percent100
                } else {
                    SvgRootWidth::None
                },
                height_attr: None,
                style_attr: if responsive {
                    Some(SvgRootAttributeValue::tracked(
                        "max-width: ",
                        MAX_WIDTH_PLACEHOLDER,
                        &deferred_style_suffix,
                    ))
                } else {
                    fixed_style.as_deref().map(SvgRootAttributeValue::plain)
                },
                viewbox_attr: Some(SvgRootAttributeValue::tracked("", VIEW_BOX_PLACEHOLDER, "")),
                style_viewbox_order: chrome.dom.style_viewbox_order,
                style_placement,
                responsive_height_placement: chrome.dom.responsive_height_placement,
                extra_attrs: chrome.extra_attrs,
                aria_roledescription: chrome.aria_roledescription,
                aria_labelledby: chrome.aria_labelledby,
                aria_describedby: chrome.aria_describedby,
                after_roledescription_attrs: chrome.after_roledescription_attrs,
                tail_attrs: chrome.tail_attrs,
                fixed_height_placement: chrome.dom.fixed_height_placement,
                trailing_newline: chrome.dom.trailing_newline,
                aria_attr_order: chrome.dom.aria_attr_order,
            },
        );
        let view_box_range = tracked_ranges.view_box.ok_or_else(|| Error::InvalidModel {
            message: "deferred SVG root is missing its viewBox placeholder".to_string(),
        })?;
        let max_width_range = if responsive {
            Some(
                tracked_ranges
                    .max_width
                    .ok_or_else(|| Error::InvalidModel {
                        message: "deferred SVG root is missing its max-width placeholder"
                            .to_string(),
                    })?,
            )
        } else {
            None
        };

        Ok(RootDocument {
            family: self.family,
            diagram_id: self.diagram_id.as_str().to_string(),
            state: RootDocumentState::Deferred {
                view_box_range,
                max_width_range,
                responsive,
                root_open_snapshot: out.clone(),
                root_open_end: out.len(),
            },
        })
    }

    pub(super) fn finish_document(
        &self,
        out: &mut String,
        document: RootDocument,
        spec: RootViewportSpec,
    ) -> Result<RootDocument> {
        if document.family != self.family || document.diagram_id != self.diagram_id.as_str() {
            return Err(Error::InvalidModel {
                message: "deferred root document belongs to a different render context".to_string(),
            });
        }
        let RootDocumentState::Deferred {
            view_box_range,
            max_width_range,
            responsive,
            root_open_snapshot,
            mut root_open_end,
        } = document.state
        else {
            return Err(Error::InvalidModel {
                message: "root document viewport was already finalized".to_string(),
            });
        };
        let plan = self.plan(spec)?;
        if plan.responsive != responsive || plan.height.is_some() {
            return Err(Error::InvalidModel {
                message: "deferred root sizing changed between open and finalize".to_string(),
            });
        }
        let view_box = plan.view_box.ok_or_else(|| Error::InvalidModel {
            message: "deferred root viewport did not resolve a viewBox".to_string(),
        })?;
        if out.get(..root_open_end) != Some(root_open_snapshot.as_str())
            || out.get(view_box_range.clone()) != Some(VIEW_BOX_PLACEHOLDER)
            || max_width_range
                .as_ref()
                .is_some_and(|range| out.get(range.clone()) != Some(MAX_WIDTH_PLACEHOLDER))
        {
            return Err(Error::InvalidModel {
                message: "deferred root document was mutated before viewport finalize".to_string(),
            });
        }
        let mut replacements = vec![(view_box_range, view_box.attr())];
        match (max_width_range, plan.responsive) {
            (Some(range), true) => replacements.push((range, plan.max_width.clone())),
            (None, false) => {}
            _ => {
                return Err(Error::InvalidModel {
                    message: "deferred root max-width state changed during finalize".to_string(),
                });
            }
        }
        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        for (range, replacement) in replacements {
            root_open_end = root_open_end
                .checked_add(replacement.len())
                .and_then(|end| end.checked_sub(range.len()))
                .ok_or_else(|| Error::InvalidModel {
                    message: "deferred root attribute replacement overflowed".to_string(),
                })?;
            out.replace_range(range, replacement.as_str());
        }
        let root_open = out
            .get(..root_open_end)
            .ok_or_else(|| Error::InvalidModel {
                message: "deferred root document was truncated before viewport finalize"
                    .to_string(),
            })?
            .to_string();
        Ok(RootDocument {
            family: self.family,
            diagram_id: self.diagram_id.as_str().to_string(),
            state: RootDocumentState::Ready { root_open },
        })
    }

    pub(super) fn write_open(
        &self,
        out: &mut String,
        spec: RootViewportSpec,
        chrome: RootChrome<'_>,
    ) -> Result<RootDocument> {
        if chrome.diagram_id != self.diagram_id {
            return Err(Error::InvalidModel {
                message: format!(
                    "root viewport diagram id '{}' does not match root chrome id '{}'",
                    self.diagram_id, chrome.diagram_id
                ),
            });
        }
        let plan = self.plan(spec)?;
        self.write_plan(out, &plan, chrome)
    }

    pub(super) fn write_plan(
        &self,
        out: &mut String,
        plan: &RootViewportPlan,
        chrome: RootChrome<'_>,
    ) -> Result<RootDocument> {
        self.require_empty_document(out)?;
        if plan.family != self.family
            || plan.diagram_id != self.diagram_id.as_str()
            || chrome.diagram_id != self.diagram_id
        {
            return Err(Error::InvalidModel {
                message: "root viewport plan belongs to a different render context".to_string(),
            });
        }
        let viewbox_attr = plan.view_box.map(ViewBox::attr);
        let style = with_custom_properties(plan.style.clone(), chrome.custom_properties)?;
        let width = match plan.width.as_deref() {
            None => SvgRootWidth::None,
            Some("100%") => SvgRootWidth::Percent100,
            Some(width) => SvgRootWidth::Fixed(width),
        };
        let style_placement = if plan.responsive {
            chrome.dom.responsive_style_placement
        } else {
            chrome.dom.fixed_style_placement
        };

        push_svg_root_open(
            out,
            SvgRootAttrs {
                diagram_id: chrome.diagram_id,
                class: chrome.class,
                width,
                height_attr: plan.height.as_deref(),
                style_attr: style.as_deref().map(SvgRootAttributeValue::plain),
                viewbox_attr: viewbox_attr.as_deref().map(SvgRootAttributeValue::plain),
                style_viewbox_order: chrome.dom.style_viewbox_order,
                style_placement,
                responsive_height_placement: chrome.dom.responsive_height_placement,
                extra_attrs: chrome.extra_attrs,
                aria_roledescription: chrome.aria_roledescription,
                aria_labelledby: chrome.aria_labelledby,
                aria_describedby: chrome.aria_describedby,
                after_roledescription_attrs: chrome.after_roledescription_attrs,
                tail_attrs: chrome.tail_attrs,
                fixed_height_placement: chrome.dom.fixed_height_placement,
                trailing_newline: chrome.dom.trailing_newline,
                aria_attr_order: chrome.dom.aria_attr_order,
            },
        );
        Ok(RootDocument {
            family: self.family,
            diagram_id: self.diagram_id.as_str().to_string(),
            state: RootDocumentState::Ready {
                root_open: out.clone(),
            },
        })
    }

    fn complete_document(&self, out: String, document: RootDocument) -> Result<RootedSvg> {
        if document.family != self.family || document.diagram_id != self.diagram_id.as_str() {
            return Err(Error::InvalidModel {
                message: "root document belongs to a different render context".to_string(),
            });
        }
        let RootDocumentState::Ready { root_open } = document.state else {
            return Err(Error::InvalidModel {
                message: "root document viewport was not finalized".to_string(),
            });
        };
        if !out.starts_with(&root_open) {
            return Err(Error::InvalidModel {
                message: "family mutated operation-owned SVG root attributes".to_string(),
            });
        }
        if !out.trim_end().ends_with("</svg>") {
            return Err(Error::InvalidModel {
                message: "family returned an incomplete SVG root document".to_string(),
            });
        }
        Ok(RootedSvg {
            svg: out,
            family: self.family,
            diagram_id: self.diagram_id.as_str().to_string(),
        })
    }

    fn require_empty_document(&self, out: &str) -> Result<()> {
        if out.is_empty() {
            return Ok(());
        }
        Err(Error::InvalidModel {
            message: "root SVG emission must begin with an empty document buffer".to_string(),
        })
    }

    pub(super) fn plan(&self, spec: RootViewportSpec) -> Result<RootViewportPlan> {
        let view_box = spec.view_box.map(ViewBox::from_bounds).transpose()?;
        let max_width = spec.max_width.format(view_box)?;

        let fixed_dimensions = || {
            if let Some((width, height)) = spec.fixed_size {
                return Ok::<_, Error>((
                    fmt_string(checked_viewport_dimension(width, "fixed root width")?),
                    fmt_string(checked_viewport_dimension(height, "fixed root height")?),
                ));
            }
            let view_box = view_box.ok_or_else(|| Error::InvalidModel {
                message: format!(
                    "fixed root sizing for {} diagram '{}' requires a viewBox",
                    self.family, self.diagram_id
                ),
            })?;
            Ok::<_, Error>((fmt_string(view_box.width), fmt_string(view_box.height)))
        };
        let (responsive, width, height) = match spec.sizing {
            RootSizing::Responsive => (true, Some("100%".to_string()), None),
            RootSizing::Mermaid {
                use_max_width: true,
            } => (true, Some("100%".to_string()), None),
            #[cfg(feature = "layout-cytoscape")]
            RootSizing::MermaidOrIntrinsic {
                use_max_width: true,
            } => (true, Some("100%".to_string()), None),
            RootSizing::MermaidWithResponsiveHeight {
                use_max_width: true,
                height,
            } => (
                true,
                Some("100%".to_string()),
                Some(fmt_string(checked_viewport_dimension(
                    height,
                    "responsive root height",
                )?)),
            ),
            RootSizing::Mermaid {
                use_max_width: false,
            } => {
                let (width, height) = fixed_dimensions()?;
                (false, Some(width), Some(height))
            }
            RootSizing::MermaidWithResponsiveHeight {
                use_max_width: false,
                ..
            } => {
                let (width, height) = fixed_dimensions()?;
                (false, Some(width), Some(height))
            }
            #[cfg(feature = "layout-cytoscape")]
            RootSizing::MermaidOrIntrinsic {
                use_max_width: false,
            } => (false, None, None),
        };
        let style = responsive.then(|| format!("max-width: {max_width}px;"));

        Ok(RootViewportPlan {
            family: self.family,
            diagram_id: self.diagram_id.as_str().to_string(),
            view_box,
            width,
            height,
            style,
            responsive,
            max_width,
        })
    }
}

impl RootedSvg {
    pub(super) fn into_string_for(self, expected_family: RenderFamilyKind) -> Result<String> {
        if self.family != expected_family {
            return Err(Error::InvalidModel {
                message: format!(
                    "{} root document '{}' was returned for {expected_family}",
                    self.family, self.diagram_id
                ),
            });
        }
        Ok(self.svg)
    }
}

impl std::ops::Deref for RootedSvg {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.svg
    }
}

impl std::fmt::Display for RootedSvg {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.svg)
    }
}

impl RootDocument {
    pub(super) fn complete(self, out: String) -> Result<RootedSvg> {
        let family = self.family;
        let diagram_id = self.diagram_id.clone();
        RootViewportContext::new(family, diagram_id.as_str()).complete_document(out, self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RootViewportPlan {
    family: RenderFamilyKind,
    diagram_id: String,
    view_box: Option<ViewBox>,
    width: Option<String>,
    height: Option<String>,
    style: Option<String>,
    responsive: bool,
    max_width: String,
}

impl RootViewportPlan {
    pub(super) fn view_box(&self) -> Option<ViewBox> {
        self.view_box
    }
}

fn with_custom_properties(
    style: Option<String>,
    properties: &[(&str, &str)],
) -> Result<Option<String>> {
    let mut style = style.unwrap_or_default();
    for (key, value) in properties {
        let declaration = format!("{key}:{value}");
        if !key.starts_with("--")
            || crate::mermaid_style::parse_safe_style_decl(&declaration).is_none()
        {
            return Err(Error::InvalidModel {
                message: format!("invalid SVG root custom property {key}"),
            });
        }
        if !style.is_empty() {
            style.push(' ');
        }
        let _ = write!(style, "{key}: {value};");
    }
    Ok((!style.is_empty()).then_some(style))
}

fn format_css_max_width(value: f64) -> String {
    if !value.is_finite() || value.abs() < 0.0005 {
        return "0".to_string();
    }
    let exponent = value.abs().max(0.0005).log10().floor() as i32;
    let decimals = (5 - exponent).clamp(0, 6) as usize;
    let scale = 10f64.powi(decimals as i32);
    let mut rounded = round_ties_to_even(value * scale) / scale;
    if rounded.abs() < 0.0005 {
        rounded = 0.0;
    }
    let mut formatted = format!("{rounded:.decimals$}");
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    formatted
}

fn round_ties_to_even(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let sign = if value.is_sign_negative() { -1.0 } else { 1.0 };
    let absolute = value.abs();
    let floor = absolute.floor();
    let fraction = absolute - floor;
    let rounded = if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else if (floor as i64) % 2 == 0 {
        floor
    } else {
        floor + 1.0
    };
    sign * rounded
}

fn format_precision_fixed(value: f64, significant_digits: u8) -> String {
    let precision = i32::from(significant_digits.max(1));
    if !value.is_finite() {
        return "0".to_string();
    }
    if value == 0.0 {
        return format!("{:.*}", (precision - 1) as usize, 0.0);
    }
    let exponent = value.abs().log10().floor() as i32;
    let decimals = (precision - exponent - 1).max(0) as usize;
    format!("{value:.decimals$}")
}

fn checked_svg_coordinate(value: f64, field: &str) -> Result<f64> {
    if value.is_finite() {
        return Ok(value);
    }
    Err(Error::InvalidModel {
        message: format!("root SVG {field} must be finite"),
    })
}

fn checked_viewport_dimension(value: f64, field: &str) -> Result<f64> {
    Ok(checked_svg_coordinate(value, field)?.max(1.0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SvgRootWidth<'a> {
    None,
    Percent100,
    Fixed(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SvgRootStyleViewBoxOrder {
    StyleThenViewBox,
    ViewBoxThenStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SvgRootAriaAttrOrder {
    DescribedbyThenLabelledby,
    LabelledbyThenDescribedby,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SvgRootFixedHeightPlacement {
    BeforeXmlns,
    AfterXmlns,
    AfterClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SvgRootAttributeValue<'a> {
    Plain(&'a str),
    Tracked {
        prefix: &'a str,
        value: &'a str,
        suffix: &'a str,
    },
}

impl<'a> SvgRootAttributeValue<'a> {
    const fn plain(value: &'a str) -> Self {
        Self::Plain(value)
    }

    const fn tracked(prefix: &'a str, value: &'a str, suffix: &'a str) -> Self {
        Self::Tracked {
            prefix,
            value,
            suffix,
        }
    }

    fn write_escaped(self, out: &mut String) -> Option<Range<usize>> {
        match self {
            Self::Plain(value) => {
                escape_attr_into(out, value);
                None
            }
            Self::Tracked {
                prefix,
                value,
                suffix,
            } => {
                escape_attr_into(out, prefix);
                let start = out.len();
                escape_attr_into(out, value);
                let end = out.len();
                escape_attr_into(out, suffix);
                Some(start..end)
            }
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SvgRootTrackedRanges {
    view_box: Option<Range<usize>>,
    max_width: Option<Range<usize>>,
}

struct SvgRootAttrs<'a> {
    diagram_id: RootDiagramId<'a>,
    class: Option<&'a str>,
    width: SvgRootWidth<'a>,
    height_attr: Option<&'a str>,
    style_attr: Option<SvgRootAttributeValue<'a>>,
    viewbox_attr: Option<SvgRootAttributeValue<'a>>,
    style_viewbox_order: SvgRootStyleViewBoxOrder,
    style_placement: RootStylePlacement,
    responsive_height_placement: RootResponsiveHeightPlacement,
    extra_attrs: &'a [(&'a str, &'a str)],
    aria_roledescription: &'a str,
    aria_labelledby: Option<&'a str>,
    aria_describedby: Option<&'a str>,
    after_roledescription_attrs: &'a [(&'a str, &'a str)],
    tail_attrs: &'a [(&'a str, &'a str)],
    fixed_height_placement: SvgRootFixedHeightPlacement,
    trailing_newline: bool,
    aria_attr_order: SvgRootAriaAttrOrder,
}

fn push_svg_root_attribute(
    out: &mut String,
    name: &str,
    value: SvgRootAttributeValue<'_>,
) -> Option<Range<usize>> {
    out.push(' ');
    out.push_str(name);
    out.push_str(r#"=""#);
    let tracked_range = value.write_escaped(out);
    out.push('"');
    tracked_range
}

fn push_svg_root_open(out: &mut String, attrs: SvgRootAttrs<'_>) -> SvgRootTrackedRanges {
    let SvgRootAttrs {
        diagram_id,
        class,
        width,
        height_attr,
        style_attr,
        viewbox_attr,
        style_viewbox_order,
        style_placement,
        responsive_height_placement,
        extra_attrs,
        aria_roledescription,
        aria_labelledby,
        aria_describedby,
        after_roledescription_attrs,
        tail_attrs,
        fixed_height_placement,
        trailing_newline,
        aria_attr_order,
    } = attrs;

    // Keep attribute order stable (helps strict-mode diffs) and match existing renderers:
    // id, width/height (with configurable fixed-height placement), xmlns, class?,
    // style?/viewBox (configurable), extra-attrs..., role, aria-roledescription, aria-*, tail-attrs..., >\n?
    let mut deferred_height_after_class: Option<&str> = None;
    let mut tracked_ranges = SvgRootTrackedRanges::default();
    out.push_str(r#"<svg id=""#);
    diagram_id.write_escaped(out);
    let responsive_height_attr = matches!(width, SvgRootWidth::Percent100)
        .then_some(height_attr)
        .flatten();
    match width {
        SvgRootWidth::None => {
            out.push_str(r#"" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#);
        }
        SvgRootWidth::Percent100 => {
            out.push_str(
                r#"" width="100%" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#,
            );
        }
        SvgRootWidth::Fixed(w) => {
            out.push_str(r#"" width=""#);
            escape_attr_into(out, w);
            out.push('"');
            match fixed_height_placement {
                SvgRootFixedHeightPlacement::BeforeXmlns => {
                    out.push_str(r#" height=""#);
                    escape_attr_into(out, height_attr.unwrap_or("0"));
                    out.push('"');
                    out.push_str(
                        r#" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#,
                    );
                }
                SvgRootFixedHeightPlacement::AfterXmlns => {
                    out.push_str(
                        r#" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#,
                    );
                    out.push_str(r#" height=""#);
                    escape_attr_into(out, height_attr.unwrap_or("0"));
                    out.push('"');
                }
                SvgRootFixedHeightPlacement::AfterClass => {
                    out.push_str(
                        r#" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#,
                    );
                    deferred_height_after_class = Some(height_attr.unwrap_or("0"));
                }
            }
        }
    }

    if let Some(class) = class {
        out.push_str(r#" class=""#);
        escape_attr_into(out, class);
        out.push('"');
    }
    if let Some(h) = deferred_height_after_class.take() {
        out.push_str(r#" height=""#);
        escape_attr_into(out, h);
        out.push('"');
    }
    match style_viewbox_order {
        SvgRootStyleViewBoxOrder::StyleThenViewBox => {
            if style_placement == RootStylePlacement::Viewport
                && let Some(style_attr) = style_attr
            {
                tracked_ranges.max_width = push_svg_root_attribute(out, "style", style_attr);
            }
            if let Some(viewbox_attr) = viewbox_attr {
                tracked_ranges.view_box = push_svg_root_attribute(out, "viewBox", viewbox_attr);
            }
        }
        SvgRootStyleViewBoxOrder::ViewBoxThenStyle => {
            if let Some(viewbox_attr) = viewbox_attr {
                tracked_ranges.view_box = push_svg_root_attribute(out, "viewBox", viewbox_attr);
            }
            if style_placement == RootStylePlacement::Viewport
                && let Some(style_attr) = style_attr
            {
                tracked_ranges.max_width = push_svg_root_attribute(out, "style", style_attr);
            }
        }
    }
    if responsive_height_placement == RootResponsiveHeightPlacement::BeforeExtraAttrs
        && let Some(height_attr) = responsive_height_attr
    {
        out.push_str(r#" height=""#);
        escape_attr_into(out, height_attr);
        out.push('"');
    }
    for (k, v) in extra_attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str(r#"=""#);
        escape_attr_into(out, v);
        out.push('"');
    }
    if responsive_height_placement == RootResponsiveHeightPlacement::AfterExtraAttrs
        && let Some(height_attr) = responsive_height_attr
    {
        out.push_str(r#" height=""#);
        escape_attr_into(out, height_attr);
        out.push('"');
    }

    out.push_str(r#" role="graphics-document document" aria-roledescription=""#);
    escape_attr_into(out, aria_roledescription);
    out.push('"');
    if style_placement == RootStylePlacement::AfterRoleDescription
        && let Some(style_attr) = style_attr
    {
        tracked_ranges.max_width = push_svg_root_attribute(out, "style", style_attr);
    }
    for (k, v) in after_roledescription_attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str(r#"=""#);
        escape_attr_into(out, v);
        out.push('"');
    }
    match aria_attr_order {
        SvgRootAriaAttrOrder::DescribedbyThenLabelledby => {
            if let Some(v) = aria_describedby {
                out.push_str(r#" aria-describedby=""#);
                escape_attr_into(out, v);
                out.push('"');
            }
            if let Some(v) = aria_labelledby {
                out.push_str(r#" aria-labelledby=""#);
                escape_attr_into(out, v);
                out.push('"');
            }
        }
        SvgRootAriaAttrOrder::LabelledbyThenDescribedby => {
            if let Some(v) = aria_labelledby {
                out.push_str(r#" aria-labelledby=""#);
                escape_attr_into(out, v);
                out.push('"');
            }
            if let Some(v) = aria_describedby {
                out.push_str(r#" aria-describedby=""#);
                escape_attr_into(out, v);
                out.push('"');
            }
        }
    }

    if style_placement == RootStylePlacement::Tail
        && let Some(style_attr) = style_attr
    {
        tracked_ranges.max_width = push_svg_root_attribute(out, "style", style_attr);
    }
    for (k, v) in tail_attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str(r#"=""#);
        escape_attr_into(out, v);
        out.push('"');
    }

    out.push('>');
    if trailing_newline {
        out.push('\n');
    }
    tracked_ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn computed_context(family: RenderFamilyKind, diagram_id: &str) -> RootViewportContext<'_> {
        RootViewportContext::new(family, diagram_id)
    }

    #[test]
    fn root_plan_rejects_non_finite_viewbox_geometry() {
        let err = computed_context(RenderFamilyKind::Venn, "root-id")
            .plan(RootViewportSpec::mermaid(
                DiagramBounds::from_view_box(-2.0, f64::NAN, -42.5, f64::INFINITY),
                false,
            ))
            .unwrap_err();

        assert!(
            err.to_string().contains("viewBox min-y must be finite"),
            "{err}"
        );
    }

    #[test]
    fn responsive_root_emits_declared_dom_order() {
        let context = computed_context(RenderFamilyKind::Journey, "root-id");
        let extra_attrs = [("preserveAspectRatio", "xMinYMin meet")];
        let mut chrome = RootChrome::new("root-id", "journey");
        chrome.extra_attrs = &extra_attrs;
        chrome.dom = RootDomProfile {
            style_viewbox_order: SvgRootStyleViewBoxOrder::ViewBoxThenStyle,
            responsive_height_placement: RootResponsiveHeightPlacement::AfterExtraAttrs,
            trailing_newline: false,
            ..RootDomProfile::default()
        };
        let mut out = String::new();
        context
            .write_open(
                &mut out,
                RootViewportSpec::responsive(DiagramBounds::from_view_box(-2.0, 0.0, 42.0, 24.0))
                    .with_mermaid_responsive_height(true, 30.0),
                chrome,
            )
            .unwrap();

        assert!(out.starts_with(r#"<svg id="root-id" width="100%""#));
        assert!(out.contains(
            r#"viewBox="-2 0 42 24" style="max-width: 42px;" preserveAspectRatio="xMinYMin meet" height="30""#
        ));
        assert!(out.contains(r#"style="max-width: 42px;""#));
        assert!(!out.contains("background-color"));
    }

    #[test]
    fn responsive_root_can_omit_viewbox() {
        let context = computed_context(RenderFamilyKind::Info, "info");
        let mut chrome = RootChrome::new("info", "info");
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        context
            .write_open(
                &mut out,
                RootViewportSpec::responsive_without_view_box(400.0),
                chrome,
            )
            .unwrap();

        assert!(out.contains(r#"width="100%""#));
        assert!(out.contains(r#"style="max-width: 400px;""#));
        assert!(!out.contains("viewBox="));
    }

    #[test]
    fn root_chrome_escapes_every_dynamic_attribute_once() {
        let diagram_id = r#"root" onload="alert(1)&"#;
        let context = computed_context(RenderFamilyKind::Info, diagram_id);
        let extra_attrs = [("data-note", r#""<&"#)];
        let mut chrome = RootChrome::new(diagram_id, r#"info" aria-hidden="true"#);
        chrome.class = Some(r#"diagram" injected="yes"#);
        chrome.extra_attrs = &extra_attrs;
        chrome.aria_labelledby = Some(r#"title" autofocus="true"#);
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        context
            .write_open(
                &mut out,
                RootViewportSpec::responsive_without_view_box(400.0),
                chrome,
            )
            .unwrap();

        assert!(out.contains(r#"id="root&quot; onload=&quot;alert(1)&amp;""#));
        assert!(out.contains(r#"class="diagram&quot; injected=&quot;yes""#));
        assert!(out.contains(r#"data-note="&quot;&lt;&amp;""#));
        assert!(out.contains(r#"aria-roledescription="info&quot; aria-hidden=&quot;true""#));
        assert!(out.contains(r#"aria-labelledby="title&quot; autofocus=&quot;true""#));
        assert!(!out.contains(r#" onload="alert(1)""#));
        assert!(!out.contains(r#" injected="yes""#));
    }

    #[test]
    fn completed_root_document_carries_family_provenance() {
        let context = computed_context(RenderFamilyKind::Info, "root-id");
        let mut chrome = RootChrome::new("root-id", "info");
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        let document = context
            .write_open(
                &mut out,
                RootViewportSpec::responsive_without_view_box(400.0),
                chrome,
            )
            .unwrap();
        out.push_str("</svg>");

        let svg = document
            .complete(out)
            .unwrap()
            .into_string_for(RenderFamilyKind::Info)
            .unwrap();

        assert!(svg.starts_with(r#"<svg id="root-id""#));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn completed_root_document_rejects_root_attribute_mutation() {
        let context = computed_context(RenderFamilyKind::Info, "root-id");
        let mut chrome = RootChrome::new("root-id", "info");
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        let document = context
            .write_open(
                &mut out,
                RootViewportSpec::responsive_without_view_box(400.0),
                chrome,
            )
            .unwrap();
        out = out.replacen(r#"id="root-id""#, r#"id="forged""#, 1);
        out.push_str("</svg>");

        let error = document.complete(out).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("mutated operation-owned SVG root attributes")
        );
    }

    #[test]
    fn completed_root_document_rejects_incomplete_and_wrong_family_outputs() {
        let context = computed_context(RenderFamilyKind::Info, "root-id");
        let mut chrome = RootChrome::new("root-id", "info");
        chrome.dom.trailing_newline = false;
        let mut incomplete = String::new();
        let incomplete_document = context
            .write_open(
                &mut incomplete,
                RootViewportSpec::responsive_without_view_box(400.0),
                chrome,
            )
            .unwrap();
        let error = incomplete_document.complete(incomplete).unwrap_err();
        assert!(error.to_string().contains("incomplete SVG root document"));

        let mut complete = String::new();
        let complete_document = context
            .write_open(
                &mut complete,
                RootViewportSpec::responsive_without_view_box(400.0),
                RootChrome::new("root-id", "info"),
            )
            .unwrap();
        complete.push_str("</svg>");
        let error = complete_document
            .complete(complete)
            .unwrap()
            .into_string_for(RenderFamilyKind::Venn)
            .unwrap_err();
        assert!(error.to_string().contains("was returned for venn"));
    }

    #[test]
    fn deferred_document_finalizes_computed_root_without_leaking_markers() {
        let diagram_id = "stress_state_accdescr_block_and_markdown_labels_049";
        let context = RootViewportContext::new(RenderFamilyKind::State, diagram_id);
        let mut chrome = RootChrome::new(diagram_id, "stateDiagram");
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        let document = context
            .begin_document(&mut out, DeferredRootSpec::responsive(), chrome)
            .unwrap();
        out.push_str("<g/></svg>");
        let document = context
            .finish_document(
                &mut out,
                document,
                RootViewportSpec::responsive(DiagramBounds::from_view_box(0.0, 0.0, 10.0, 10.0))
                    .with_max_width(RootMaxWidth::CssSixSignificant(10.0)),
            )
            .unwrap();

        let rooted = document.complete(out.clone()).unwrap();
        assert_eq!(rooted.family, RenderFamilyKind::State);
        assert!(out.contains(r#"viewBox="0 0 10 10""#));
        assert!(out.contains("max-width: 10px"));
        assert!(!out.contains("__MERMAN_ROOT_"));
    }

    #[test]
    fn deferred_document_tracks_root_attributes_when_a_valid_id_matches_markers() {
        let diagram_id = format!("valid_{VIEW_BOX_PLACEHOLDER}_{MAX_WIDTH_PLACEHOLDER}_diagram");
        let context = RootViewportContext::new(RenderFamilyKind::State, diagram_id.as_str());
        let mut chrome = RootChrome::new(diagram_id.as_str(), "stateDiagram");
        let properties = [
            ("--font-family", "\"Arial\", sans-serif"),
            ("--marker-text", MAX_WIDTH_PLACEHOLDER),
        ];
        chrome.custom_properties = &properties;
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        let document = context
            .begin_document(&mut out, DeferredRootSpec::responsive(), chrome)
            .unwrap();
        out.push_str("</svg>");
        let document = context
            .finish_document(
                &mut out,
                document,
                RootViewportSpec::responsive(DiagramBounds::from_view_box(1.0, 2.0, 30.0, 40.0))
                    .with_max_width(RootMaxWidth::CssSixSignificant(30.0)),
            )
            .unwrap();

        let rooted = document.complete(out).unwrap();
        let document = roxmltree::Document::parse(&rooted).unwrap();
        let root = document.root_element();
        assert_eq!(root.attribute("id"), Some(diagram_id.as_str()));
        assert_eq!(root.attribute("viewBox"), Some("1 2 30 40"));
        assert_eq!(
            root.attribute("style"),
            Some(
                "max-width: 30px; --font-family: \"Arial\", sans-serif; --marker-text: __MERMAN_ROOT_MAX_WIDTH__;"
            )
        );
    }

    #[test]
    fn deferred_document_rejects_prefix_mutation_instead_of_patching_wrong_range() {
        let context = computed_context(RenderFamilyKind::State, "state");
        let mut chrome = RootChrome::new("state", "stateDiagram");
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        let document = context
            .begin_document(&mut out, DeferredRootSpec::responsive(), chrome)
            .unwrap();
        out.insert_str(0, "prefix");

        let error = context
            .finish_document(
                &mut out,
                document,
                RootViewportSpec::responsive(DiagramBounds::from_view_box(0.0, 0.0, 10.0, 10.0)),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("mutated before viewport finalize")
        );
    }

    #[test]
    fn deferred_document_rejects_untracked_root_attribute_insertion() {
        let context = computed_context(RenderFamilyKind::State, "state");
        let mut chrome = RootChrome::new("state", "stateDiagram");
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        let document = context
            .begin_document(&mut out, DeferredRootSpec::responsive(), chrome)
            .unwrap();
        let root_open_end = out.rfind('>').expect("root open delimiter");
        out.insert_str(root_open_end, r#" data-forged="true""#);

        let error = context
            .finish_document(
                &mut out,
                document,
                RootViewportSpec::responsive(DiagramBounds::from_view_box(0.0, 0.0, 10.0, 10.0)),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("mutated before viewport finalize")
        );
    }

    #[test]
    fn deferred_document_rejects_same_length_root_tail_mutation() {
        let context = computed_context(RenderFamilyKind::State, "state");
        let mut chrome = RootChrome::new("state", "stateDiagram");
        chrome.dom.trailing_newline = false;
        let mut out = String::new();
        let document = context
            .begin_document(&mut out, DeferredRootSpec::responsive(), chrome)
            .unwrap();
        let start = out
            .find("stateDiagram")
            .expect("root aria role description");
        out.replace_range(start..start + "stateDiagram".len(), "stateDiagrax");

        let error = context
            .finish_document(
                &mut out,
                document,
                RootViewportSpec::responsive(DiagramBounds::from_view_box(0.0, 0.0, 10.0, 10.0)),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("mutated before viewport finalize")
        );
    }

    #[test]
    fn css_max_width_uses_mermaid_six_significant_digit_format() {
        assert_eq!(format_css_max_width(1184.88), "1184.88");
        assert_eq!(format_css_max_width(2019.2), "2019.2");
        assert_eq!(format_css_max_width(658.6762084960938), "658.676");
    }

    #[test]
    fn extents_apply_non_negative_padding_and_support_large_negative_origins() {
        let bounds = DiagramBounds::from_extents(-1_000_000.0, -20.0, 50.0, 80.0, 8.0);
        assert_eq!(
            bounds,
            DiagramBounds::from_view_box(-1_000_008.0, -28.0, 1_000_066.0, 116.0)
        );
    }

    #[test]
    fn root_plan_preserves_f64_get_bbox_extents_and_padding() {
        let (min_x, min_y) = (1.123_456_789, 2.123_456_789);
        let (max_x, max_y) = (111.987_654_321, 222.987_654_321);
        let padding = 40.0;
        let bounds = DiagramBounds::from_extents(min_x, min_y, max_x, max_y, padding);
        let plan = computed_context(RenderFamilyKind::Architecture, "architecture")
            .plan(RootViewportSpec::responsive(bounds))
            .unwrap();

        assert_eq!(
            plan.view_box(),
            Some(ViewBox::new(
                min_x - padding,
                min_y - padding,
                max_x - min_x + 2.0 * padding,
                max_y - min_y + 2.0 * padding,
            ))
        );
    }
}
