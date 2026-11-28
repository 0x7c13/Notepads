//! Infallible text measurement trait shared by renderers and wrapping helpers.
//!
//! Headless Mermaid layout has to size labels before there is a browser DOM. The built-in
//! measurers are therefore compatibility profiles, not a promise that every host browser will pick
//! the same font fallback at display time. Hosts with a complete, infallible text stack can
//! implement this trait, wrap it in a [`crate::environment::TextMeasurementProfile`], and install
//! a [`crate::environment::TextMeasurementPolicy`] on the operation-owned
//! [`crate::environment::RenderEnvironment`]. Fallible host callbacks should implement
//! [`crate::environment::HostTextMeasurer`] instead; the environment's phase policy owns their
//! deterministic fallback and records its provenance.

use super::{TextMetrics, TextStyle, WrapMode};

pub(crate) const MERMAID_CREATE_TEXT_DEFAULT_WIDTH_PX: f64 = 200.0;

/// Measures label text for layout decisions.
///
/// This trait is deliberately infallible. Implementations must not call a host and silently choose
/// their own fallback; that would bypass the operation's phase routing. Use
/// [`crate::environment::HostTextMeasurer`] for host callbacks that can decline or fail.
///
/// `TextMeasurer` is the extension point for editors and other hosts that need layout to match
/// their own font system. Implementations should cache aggressively: flowchart/class/sequence
/// layout can ask for the same label in several wrap modes while computing nodes, edges, and final
/// SVG.
///
/// The default deterministic measurer provides portable, bounded heuristics without font assets.
/// A host-provided measurer can instead use platform text APIs, a UI toolkit text system, or an
/// optional font engine while preserving the rest of merman's parser/layout/render pipeline.
/// HTML measurement requests preserve unselected U+00AD controls for shaping. They have zero
/// glyph advance; deleting them before shaping can change adjacent presentation selectors and
/// combining sequences. A selected discretionary break replaces only its final U+00AD with `-`.
pub trait TextMeasurer {
    /// Reports whether the owning render operation has been cancelled.
    ///
    /// The measurement trait remains infallible for custom hosts. The routed operation measurer
    /// uses this hook so long-running built-in wrapping probes can stop promptly and let the next
    /// fallible layout boundary replay the structured cancellation error.
    #[doc(hidden)]
    fn cancellation_requested(&self) -> bool {
        false
    }

    /// Returns crate-private authority for reusing a measurement within the same routed operation.
    ///
    /// This hook is deliberately unnameable outside `merman-render`: external custom measurers use
    /// the default `None`, while the operation-owned routed facade can validate an exact built-in
    /// profile, phase, and operation without exposing a replayable public token.
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn builtin_operation_carrier(
        &self,
        _operation: crate::environment::TextMeasurementOperation,
    ) -> Option<crate::environment::BuiltinTextMeasurementOperationCarrier> {
        None
    }

    /// Starts an exact streaming computed-length probe when this measurer is a built-in profile.
    ///
    /// The return type is crate-private so external measurers cannot bypass their observable host
    /// callback order. They use the default `None` and continue receiving every complete request.
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn begin_svg_text_computed_length(
        &self,
        _style: &TextStyle,
    ) -> Option<crate::environment::BuiltinSvgComputedLength> {
        None
    }

    /// Starts an exact streaming width probe for one decoded HTML text line.
    ///
    /// The crate-private state is available only to qualified built-in wrapping routes. External
    /// custom and host-backed measurers retain the default `None`, so each complete candidate
    /// remains an observable measurement request and may use non-additive text shaping.
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn begin_html_line_width(
        &self,
        _style: &TextStyle,
    ) -> Option<crate::environment::BuiltinHtmlLineWidth> {
        None
    }

    /// Measures one decoded HTML text line without interpreting literal markup spellings.
    ///
    /// Callers have already normalized whitespace and selected discretionary glyphs. The default
    /// preserves the existing complete wrapped-measurement contract for custom implementations.
    #[doc(hidden)]
    fn measure_html_line_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.measure_wrapped(text, style, None, WrapMode::HtmlLike)
            .width
    }

    fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics;

    /// Measures SVG `<tspan>.getComputedTextLength()`-like widths (advance length along the
    /// baseline).
    ///
    /// Mermaid's Timeline diagram uses `getComputedTextLength()` to decide when to wrap tokens
    /// into additional `<tspan>` lines. This length can differ meaningfully from `getBBox().width`
    /// (which includes glyph overhang), especially near wrapping boundaries.
    ///
    /// Default implementation falls back to bbox-derived widths.
    fn measure_svg_text_computed_length_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.measure_svg_simple_text_bbox_width_px(text, style)
    }

    /// Measures the horizontal extents of an SVG `<text>` element relative to its anchor `x`.
    ///
    /// Mermaid's flowchart-v2 viewport sizing uses `getBBox()` on the rendered SVG. For `<text>`
    /// elements this bbox can be slightly asymmetric around the anchor due to glyph overhangs.
    ///
    /// Default implementation assumes a symmetric bbox: `left = right = width/2`.
    fn measure_svg_text_bbox_x(&self, text: &str, style: &TextStyle) -> (f64, f64) {
        let m = self.measure(text, style);
        let half = (m.width.max(0.0)) / 2.0;
        (half, half)
    }

    /// Measures SVG `<text>.getBBox()` horizontal extents while including ASCII overhang.
    ///
    /// Upstream Mermaid bbox behavior can be asymmetric even for ASCII strings due to glyph
    /// outlines and hinting. Most diagrams in this codebase intentionally ignore ASCII overhang
    /// to avoid systematic `viewBox` drift, but some diagrams (notably `timeline`) rely on the
    /// actual `getBBox()` extents when labels can overflow node shapes.
    ///
    /// Default implementation falls back to the symmetric bbox measurement.
    fn measure_svg_text_bbox_x_with_ascii_overhang(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> (f64, f64) {
        self.measure_svg_text_bbox_x(text, style)
    }

    /// Measures the horizontal extents for Mermaid diagram titles rendered as a single `<text>`
    /// node (no whitespace-tokenized `<tspan>` runs).
    ///
    /// Mermaid flowchart-v2 uses this style for `flowchartTitleText`, and the bbox impacts the
    /// final `viewBox` / `max-width` computed via `getBBox()`.
    fn measure_svg_title_bbox_x(&self, text: &str, style: &TextStyle) -> (f64, f64) {
        self.measure_svg_text_bbox_x(text, style)
    }

    /// Measures the bbox width for Mermaid `drawSimpleText(...).getBBox().width`-style probes
    /// (used by upstream `calculateTextWidth`).
    ///
    /// This should reflect actual glyph outline extents (including ASCII overhang where present),
    /// rather than the symmetric/center-anchored title bbox approximation.
    fn measure_svg_simple_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        let (l, r) = self.measure_svg_title_bbox_x(text, style);
        (l + r).max(0.0)
    }

    /// Measures raw SVG `<text>.getBBox().width` for diagram renderers that append text directly.
    ///
    /// Unlike [`TextMeasurer::measure_svg_simple_text_bbox_width_px`], this models the DOM shape
    /// directly and does not inherit `drawSimpleText(...)`-specific behavior.
    fn measure_svg_raw_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.measure_svg_simple_text_bbox_width_px(text, style)
    }

    /// Measures raw SVG `<text>.getBBox().height` for direct text content.
    ///
    /// This stays distinct from tspan and Mermaid helper probes so browser hosts can preserve the
    /// exact DOM shape used by renderers such as TreeView.
    fn measure_svg_raw_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        // This is an explicitly approximate compatibility default. Exact profiles override the
        // raw-text operation instead of borrowing another DOM shape's result.
        self.measure(text, style).height.max(0.0)
    }

    /// Signed bbox Y of direct SVG text at baseline zero. `None` means this profile does not
    /// supply baseline facts; callers retain their established compatibility bounds.
    fn measure_svg_raw_text_bbox_y_offset_px(
        &self,
        _text: &str,
        _style: &TextStyle,
    ) -> Option<f64> {
        None
    }

    /// Signed bbox Y of direct SVG text with inherited `dominant-baseline: middle`.
    /// This is not the specially positioned Mermaid createFormattedText DOM.
    fn measure_svg_raw_text_middle_bbox_y_offset_px(
        &self,
        _text: &str,
        _style: &TextStyle,
    ) -> Option<f64> {
        None
    }

    /// Signed bbox Y of direct SVG text with inherited `dominant-baseline: hanging`.
    fn measure_svg_raw_text_hanging_bbox_y_offset_px(
        &self,
        _text: &str,
        _style: &TextStyle,
    ) -> Option<f64> {
        None
    }

    /// Measures the CSS pixel width returned by SVG `<text>.getBoundingClientRect().width`.
    ///
    /// This is a distinct browser primitive from `getBBox().width`: transforms, CSS pixel
    /// quantization, and layout-engine rounding can make their results diverge. Mermaid uses the
    /// client rect directly for Journey actor labels and Pie legend/title sizing. Headless
    /// profiles fall back to the closest raw SVG text width; browser hosts should implement the
    /// exact DOM operation.
    fn measure_svg_text_bounding_client_rect_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.measure_svg_raw_text_bbox_width_px(text, style)
    }

    /// Measures a `<text>` node whose visible rows are emitted as child `<tspan>` elements.
    ///
    /// Browsers can apply different hinting and endpoint extents to this DOM shape than to a
    /// direct text node. The default keeps custom measurers source-compatible by falling back to
    /// raw SVG text measurement.
    fn measure_svg_tspan_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.measure_svg_raw_text_bbox_width_px(text, style)
    }

    /// Measures child `<tspan>` rows with explicit `xml:space="preserve"` semantics.
    ///
    /// Mermaid's `gitGraphRenderer.drawText` preserves interior row whitespace, as does Merman's
    /// Gitgraph SVG emitter. Routed hosts receive the existing `TspanBBoxWidth` operation with
    /// `WrapMode::HtmlLike` to distinguish preserved spaces from ordinary SVG collapsing; this
    /// probe remains unwrapped. Existing custom profiles keep their tspan-width implementation.
    fn measure_svg_preserved_tspan_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.measure_svg_tspan_text_bbox_width_px(text, style)
    }

    /// Measures the bbox height of a `<text>` node whose visible rows are child `<tspan>` nodes.
    fn measure_svg_tspan_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        // This is an explicitly approximate compatibility default. Exact profiles override the
        // single-tspan operation independently from raw `<text>` measurement.
        self.measure(text, style).height.max(0.0)
    }

    /// Measures the `getBBox().y` offset of Mermaid's `createFormattedText(...)` SVG label.
    ///
    /// That DOM shape positions an outer `<tspan>` with `y="-0.1em"` and `dy="1.1em"` under a
    /// `<text y="-10.1">` element. The resulting offset depends on the selected font and may be
    /// negative, so browser-backed hosts should implement this operation directly.
    fn measure_svg_create_text_bbox_y_offset_px(&self, _text: &str, _style: &TextStyle) -> f64 {
        // No font-independent y offset exists. Profiles without this operation return the neutral
        // baseline; operation-owned deterministic and host profiles provide the real DOM fact.
        0.0
    }

    /// Measures the same `createFormattedText(...)` bbox y offset after the outer Architecture
    /// label container applies inherited `dominant-baseline: middle`.
    ///
    /// SVG middle-baseline positioning depends on the selected font's x-height and is distinct
    /// from the raw createText bbox operation. Browser-backed hosts should measure this DOM shape
    /// directly. Profiles without this operation return a neutral value rather than reusing the
    /// ordinary formatted-text result or guessing an x-height from an unrelated font.
    fn measure_svg_create_text_middle_bbox_y_offset_px(
        &self,
        _text: &str,
        _style: &TextStyle,
    ) -> f64 {
        0.0
    }

    /// Measures simple SVG text for wrap decisions.
    ///
    /// Incremental `wrapLabel(...)` probes and final rendered text can use different DOM shapes.
    /// Implementations may specialize this method when the wrap probe's browser API differs from
    /// the final node's bbox API.
    fn measure_svg_simple_text_bbox_width_for_wrap_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.measure_svg_simple_text_bbox_width_px(text, style)
    }

    /// Measures one SVG row using Mermaid's body-attached `calculateTextDimensions(...)` probe.
    ///
    /// The probe's CSSOM assignment and DOM attachment can affect both dimensions, so this stays a
    /// single metrics operation rather than combining an operation-specific width with a generic
    /// height. Profiles may specialize it when invalid CSS falls back to the host's default font.
    fn measure_mermaid_calculate_text_dimensions(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> TextMetrics {
        TextMetrics {
            width: self.measure_svg_simple_text_bbox_width_for_wrap_px(text, style),
            height: self.measure_svg_simple_text_bbox_height_px(text, style),
            line_count: 1,
        }
    }

    /// Measures Canvas2D `measureText(...).width` for canvas-backed layout engines.
    ///
    /// Canvas text advance is a distinct host primitive from SVG bbox and computed-length APIs.
    /// The default uses the closest baseline-advance operation so existing custom profiles remain
    /// source-compatible; browser-backed hosts should implement the exact Canvas2D operation.
    fn measure_canvas_text_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.measure_svg_text_computed_length_px(text, style)
    }

    /// Measures the bbox height for Mermaid `drawSimpleText(...).getBBox().height`-style probes.
    ///
    /// Despite this method's historical "simple text" name, upstream `drawSimpleText(...)`
    /// appends one child `<tspan>` to the `<text>` node. Exact implementations therefore route it
    /// through their single-tspan DOM-shape profile, not raw direct text content.
    ///
    /// Default implementation falls back to `measure(...).height`.
    fn measure_svg_simple_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        let m = self.measure(text, style);
        m.height.max(0.0)
    }

    fn measure_wrapped(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> TextMetrics {
        let _ = max_width;
        let _ = wrap_mode;
        self.measure(text, style)
    }

    /// Measures wrapped text and (optionally) returns the unwrapped width for the same payload.
    ///
    /// This exists mainly to avoid redundant measurement passes in diagrams that need both:
    /// - wrapped metrics (for height/line breaks), and
    /// - a raw "overflow width" probe (for sizing containers that can visually overflow).
    ///
    /// Default implementation returns `None` for `raw_width_px` and callers may fall back to an
    /// explicit second measurement if needed.
    fn measure_wrapped_with_raw_width(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> (TextMetrics, Option<f64>) {
        (
            self.measure_wrapped(text, style, max_width, wrap_mode),
            None,
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MermaidTextDimensions {
    pub(crate) width: i64,
    pub(crate) height: i64,
    pub(crate) line_height: i64,
}

fn try_measure_mermaid_text_dimensions_for_family<E>(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<MermaidTextDimensions, E> {
    if text.is_empty() {
        return Ok(MermaidTextDimensions::default());
    }

    let mut dimensions = MermaidTextDimensions::default();
    for line in super::split_html_br_lines(text) {
        checkpoint()?;
        let measured_line = if line.is_empty() { "\u{200b}" } else { line };
        checkpoint()?;
        let measured = measurer.measure_mermaid_calculate_text_dimensions(measured_line, style);
        checkpoint()?;
        let width = measured.width.max(0.0).round() as i64;
        let height = measured.height.max(0.0).round() as i64;
        dimensions.width = dimensions.width.max(width);
        dimensions.height += height;
        dimensions.line_height = dimensions.line_height.max(height);
    }
    Ok(dimensions)
}

fn try_measure_mermaid_text_dimensions<E>(
    measurer: &dyn TextMeasurer,
    text: &str,
    configured_style: &TextStyle,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<MermaidTextDimensions, E> {
    let mut sans_style = configured_style.clone();
    sans_style.font_family = Some("sans-serif".to_string());

    let sans =
        try_measure_mermaid_text_dimensions_for_family(measurer, text, &sans_style, checkpoint)?;
    let configured = try_measure_mermaid_text_dimensions_for_family(
        measurer,
        text,
        configured_style,
        checkpoint,
    )?;

    if sans.width > configured.width
        && sans.height > configured.height
        && sans.line_height > configured.line_height
    {
        Ok(sans)
    } else {
        Ok(configured)
    }
}

/// Mirrors Mermaid's shared `calculateTextDimensions` utility.
pub(crate) fn measure_mermaid_text_dimensions(
    measurer: &dyn TextMeasurer,
    text: &str,
    configured_style: &TextStyle,
) -> MermaidTextDimensions {
    let mut checkpoint = || Ok::<(), std::convert::Infallible>(());
    match try_measure_mermaid_text_dimensions(measurer, text, configured_style, &mut checkpoint) {
        Ok(dimensions) => dimensions,
        Err(error) => match error {},
    }
}

pub(crate) fn measure_mermaid_text_dimensions_with_checkpoint(
    measurer: &dyn TextMeasurer,
    text: &str,
    configured_style: &TextStyle,
    mut checkpoint: impl FnMut() -> crate::Result<()>,
) -> crate::Result<MermaidTextDimensions> {
    try_measure_mermaid_text_dimensions(measurer, text, configured_style, &mut checkpoint)
}
