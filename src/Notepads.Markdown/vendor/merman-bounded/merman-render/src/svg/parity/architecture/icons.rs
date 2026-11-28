use crate::svg::icon_registry::{IconIdScope, scope_svg_internal_ids};

use super::super::fmt_into;
use std::borrow::Cow;

pub(super) fn arch_icon_body(name: &str) -> &'static str {
    // Copied from Mermaid@11.12.2 `packages/mermaid/src/diagrams/architecture/architectureIcons.ts`.
    //
    // Note: SVG DOM parity checks ignore `style` attributes, but we keep the upstream bodies as-is
    // to preserve element structure and any stable non-style attributes (e.g. `id`).
    match name {
        "database" => {
            r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><path id="b" data-name="4" d="m20,57.86c0,3.94,8.95,7.14,20,7.14s20-3.2,20-7.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path id="c" data-name="3" d="m20,45.95c0,3.94,8.95,7.14,20,7.14s20-3.2,20-7.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path id="d" data-name="2" d="m20,34.05c0,3.94,8.95,7.14,20,7.14s20-3.2,20-7.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse id="e" data-name="1" cx="40" cy="22.14" rx="20" ry="7.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="20" y1="57.86" x2="20" y2="22.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="60" y1="57.86" x2="60" y2="22.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/></g>"#
        }
        "server" => {
            r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><rect x="17.5" y="17.5" width="45" height="45" rx="2" ry="2" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="17.5" y1="32.5" x2="62.5" y2="32.5" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="17.5" y1="47.5" x2="62.5" y2="47.5" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><g><path d="m56.25,25c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: #fff; stroke-width: 0px;"/><path d="m56.25,25c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: none; stroke: #fff; stroke-miterlimit: 10;"/></g><g><path d="m56.25,40c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: #fff; stroke-width: 0px;"/><path d="m56.25,40c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: none; stroke: #fff; stroke-miterlimit: 10;"/></g><g><path d="m56.25,55c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: #fff; stroke-width: 0px;"/><path d="m56.25,55c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: none; stroke: #fff; stroke-miterlimit: 10;"/></g><g><circle cx="32.5" cy="25" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="27.5" cy="25" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="22.5" cy="25" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/></g><g><circle cx="32.5" cy="40" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="27.5" cy="40" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="22.5" cy="40" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/></g><g><circle cx="32.5" cy="55" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="27.5" cy="55" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="22.5" cy="55" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/></g></g>"#
        }
        "disk" => {
            r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><rect x="20" y="15" width="40" height="50" rx="1" ry="1" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="24" cy="19.17" rx=".8" ry=".83" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="56" cy="19.17" rx=".8" ry=".83" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="24" cy="60.83" rx=".8" ry=".83" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="56" cy="60.83" rx=".8" ry=".83" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="40" cy="33.75" rx="14" ry="14.58" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="40" cy="33.75" rx="4" ry="4.17" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path d="m37.51,42.52l-4.83,13.22c-.26.71-1.1,1.02-1.76.64l-4.18-2.42c-.66-.38-.81-1.26-.33-1.84l9.01-10.8c.88-1.05,2.56-.08,2.09,1.2Z" style="fill: #fff; stroke-width: 0px;"/></g>"#
        }
        "internet" => {
            r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><circle cx="40" cy="40" r="22.5" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="40" y1="17.5" x2="40" y2="62.5" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="17.5" y1="40" x2="62.5" y2="40" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path d="m39.99,17.51c-15.28,11.1-15.28,33.88,0,44.98" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path d="m40.01,17.51c15.28,11.1,15.28,33.88,0,44.98" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="19.75" y1="30.1" x2="60.25" y2="30.1" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="19.75" y1="49.9" x2="60.25" y2="49.9" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/></g>"#
        }
        "cloud" => {
            r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><path d="m65,47.5c0,2.76-2.24,5-5,5H20c-2.76,0-5-2.24-5-5,0-1.87,1.03-3.51,2.56-4.36-.04-.21-.06-.42-.06-.64,0-2.6,2.48-4.74,5.65-4.97,1.65-4.51,6.34-7.76,11.85-7.76.86,0,1.69.08,2.5.23,2.09-1.57,4.69-2.5,7.5-2.5,6.1,0,11.19,4.38,12.28,10.17,2.14.56,3.72,2.51,3.72,4.83,0,.03,0,.07-.01.1,2.29.46,4.01,2.48,4.01,4.9Z" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/></g>"#
        }
        "unknown" => {
            r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><text transform="translate(21.16 64.67)" style="fill: #fff; font-family: ArialMT, Arial; font-size: 67.75px;"><tspan x="0" y="0">?</tspan></text></g>"#
        }
        "blank" => {
            r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/></g>"#
        }
        _ => arch_icon_body("unknown"),
    }
}

fn arch_icon_body_has_internal_ids(name: &str) -> bool {
    matches!(name, "database")
}

pub(super) fn arch_icon_needs_id_scope(icon_name: &str, has_registry: bool) -> bool {
    has_registry || arch_icon_body_has_internal_ids(icon_name)
}

pub(super) fn write_arch_icon_svg(
    out: &mut String,
    icon_name: &str,
    icon_size_px: f64,
    id_scope: Option<IconIdScope>,
) -> crate::Result<()> {
    let body = arch_icon_body(icon_name);
    let body = if arch_icon_body_has_internal_ids(icon_name) {
        let id_scope = id_scope.ok_or_else(|| {
            crate::Error::icon_processing("built-in icon with internal IDs is missing its scope")
        })?;
        Cow::Owned(scope_svg_internal_ids(body, id_scope)?)
    } else {
        Cow::Borrowed(body)
    };
    out.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" width=""#);
    fmt_into(out, icon_size_px);
    out.push_str(r#"" height=""#);
    fmt_into(out, icon_size_px);
    out.push_str(r#"" viewBox="0 0 80 80">"#);
    out.push_str(body.as_ref());
    out.push_str("</svg>");
    Ok(())
}

pub(super) fn write_arch_icon_svg_with_registry(
    out: &mut String,
    icon_name: &str,
    icon_size_px: f64,
    icon_registry: Option<&crate::svg::IconRegistry>,
    id_scope: IconIdScope,
    effective_config: &merman_core::MermaidConfig,
    work_meter: &crate::resources::OperationWorkMeter,
) -> crate::Result<()> {
    let svg = match icon_registry {
        Some(registry) => registry.render_icon(crate::svg::icon_registry::IconRenderRequest {
            icon_name,
            width_px: icon_size_px,
            height_px: icon_size_px,
            fallback_prefix: Some("mermaid-architecture"),
            extra_class: None,
            id_scope,
            effective_config,
            work_meter,
        })?,
        None => None,
    };
    if let Some(svg) = svg {
        out.push_str(&svg);
    } else {
        write_arch_icon_svg(out, icon_name, icon_size_px, Some(id_scope))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use crate::svg::icon_registry::icon_id_scope_for_test;
    use crate::svg::{IconPack, IconRegistry};

    #[test]
    fn write_arch_icon_svg_preserves_builtin_id_scoping_behavior() {
        let mut server = String::new();
        write_arch_icon_svg(&mut server, "server", 80.0, None).unwrap();
        assert!(server.starts_with(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80" viewBox="0 0 80 80">"#
        ));
        assert!(server.contains(r#"<rect x="17.5" y="17.5""#));
        assert!(!server.contains("IconifyId"), "{server}");

        let mut database = String::new();
        write_arch_icon_svg(
            &mut database,
            "database",
            80.0,
            Some(icon_id_scope_for_test("scope-b")),
        )
        .unwrap();
        assert!(database.contains(r#"id="IconifyId"#), "{database}");
        assert!(!database.contains(r#"id="b""#), "{database}");
    }

    #[test]
    fn arch_icon_scope_requirement_tracks_registry_and_internal_ids() {
        assert!(arch_icon_needs_id_scope("database", false));
        assert!(arch_icon_needs_id_scope("server", true));
        assert!(!arch_icon_needs_id_scope("server", false));
        assert!(!arch_icon_needs_id_scope("blank", false));
    }

    #[test]
    fn write_arch_icon_svg_with_registry_scopes_internal_ids_per_architecture_node() {
        let pack = br##"{
            "prefix":"test",
            "icons":{
                "clip":{
                    "body":"<defs><clipPath id=\"clip\"><path id=\"shape\" d=\"M0 0H16V16H0z\"/></clipPath></defs><path data-icon=\"fixture\" clip-path=\"url(#clip)\" d=\"M0 0H16V16H0z\"/><use href=\"#shape\" xlink:href=\"#shape\"/>"
                }
            }
        }"##;
        let registry = IconRegistry::from_packs([IconPack::new(pack)]).unwrap();
        let effective_config =
            merman_core::MermaidConfig::from_value(serde_json::json!({ "securityLevel": "loose" }));
        let work_meter =
            OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        let mut service = String::new();
        write_arch_icon_svg_with_registry(
            &mut service,
            "test:clip",
            80.0,
            Some(&registry),
            icon_id_scope_for_test("diagram-service-a-icon"),
            &effective_config,
            &work_meter,
        )
        .unwrap();
        let mut group = String::new();
        write_arch_icon_svg_with_registry(
            &mut group,
            "test:clip",
            60.0,
            Some(&registry),
            icon_id_scope_for_test("diagram-group-app-icon"),
            &effective_config,
            &work_meter,
        )
        .unwrap();

        for svg in [&service, &group] {
            assert!(!svg.contains(r#"id="clip""#), "{svg}");
            assert!(!svg.contains(r#"id="shape""#), "{svg}");
            assert!(!svg.contains(r#"url(#clip)"#), "{svg}");
            assert!(!svg.contains(r##"href="#shape""##), "{svg}");
            assert_eq!(svg.matches(r#"data-icon="fixture""#).count(), 1, "{svg}");
            assert_eq!(svg.matches(r#"id="IconifyId"#).count(), 2, "{svg}");
        }

        assert_ne!(service, group);
        assert!(service.contains(r#"width="80" height="80""#), "{service}");
        assert!(group.contains(r#"width="60" height="60""#), "{group}");
    }
}
