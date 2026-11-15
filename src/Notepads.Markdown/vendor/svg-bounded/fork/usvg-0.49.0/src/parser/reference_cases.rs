// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Engine-independent fixtures, shared verbatim with the retained before/after host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    Accept,
    Quota,
    Decode,
}
pub struct Case {
    pub name: String,
    pub source: String,
    pub expected: Expected,
    pub plain: bool,
}
pub fn document(body: &str) -> String {
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'>{body}</svg>")
}
pub fn use_body(edges: usize, terminal: &str, final_style: &str) -> String {
    let mut body = String::from("<defs>");
    for i in 1..edges {
        let style = if i + 1 == edges { final_style } else { "" };
        body.push_str(&format!("<use id='v{i}' href='#v{}' {style}/>", i + 1));
    }
    body.push_str(&format!(
        "<g id='v{edges}'>{terminal}</g></defs><use href='#v1'/>"
    ));
    body
}
fn paint_defs() -> &'static str {
    "<defs><linearGradient id='p' href='#q'/><linearGradient id='q'><stop stop-color='red'/><stop offset='1' stop-color='blue'/></linearGradient></defs>"
}
fn add(v: &mut Vec<Case>, name: impl Into<String>, body: String, expected: Expected, plain: bool) {
    v.push(Case {
        name: name.into(),
        source: document(&body),
        expected,
        plain,
    });
}
pub fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    for edges in [63, 64, 65] {
        let expected = if edges <= 64 {
            Expected::Accept
        } else {
            Expected::Quota
        };
        add(
            &mut v,
            format!("use-{edges}"),
            use_body(edges, "<rect width='20' height='20'/>", ""),
            expected,
            true,
        );
        let mut b = String::from("<defs>");
        for i in 1..edges {
            b.push_str(&format!("<linearGradient id='g{i}' href='#g{}'/>", i + 1));
        }
        b.push_str(&format!("<linearGradient id='g{edges}'><stop stop-color='red'/><stop offset='1' stop-color='blue'/></linearGradient></defs><rect width='20' height='20' fill='url(#g1)'/>"));
        add(&mut v, format!("gradient-{edges}"), b, expected, true);
    }
    for edges in [64, 65] {
        for kind in ["clipPath", "mask"] {
            let mut b = String::from("<defs>");
            let attr = if kind == "mask" { "mask" } else { "clip-path" };
            let units = if kind == "mask" {
                "maskUnits='userSpaceOnUse' maskContentUnits='userSpaceOnUse'"
            } else {
                "clipPathUnits='userSpaceOnUse'"
            };
            for i in 1..=edges {
                let link = if i < edges {
                    format!("{attr}='url(#c{})'", i + 1)
                } else {
                    String::new()
                };
                b.push_str(&format!("<{kind} id='c{i}' {units} {link}><rect width='20' height='20' fill='white'/></{kind}>"));
            }
            b.push_str(&format!(
                "</defs><rect width='20' height='20' {attr}='url(#c1)'/>"
            ));
            add(
                &mut v,
                format!("{kind}-{edges}"),
                b,
                if edges == 64 {
                    Expected::Accept
                } else {
                    Expected::Quota
                },
                true,
            );
        }
    }
    for (n, expected) in [(62, Expected::Accept), (63, Expected::Quota)] {
        for warm_first in [false, true] {
            let warm = "<rect x='21' width='8' height='20' fill='url(#p)'/>";
            let chain = use_body(n, "<rect width='20' height='20' fill='url(#p)'/>", "");
            let b = if warm_first {
                format!("{}{warm}{chain}", paint_defs())
            } else {
                format!("{}{chain}{warm}", paint_defs())
            };
            add(
                &mut v,
                format!("cache-{}-warm-{warm_first}", n + 2),
                b,
                expected,
                true,
            );
        }
        for attr in ["fill", "stroke"] {
            let terminal = format!("<path d='M2 2L20 2L20 20Z' {attr}='context-{attr}'/>");
            let style = format!("{attr}='url(#p)'");
            add(
                &mut v,
                format!("context-{attr}-{}", n + 2),
                format!("{}{}", paint_defs(), use_body(n, &terminal, &style)),
                expected,
                true,
            );
        }
        add(
            &mut v,
            format!("inline-{}", n + 2),
            format!(
                "{}{}",
                paint_defs(),
                use_body(
                    n,
                    "<svg width='32' height='32'><rect width='20' height='20' fill='url(#p)'/></svg>",
                    ""
                )
            ),
            expected,
            true,
        );
        let nested = String::from(
            "<defs><pattern id='p' width='20' height='20' patternUnits='userSpaceOnUse'><rect width='20' height='20'/><text><tref href='#t'/></text></pattern><text id='t'>x</text></defs>",
        );
        add(
            &mut v,
            format!("tref-{}", n + 2),
            format!(
                "{nested}{}",
                use_body(n, "<rect width='20' height='20' fill='url(#p)'/>", "")
            ),
            expected,
            true,
        );
    }
    for (n, expected) in [(61, Expected::Accept), (62, Expected::Quota)] {
        let definitions = "<defs><pattern id='p1' href='#p2'/><pattern id='p2' width='20' height='20' patternUnits='userSpaceOnUse'><rect width='20' height='20' fill='url(#g)'/></pattern><linearGradient id='g'><stop/><stop offset='1' stop-color='red'/></linearGradient></defs>";
        add(
            &mut v,
            format!("selected-pattern-{}", n + 3),
            format!(
                "{definitions}{}",
                use_body(n, "<rect width='20' height='20' fill='url(#p1)'/>", "")
            ),
            expected,
            true,
        );
        let definitions = "<defs><filter id='f1' href='#f2'/><filter id='f2' filterUnits='userSpaceOnUse' primitiveUnits='userSpaceOnUse' x='0' y='0' width='32' height='32'><feImage href='#s'/></filter><rect id='s' width='20' height='20'/></defs>";
        add(
            &mut v,
            format!("selected-filter-feImage-{}", n + 3),
            format!(
                "{definitions}{}",
                use_body(n, "<rect width='20' height='20' filter='url(#f1)'/>", "")
            ),
            expected,
            true,
        );
    }
    for edges in [64, 65] {
        let child = document(&use_body(edges, "<rect width='20' height='20'/>", ""));
        let encoded = child
            .bytes()
            .map(|b| format!("%{b:02X}"))
            .collect::<String>();
        let image = format!("<image width='32' height='32' href='data:image/svg+xml,{encoded}'/>");
        add(
            &mut v,
            format!("nested-parent64-child{edges}"),
            use_body(64, &image, ""),
            if edges == 64 {
                Expected::Accept
            } else {
                Expected::Quota
            },
            true,
        );
    }
    add(
        &mut v,
        "shared",
        format!(
            "{}<rect width='8' height='20' fill='url(#p)'/><rect x='10' width='8' height='20' fill='url(#p)'/>",
            paint_defs()
        ),
        Expected::Accept,
        true,
    );
    add(
        &mut v,
        "use-self",
        "<use id='a' href='#a'/>".into(),
        Expected::Decode,
        true,
    );
    add(&mut v,"gradient-self","<defs><linearGradient id='a' href='#a'><stop/><stop offset='1'/></linearGradient></defs><rect width='20' height='20' fill='url(#a)'/>".into(),Expected::Decode,true);
    add(&mut v,"gradient-lasso","<defs><linearGradient id='a' href='#b'/><linearGradient id='b' href='#c'/><linearGradient id='c' href='#b'/></defs><rect width='20' height='20' fill='url(#a)'/>".into(),Expected::Decode,false);
    add(&mut v,"mixed-cycle","<style>.paint{fill:url(#p)}</style><defs><pattern id='p' width='20' height='20' patternUnits='userSpaceOnUse'><rect width='20' height='20' mask='url(#m)'/></pattern><mask id='m' maskUnits='userSpaceOnUse' maskContentUnits='userSpaceOnUse'><use href='#s'/></mask><rect id='s' width='20' height='20' class='paint'/></defs><rect width='20' height='20' class='paint'/>".into(),Expected::Decode,false);
    v
}
