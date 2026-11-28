//! `texify.go` — turn parser output into TeX (KaTeX mhchem 3.3).

use crate::expansion_budget::ExpansionBudget;
use crate::mhchem::error::{MhchemError, MhchemResult};
use crate::stack_safety::DepthBudget;
#[cfg(test)]
use crate::stack_safety::MAX_INPUT_DEPTH;
use serde_json::Value;

#[cfg(test)]
pub fn go(input: &[Value], is_inner: bool) -> MhchemResult<String> {
    go_with_budget(
        input,
        is_inner,
        &DepthBudget::new(MAX_INPUT_DEPTH),
        &ExpansionBudget::default(),
    )
}

pub(crate) fn go_with_budget(
    input: &[Value],
    is_inner: bool,
    budget: &DepthBudget,
    resources: &ExpansionBudget,
) -> MhchemResult<String> {
    go_scoped(input, is_inner, budget, resources, false)
}

fn go_scoped(
    input: &[Value],
    is_inner: bool,
    budget: &DepthBudget,
    resources: &ExpansionBudget,
    count_scope: bool,
) -> MhchemResult<String> {
    let _guard = if count_scope {
        Some(
            budget
                .enter()
                .map_err(|_| MhchemError::msg("Recursion limit exceeded"))?,
        )
    } else {
        None
    };

    let mut res = String::new();
    let mut cee = false;
    for inputi in input {
        match inputi {
            Value::String(s) => resources.append(&mut res, s)?,
            v => {
                if type_str(v) == Some("1st-level escape") {
                    cee = true;
                }
                resources.append(&mut res, &go2(v, budget, resources)?)?;
            }
        }
    }
    if !is_inner && !cee && !res.is_empty() {
        res = resources.format(format_args!("{{{res}}}"))?;
    }
    Ok(res)
}

fn arr_inner(
    v: &Value,
    key: &str,
    budget: &DepthBudget,
    resources: &ExpansionBudget,
) -> MhchemResult<String> {
    let sl = v
        .get(key)
        .and_then(|x| x.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    go_scoped(sl, true, budget, resources, true)
}

fn type_str(v: &Value) -> Option<&str> {
    v.get("type_").and_then(|x| x.as_str())
}

fn go2(buf: &Value, budget: &DepthBudget, resources: &ExpansionBudget) -> MhchemResult<String> {
    let typ = type_str(buf).ok_or_else(|| MhchemError::msg("texify: missing type_"))?;
    Ok(match typ {
        "chemfive" => {
            let b5a = arr_inner(buf, "a", budget, resources)?;
            let b5b = arr_inner(buf, "b", budget, resources)?;
            let b5p = arr_inner(buf, "p", budget, resources)?;
            let b5o = arr_inner(buf, "o", budget, resources)?;
            let b5q = arr_inner(buf, "q", budget, resources)?;
            let b5d = arr_inner(buf, "d", budget, resources)?;
            let mut res = String::new();
            if !b5a.is_empty() {
                let a = if b5a.starts_with('+') || b5a.starts_with('-') {
                    resources.format(format_args!("{{{b5a}}}"))?
                } else {
                    b5a
                };
                resources.append(&mut res, &a)?;
                resources.append(&mut res, "\\,")?;
            }
            if !b5b.is_empty() || !b5p.is_empty() {
                resources.append(&mut res, "{\\vphantom{X}}")?;
                resources.append(
                    &mut res,
                    &resources.format(format_args!(
                        "^{{\\hphantom{{{}}}}}_{{\\hphantom{{{}}}}}",
                        b5b, b5p
                    ))?,
                )?;
                resources.append(&mut res, "{\\vphantom{X}}")?;
                resources.append(
                    &mut res,
                    &resources.format(format_args!(
                        "^{{\\smash[t]{{\\vphantom{{2}}}}\\mathllap{{{}}}}}",
                        b5b
                    ))?,
                )?;
                resources.append(
                    &mut res,
                    &resources.format(format_args!(
                        "_{{\\vphantom{{2}}\\mathllap{{\\smash[t]{{{}}}}}}}",
                        b5p
                    ))?,
                )?;
            }
            if !b5o.is_empty() {
                let o = if b5o.starts_with('+') || b5o.starts_with('-') {
                    resources.format(format_args!("{{{b5o}}}"))?
                } else {
                    b5o
                };
                resources.append(&mut res, &o)?;
            }
            let d_ty = buf.get("dType").and_then(|x| x.as_str()).unwrap_or("");
            if d_ty == "kv" {
                if !b5d.is_empty() || !b5q.is_empty() {
                    resources.append(&mut res, "{\\vphantom{X}}")?;
                }
                if !b5d.is_empty() {
                    resources.append(&mut res, &resources.format(format_args!("^{{{b5d}}}"))?)?;
                }
                if !b5q.is_empty() {
                    resources.append(
                        &mut res,
                        &resources.format(format_args!("_{{\\smash[t]{{{b5q}}}}}"))?,
                    )?;
                }
            } else if d_ty == "oxidation" {
                if !b5d.is_empty() {
                    resources.append(&mut res, "{\\vphantom{X}}")?;
                    resources.append(&mut res, &resources.format(format_args!("^{{{b5d}}}"))?)?;
                }
                if !b5q.is_empty() {
                    resources.append(&mut res, "{\\vphantom{X}}")?;
                    resources.append(
                        &mut res,
                        &resources.format(format_args!("_{{\\smash[t]{{{b5q}}}}}"))?,
                    )?;
                }
            } else {
                if !b5q.is_empty() {
                    resources.append(&mut res, "{\\vphantom{X}}")?;
                    resources.append(
                        &mut res,
                        &resources.format(format_args!("_{{\\smash[t]{{{b5q}}}}}"))?,
                    )?;
                }
                if !b5d.is_empty() {
                    resources.append(&mut res, "{\\vphantom{X}}")?;
                    resources.append(&mut res, &resources.format(format_args!("^{{{b5d}}}"))?)?;
                }
            }
            res
        }
        "rm" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            resources.format(format_args!("\\mathrm{{{p1}}}"))?
        }
        "text" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            if p1.contains('^') || p1.contains('_') {
                let mut replaced = String::new();
                for ch in p1.chars() {
                    let mut bytes = [0; 4];
                    let text = match ch {
                        ' ' => "~",
                        '-' => "\\text{-}",
                        _ => ch.encode_utf8(&mut bytes),
                    };
                    resources.append(&mut replaced, text)?;
                }
                resources.format(format_args!("\\mathrm{{{replaced}}}"))?
            } else {
                resources.format(format_args!("\\text{{{p1}}}"))?
            }
        }

        "roman numeral" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            resources.format(format_args!("\\mathrm{{{p1}}}"))?
        }
        "state of aggregation" => {
            let i = arr_inner(buf, "p1", budget, resources)?;
            resources.format(format_args!("\\mskip2mu {i}"))?
        }
        "state of aggregation subscript" => {
            let i = arr_inner(buf, "p1", budget, resources)?;
            resources.format(format_args!("\\mskip1mu {i}"))?
        }
        "bond" => {
            let k = buf.get("kind_").and_then(|x| x.as_str()).unwrap_or("");
            resources.text(get_bond(k).ok_or_else(|| MhchemError::msg("Unknown bond"))?)?
        }
        "frac" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            let p2 = buf.get("p2").and_then(|x| x.as_str()).unwrap_or("");
            let c = resources.format(format_args!("\\frac{{{p1}}}{{{p2}}}"))?;
            resources.format(format_args!(
                r"\mathchoice{{\textstyle{}}}{{{}}}{{{}}}{{{}}}",
                c, c, c, c
            ))?
        }
        "pu-frac" => {
            let p1 = arr_inner(buf, "p1", budget, resources)?;
            let p2 = arr_inner(buf, "p2", budget, resources)?;
            let d = resources.format(format_args!("\\frac{{{p1}}}{{{p2}}}"))?;
            resources.format(format_args!(
                r"\mathchoice{{\textstyle{}}}{{{}}}{{{}}}{{{}}}",
                d, d, d, d
            ))?
        }
        "tex-math" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            resources.format(format_args!("{p1} "))?
        }
        "frac-ce" => {
            let p1 = arr_inner(buf, "p1", budget, resources)?;
            let p2 = arr_inner(buf, "p2", budget, resources)?;
            resources.format(format_args!("\\frac{{{p1}}}{{{p2}}}"))?
        }
        "overset" => {
            let p1 = arr_inner(buf, "p1", budget, resources)?;
            let p2 = arr_inner(buf, "p2", budget, resources)?;
            resources.format(format_args!("\\overset{{{p1}}}{{{p2}}}"))?
        }
        "underset" => {
            let p1 = arr_inner(buf, "p1", budget, resources)?;
            let p2 = arr_inner(buf, "p2", budget, resources)?;
            resources.format(format_args!("\\underset{{{p1}}}{{{p2}}}"))?
        }
        "underbrace" => {
            let p1 = arr_inner(buf, "p1", budget, resources)?;
            let p2 = arr_inner(buf, "p2", budget, resources)?;
            resources.format(format_args!("\\underbrace{{{p1}}}_{{{p2}}}"))?
        }
        "color" => {
            let c1 = buf.get("color1").and_then(|x| x.as_str()).unwrap_or("");
            let c2 = arr_inner(buf, "color2", budget, resources)?;
            resources.format(format_args!("{{\\color{{{c1}}}{{{c2}}}}}"))?
        }
        "color0" => {
            let c = buf.get("color").and_then(|x| x.as_str()).unwrap_or("");
            resources.format(format_args!("\\color{{{c}}}"))?
        }
        "arrow" => {
            let rd = arr_inner(buf, "rd", budget, resources)?;
            let rq = arr_inner(buf, "rq", budget, resources)?;
            let r = buf.get("r").and_then(|x| x.as_str()).unwrap_or("");
            let mut arrow = resources.format(format_args!("\\x{}", get_arrow(r)?))?;
            if !rq.is_empty() {
                resources.append(&mut arrow, &resources.format(format_args!("[{{{rq}}}]"))?)?;
            }
            if !rd.is_empty() {
                resources.append(&mut arrow, &resources.format(format_args!("{{{rd}}}"))?)?;
            } else {
                resources.append(&mut arrow, "{}")?;
            }
            arrow
        }
        "operator" => {
            let k = buf.get("kind_").and_then(|x| x.as_str()).unwrap_or("");
            resources.text(get_operator(k).ok_or_else(|| MhchemError::msg("Unknown operator"))?)?
        }
        "1st-level escape" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            resources.format(format_args!("{p1} "))?
        }
        "space" => resources.text(" ")?,
        "entitySkip" => resources.text("~")?,
        "pu-space-1" => resources.text("~")?,
        "pu-space-2" => resources.text("\\mkern3mu ")?,
        "1000 separator" => resources.text("\\mkern2mu ")?,
        "commaDecimal" => resources.text("{,}")?,
        "comma enumeration L" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            resources.format(format_args!("{{{p1}}}\\mkern6mu "))?
        }
        "comma enumeration M" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            resources.format(format_args!("{{{p1}}}\\mkern3mu "))?
        }
        "comma enumeration S" => {
            let p1 = buf.get("p1").and_then(|x| x.as_str()).unwrap_or("");
            resources.format(format_args!("{{{p1}}}\\mkern1mu "))?
        }
        "hyphen" => resources.text("\\text{-}")?,
        "addition compound" => resources.text("\\,{\\cdot}\\,")?,
        "electron dot" => resources.text("\\mkern1mu \\bullet\\mkern1mu ")?,
        "KV x" => resources.text("{\\times}")?,
        "prime" => resources.text("\\prime ")?,
        "cdot" => resources.text("\\cdot ")?,
        "tight cdot" => resources.text("\\mkern1mu{\\cdot}\\mkern1mu ")?,
        "times" => resources.text("\\times ")?,
        "circa" => resources.text("{\\sim}")?,
        "^" => resources.text("\\uparrow ")?,
        "v" => resources.text("\\downarrow ")?,
        "ellipsis" => resources.text("\\ldots ")?,
        "/" => resources.text("/")?,
        " / " => resources.text("\\,/\\,")?,
        t => {
            return Err(MhchemError::msg(format!("texify: unknown type ({t:.64})")));
        }
    })
}

fn get_arrow(a: &str) -> MhchemResult<&'static str> {
    Ok(match a {
        "->" | "\u{2192}" | "\u{27f6}" => "rightarrow",
        "<-" => "leftarrow",
        "<->" => "leftrightarrow",
        "<-->" => "rightleftarrows",
        "<=>" | "\u{21cc}" => "rightleftharpoons",
        "<=>>" => "rightequilibrium",
        "<<=>" => "leftequilibrium",
        _ => return Err(MhchemError::msg(format!("unknown arrow {a:.64}"))),
    })
}

fn get_bond(a: &str) -> Option<&'static str> {
    Some(match a {
        "-" | "1" => "{-}",
        "=" | "2" => "{=}",
        "#" | "3" => "{\\equiv}",
        "~" => "{\\tripledash}",
        "~-" => "{\\mathrlap{\\raisebox{-.1em}{$-$}}\\raisebox{.1em}{$\\tripledash$}}",
        "~=" => "{\\mathrlap{\\raisebox{-.2em}{$-$}}\\mathrlap{\\raisebox{.2em}{$\\tripledash$}}-}",
        "~--" => {
            "{\\mathrlap{\\raisebox{-.2em}{$-$}}\\mathrlap{\\raisebox{.2em}{$\\tripledash$}}-}"
        }
        "-~-" => {
            "{\\mathrlap{\\raisebox{-.2em}{$-$}}\\mathrlap{\\raisebox{.2em}{$-$}}\\tripledash}"
        }
        "..." => "{{\\cdot}{\\cdot}{\\cdot}}",
        "...." => "{{\\cdot}{\\cdot}{\\cdot}{\\cdot}}",
        "->" => "{\\rightarrow}",
        "<-" => "{\\leftarrow}",
        "<" => "{<}",
        ">" => "{>}",
        _ => return None,
    })
}

fn get_operator(a: &str) -> Option<&'static str> {
    Some(match a {
        "+" => " {}+{} ",
        "-" => " {}-{} ",
        "=" => " {}={} ",
        "<" => " {}<{} ",
        ">" => " {}>{} ",
        "<<" => " {}\\ll{} ",
        ">>" => " {}\\gg{} ",
        "\\pm" => " {}\\pm{} ",
        "\\approx" | "$\\approx$" => " {}\\approx{} ",
        "v" | "(v)" => " \\downarrow{} ",
        "^" | "(^)" => " \\uparrow{} ",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::go;

    fn nested_texify_value(depth: usize) -> Value {
        let mut value = json!({"type_": "rm", "p1": "H"});
        for _ in 0..depth {
            value = json!({
                "type_": "state of aggregation",
                "p1": [value],
            });
        }
        value
    }

    #[test]
    fn nested_values_have_a_depth_budget() {
        assert!(go(&[nested_texify_value(32)], false).is_ok());
        let error = go(&[nested_texify_value(33)], false).unwrap_err();
        assert!(
            error.to_string().contains("Recursion limit exceeded"),
            "unexpected error: {error}"
        );
    }
}
