use std::fmt::Write as _;

use crate::model::Bounds;
use crate::svg::icon_registry::{IconIdScope, IconIdScopePrefix};
use crate::text::TextMeasurer;

use super::super::{SvgDiagramId, escape_xml_into, fmt};
use super::ArchitectureEmitCheckpoints;
use super::foreign_object::{
    escape_xml_ampersands_preserving_xml_entities, normalize_xhtml_fragment_for_foreign_object,
};
use super::geometry::{GroupRect, extend_bounds};
use super::icons::{
    arch_icon_needs_id_scope, write_arch_icon_svg, write_arch_icon_svg_with_registry,
};
use super::labels::{
    svg_line_formatted_bbox_width_px, wrap_svg_words_to_lines, write_architecture_service_title,
    write_svg_text_lines,
};
use super::model::ArchitectureModelAccess;
use super::settings::ArchitectureRenderSettings;

pub(super) struct ArchitectureNodeRenderContext<'a, M: ArchitectureModelAccess> {
    pub(super) out: &'a mut String,
    pub(super) diagram_id: SvgDiagramId<'a>,
    pub(super) model: &'a M,
    pub(super) node_xy: &'a rustc_hash::FxHashMap<&'a str, (f64, f64)>,
    pub(super) settings: &'a ArchitectureRenderSettings,
    pub(super) text_measurer: &'a dyn TextMeasurer,
    pub(super) sanitize_config: &'a merman_core::MermaidConfig,
    pub(super) icon_registry: Option<&'a crate::svg::IconRegistry>,
    pub(super) work_meter: &'a crate::resources::OperationWorkMeter,
    pub(super) icon_scope_root: Option<IconIdScopePrefix>,
    pub(super) service_icon_scope_prefix: Option<IconIdScopePrefix>,
    pub(super) group_icon_scope_prefix: Option<IconIdScopePrefix>,
    pub(super) content_bounds: &'a mut Option<Bounds>,
    pub(super) checkpoints: ArchitectureEmitCheckpoints<'a>,
}

fn architecture_icon_scope(
    root: &mut Option<IconIdScopePrefix>,
    branch: &mut Option<IconIdScopePrefix>,
    diagram_id: SvgDiagramId<'_>,
    branch_name: &'static str,
    local_id: &str,
    work_meter: &crate::resources::OperationWorkMeter,
) -> crate::Result<IconIdScope> {
    let root_prefix = match *root {
        Some(prefix) => prefix,
        None => {
            let prefix = IconIdScopePrefix::from_parts(&[diagram_id.semantic_str()], work_meter)?;
            *root = Some(prefix);
            prefix
        }
    };
    let branch_prefix = match *branch {
        Some(prefix) => prefix,
        None => {
            let prefix = root_prefix.extend_parts(&[branch_name], work_meter)?;
            *branch = Some(prefix);
            prefix
        }
    };
    branch_prefix.scope_parts(&[local_id, "-icon"], work_meter)
}

fn write_diagram_service_id(out: &mut String, diagram_id: SvgDiagramId<'_>, service_id: &str) {
    let _ = write!(out, "{diagram_id}");
    out.push_str("-service-");
    escape_xml_into(out, service_id);
}

fn write_diagram_node_id(out: &mut String, diagram_id: SvgDiagramId<'_>, node_id: &str) {
    let _ = write!(out, "{diagram_id}");
    out.push_str("-node-");
    escape_xml_into(out, node_id);
}

pub(super) fn push_architecture_services_and_junctions<M: ArchitectureModelAccess>(
    ctx: &mut ArchitectureNodeRenderContext<'_, M>,
) -> crate::Result<()> {
    let checkpoints = ctx.checkpoints;
    checkpoints.checkpoint()?;
    let out = &mut *ctx.out;
    let diagram_id = ctx.diagram_id;
    let model = ctx.model;
    let node_xy = ctx.node_xy;
    let settings = ctx.settings;
    let text_measurer = ctx.text_measurer;
    let sanitize_config = ctx.sanitize_config;
    let icon_registry = ctx.icon_registry;
    let work_meter = ctx.work_meter;
    let icon_scope_root = &mut ctx.icon_scope_root;
    let service_icon_scope_prefix = &mut ctx.service_icon_scope_prefix;
    let service_count = model.services().count();
    let junction_count = model.junctions().count();

    if service_count == 0 && junction_count == 0 {
        out.push_str(r#"<g class="architecture-services"/>"#);
    } else {
        out.push_str(r#"<g class="architecture-services">"#);
        for svc in model.services() {
            checkpoints.checkpoint()?;
            let (x, y) = node_xy.get(svc.id).copied().unwrap_or((0.0, 0.0));

            out.push_str(r#"<g id=""#);
            write_diagram_service_id(out, diagram_id, svc.id);
            checkpoints.checkpoint()?;
            let _ = write!(
                out,
                r#"" class="architecture-service" transform="translate({x},{y})">"#,
                x = fmt(x),
                y = fmt(y)
            );

            if let Some(title) = svc.title.map(str::trim).filter(|t| !t.is_empty()) {
                // Mermaid uses `width = iconSize * 1.5` for service titles.
                write_architecture_service_title(
                    out,
                    title,
                    settings.icon_size_px,
                    settings.icon_size_px * 1.5,
                    text_measurer,
                    &settings.text_style,
                );
            }

            out.push_str("<g>");
            match (svc.icon, svc.icon_text) {
                (Some(icon), _) => {
                    out.push_str("<g>");
                    if arch_icon_needs_id_scope(icon, icon_registry.is_some()) {
                        let id_scope = architecture_icon_scope(
                            icon_scope_root,
                            service_icon_scope_prefix,
                            diagram_id,
                            "-service-",
                            svc.id,
                            work_meter,
                        )?;
                        write_arch_icon_svg_with_registry(
                            out,
                            icon,
                            settings.icon_size_px,
                            icon_registry,
                            id_scope,
                            sanitize_config,
                            work_meter,
                        )?;
                    } else {
                        write_arch_icon_svg(out, icon, settings.icon_size_px, None)?;
                    }
                    out.push_str("</g>");
                }
                (None, Some(icon_text)) => {
                    out.push_str("<g>");
                    write_arch_icon_svg(out, "blank", settings.icon_size_px, None)?;
                    out.push_str("</g>");

                    // Mermaid computes `iconText` clamp from the DOM `font-size` applied to the
                    // foreignObject content. For Architecture this tracks `architecture.fontSize`,
                    // not the separate SVG text measurement/font-size path used for service/group
                    // labels.
                    let line_clamp = ((settings.icon_size_px - 2.0) / settings.arch_font_size_px)
                        .floor()
                        .max(1.0) as i64;
                    let sanitized =
                        merman_core::sanitize::sanitize_text(icon_text.trim(), sanitize_config);
                    let sanitized = normalize_xhtml_fragment_for_foreign_object(&sanitized);
                    let sanitized = escape_xml_ampersands_preserving_xml_entities(&sanitized);
                    let _ = write!(
                        out,
                        r#"<g><foreignObject width="{w}" height="{h}"><div class="node-icon-text" style="height: {h}px;" xmlns="http://www.w3.org/1999/xhtml"><div style="-webkit-line-clamp: {clamp};">{text}</div></div></foreignObject></g>"#,
                        w = fmt(settings.icon_size_px),
                        h = fmt(settings.icon_size_px),
                        clamp = line_clamp,
                        text = sanitized
                    );
                }
                (None, None) => {
                    out.push_str(r#"<path class="node-bkg" id=""#);
                    write_diagram_node_id(out, diagram_id, svc.id);
                    checkpoints.checkpoint()?;
                    let _ = write!(
                        out,
                        r#"" d="M0,{s} V5 Q0,0 5,0 H{inner_s} Q{s},0 {s},5 V{s} Z"/>"#,
                        s = fmt(settings.icon_size_px),
                        inner_s = fmt(settings.icon_size_px - 5.0)
                    );
                }
            }
            out.push_str("</g>");

            out.push_str("</g>");
        }

        for junction in model.junctions() {
            checkpoints.checkpoint()?;
            let (x, y) = node_xy.get(junction.id).copied().unwrap_or((0.0, 0.0));

            let _ = write!(
                out,
                r#"<g class="architecture-junction" transform="translate({x},{y})"><g><rect id=""#,
                x = fmt(x),
                y = fmt(y),
            );
            write_diagram_node_id(out, diagram_id, junction.id);
            checkpoints.checkpoint()?;
            let _ = write!(
                out,
                r#"" fill-opacity="0" width="{s}" height="{s}"/></g></g>"#,
                s = fmt(settings.icon_size_px)
            );
        }
        out.push_str("</g>");
    }
    Ok(())
}

pub(super) fn push_architecture_groups<'a, M: ArchitectureModelAccess>(
    ctx: &mut ArchitectureNodeRenderContext<'a, M>,
    group_rects: &[GroupRect<'a>],
) -> crate::Result<()> {
    let checkpoints = ctx.checkpoints;
    checkpoints.checkpoint()?;
    let out = &mut *ctx.out;
    let settings = ctx.settings;
    let text_measurer = ctx.text_measurer;
    let content_bounds = &mut *ctx.content_bounds;
    let icon_registry = ctx.icon_registry;
    let work_meter = ctx.work_meter;
    let icon_scope_root = &mut ctx.icon_scope_root;
    let group_icon_scope_prefix = &mut ctx.group_icon_scope_prefix;

    if ctx.model.groups_len() == 0 {
        out.push_str(r#"<g class="architecture-groups"/>"#);
    } else {
        out.push_str(r#"<g class="architecture-groups">"#);

        for grp in group_rects {
            checkpoints.checkpoint()?;
            let x = grp.x;
            let y = grp.y;
            let w = grp.w;
            let h = grp.h;
            let group_icon_size_px = settings.padding_px * 0.75;
            let x1 = x - settings.half_icon;
            let y1 = y - settings.half_icon;

            out.push_str(r#"<rect id=""#);
            let _ = write!(out, "{}", ctx.diagram_id);
            out.push_str("-group-");
            escape_xml_into(out, grp.id);
            checkpoints.checkpoint()?;
            let _ = write!(
                out,
                r#"" x="{x}" y="{y}" width="{w}" height="{h}" class="node-bkg"/>"#,
                x = fmt(x),
                y = fmt(y),
                w = fmt(w.max(1.0)),
                h = fmt(h.max(1.0))
            );

            out.push_str("<g>");

            let mut shifted_x1 = x1;
            let mut shifted_y1 = y1;
            if let Some(icon) = grp.icon.map(str::trim).filter(|t| !t.is_empty()) {
                let _ = write!(
                    out,
                    r#"<g transform="translate({x}, {y})"><g>"#,
                    x = fmt(shifted_x1 + settings.half_icon + 1.0),
                    y = fmt(shifted_y1 + settings.half_icon + 1.0)
                );
                if arch_icon_needs_id_scope(icon, icon_registry.is_some()) {
                    let id_scope = architecture_icon_scope(
                        icon_scope_root,
                        group_icon_scope_prefix,
                        ctx.diagram_id,
                        "-group-",
                        grp.id,
                        work_meter,
                    )?;
                    write_arch_icon_svg_with_registry(
                        out,
                        icon,
                        group_icon_size_px,
                        icon_registry,
                        id_scope,
                        ctx.sanitize_config,
                        work_meter,
                    )?;
                } else {
                    write_arch_icon_svg(out, icon, group_icon_size_px, None)?;
                }
                out.push_str("</g></g>");
                shifted_x1 += group_icon_size_px;
                // Mermaid uses `architecture.fontSize` for this alignment tweak (not the global SVG
                // font size used for label rendering).
                shifted_y1 += settings.arch_font_size_px / 2.0 - 3.0;
            }

            if let Some(title) = grp.title.map(str::trim).filter(|t| !t.is_empty()) {
                let lines = wrap_svg_words_to_lines(title, w, text_measurer, &settings.text_style);

                // Group titles are SVG `<text>` (no explicit bbox geometry), so our SVG bbox pass
                // cannot "see" their extents. Measure the emitted outer tspan DOM primitive so
                // `setupGraphViewbox(svg.getBBox() + padding)` matches upstream in parity-root.
                let title_bbox_w = lines
                    .iter()
                    .map(|line| {
                        svg_line_formatted_bbox_width_px(line, text_measurer, &settings.text_style)
                    })
                    .fold(0.0_f64, f64::max);
                if title_bbox_w.is_finite() && title_bbox_w > 0.0 {
                    let title_x = shifted_x1 + settings.half_icon + 4.0;
                    // Keep Y extents within the group rect; we only need this to expand X.
                    let title_bounds = Bounds {
                        min_x: title_x,
                        min_y: y,
                        max_x: title_x + title_bbox_w,
                        max_y: y + h,
                    };
                    extend_bounds(content_bounds, title_bounds);
                }
                let _ = write!(
                    out,
                    r#"<g dy="1em" alignment-baseline="middle" dominant-baseline="start" text-anchor="start" transform="translate({x}, {y})"><g><rect class="background" style="stroke: none"/>"#,
                    x = fmt(shifted_x1 + settings.half_icon + 4.0),
                    y = fmt(shifted_y1 + settings.half_icon + 2.0)
                );
                write_svg_text_lines(out, &lines);
                out.push_str("</g></g>");
            }

            out.push_str("</g>");
        }

        out.push_str("</g>");
    }
    Ok(())
}
