//! Optional math rendering hooks.
//!
//! Upstream Mermaid renders `$$...$$` fragments via KaTeX and measures the resulting HTML in a
//! browser DOM. Merman models math rendering as an operation-scoped backend. An operation without
//! one rejects typed diagrams that contain delimited math instead of emitting the source as plain
//! text. The `math` feature installs the pure-Rust RaTeX backend by default; hosts may still supply
//! another implementation explicitly.

#[cfg(feature = "math")]
use crate::text::split_html_br_lines;
use crate::text::{TextMetrics, TextStyle, WrapMode};
use merman_core::MermaidConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Optional math renderer used to transform label HTML and (optionally) provide measurements.
///
/// Implementations should be:
/// - deterministic (stable output across runs),
/// - side-effect free (no global mutations),
/// - non-panicking (return `None` to decline handling).
pub trait MathRenderer: std::fmt::Debug {
    /// Attempts to render math fragments within an HTML label string.
    ///
    /// If the renderer declines to handle the input, it should return `None`.
    ///
    /// The returned string is treated as raw HTML and will still be sanitized by merman before
    /// emitting into an SVG `<foreignObject>`.
    fn render_html_label(&self, text: &str, config: &MermaidConfig) -> Option<String>;

    /// Attempts to render a Sequence `drawKatex(...)` label.
    ///
    /// Sequence uses a bare `foreignObject` with `width: fit-content` rather than Flowchart's
    /// HTML-label shell, so math backends may support a slightly different surface here.
    fn render_sequence_html_label(&self, text: &str, config: &MermaidConfig) -> Option<String> {
        self.render_html_label(text, config)
    }

    /// Optionally measures the rendered HTML label in pixels.
    ///
    /// This is intended to mirror upstream Mermaid's DOM measurement behavior for math labels.
    /// The default implementation returns `None`.
    fn measure_html_label(
        &self,
        _text: &str,
        _config: &MermaidConfig,
        _style: &TextStyle,
        _max_width_px: Option<f64>,
        _wrap_mode: WrapMode,
    ) -> Option<TextMetrics> {
        None
    }

    /// Optionally measures a Sequence `drawKatex(...)` label in pixels.
    ///
    /// Mermaid Sequence does not wrap KaTeX labels in the flowchart HTML-label shell; it appends
    /// a bare `<foreignObject><div style="width: fit-content;">...</div></foreignObject>`.
    /// This hook lets Sequence callers avoid inheriting flowchart-specific table-cell metrics.
    fn measure_sequence_html_label(
        &self,
        _text: &str,
        _config: &MermaidConfig,
    ) -> Option<TextMetrics> {
        None
    }
}

/// Explicit no-op math renderer for hosts that want a backend which declines every label.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMathRenderer;

impl MathRenderer for NoopMathRenderer {
    fn render_html_label(&self, _text: &str, _config: &MermaidConfig) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DelimitedMathFragment<'a> {
    pub(crate) leading_text: &'a str,
    #[cfg(feature = "math")]
    pub(crate) formula: &'a str,
    pub(crate) delimited: &'a str,
}

#[derive(Debug)]
pub(crate) struct DelimitedMathLine<'a> {
    pub(crate) fragments: Vec<DelimitedMathFragment<'a>>,
    pub(crate) trailing_text: &'a str,
}

pub(crate) fn parse_delimited_math_line(text: &str) -> Option<DelimitedMathLine<'_>> {
    let mut fragments = Vec::new();
    let mut search_from = 0usize;

    while let Some(start_rel) = text[search_from..].find("$$") {
        let start = search_from + start_rel;
        let content_start = start + 2;
        let Some(end_rel) = text[content_start..].find("$$") else {
            break;
        };
        let end_start = content_start + end_rel;
        let end = end_start + 2;
        fragments.push(DelimitedMathFragment {
            leading_text: &text[search_from..start],
            #[cfg(feature = "math")]
            formula: &text[content_start..end_start],
            delimited: &text[start..end],
        });
        search_from = end;
    }

    (!fragments.is_empty()).then_some(DelimitedMathLine {
        fragments,
        trailing_text: &text[search_from..],
    })
}

pub(crate) fn contains_delimited_math(text: &str) -> bool {
    crate::text::split_html_br_lines(text)
        .into_iter()
        .any(|line| parse_delimited_math_line(line).is_some())
}

pub(crate) struct MathLabelMetricsRequest<'a> {
    pub(crate) measurer: &'a dyn crate::text::TextMeasurer,
    pub(crate) raw_label: &'a str,
    pub(crate) style: &'a TextStyle,
    pub(crate) max_width_px: Option<f64>,
    pub(crate) wrap_mode: WrapMode,
    pub(crate) config: &'a MermaidConfig,
    pub(crate) math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
}

fn measure_text_fragment(
    measurer: &dyn crate::text::TextMeasurer,
    text: &str,
    style: &TextStyle,
) -> TextMetrics {
    measurer.measure_wrapped(text, style, None, WrapMode::HtmlLike)
}

fn measure_mixed_math_line(
    measurer: &dyn crate::text::TextMeasurer,
    line: &str,
    style: &TextStyle,
    config: &MermaidConfig,
    math_renderer: &(dyn MathRenderer + Send + Sync),
) -> Option<(f64, f64)> {
    let start = line.find("$$")?;
    let content_start = start + 2;
    let end_start = line[content_start..].rfind("$$")? + content_start;
    if line[content_start..end_start].contains("$$") {
        return None;
    }

    let mut width = 0.0_f64;
    let mut height = 0.0_f64;
    for text in [&line[..start], &line[end_start + 2..]] {
        if text.is_empty() {
            continue;
        }
        let metrics = measure_text_fragment(measurer, text, style);
        width += metrics.width.max(0.0);
        height = height.max(metrics.height.max(0.0));
    }

    let math_metrics = math_renderer.measure_html_label(
        &line[start..end_start + 2],
        config,
        style,
        Some(10_000.0),
        WrapMode::HtmlLike,
    )?;
    width += math_metrics.width.max(0.0);
    height = height.max(math_metrics.height.max(0.0));

    Some((width, height.max(1.0)))
}

fn measure_mixed_math_label(
    request: &MathLabelMetricsRequest<'_>,
    math_renderer: &(dyn MathRenderer + Send + Sync),
) -> Option<TextMetrics> {
    if !request.raw_label.contains("$$") {
        return None;
    }
    math_renderer.render_html_label(request.raw_label, request.config)?;

    let mut saw_math = false;
    let mut width = 0.0_f64;
    let mut height = 0.0_f64;
    let mut line_count = 0usize;
    for line in crate::text::split_html_br_lines(request.raw_label) {
        line_count += 1;
        let (line_width, line_height) = if line.contains("$$") {
            saw_math = true;
            measure_mixed_math_line(
                request.measurer,
                line,
                request.style,
                request.config,
                math_renderer,
            )?
        } else {
            let metrics = request.measurer.measure_wrapped(
                line,
                request.style,
                request.max_width_px,
                WrapMode::HtmlLike,
            );
            (metrics.width.max(0.0), metrics.height.max(0.0))
        };
        width = width.max(line_width);
        height += line_height;
    }

    saw_math.then_some(TextMetrics {
        width,
        height: height.max(1.0),
        line_count: line_count.max(1),
    })
}

/// Measures a Mermaid HTML label after delegating its math fragments to the active backend.
pub(crate) fn math_label_metrics_for_layout(
    request: MathLabelMetricsRequest<'_>,
) -> Option<TextMetrics> {
    if request.wrap_mode != WrapMode::HtmlLike || !request.raw_label.contains("$$") {
        return None;
    }
    let math_renderer = request.math_renderer?;
    math_renderer
        .measure_html_label(
            request.raw_label,
            request.config,
            request.style,
            request.max_width_px,
            request.wrap_mode,
        )
        .or_else(|| measure_mixed_math_label(&request, math_renderer))
}

/// Renders, sanitizes, and XML-normalizes a shared Mermaid math label fragment.
pub(crate) fn render_math_html_label(
    text: &str,
    config: &MermaidConfig,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> Option<String> {
    if !text.contains("$$") {
        return None;
    }
    let rendered = math_renderer?.render_html_label(text, config)?;
    Some(crate::xml::normalize_html_fragment_for_xhtml(
        &merman_core::sanitize::sanitize_text(&rendered, config),
    ))
}

/// Pure-Rust math renderer backed by RaTeX.
///
/// The first Flowchart surface is intentionally narrow: labels where each non-empty line is a
/// single `$$...$$` formula. Sequence additionally supports formulas embedded in surrounding
/// prose, matching Mermaid's `drawKatex(...)` shell.
#[cfg(feature = "math")]
#[derive(Debug, Default, Clone, Copy)]
pub struct RatexMathRenderer;

#[cfg(feature = "math")]
#[derive(Debug, Clone)]
struct RatexMathMetrics {
    width_em: f64,
    height_em: f64,
    line_count: usize,
}

#[cfg(feature = "math")]
impl RatexMathRenderer {
    fn normalized_text(text: &str) -> String {
        text.replace("\\\\", "\\")
    }

    fn math_only_lines(text: &str) -> Option<Vec<String>> {
        let normalized = Self::normalized_text(text);
        let mut formulas = Vec::new();
        for raw_line in split_html_br_lines(&normalized) {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            let inner = line.strip_prefix("$$")?.strip_suffix("$$")?;
            if inner.contains("$$") {
                return None;
            }
            formulas.push(inner.to_string());
        }
        if formulas.is_empty() {
            None
        } else {
            Some(formulas)
        }
    }

    fn layout_formula_em(latex: &str) -> Option<(ratex_types::DisplayList, f64, f64)> {
        let ast = ratex_parser::parse(latex).ok()?;
        let layout_options = ratex_layout::LayoutOptions::default()
            .with_style(ratex_types::MathStyle::Display)
            .with_color(ratex_types::Color::BLACK);
        let layout_box = ratex_layout::layout(&ast, &layout_options);
        // Display-list bounds include overflowing ink (for example, smash and lap commands).
        // Measurement must use those same rounded bounds as the emitted SVG viewport.
        let display_list = ratex_layout::to_display_list(&layout_box);
        let width_em = Self::emitted_em_dimension(display_list.width.max(0.0));
        let height_em = Self::emitted_em_dimension(display_list.total_height().max(0.0));
        Some((display_list, width_em, height_em))
    }

    fn render_formula_svg_em(latex: &str) -> Option<(String, f64, f64)> {
        let (display_list, width_em, height_em) = Self::layout_formula_em(latex)?;
        let svg = ratex_svg::render_to_svg(
            &display_list,
            &ratex_svg::SvgOptions {
                font_size: 1.0,
                padding: 0.0,
                stroke_width: 0.04,
                embed_glyphs: true,
                font_dir: String::new(),
            },
        );
        Some((
            Self::svg_with_em_size(svg, width_em, height_em),
            width_em,
            height_em,
        ))
    }

    fn svg_with_em_size(svg: String, width_em: f64, height_em: f64) -> String {
        let Some(open_end) = svg.find('>') else {
            return svg;
        };
        let Some(body_with_close) = svg.get(open_end + 1..) else {
            return svg;
        };
        let Some(body) = body_with_close.strip_suffix("</svg>") else {
            return svg;
        };
        let width = Self::fmt_num(width_em);
        let height = Self::fmt_num(height_em);
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}em" height="{height}em">{body}</svg>"#
        )
    }

    fn measure_math_only_label(text: &str) -> Option<RatexMathMetrics> {
        let formulas = Self::math_only_lines(text)?;
        let mut width_em: f64 = 0.0;
        let mut height_em: f64 = 0.0;
        let mut line_count = 0usize;
        for formula in formulas {
            let (_display_list, line_width_em, line_height_em) = Self::layout_formula_em(&formula)?;
            width_em = width_em.max(line_width_em);
            height_em += line_height_em;
            line_count += 1;
        }
        Some(RatexMathMetrics {
            width_em,
            height_em,
            line_count: line_count.max(1),
        })
    }

    fn render_katex_like_line_html(parsed: DelimitedMathLine<'_>) -> Option<String> {
        let mut html = String::new();
        for fragment in parsed.fragments {
            let (svg, _width_em, _height_em) = Self::render_formula_svg_em(fragment.formula)?;
            html.push_str(fragment.leading_text);
            html.push_str(&svg);
        }
        html.push_str(parsed.trailing_text);
        Some(html)
    }

    fn render_katex_like_label(text: &str) -> Option<String> {
        let normalized = Self::normalized_text(text);
        if !normalized.contains("$$") {
            return None;
        }

        let mut html = String::new();
        let mut saw_math = false;
        for line in split_html_br_lines(&normalized) {
            if let Some(parsed) = parse_delimited_math_line(line) {
                saw_math = true;
                let rendered_line = Self::render_katex_like_line_html(parsed)?;
                let _ = write!(
                    &mut html,
                    r#"<div style="display: flex; align-items: center; justify-content: center; white-space: nowrap;">{rendered_line}</div>"#
                );
            } else {
                let _ = write!(&mut html, "<div>{line}</div>");
            }
        }

        saw_math.then_some(html)
    }

    fn metrics_from_em(measured: &RatexMathMetrics, font_size: f64) -> TextMetrics {
        let font_size = font_size.max(1.0);
        TextMetrics {
            width: measured.width_em * font_size,
            height: measured.height_em * font_size,
            line_count: measured.line_count,
        }
    }

    fn emitted_em_dimension(value: f64) -> f64 {
        Self::fmt_num(value).parse().unwrap_or(0.0)
    }

    fn fmt_num(n: f64) -> String {
        let s = format!("{n:.6}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s.is_empty() || s == "-" {
            "0".to_string()
        } else {
            s.to_string()
        }
    }
}

#[cfg(feature = "math")]
impl MathRenderer for RatexMathRenderer {
    fn render_html_label(&self, text: &str, _config: &MermaidConfig) -> Option<String> {
        if !text.contains("$$") {
            return None;
        }
        Self::render_katex_like_label(text)
    }

    fn render_sequence_html_label(&self, text: &str, _config: &MermaidConfig) -> Option<String> {
        Self::render_katex_like_label(text)
    }

    fn measure_html_label(
        &self,
        text: &str,
        _config: &MermaidConfig,
        style: &TextStyle,
        _max_width_px: Option<f64>,
        wrap_mode: WrapMode,
    ) -> Option<TextMetrics> {
        if wrap_mode != WrapMode::HtmlLike || !text.contains("$$") {
            return None;
        }
        let measured = Self::measure_math_only_label(text)?;
        Some(Self::metrics_from_em(&measured, style.font_size))
    }

    fn measure_sequence_html_label(
        &self,
        text: &str,
        _config: &MermaidConfig,
    ) -> Option<TextMetrics> {
        if !text.contains("$$") {
            return None;
        }
        let measured = Self::measure_math_only_label(text)?;
        Some(Self::metrics_from_em(
            &measured,
            TextStyle::default().font_size,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RenderCacheKey {
    text: String,
    legacy_mathml: bool,
    force_legacy_mathml: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProbeCacheKey {
    render: RenderCacheKey,
    font_family: Option<String>,
    font_size_bits: u64,
    font_weight: Option<String>,
    max_width_bits: u64,
}

#[derive(Debug, Clone)]
struct ProbeCacheValue {
    html: String,
    width: f64,
    height: f64,
    line_count: usize,
}

#[derive(Debug, Serialize)]
struct NodeRenderRequest {
    text: String,
    config: NodeMathConfig,
}

#[derive(Debug, Serialize)]
struct NodeProbeRequest {
    text: String,
    config: NodeMathConfig,
    #[serde(rename = "styleCss")]
    style_css: String,
    #[serde(rename = "maxWidthPx")]
    max_width_px: f64,
}

#[derive(Debug, Serialize)]
struct NodeMathConfig {
    #[serde(rename = "legacyMathML")]
    legacy_mathml: bool,
    #[serde(rename = "forceLegacyMathML")]
    force_legacy_mathml: bool,
}

#[derive(Debug, Deserialize)]
struct NodeRenderResponse {
    html: String,
}

#[derive(Debug, Deserialize)]
struct NodeProbeResponse {
    html: String,
    width: f64,
    height: f64,
}

/// Optional KaTeX backend that shells out to a local Node.js toolchain.
///
/// This backend is intended for parity work where a real browser DOM is available. It mirrors
/// Mermaid's flowchart HTML-label KaTeX path closely by:
/// - rendering KaTeX through the local `katex` npm package, and
/// - measuring the wrapped `<foreignObject>` HTML through local `puppeteer`.
///
/// The backend is completely opt-in; if the configured Node.js environment is unavailable or the
/// probe fails, it simply returns `None` and lets callers fall back to the default text path.
#[derive(Debug)]
pub struct NodeKatexMathRenderer {
    node_cwd: PathBuf,
    node_command: PathBuf,
    render_cache: Mutex<HashMap<RenderCacheKey, Option<String>>>,
    probe_cache: Mutex<HashMap<ProbeCacheKey, Option<ProbeCacheValue>>>,
    sequence_probe_cache: Mutex<HashMap<RenderCacheKey, Option<ProbeCacheValue>>>,
}

impl NodeKatexMathRenderer {
    pub fn new(node_cwd: impl Into<PathBuf>) -> Self {
        Self {
            node_cwd: node_cwd.into(),
            node_command: PathBuf::from("node"),
            render_cache: Mutex::new(HashMap::new()),
            probe_cache: Mutex::new(HashMap::new()),
            sequence_probe_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_node_command(mut self, node_command: impl Into<PathBuf>) -> Self {
        self.node_command = node_command.into();
        self
    }

    fn script_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("katex_flowchart_probe.cjs")
    }

    fn normalized_text(text: &str) -> String {
        text.replace("\\\\", "\\")
    }

    fn math_config(config: &MermaidConfig) -> NodeMathConfig {
        let config_value = config.as_value();
        let legacy_mathml = config_value
            .get("legacyMathML")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let force_legacy_mathml = config_value
            .get("forceLegacyMathML")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        NodeMathConfig {
            legacy_mathml,
            force_legacy_mathml,
        }
    }

    fn render_key(text: &str, config: &MermaidConfig) -> RenderCacheKey {
        let config = Self::math_config(config);
        RenderCacheKey {
            text: Self::normalized_text(text),
            legacy_mathml: config.legacy_mathml,
            force_legacy_mathml: config.force_legacy_mathml,
        }
    }

    fn style_css(style: &TextStyle) -> String {
        let mut out = String::new();
        let font_family = style
            .font_family
            .as_deref()
            .unwrap_or("\"trebuchet ms\",verdana,arial,sans-serif");
        let _ = write!(&mut out, "font-size: {}px;", style.font_size);
        let _ = write!(&mut out, "font-family: {};", font_family);
        if let Some(font_weight) = style.font_weight.as_deref()
            && !font_weight.trim().is_empty()
        {
            let _ = write!(&mut out, "font-weight: {};", font_weight.trim());
        }
        out
    }

    fn run_node_request<T, R>(&self, mode: &str, payload: &T) -> Option<R>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        if !self.node_cwd.join("package.json").is_file() {
            return None;
        }

        let mut child = Command::new(&self.node_command)
            .arg(Self::script_path())
            .arg(mode)
            .current_dir(&self.node_cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;

        if let Some(mut stdin) = child.stdin.take() {
            if serde_json::to_writer(&mut stdin, payload).is_err() {
                return None;
            }
            let _ = stdin.flush();
        }

        let output = child.wait_with_output().ok()?;
        if !output.status.success() {
            return None;
        }

        serde_json::from_slice(&output.stdout).ok()
    }

    fn render_cached(&self, text: &str, config: &MermaidConfig) -> Option<String> {
        let key = Self::render_key(text, config);
        if let Some(cached) = self
            .render_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).cloned())
        {
            return cached;
        }

        let response: Option<NodeRenderResponse> = self.run_node_request(
            "render",
            &NodeRenderRequest {
                text: key.text.clone(),
                config: NodeMathConfig {
                    legacy_mathml: key.legacy_mathml,
                    force_legacy_mathml: key.force_legacy_mathml,
                },
            },
        );
        let html = response.map(|value| value.html);

        if let Ok(mut cache) = self.render_cache.lock() {
            cache.insert(key, html.clone());
        }

        html
    }

    fn probe_cached(
        &self,
        text: &str,
        config: &MermaidConfig,
        style: &TextStyle,
        max_width_px: Option<f64>,
        _wrap_mode: WrapMode,
    ) -> Option<ProbeCacheValue> {
        let render = Self::render_key(text, config);
        let max_width = max_width_px.unwrap_or(200.0).max(1.0);
        let key = ProbeCacheKey {
            render: render.clone(),
            font_family: style.font_family.clone(),
            font_size_bits: style.font_size.to_bits(),
            font_weight: style.font_weight.clone(),
            max_width_bits: max_width.to_bits(),
        };
        if let Some(cached) = self
            .probe_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).cloned())
        {
            return cached;
        }

        let style_css = Self::style_css(style);
        let response: Option<NodeProbeResponse> = self.run_node_request(
            "probe",
            &NodeProbeRequest {
                text: render.text.clone(),
                config: NodeMathConfig {
                    legacy_mathml: render.legacy_mathml,
                    force_legacy_mathml: render.force_legacy_mathml,
                },
                style_css,
                max_width_px: max_width,
            },
        );
        let probed = response.and_then(|value| {
            if !value.width.is_finite() || !value.height.is_finite() {
                return None;
            }
            let line_count = value.html.match_indices("<div").count().max(1);
            Some(ProbeCacheValue {
                html: value.html,
                width: value.width.max(0.0),
                height: value.height.max(0.0),
                line_count,
            })
        });

        if let Some(probed_value) = probed.clone()
            && let Ok(mut render_cache) = self.render_cache.lock()
        {
            render_cache
                .entry(render)
                .or_insert_with(|| Some(probed_value.html.clone()));
        }
        if let Ok(mut cache) = self.probe_cache.lock() {
            cache.insert(key, probed.clone());
        }

        probed
    }

    fn sequence_probe_cached(&self, text: &str, config: &MermaidConfig) -> Option<ProbeCacheValue> {
        let key = Self::render_key(text, config);
        if let Some(cached) = self
            .sequence_probe_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).cloned())
        {
            return cached;
        }

        let response: Option<NodeProbeResponse> = self.run_node_request(
            "probe-sequence",
            &NodeRenderRequest {
                text: key.text.clone(),
                config: NodeMathConfig {
                    legacy_mathml: key.legacy_mathml,
                    force_legacy_mathml: key.force_legacy_mathml,
                },
            },
        );
        let probed = response.and_then(|value| {
            if !value.width.is_finite() || !value.height.is_finite() {
                return None;
            }
            let line_count = value.html.match_indices("<div").count().max(1);
            Some(ProbeCacheValue {
                html: value.html,
                width: value.width.max(0.0),
                height: value.height.max(0.0),
                line_count,
            })
        });

        if let Some(probed_value) = probed.clone()
            && let Ok(mut render_cache) = self.render_cache.lock()
        {
            render_cache
                .entry(key.clone())
                .or_insert_with(|| Some(probed_value.html.clone()));
        }
        if let Ok(mut cache) = self.sequence_probe_cache.lock() {
            cache.insert(key, probed.clone());
        }

        probed
    }
}

impl MathRenderer for NodeKatexMathRenderer {
    fn render_html_label(&self, text: &str, config: &MermaidConfig) -> Option<String> {
        if !text.contains("$$") {
            return None;
        }
        self.render_cached(text, config)
    }

    fn measure_html_label(
        &self,
        text: &str,
        config: &MermaidConfig,
        style: &TextStyle,
        max_width_px: Option<f64>,
        wrap_mode: WrapMode,
    ) -> Option<TextMetrics> {
        if wrap_mode != WrapMode::HtmlLike || !text.contains("$$") {
            return None;
        }
        let probed = self.probe_cached(text, config, style, max_width_px, wrap_mode)?;
        Some(TextMetrics {
            width: probed.width,
            height: probed.height,
            line_count: probed.line_count,
        })
    }

    fn measure_sequence_html_label(
        &self,
        text: &str,
        config: &MermaidConfig,
    ) -> Option<TextMetrics> {
        if !text.contains("$$") {
            return None;
        }
        let probed = self.sequence_probe_cached(text, config)?;
        Some(TextMetrics {
            width: probed.width,
            height: probed.height,
            line_count: probed.line_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimited_math_detection_requires_a_complete_pair_on_one_label_line() {
        assert!(contains_delimited_math("value: $$x^2$$"));
        assert!(contains_delimited_math("plain<br>value: $$x^2$$"));
        assert!(!contains_delimited_math("literal $$"));
        assert!(!contains_delimited_math("literal $$<br>literal $$"));
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_renderer_splits_math_only_labels_with_source_br_shape() {
        assert_eq!(
            RatexMathRenderer::math_only_lines("$$x$$<BR /> $$y$$<bR\t/>$$z$$"),
            Some(vec!["x".to_string(), "y".to_string(), "z".to_string()])
        );
        assert!(
            RatexMathRenderer::math_only_lines("$$x$$<brx>$$y$$").is_none(),
            "non-source <br> lookalikes must not split a same-line multi-formula label"
        );
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_metrics_preserve_emitted_em_precision() {
        let rendered = RatexMathMetrics {
            width_em: 0.6255,
            height_em: 1.2505,
            line_count: 1,
        };

        let metrics = RatexMathRenderer::metrics_from_em(&rendered, 16.0);

        assert_eq!(metrics.width, rendered.width_em * 16.0);
        assert_eq!(metrics.height, rendered.height_em * 16.0);
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_renderer_renders_pure_math_label_as_inline_svg() {
        let renderer = RatexMathRenderer;
        let config = MermaidConfig::from_value(serde_json::json!({ "securityLevel": "loose" }));

        let html = renderer
            .render_html_label("$$x^2$$", &config)
            .expect("ratex should render pure math labels");

        assert!(html.contains("<svg"), "expected inline SVG: {html}");
        assert!(
            html.contains("<path"),
            "expected outlined glyph paths: {html}"
        );
        assert!(
            html.contains(r#"width="0.97153em""#),
            "unexpected SVG size: {html}"
        );
        let sanitized = merman_core::sanitize::sanitize_text(&html, &config);
        assert!(
            sanitized.contains("<svg") && sanitized.contains("<path"),
            "sanitizer should preserve RaTeX inline SVG: {sanitized}"
        );
        let mixed_html = renderer
            .render_html_label("value: $$x^2$$", &config)
            .expect("RaTeX HTML rendering should support prose plus math");
        assert!(
            mixed_html.contains("value: ") && mixed_html.contains("<svg"),
            "unexpected mixed math HTML: {mixed_html}"
        );
        assert!(
            !mixed_html.contains("$$"),
            "mixed math HTML should replace source delimiters: {mixed_html}"
        );

        let mixed_sequence = renderer
            .render_sequence_html_label("value: $$x^2$$", &config)
            .expect("Sequence RaTeX labels should support prose plus math");
        assert!(
            mixed_sequence.contains("value: ") && mixed_sequence.contains("<svg"),
            "unexpected Sequence mixed math HTML: {mixed_sequence}"
        );
        assert!(
            !mixed_sequence.contains("$$"),
            "Sequence mixed math HTML should replace source delimiters: {mixed_sequence}"
        );
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_renderer_renders_at_depth_limit_and_declines_deeper_input() {
        let renderer = RatexMathRenderer;
        let config = MermaidConfig::default();
        let nested_label =
            |depth: usize| format!("$${}x{}$$", "{".repeat(depth), "}".repeat(depth));

        let at_limit = nested_label(32);
        let html = renderer
            .render_html_label(&at_limit, &config)
            .expect("RaTeX should render input at its supported logical depth limit");
        assert!(html.contains("<svg"), "expected inline SVG: {html}");

        for depth in [33, 300] {
            let over_limit = nested_label(depth);
            assert!(
                renderer.render_html_label(&over_limit, &config).is_none(),
                "depth {depth} should be declined for plain-text fallback without panicking"
            );
        }
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_renderer_renders_multiple_formulas_on_one_line_independently() {
        let renderer = RatexMathRenderer;
        let config = MermaidConfig::default();
        let label = "a $$x$$ b $$y$$ c";

        let html = renderer
            .render_html_label(label, &config)
            .expect("same-line formulas should render");
        assert_eq!(
            html.matches(r#"<svg xmlns="http://www.w3.org/2000/svg""#)
                .count(),
            2,
            "each non-greedy math fragment should render independently: {html}"
        );
        assert!(
            html.contains("a ") && html.contains(" b ") && html.contains(" c"),
            "plain text between formulas should be preserved: {html}"
        );
        assert!(
            !html.contains("$$"),
            "all complete math delimiters should be replaced: {html}"
        );

        let sequence_html = renderer
            .render_sequence_html_label(label, &config)
            .expect("Sequence should share the non-greedy delimiter policy");
        assert_eq!(
            sequence_html
                .matches(r#"<svg xmlns="http://www.w3.org/2000/svg""#)
                .count(),
            2,
            "Sequence should render both same-line formulas: {sequence_html}"
        );
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_renderer_preserves_unclosed_delimiters_on_plain_lines() {
        let renderer = RatexMathRenderer;
        let config = MermaidConfig::default();

        assert!(
            renderer
                .render_sequence_html_label("literal $$", &config)
                .is_none(),
            "an unmatched delimiter alone must not enable Sequence math rendering"
        );

        let label = "valid $$x$$<br>literal $$";

        let html = renderer
            .render_sequence_html_label(label, &config)
            .expect("the complete formula should still render");

        assert_eq!(
            html.matches(r#"<svg xmlns="http://www.w3.org/2000/svg""#)
                .count(),
            1,
            "only the complete formula should render: {html}"
        );
        assert!(
            html.contains("<div>literal $$</div>"),
            "the unmatched delimiter should remain plain text: {html}"
        );
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_measurements_match_emitted_svg_dimensions() {
        let renderer = RatexMathRenderer;
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let (svg, width_em, height_em) = RatexMathRenderer::render_formula_svg_em("x^2")
            .expect("ratex should emit the formula SVG");
        assert!(
            svg.contains(&format!(
                "width=\"{}em\"",
                RatexMathRenderer::fmt_num(width_em)
            )),
            "unexpected emitted SVG width: {svg}"
        );
        assert!(
            svg.contains(&format!(
                "height=\"{}em\"",
                RatexMathRenderer::fmt_num(height_em)
            )),
            "unexpected emitted SVG height: {svg}"
        );

        let flowchart = renderer
            .measure_html_label("$$x^2$$", &config, &style, Some(200.0), WrapMode::HtmlLike)
            .expect("ratex should measure pure flowchart math labels");
        assert_eq!(flowchart.width, width_em * style.font_size);
        assert_eq!(flowchart.height, height_em * style.font_size);
        assert_eq!(flowchart.line_count, 1);

        let sequence = renderer
            .measure_sequence_html_label("$$x^2$$", &config)
            .expect("ratex should measure pure sequence math labels");
        assert_eq!(sequence.width, flowchart.width);
        assert_eq!(sequence.height, flowchart.height);
        assert_eq!(sequence.line_count, 1);

        let flowchart_request = crate::flowchart::FlowchartLabelMetricsRequest {
            measurer: &crate::text::DeterministicTextMeasurer::default(),
            raw_label: "$$x^2$$",
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &config,
            math_renderer: Some(&renderer),
        };
        let through_flowchart =
            crate::flowchart::flowchart_label_metrics_for_layout(flowchart_request);
        assert_eq!(through_flowchart.width, flowchart.width);
        assert_eq!(through_flowchart.height, flowchart.height);
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_measurements_match_svg_viewports_across_formulas_and_lines() {
        let renderer = RatexMathRenderer;
        let config = MermaidConfig::default();
        for label in [
            "$$x^2$$",
            r"$$\frac{1}{\sqrt{x}}$$",
            "$$α+β$$",
            r"$$\text{中文}$$",
            r"$$\smash{x^2}$$",
            r"$$\mathllap{x^2}$$",
            r"$$x^2$$<BR />$$\frac{1}{\sqrt{x}}$$<br>$$α+β$$",
        ] {
            let html = renderer.render_html_label(label, &config).unwrap();
            let document_source = format!("<div>{html}</div>");
            let document = roxmltree::Document::parse(&document_source).unwrap();
            let mut width_em = 0.0_f64;
            let mut height_em = 0.0_f64;
            let mut line_count = 0;
            for svg in document
                .descendants()
                .filter(|node| node.has_tag_name("svg"))
            {
                let dimension = |name| {
                    svg.attribute(name)
                        .unwrap()
                        .strip_suffix("em")
                        .unwrap()
                        .parse::<f64>()
                        .unwrap()
                };
                width_em = width_em.max(dimension("width"));
                height_em += dimension("height");
                line_count += 1;
            }
            assert!(line_count > 0, "expected formula SVGs for {label}");

            for font_size in [0.5, 16.0, 23.75] {
                let style = TextStyle {
                    font_size,
                    ..TextStyle::default()
                };
                let metrics = renderer
                    .measure_html_label(label, &config, &style, Some(1.0), WrapMode::HtmlLike)
                    .unwrap();
                assert_eq!(metrics.width, width_em * font_size.max(1.0), "{label}");
                assert_eq!(metrics.height, height_em * font_size.max(1.0), "{label}");
                assert_eq!(metrics.line_count, line_count, "{label}");
            }

            let sequence = renderer
                .measure_sequence_html_label(label, &config)
                .unwrap();
            let font_size = TextStyle::default().font_size;
            assert_eq!(sequence.width, width_em * font_size, "{label}");
            assert_eq!(sequence.height, height_em * font_size, "{label}");
            assert_eq!(sequence.line_count, line_count, "{label}");
        }
    }

    #[cfg(feature = "math")]
    #[test]
    fn ratex_math_measurement_preserves_declined_labels() {
        let renderer = RatexMathRenderer;
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        for label in ["$$x", r"$$\frac{1}{$$", r"$$x$$<br>$$\frac{1}{$$"] {
            assert!(
                renderer.render_html_label(label, &config).is_none(),
                "{label}"
            );
            assert!(
                renderer
                    .measure_html_label(label, &config, &style, None, WrapMode::HtmlLike)
                    .is_none(),
                "{label}"
            );
            assert!(
                renderer
                    .measure_sequence_html_label(label, &config)
                    .is_none(),
                "{label}"
            );
        }
        for label in ["value: $$x$$", "$$x$$ $$y$$"] {
            assert!(
                renderer.render_html_label(label, &config).is_some(),
                "{label}"
            );
            assert!(
                renderer
                    .measure_html_label(label, &config, &style, None, WrapMode::HtmlLike)
                    .is_none(),
                "mixed labels must remain delegated to the existing fallback: {label}"
            );
            assert!(
                renderer
                    .measure_sequence_html_label(label, &config)
                    .is_none()
            );
        }
    }

    #[test]
    fn node_katex_math_renderer_smoke() {
        let node_cwd = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tools")
            .join("mermaid-cli");
        if !node_cwd.join("package.json").is_file() || !node_cwd.join("node_modules").is_dir() {
            return;
        }

        let renderer = NodeKatexMathRenderer::new(node_cwd);
        let config = MermaidConfig::default();
        let style = TextStyle::default();

        let Some(html) = renderer.render_html_label("$$x^2$$", &config) else {
            return;
        };
        assert!(html.contains("katex"), "unexpected HTML: {html}");

        let Some(metrics) = renderer.measure_html_label(
            "$$x^2$$",
            &config,
            &style,
            Some(200.0),
            WrapMode::HtmlLike,
        ) else {
            return;
        };
        assert!(metrics.width.is_finite() && metrics.width > 0.0);
        assert!(metrics.height.is_finite() && metrics.height > 0.0);
    }

    #[test]
    fn node_katex_metrics_preserve_browser_probe_precision() {
        let renderer = NodeKatexMathRenderer::new("missing-node-environment");
        let config = MermaidConfig::default();
        let style = TextStyle::default();
        let render = NodeKatexMathRenderer::render_key("$$x$$", &config);
        let probe = ProbeCacheValue {
            html: "<div>x</div>".to_string(),
            width: 10.008,
            height: 20.008,
            line_count: 1,
        };
        let key = ProbeCacheKey {
            render: render.clone(),
            font_family: style.font_family.clone(),
            font_size_bits: style.font_size.to_bits(),
            font_weight: style.font_weight.clone(),
            max_width_bits: 200.0_f64.to_bits(),
        };
        renderer
            .probe_cache
            .lock()
            .unwrap()
            .insert(key, Some(probe.clone()));
        renderer
            .sequence_probe_cache
            .lock()
            .unwrap()
            .insert(render, Some(probe));

        let flowchart = renderer
            .measure_html_label("$$x$$", &config, &style, Some(200.0), WrapMode::HtmlLike)
            .unwrap();
        let sequence = renderer
            .measure_sequence_html_label("$$x$$", &config)
            .unwrap();

        assert_eq!(flowchart.width, 10.008);
        assert_eq!(flowchart.height, 20.008);
        assert_eq!(sequence.width, 10.008);
        assert_eq!(sequence.height, 20.008);
    }

    #[test]
    fn node_katex_math_renderer_measures_sanitized_flowchart_browser_shell() {
        let node_cwd = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tools")
            .join("mermaid-cli");
        if !node_cwd.join("package.json").is_file() || !node_cwd.join("node_modules").is_dir() {
            return;
        }

        let renderer = NodeKatexMathRenderer::new(node_cwd);
        let config = MermaidConfig::default();
        let style = TextStyle::default();

        let long_integral = "$$f(\\relax{x}) = \\int_{-\\infty}^\\infty \\hat{f}(\\xi)\\,e^{2 \\pi i \\xi x}\\,d\\xi$$";
        let Some(node_metrics) = renderer.measure_html_label(
            long_integral,
            &config,
            &style,
            Some(200.0),
            WrapMode::HtmlLike,
        ) else {
            return;
        };
        assert!(
            (150.0..=260.0).contains(&node_metrics.width),
            "node width = {}",
            node_metrics.width
        );
        assert!(
            (20.0..=70.0).contains(&node_metrics.height),
            "node height = {}",
            node_metrics.height
        );

        let matrix_label =
            "$$x(t)=c_1\\begin{bmatrix}-\\cos{t}+\\sin{t}\\\\ 2\\cos{t} \\end{bmatrix}e^{2t}$$";
        let Some(matrix_metrics) = renderer.measure_html_label(
            matrix_label,
            &config,
            &style,
            Some(200.0),
            WrapMode::HtmlLike,
        ) else {
            return;
        };
        // This is a Node/KaTeX shell smoke, not a browser-font parity gate.
        assert!(
            (250.0..=290.0).contains(&matrix_metrics.width),
            "matrix width = {}",
            matrix_metrics.width
        );
        assert!(
            (20.0..=32.0).contains(&matrix_metrics.height),
            "matrix height = {}",
            matrix_metrics.height
        );

        let Some(html) = renderer.render_html_label(long_integral, &config) else {
            panic!("expected rendered math HTML after successful probe");
        };
        assert!(html.contains("<math"), "unexpected HTML: {html}");
        assert!(!html.contains("<semantics>"), "unsanitized HTML: {html}");

        let nested_delimiters = "$$\\Bigg(\\bigg(\\Big(\\big((\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a})\\big)\\Big)\\bigg)\\Bigg)$$";
        let Some(edge_metrics) = renderer.measure_html_label(
            nested_delimiters,
            &config,
            &style,
            Some(200.0),
            WrapMode::HtmlLike,
        ) else {
            return;
        };
        assert!(
            (150.0..=320.0).contains(&edge_metrics.width),
            "edge width = {}",
            edge_metrics.width
        );
        assert!(
            (30.0..=100.0).contains(&edge_metrics.height),
            "edge height = {}",
            edge_metrics.height
        );
    }
}
