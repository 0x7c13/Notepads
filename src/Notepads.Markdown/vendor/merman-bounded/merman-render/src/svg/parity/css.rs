use super::*;

// Shared Mermaid diagram CSS fragments (split from parity.rs).
//
// Keep Mermaid@11.16.0 ordering quirks to preserve DOM parity.

#[derive(Clone, Copy)]
struct MermaidBaseCss<'a> {
    font_family: &'a str,
    font_size_css: &'a str,
    normal_edge_stroke_width_css: &'a str,
    text_color: &'a str,
    line_color: &'a str,
    error_bkg: &'a str,
    error_text: &'a str,
}

fn write_mermaid_base_css_prefix<I>(out: &mut String, id: I, css: MermaidBaseCss<'_>)
where
    I: Copy + std::fmt::Display,
{
    let _ = write_mermaid_base_css_prefix_to(out, id, css);
}

fn write_mermaid_base_css_prefix_to<I>(
    out: &mut dyn std::fmt::Write,
    id: I,
    css: MermaidBaseCss<'_>,
) -> std::fmt::Result
where
    I: Copy + std::fmt::Display,
{
    write!(
        out,
        r#"#{}{{font-family:{};font-size:{};fill:{};}}"#,
        id, css.font_family, css.font_size_css, css.text_color
    )?;
    out.write_str(
        r#"@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}@keyframes dash{to{stroke-dashoffset:0;}}"#,
    )?;
    write!(
        out,
        r#"#{} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}#{} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}"#,
        id, id
    )?;
    write!(
        out,
        r#"#{} .error-icon{{fill:{};}}#{} .error-text{{fill:{};stroke:{};}}"#,
        id, css.error_bkg, id, css.error_text, css.error_text
    )?;
    write!(
        out,
        r#"#{} .edge-thickness-normal{{stroke-width:{};}}#{} .edge-thickness-thick{{stroke-width:3.5px;}}#{} .edge-pattern-solid{{stroke-dasharray:0;}}#{} .edge-thickness-invisible{{stroke-width:0;fill:none;}}#{} .edge-pattern-dashed{{stroke-dasharray:3;}}#{} .edge-pattern-dotted{{stroke-dasharray:2;}}"#,
        id, css.normal_edge_stroke_width_css, id, id, id, id, id
    )?;
    write!(
        out,
        r#"#{} .marker{{fill:{};stroke:{};}}#{} .marker.cross{{stroke:{};}}"#,
        id, css.line_color, css.line_color, id, css.line_color
    )?;
    write!(
        out,
        r#"#{} svg{{font-family:{};font-size:{};}}"#,
        id, css.font_family, css.font_size_css
    )?;
    write_mermaid_paragraph_css_to(out, id)
}

pub(super) fn write_mermaid_paragraph_css_to(
    out: &mut dyn std::fmt::Write,
    id: impl std::fmt::Display,
) -> std::fmt::Result {
    write!(out, "#{id} p{{margin:0;}}")
}

fn mermaid_stroke_width_px(effective_config: &serde_json::Value) -> String {
    let value = crate::config::config_css_number_or_string(
        effective_config,
        &["themeVariables", "strokeWidth"],
    )
    .unwrap_or_else(|| "1".to_string());
    let value = value.trim();
    if value.parse::<f64>().is_ok() {
        format!("{value}px")
    } else {
        value.to_string()
    }
}

struct MermaidCommonNeoCss {
    node_border: String,
    use_gradient: bool,
    drop_shadow: String,
    stroke_width: String,
}

impl MermaidCommonNeoCss {
    fn new(effective_config: &serde_json::Value) -> Self {
        Self {
            node_border: theme_token(effective_config, "nodeBorder", "#9370DB"),
            use_gradient: config_bool(effective_config, &["themeVariables", "useGradient"])
                .unwrap_or(false),
            drop_shadow: crate::config::config_css_number_or_string(
                effective_config,
                &["themeVariables", "dropShadow"],
            )
            .unwrap_or_else(|| "none".to_string()),
            stroke_width: mermaid_stroke_width_px(effective_config),
        }
    }
}

struct NeoStroke<'a, I> {
    id: I,
    css: &'a MermaidCommonNeoCss,
}

impl<I> std::fmt::Display for NeoStroke<'_, I>
where
    I: Copy + std::fmt::Display,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.css.use_gradient {
            write!(formatter, "url(#{}-gradient)", self.id)
        } else {
            formatter.write_str(&self.css.node_border)
        }
    }
}

pub(super) fn write_mermaid_common_neo_css<I>(
    out: &mut String,
    id: I,
    effective_config: &serde_json::Value,
) where
    I: Copy + std::fmt::Display,
{
    let css = MermaidCommonNeoCss::new(effective_config);
    let _ = write_mermaid_common_neo_css_to(out, id, &css);
}

fn write_mermaid_common_neo_css_to<I>(
    out: &mut dyn std::fmt::Write,
    id: I,
    css: &MermaidCommonNeoCss,
) -> std::fmt::Result
where
    I: Copy + std::fmt::Display,
{
    let neo_stroke = NeoStroke { id, css };
    let drop_shadow = scoped_drop_shadow(id, &css.drop_shadow);

    write!(
        out,
        r#"#{} .node .neo-node{{stroke:{};}}"#,
        id, css.node_border
    )?;
    write!(
        out,
        r#"#{} [data-look="neo"].node rect,#{} [data-look="neo"].cluster rect,#{} [data-look="neo"].node polygon{{stroke:{};filter:{};}}"#,
        id, id, id, neo_stroke, drop_shadow
    )?;
    write!(
        out,
        r#"#{} [data-look="neo"].swimlane.cluster rect{{filter:none;}}"#,
        id
    )?;
    write!(
        out,
        r#"#{} [data-look="neo"].node path{{stroke:{};stroke-width:{};}}"#,
        id, neo_stroke, css.stroke_width
    )?;
    write!(
        out,
        r#"#{} [data-look="neo"].node .outer-path{{filter:{};}}"#,
        id, drop_shadow
    )?;
    write!(
        out,
        r#"#{} [data-look="neo"].node .neo-line path{{stroke:{};filter:none;}}"#,
        id, css.node_border
    )?;
    write!(
        out,
        r#"#{} [data-look="neo"].node circle{{stroke:{};filter:{};}}"#,
        id, neo_stroke, drop_shadow
    )?;
    write!(
        out,
        r##"#{} [data-look="neo"].node circle .state-start{{fill:#000000;}}"##,
        id
    )?;
    write!(
        out,
        r#"#{} [data-look="neo"].icon-shape .icon{{fill:{};filter:{};}}"#,
        id, neo_stroke, drop_shadow
    )?;
    write!(
        out,
        r#"#{} [data-look="neo"].icon-shape .icon-neo path{{stroke:{};filter:{};}}"#,
        id, neo_stroke, drop_shadow
    )?;
    Ok(())
}

fn mermaid_base_css_root_rule<I>(id: I, font_family: &str) -> String
where
    I: Copy + std::fmt::Display,
{
    let mut out = String::new();
    let _ = write_mermaid_base_css_root_rule_to(&mut out, id, font_family);
    out
}

pub(in crate::svg::parity) fn write_mermaid_base_css_root_rule_to<I>(
    out: &mut dyn std::fmt::Write,
    id: I,
    font_family: &str,
) -> std::fmt::Result
where
    I: Copy + std::fmt::Display,
{
    if font_family.is_empty() {
        return Ok(());
    }
    write!(
        out,
        r#"#{} :root{{--mermaid-font-family:{};}}"#,
        id, font_family
    )
}

pub(super) fn info_css_into<I>(out: &mut String, diagram_id: I)
where
    I: Copy + std::fmt::Display,
{
    // Production callers pass a normalized `SvgDiagramId`, whose grammar is already safe for
    // direct CSS selector interpolation. Keeping the wrapper intact makes every occurrence flow
    // through the shared resource projection instead of caching one escaped string.
    let id = diagram_id;
    let font = r#""trebuchet ms",verdana,arial,sans-serif"#;
    write_mermaid_base_css_prefix(
        out,
        id,
        MermaidBaseCss {
            font_family: font,
            font_size_css: "16px",
            normal_edge_stroke_width_css: "1px",
            text_color: "#333",
            line_color: "#333333",
            error_bkg: "#552222",
            error_text: "#552222",
        },
    );
    write_mermaid_common_neo_css(out, id, &serde_json::Value::Null);
    out.push_str(&mermaid_base_css_root_rule(id, font));
}

pub(super) struct InfoCssParts {
    pub(super) css_prefix: String,
    pub(super) root_rule: String,
    pub(super) font_family: String,
    pub(super) text_color: String,
    pub(super) line_color: String,
}

#[derive(Clone, Copy)]
enum InfoCssFontSizeSource {
    ThemeThenTopLevel,
    ThemeOnly,
    RawTheme,
}

struct InfoCssValues {
    font_family: String,
    root_font_family: String,
    font_size_css: String,
    normal_edge_stroke_width_css: String,
    text_color: String,
    line_color: String,
    error_bkg: String,
    error_text: String,
    neo: MermaidCommonNeoCss,
}

impl InfoCssValues {
    fn new(effective_config: &serde_json::Value, font_size_source: InfoCssFontSizeSource) -> Self {
        let font_family = crate::config::config_font_family_css(effective_config);
        let font_size_css = match font_size_source {
            InfoCssFontSizeSource::ThemeThenTopLevel => {
                crate::config::config_theme_font_size_css_or_root_number_px_opt(effective_config)
                    .map(|font_size| format!("{}px", fmt(font_size.max(1.0))))
            }
            InfoCssFontSizeSource::ThemeOnly => {
                config_f64_css_px(effective_config, &["themeVariables", "fontSize"])
                    .map(|font_size| format!("{}px", fmt(font_size.max(1.0))))
            }
            InfoCssFontSizeSource::RawTheme => crate::config::config_css_number_or_string(
                effective_config,
                &["themeVariables", "fontSize"],
            ),
        }
        .unwrap_or_else(|| "16px".to_string());

        let root_font_family = crate::config::config_root_font_family_css(effective_config);
        Self {
            font_family,
            root_font_family,
            font_size_css,
            normal_edge_stroke_width_css: mermaid_stroke_width_px(effective_config),
            text_color: theme_token(effective_config, "textColor", "#333"),
            line_color: theme_token(effective_config, "lineColor", "#333333"),
            error_bkg: theme_token(effective_config, "errorBkgColor", "#552222"),
            error_text: theme_token(effective_config, "errorTextColor", "#552222"),
            neo: MermaidCommonNeoCss::new(effective_config),
        }
    }

    fn write_prefix<I>(&self, out: &mut dyn std::fmt::Write, id: I) -> std::fmt::Result
    where
        I: Copy + std::fmt::Display,
    {
        write_mermaid_base_css_prefix_to(
            out,
            id,
            MermaidBaseCss {
                font_family: &self.font_family,
                font_size_css: &self.font_size_css,
                normal_edge_stroke_width_css: &self.normal_edge_stroke_width_css,
                text_color: &self.text_color,
                line_color: &self.line_color,
                error_bkg: &self.error_bkg,
                error_text: &self.error_text,
            },
        )
    }

    fn write_root<I>(&self, out: &mut dyn std::fmt::Write, id: I) -> std::fmt::Result
    where
        I: Copy + std::fmt::Display,
    {
        write_mermaid_common_neo_css_to(out, id, &self.neo)?;
        write_mermaid_base_css_root_rule_to(out, id, &self.root_font_family)
    }
}

pub(super) fn info_css_parts_with_config<I>(
    diagram_id: I,
    effective_config: &serde_json::Value,
) -> InfoCssParts
where
    I: Copy + std::fmt::Display,
{
    info_css_parts_with_font_size_source(
        diagram_id,
        effective_config,
        InfoCssFontSizeSource::ThemeThenTopLevel,
    )
}

pub(super) fn info_css_parts_with_theme_font_size_only<I>(
    diagram_id: I,
    effective_config: &serde_json::Value,
) -> InfoCssParts
where
    I: Copy + std::fmt::Display,
{
    info_css_parts_with_font_size_source(
        diagram_id,
        effective_config,
        InfoCssFontSizeSource::ThemeOnly,
    )
}

pub(super) fn info_css_parts_with_raw_theme_font_size<I>(
    diagram_id: I,
    effective_config: &serde_json::Value,
) -> InfoCssParts
where
    I: Copy + std::fmt::Display,
{
    info_css_parts_with_font_size_source(
        diagram_id,
        effective_config,
        InfoCssFontSizeSource::RawTheme,
    )
}

fn info_css_parts_with_font_size_source<I>(
    diagram_id: I,
    effective_config: &serde_json::Value,
    font_size_source: InfoCssFontSizeSource,
) -> InfoCssParts
where
    I: Copy + std::fmt::Display,
{
    let id = diagram_id;
    let values = InfoCssValues::new(effective_config, font_size_source);

    let mut out = String::new();
    let _ = values.write_prefix(&mut out, id);
    // Keep `:root` last (matches upstream Mermaid SVG baselines).
    let mut root_rule = String::new();
    let _ = values.write_root(&mut root_rule, id);

    InfoCssParts {
        css_prefix: out,
        root_rule,
        font_family: values.font_family,
        text_color: values.text_color,
        line_color: values.line_color,
    }
}

pub(super) fn info_css_with_config<I>(diagram_id: I, effective_config: &serde_json::Value) -> String
where
    I: Copy + std::fmt::Display,
{
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let mut out = parts.css_prefix;
    out.push_str(&parts.root_rule);
    out
}

#[cfg(feature = "layout-cytoscape")]
#[cfg(feature = "diagram-architecture")]
pub(super) struct ArchitectureCssParts {
    pub(super) css: String,
    pub(super) font_family: String,
    pub(super) font_size: f64,
}

#[cfg(feature = "layout-cytoscape")]
#[cfg(feature = "diagram-architecture")]
pub(super) fn architecture_css_parts_with_config<I>(
    diagram_id: I,
    effective_config: &serde_json::Value,
) -> ArchitectureCssParts
where
    I: Copy + std::fmt::Display,
{
    // Architecture uses the same "info-like" base stylesheet as Mermaid, but should honor
    // user-configured `fontFamily` / `fontSize` and theme variable colors.
    let id = diagram_id;

    let font_family = SvgTheme::new(effective_config).font_family_css();
    let font_size =
        crate::config::config_theme_font_size_css_or_root_number_px(effective_config, 16.0)
            .max(1.0);
    let font_size_css = format!("{}px", fmt(font_size));
    let normal_edge_stroke_width_css = mermaid_stroke_width_px(effective_config);

    let text_color = theme_token(effective_config, "textColor", "#333");
    let line_color = theme_token(effective_config, "lineColor", "#333333");
    let error_bkg = theme_token(effective_config, "errorBkgColor", "#552222");
    let error_text = theme_token(effective_config, "errorTextColor", "#552222");
    let primary_border = theme_token(
        effective_config,
        "primaryBorderColor",
        "hsl(240, 60%, 86.2745098039%)",
    );
    let arch_edge_color = theme_token(effective_config, "archEdgeColor", &line_color);
    let arch_edge_arrow_color =
        theme_token(effective_config, "archEdgeArrowColor", &arch_edge_color);
    let arch_edge_width = crate::config::config_css_number_or_string(
        effective_config,
        &["themeVariables", "archEdgeWidth"],
    )
    .unwrap_or_else(|| "3".to_string());
    let arch_group_border_color =
        theme_token(effective_config, "archGroupBorderColor", &primary_border);
    let arch_group_border_width = crate::config::config_css_number_or_string(
        effective_config,
        &["themeVariables", "archGroupBorderWidth"],
    )
    .unwrap_or_else(|| "2px".to_string());

    let mut out = String::new();
    write_mermaid_base_css_prefix(
        &mut out,
        id,
        MermaidBaseCss {
            font_family: &font_family,
            font_size_css: &font_size_css,
            normal_edge_stroke_width_css: &normal_edge_stroke_width_css,
            text_color: &text_color,
            line_color: &line_color,
            error_bkg: &error_bkg,
            error_text: &error_text,
        },
    );
    let _ = write!(
        &mut out,
        r#"#{} .edge{{stroke-width:{};stroke:{};fill:none;}}"#,
        id, arch_edge_width, arch_edge_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .arrow{{fill:{};}}"#,
        id, arch_edge_arrow_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .node-bkg{{fill:none;stroke:{};stroke-width:{};stroke-dasharray:8;}}"#,
        id, arch_group_border_color, arch_group_border_width
    );
    let _ = write!(
        &mut out,
        r#"#{} .node-icon-text{{display:flex;align-items:center;}}"#,
        id
    );
    let _ = write!(
        &mut out,
        r#"#{} .node-icon-text>div{{color:#fff;margin:1px;height:fit-content;text-align:center;overflow:hidden;display:-webkit-box;-webkit-box-orient:vertical;}}"#,
        id
    );

    write_mermaid_common_neo_css(&mut out, id, effective_config);
    // Keep `:root` last (matches upstream Mermaid SVG baselines).
    out.push_str(&mermaid_base_css_root_rule(
        id,
        &crate::config::config_root_font_family_css(effective_config),
    ));
    ArchitectureCssParts {
        css: out,
        font_family,
        font_size,
    }
}

#[cfg(feature = "layout-cytoscape")]
#[cfg_attr(not(test), allow(dead_code))]
#[cfg(feature = "diagram-architecture")]
pub(super) fn architecture_css_with_config<I>(
    diagram_id: I,
    effective_config: &serde_json::Value,
) -> String
where
    I: Copy + std::fmt::Display,
{
    architecture_css_parts_with_config(diagram_id, effective_config).css
}

#[cfg(feature = "diagram-requirement")]
pub(super) fn requirement_css<I>(diagram_id: I, effective_config: &serde_json::Value) -> String
where
    I: Copy + std::fmt::Display,
{
    // Mirrors Mermaid 11.15 `diagrams/requirement/styles.js` + shared base stylesheet ordering.
    // Keep `:root` last to match upstream fixtures.
    let id = diagram_id;
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let mut out = parts.css_prefix;
    let font = parts.font_family;
    let text_color = parts.text_color;
    let node_text_color = config_string(effective_config, &["themeVariables", "nodeTextColor"])
        .unwrap_or_else(|| text_color.clone());

    let option = |key: &str, default_value: &str| -> String {
        crate::config::config_css_number_or_string(effective_config, &["themeVariables", key])
            .unwrap_or_else(|| default_value.to_string())
    };

    let relation_color = option("relationColor", "#333333");
    let line_color = option("lineColor", "#333333");
    let font_size = crate::config::config_css_number_or_string(
        effective_config,
        &["themeVariables", "fontSize"],
    )
    .or_else(|| crate::config::config_css_number_or_string(effective_config, &["fontSize"]))
    .unwrap_or_else(|| "16px".to_string());
    let requirement_background = option("requirementBackground", "#ECECFF");
    let requirement_border_color =
        option("requirementBorderColor", "hsl(240, 60%, 86.2745098039%)");
    let requirement_border_size = option("requirementBorderSize", "1");
    let requirement_text_color = option("requirementTextColor", "#131300");
    let relation_label_background = option("relationLabelBackground", "rgba(232,232,232, 0.8)");
    let relation_label_color = option("relationLabelColor", "black");
    let edge_label_background = option("edgeLabelBackground", "rgba(232,232,232, 0.8)");
    let requirement_edge_label_background = config_string(
        effective_config,
        &["themeVariables", "requirementEdgeLabelBackground"],
    )
    .unwrap_or_else(|| edge_label_background.clone());
    let node_border = option("nodeBorder", "#9370DB");
    let look = config_diagram_look(effective_config);
    let relationship_line_stroke_width = if look.is_neo() {
        option("strokeWidth", "1")
    } else {
        "1px".to_string()
    };
    let _ = write!(
        &mut out,
        r#"#{} marker{{fill:{};stroke:{};}}#{} marker.cross{{stroke:{};}}"#,
        id, relation_color, relation_color, id, line_color
    );
    let _ = write!(
        &mut out,
        r#"#{} svg{{font-family:{};font-size:{}}}#{} .reqBox{{fill:{};fill-opacity:1.0;stroke:{};stroke-width:{};}}#{} .reqTitle,#{} .reqLabel{{fill:{};}}#{} .reqLabelBox{{fill:{};fill-opacity:1.0;}}#{} .req-title-line{{stroke:{};stroke-width:{};}}#{} .relationshipLine{{stroke:{};stroke-width:{};}}#{} .relationshipLabel{{fill:{};}}#{} .edgeLabel{{background-color:{};}}#{} .edgeLabel .label rect{{fill:{};}}#{} .edgeLabel .label text{{fill:{};}}#{} .divider{{stroke:{};stroke-width:1;}}#{} .label{{font-family:{};color:{};}}#{} .label text,#{} span{{fill:{};color:{};}}#{} .labelBkg{{background-color:{};}}"#,
        id,
        font,
        font_size,
        id,
        requirement_background,
        requirement_border_color,
        requirement_border_size,
        id,
        id,
        requirement_text_color,
        id,
        relation_label_background,
        id,
        requirement_border_color,
        requirement_border_size,
        id,
        relation_color,
        relationship_line_stroke_width,
        id,
        relation_label_color,
        id,
        edge_label_background,
        id,
        edge_label_background,
        id,
        relation_label_color,
        id,
        node_border,
        id,
        font,
        node_text_color,
        id,
        id,
        node_text_color,
        node_text_color,
        id,
        requirement_edge_label_background
    );
    out.push_str(&parts.root_rule);
    out
}

#[cfg(feature = "diagram-er")]
pub(super) fn er_css<I>(diagram_id: I, effective_config: &serde_json::Value) -> Result<String>
where
    I: Copy + std::fmt::Display,
{
    // Mirrors pinned Mermaid ER unified renderer stylesheet ordering (see `diagrams/er/styles.ts`
    // and shared base stylesheet).
    // Keep `:root` last (matches upstream fixtures).
    let id = diagram_id;
    let theme = SvgTheme::new(effective_config);
    let font = theme.font_family_css();
    let font_size = crate::config::config_theme_or_root_font_size_px_opt(effective_config)
        .or_else(|| config_f64_css_px(effective_config, &["er", "fontSize"]))
        .unwrap_or(16.0)
        .max(1.0);
    let text_color = theme.color("textColor", "#333");
    let line_color = theme.color("lineColor", "#333333");
    let error_bkg = theme.color("errorBkgColor", "#552222");
    let error_text = theme.color("errorTextColor", "#552222");
    let main_bkg = theme.color("mainBkg", "#ECECFF");
    let node_border = theme.color("nodeBorder", "#9370DB");
    let cluster_bkg = theme.color("clusterBkg", &main_bkg);
    let cluster_border = theme.color("clusterBorder", &node_border);
    let title_color = theme
        .optional_color("titleColor")
        .unwrap_or_else(|| text_color.clone());
    let node_text_color = theme
        .optional_color("nodeTextColor")
        .unwrap_or_else(|| text_color.clone());
    const DEFAULT_ER_TERTIARY: &str = "hsl(80, 100%, 96.2745098039%)";
    let tertiary_color = theme.color("tertiaryColor", DEFAULT_ER_TERTIARY);
    let edge_label_background = theme.color("edgeLabelBackground", "rgba(232,232,232, 0.8)");
    let er_edge_label_background = match theme.theme_name().as_str() {
        "redux-color" | "redux-dark-color" => theme.optional_color("erEdgeLabelBackground"),
        _ => None,
    };
    let label_background = match er_edge_label_background.clone() {
        Some(background) => background,
        None => css_rgba_fade(&tertiary_color, 0.5)?,
    };
    let edge_label_background = er_edge_label_background.unwrap_or(edge_label_background);
    let stroke_width = if theme.look() == "neo" {
        theme.css_value("strokeWidth", "1px")
    } else {
        "1px".to_string()
    };
    let font_size_css = format!("{}px", fmt(font_size));
    let normal_edge_stroke_width_css = mermaid_stroke_width_px(effective_config);
    let mut out = String::new();
    write_mermaid_base_css_prefix(
        &mut out,
        id,
        MermaidBaseCss {
            font_family: &font,
            font_size_css: &font_size_css,
            normal_edge_stroke_width_css: &normal_edge_stroke_width_css,
            text_color: &text_color,
            line_color: &line_color,
            error_bkg: &error_bkg,
            error_text: &error_text,
        },
    );
    let _ = write!(
        &mut out,
        r#"#{} .entityBox{{fill:{};stroke:{};}}"#,
        id, main_bkg, node_border
    );
    let _ = write!(
        &mut out,
        r#"#{} .relationshipLabelBox{{fill:{};opacity:0.7;background-color:{};}}"#,
        id, tertiary_color, tertiary_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .relationshipLabelBox rect{{opacity:0.5;}}"#,
        id
    );
    let _ = write!(
        &mut out,
        r#"#{} .labelBkg{{background-color:{};}}"#,
        id, label_background
    );
    let _ = write!(
        &mut out,
        r#"#{} .edgeLabel{{background-color:{};}}#{} .edgeLabel .label rect{{fill:{};}}#{} .edgeLabel .label text{{fill:{};}}#{} .edgeLabel .label{{fill:{};font-size:14px;}}"#,
        id, edge_label_background, id, edge_label_background, id, text_color, id, node_border
    );
    let _ = write!(
        &mut out,
        r#"#{} .label{{font-family:{};color:{};}}"#,
        id, font, node_text_color
    );
    // Mermaid duplicates `.edge-pattern-dashed` (base rule earlier sets dasharray:3).
    let _ = write!(
        &mut out,
        r#"#{} .edge-pattern-dashed{{stroke-dasharray:8,8;}}"#,
        id
    );
    let _ = write!(
        &mut out,
        r#"#{} .node rect,#{} .node circle,#{} .node ellipse,#{} .node polygon{{fill:{};stroke:{};stroke-width:{};}}"#,
        id, id, id, id, main_bkg, node_border, stroke_width
    );
    let _ = write!(
        &mut out,
        r#"#{} .relationshipLine{{stroke:{};stroke-width:{};fill:none;}}"#,
        id, line_color, stroke_width
    );
    // Mermaid duplicates `.marker` (base rule earlier sets fill/stroke from the line color).
    let _ = write!(
        &mut out,
        r#"#{} .marker{{fill:none!important;stroke:{}!important;stroke-width:1;}}"#,
        id, line_color
    );
    let _ = write!(
        &mut out,
        r#"#{} [data-look=neo].labelBkg{{background-color:{};}}"#,
        id,
        css_rgba_fade(&tertiary_color, 0.5)?
    );
    let _ = write!(
        &mut out,
        r#"#{} .cluster rect{{fill:{};stroke:{};stroke-width:1px;}}#{} .cluster text{{fill:{};}}#{} .cluster-label text{{fill:{};}}"#,
        id, cluster_bkg, cluster_border, id, title_color, id, title_color
    );
    write_mermaid_common_neo_css(&mut out, id, effective_config);
    out.push_str(&mermaid_base_css_root_rule(
        id,
        &crate::config::config_root_font_family_css(effective_config),
    ));
    Ok(out)
}

#[cfg(feature = "diagram-pie")]
fn pie_theme_option(
    effective_config: &serde_json::Value,
    key: &str,
    default_value: &str,
) -> String {
    SvgTheme::new(effective_config).css_value(key, default_value)
}

#[cfg(feature = "diagram-pie")]
pub(super) struct PieCss {
    info: InfoCssValues,
    pie_stroke_color: String,
    pie_stroke_width: String,
    pie_opacity: String,
    pie_outer_stroke_color: String,
    pie_outer_stroke_width: String,
    pie_title_text_size: String,
    pie_title_text_color: String,
    pie_section_text_size: String,
    pie_section_text_color: String,
    pie_legend_text_size: String,
    pie_legend_text_color: String,
}

#[cfg(feature = "diagram-pie")]
impl PieCss {
    pub(super) fn new(effective_config: &serde_json::Value) -> Self {
        let info = InfoCssValues::new(effective_config, InfoCssFontSizeSource::ThemeThenTopLevel);
        let theme = SvgTheme::new(effective_config);
        let task_text_dark_color = theme.color("taskTextDarkColor", "black");
        let pie_title_text_color = theme.color("pieTitleTextColor", task_text_dark_color.as_str());
        let pie_section_text_color = theme.color("pieSectionTextColor", info.text_color.as_str());
        let pie_legend_text_color =
            theme.color("pieLegendTextColor", task_text_dark_color.as_str());

        Self {
            info,
            pie_stroke_color: pie_theme_option(effective_config, "pieStrokeColor", "black"),
            pie_stroke_width: pie_theme_option(effective_config, "pieStrokeWidth", "2px"),
            pie_opacity: pie_theme_option(effective_config, "pieOpacity", "0.7"),
            pie_outer_stroke_color: pie_theme_option(
                effective_config,
                "pieOuterStrokeColor",
                "black",
            ),
            pie_outer_stroke_width: pie_theme_option(
                effective_config,
                "pieOuterStrokeWidth",
                "2px",
            ),
            pie_title_text_size: pie_theme_option(effective_config, "pieTitleTextSize", "25px"),
            pie_title_text_color,
            pie_section_text_size: pie_theme_option(effective_config, "pieSectionTextSize", "17px"),
            pie_section_text_color,
            pie_legend_text_size: pie_theme_option(effective_config, "pieLegendTextSize", "17px"),
            pie_legend_text_color,
        }
    }

    pub(super) fn write_for_normalized_id<I>(
        &self,
        out: &mut dyn std::fmt::Write,
        diagram_id: I,
    ) -> std::fmt::Result
    where
        I: Copy + std::fmt::Display,
    {
        // Family rendering receives a normalized diagram ID whose grammar is already restricted
        // to CSS/XML-safe ASCII. Writing the wrapper directly keeps every selector occurrence on
        // the operation-owned projection path.
        self.info.write_prefix(out, diagram_id)?;
        write!(
            out,
            r#"#{} .pieCircle{{stroke:{};stroke-width:{};opacity:{};}}#{} .pieCircle.highlighted{{scale:1.05;opacity:1;}}#{} .pieCircle.highlightedOnHover:hover{{transition-duration:250ms;scale:1.05;opacity:1;}}#{} .pieOuterCircle{{stroke:{};stroke-width:{};fill:none;}}#{} .pieTitleText{{text-anchor:middle;font-size:{};fill:{};font-family:{};}}#{} .slice{{font-family:{};fill:{};font-size:{};}}#{} .legend text{{fill:{};font-family:{};font-size:{};}}"#,
            diagram_id,
            self.pie_stroke_color,
            self.pie_stroke_width,
            self.pie_opacity,
            diagram_id,
            diagram_id,
            diagram_id,
            self.pie_outer_stroke_color,
            self.pie_outer_stroke_width,
            diagram_id,
            self.pie_title_text_size,
            self.pie_title_text_color,
            self.info.font_family,
            diagram_id,
            self.info.font_family,
            self.pie_section_text_color,
            self.pie_section_text_size,
            diagram_id,
            self.pie_legend_text_color,
            self.info.font_family,
            self.pie_legend_text_size
        )?;
        self.info.write_root(out, diagram_id)
    }
}

#[cfg(feature = "diagram-sankey")]
pub(super) fn sankey_css<I>(diagram_id: I, effective_config: &serde_json::Value) -> String
where
    I: Copy + std::fmt::Display,
{
    // Mermaid's sankey diagram uses the same base CSS as "info-like" diagrams, then appends
    // `sankey/styles.js` rules. Keep `:root` last to match upstream SVG baselines.
    let id = diagram_id;
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let mut out = parts.css_prefix;
    let label_background = config_string(effective_config, &["themeVariables", "mainBkg"])
        .or_else(|| config_string(effective_config, &["themeVariables", "background"]))
        .unwrap_or_else(|| "#fff".to_string());
    let _ = write!(
        &mut out,
        r#"#{} .label{{font-family:{};}}#{} .node-labels{{font-family:{};}}#{} .sankey-label-bg{{stroke:{};stroke-width:4px;stroke-linejoin:round;paint-order:stroke;}}#{} .sankey-label-fg{{fill:{};}}#{} .node rect{{shape-rendering:crispEdges;}}#{} .link{{fill:none;stroke-opacity:0.5;mix-blend-mode:multiply;}}"#,
        id,
        parts.font_family,
        id,
        parts.font_family,
        id,
        label_background,
        id,
        parts.text_color,
        id,
        id
    );
    out.push_str(&parts.root_rule);
    out
}

#[cfg(feature = "diagram-treemap")]
pub(super) fn treemap_css<I>(diagram_id: I, effective_config: &serde_json::Value) -> Result<String>
where
    I: Copy + std::fmt::Display,
{
    // Mermaid's treemap styles merge `treemap.*` options with theme title/text colors. Keep
    // `:root` last to match upstream SVG baselines.
    let id = diagram_id;
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let theme = PresentationTheme::new(effective_config).treemap()?;
    let mut out = parts.css_prefix;

    let _ = write!(
        &mut out,
        r#"#{} .treemapNode.section{{stroke:{};stroke-width:{};fill:{};}}#{} .treemapNode.leaf{{stroke:{};stroke-width:{};fill:{};}}#{} .treemapLabel{{fill:{};font-size:{};}}#{} .treemapValue{{fill:{};font-size:{};}}#{} .treemapTitle{{fill:{};font-size:{};}}"#,
        id,
        theme.section_stroke_color,
        theme.section_stroke_width,
        theme.section_fill_color,
        id,
        theme.leaf_stroke_color,
        theme.leaf_stroke_width,
        theme.leaf_fill_color,
        id,
        theme.label_color,
        theme.label_font_size,
        id,
        theme.value_color,
        theme.value_font_size,
        id,
        theme.title_color,
        theme.title_font_size
    );
    out.push_str(&parts.root_rule);
    Ok(out)
}

#[cfg(feature = "diagram-xychart")]
pub(super) fn push_xychart_css<I>(out: &mut String, diagram_id: I)
where
    I: Copy + std::fmt::Display,
{
    // Mermaid does not ship dedicated XYChart styles at 11.12.2 (it relies on theme variables and
    // inline attributes). Keep the shared base stylesheet for consistency with upstream SVG
    // baselines. The compare tooling ignores `<style>` content in parity mode.
    info_css_into(out, diagram_id);
}

#[cfg(feature = "diagram-gantt")]
pub(super) fn gantt_css<I>(diagram_id: I, effective_config: &serde_json::Value) -> String
where
    I: Copy + std::fmt::Display,
{
    let id = diagram_id;
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let theme = PresentationTheme::new(effective_config).gantt();
    let mut out = parts.css_prefix;
    let font = &theme.font_family;
    let text_color = &theme.text_color;
    let exclude_bkg_color = &theme.exclude_bkg_color;
    let section_bkg_color = &theme.section_bkg_color;
    let section_bkg_color2 = &theme.section_bkg_color2;
    let alt_section_bkg_color = &theme.alt_section_bkg_color;
    let title_color = &theme.title_color;
    let grid_color = &theme.grid_color;
    let today_line_color = &theme.today_line_color;
    let task_text_dark_color = &theme.task_text_dark_color;
    let task_text_clickable_color = &theme.task_text_clickable_color;
    let task_text_color = &theme.task_text_color;
    let task_bkg_color = &theme.task_bkg_color;
    let task_border_color = &theme.task_border_color;
    let task_text_outside_color = &theme.task_text_outside_color;
    let active_task_bkg_color = &theme.active_task_bkg_color;
    let active_task_border_color = &theme.active_task_border_color;
    let done_task_border_color = &theme.done_task_border_color;
    let done_task_bkg_color = &theme.done_task_bkg_color;
    let crit_border_color = &theme.crit_border_color;
    let crit_bkg_color = &theme.crit_bkg_color;
    let vert_line_color = &theme.vert_line_color;
    let title_text_color = &theme.title_text_color;

    fn push_outside_done_text_rules<I>(out: &mut String, id: I, class_prefix: &str, color: &str)
    where
        I: Copy + std::fmt::Display,
    {
        let mut first = true;
        for i in 0..4 {
            for side in ["Left", "Right"] {
                if first {
                    first = false;
                } else {
                    out.push(',');
                }
                let _ = write!(
                    out,
                    "#{} .{}{}.taskTextOutside{}",
                    id, class_prefix, i, side
                );
            }
        }
        let _ = write!(out, "{{fill:{}!important;}}", color);
    }

    let _ = write!(
        &mut out,
        r#"#{} .mermaid-main-font{{font-family:{};}}"#,
        id, font
    );
    let _ = write!(
        &mut out,
        r#"#{} .exclude-range{{fill:{};}}"#,
        id, exclude_bkg_color
    );
    let _ = write!(&mut out, r#"#{} .section{{stroke:none;opacity:0.2;}}"#, id);
    let _ = write!(
        &mut out,
        r#"#{} .section0{{fill:{};}}"#,
        id, section_bkg_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .section2{{fill:{};}}"#,
        id, section_bkg_color2
    );
    let _ = write!(
        &mut out,
        r#"#{} .section1,#{} .section3{{fill:{};opacity:0.2;}}"#,
        id, id, alt_section_bkg_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .sectionTitle0{{fill:{};}}#{} .sectionTitle1{{fill:{};}}#{} .sectionTitle2{{fill:{};}}#{} .sectionTitle3{{fill:{};}}"#,
        id, title_color, id, title_color, id, title_color, id, title_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .sectionTitle{{text-anchor:start;font-family:{};}}"#,
        id, font
    );
    let _ = write!(
        &mut out,
        r#"#{} .grid .tick{{stroke:{};opacity:0.8;shape-rendering:crispEdges;}}#{} .grid .tick text{{font-family:{};fill:{};}}#{} .grid path{{stroke-width:0;}}"#,
        id, grid_color, id, font, text_color, id
    );
    let _ = write!(
        &mut out,
        r#"#{} .today{{fill:none;stroke:{};stroke-width:2px;}}"#,
        id, today_line_color
    );
    let _ = write!(&mut out, r#"#{} .task{{stroke-width:2;}}"#, id);
    let _ = write!(
        &mut out,
        r#"#{} .taskText{{text-anchor:middle;font-family:{};}}#{} .taskTextOutsideRight{{fill:{};text-anchor:start;font-family:{};}}#{} .taskTextOutsideLeft{{fill:{};text-anchor:end;}}"#,
        id, font, id, task_text_dark_color, font, id, task_text_dark_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .task.clickable{{cursor:pointer;}}#{} .taskText.clickable{{cursor:pointer;fill:{}!important;font-weight:bold;}}#{} .taskTextOutsideLeft.clickable{{cursor:pointer;fill:{}!important;font-weight:bold;}}#{} .taskTextOutsideRight.clickable{{cursor:pointer;fill:{}!important;font-weight:bold;}}"#,
        id,
        id,
        task_text_clickable_color,
        id,
        task_text_clickable_color,
        id,
        task_text_clickable_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .taskText0,#{} .taskText1,#{} .taskText2,#{} .taskText3{{fill:{};}}"#,
        id, id, id, id, task_text_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .task0,#{} .task1,#{} .task2,#{} .task3{{fill:{};stroke:{};}}"#,
        id, id, id, id, task_bkg_color, task_border_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .taskTextOutside0,#{} .taskTextOutside2{{fill:{};}}#{} .taskTextOutside1,#{} .taskTextOutside3{{fill:{};}}"#,
        id, id, task_text_outside_color, id, id, task_text_outside_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .active0,#{} .active1,#{} .active2,#{} .active3{{fill:{};stroke:{};}}"#,
        id, id, id, id, active_task_bkg_color, active_task_border_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .activeText0,#{} .activeText1,#{} .activeText2,#{} .activeText3{{fill:{}!important;}}"#,
        id, id, id, id, task_text_dark_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .done0,#{} .done1,#{} .done2,#{} .done3{{stroke:{};fill:{};stroke-width:2;}}"#,
        id, id, id, id, done_task_border_color, done_task_bkg_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .doneText0,#{} .doneText1,#{} .doneText2,#{} .doneText3{{fill:{}!important;}}"#,
        id, id, id, id, task_text_dark_color
    );
    push_outside_done_text_rules(&mut out, id, "doneText", task_text_outside_color);
    let _ = write!(
        &mut out,
        r#"#{} .crit0,#{} .crit1,#{} .crit2,#{} .crit3{{stroke:{};fill:{};stroke-width:2;}}"#,
        id, id, id, id, crit_border_color, crit_bkg_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .activeCrit0,#{} .activeCrit1,#{} .activeCrit2,#{} .activeCrit3{{stroke:{};fill:{};stroke-width:2;}}"#,
        id, id, id, id, crit_border_color, active_task_bkg_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .doneCrit0,#{} .doneCrit1,#{} .doneCrit2,#{} .doneCrit3{{stroke:{};fill:{};stroke-width:2;cursor:pointer;shape-rendering:crispEdges;}}"#,
        id, id, id, id, crit_border_color, done_task_bkg_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .milestone{{transform:rotate(45deg) scale(0.8,0.8);}}#{} .milestoneText{{font-style:italic;}}"#,
        id, id
    );
    let _ = write!(
        &mut out,
        r#"#{} .doneCritText0,#{} .doneCritText1,#{} .doneCritText2,#{} .doneCritText3{{fill:{}!important;}}"#,
        id, id, id, id, task_text_dark_color
    );
    push_outside_done_text_rules(&mut out, id, "doneCritText", task_text_outside_color);
    let _ = write!(
        &mut out,
        r#"#{} .vert{{stroke:{};}}#{} .vertText{{font-size:15px;text-anchor:middle;fill:{}!important;}}"#,
        id, vert_line_color, id, vert_line_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .activeCritText0,#{} .activeCritText1,#{} .activeCritText2,#{} .activeCritText3{{fill:{}!important;}}"#,
        id, id, id, id, task_text_dark_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .titleText{{text-anchor:middle;font-size:18px;fill:{};font-family:{};}}"#,
        id, title_text_color, font
    );

    out.push_str(&parts.root_rule);
    out
}

#[cfg(all(test, feature = "all-diagrams"))]
mod tests;
