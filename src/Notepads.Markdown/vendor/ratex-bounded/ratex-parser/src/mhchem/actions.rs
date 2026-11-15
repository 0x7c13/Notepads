//! State machine actions (global + per-machine), mirroring KaTeX `mhchem.js`.

use super::resources::node;
use crate::mhchem::buffer::Buffer;
use crate::mhchem::engine;
use crate::mhchem::error::{MhchemError, MhchemResult};
use crate::mhchem::json::ActionSpec;
use crate::mhchem::patterns::{match_pattern, MatchToken};
use crate::mhchem::RuntimeCtx;
use regex::Regex;
use serde_json::Value;
use std::borrow::Cow;
use std::sync::LazyLock;

static RE_DIGITS_ONLY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9]+$").unwrap());
static RE_CELSIUS_C: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\u{00B0}C|\^oC|\^\{o\}C").unwrap());
static RE_CELSIUS_F: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\u{00B0}F|\^oF|\^\{o\}F").unwrap());
static RE_HALF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]+|\$[a-z]\$|[a-z])\/([0-9]+)(\$[a-z]\$|[a-z])?$").unwrap()
});

pub fn apply(
    ctx: &RuntimeCtx,
    machine: &str,
    buffer: &mut Buffer,
    m: &MatchToken,
    spec: &ActionSpec,
) -> MhchemResult<Vec<Value>> {
    let t = spec.type_.as_str();
    match (machine, t) {
        ("ce", "output") => ce_output(ctx, buffer, spec.option.as_ref()),
        ("ce", "o after d") => ce_o_after_d(ctx, buffer, m),
        ("ce", "d= kv") => {
            Buffer::set_slot(&mut buffer.d, owned_token(ctx, m)?);
            Buffer::set_slot(&mut buffer.d_type, ctx.resources.text("kv")?);
            Ok(vec![])
        }
        ("ce", "charge or bond") => ce_charge_or_bond(ctx, buffer, m),
        ("ce", "- after o/d") => {
            let after_d = spec
                .option
                .as_ref()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            ce_after_od(ctx, buffer, m, after_d)
        }
        ("ce", "a to o") => {
            buffer.o = buffer.a.take();
            Ok(vec![])
        }
        ("ce", "sb=true") => {
            buffer.sb = true;
            Ok(vec![])
        }
        ("ce", "sb=false") => {
            buffer.sb = false;
            Ok(vec![])
        }
        ("ce", "beginsWithBond=true") => {
            buffer.begins_with_bond = true;
            Ok(vec![])
        }
        ("ce", "beginsWithBond=false") => {
            buffer.begins_with_bond = false;
            Ok(vec![])
        }
        ("ce", "parenthesisLevel++") => {
            buffer.parenthesis_level += 1;
            Ok(vec![])
        }
        ("ce", "parenthesisLevel--") => {
            buffer.parenthesis_level -= 1;
            Ok(vec![])
        }
        ("ce", "state of aggregation") => Ok(vec![node!(ctx, {
            "type_": "state of aggregation",
            "p1": engine::go_machine(ctx, &token_string(ctx, m)?, "o")?,
        })]),
        ("ce", "comma") => ce_comma(ctx, buffer, m),
        ("ce", "oxidation-output") => {
            let mut v = vec![ctx.value("{")?];
            v.extend(engine::go_machine(
                ctx,
                &token_string(ctx, m)?,
                "oxidation",
            )?);
            v.push(ctx.value("}")?);
            Ok(v)
        }
        ("ce", "frac-output") => {
            let MatchToken::A(parts) = m else {
                return Err(MhchemError::msg("frac-output"));
            };
            if parts.len() < 2 {
                return Err(MhchemError::msg("frac-output len"));
            }
            Ok(vec![node!(ctx, {
                "type_": "frac-ce",
                "p1": engine::go_machine(ctx, &parts[0], "ce")?,
                "p2": engine::go_machine(ctx, &parts[1], "ce")?,
            })])
        }
        ("ce", "overset-output") | ("ce", "underset-output") | ("ce", "underbrace-output") => {
            let MatchToken::A(parts) = m else {
                return Err(MhchemError::msg("two-arg output"));
            };
            if parts.len() < 2 {
                return Err(MhchemError::msg("two-arg len"));
            }
            let ty = match spec.type_.as_str() {
                "overset-output" => "overset",
                "underset-output" => "underset",
                _ => "underbrace",
            };
            Ok(vec![node!(ctx, {
                "type_": ty,
                "p1": engine::go_machine(ctx, &parts[0], "ce")?,
                "p2": engine::go_machine(ctx, &parts[1], "ce")?,
            })])
        }
        ("ce", "color-output") => {
            let MatchToken::A(parts) = m else {
                return Err(MhchemError::msg("color-output"));
            };
            if parts.len() < 2 {
                return Err(MhchemError::msg("color-output len"));
            }
            Ok(vec![node!(ctx, {
                "type_": "color",
                "color1": &parts[0],
                "color2": engine::go_machine(ctx, &parts[1], "ce")?,
            })])
        }
        ("ce", "operator") => {
            let kind = spec
                .option
                .as_ref()
                .and_then(|v| v.as_str())
                .or_else(|| match_str(m))
                .unwrap_or_default();
            Ok(vec![node!(ctx, { "type_": "operator", "kind_": kind })])
        }

        ("text", "output") => {
            if let Some(tx) = buffer.text_.take() {
                buffer.clear_all();
                Ok(vec![node!(ctx, {"type_": "text", "p1": tx})])
            } else {
                Ok(vec![])
            }
        }

        ("pq", "state of aggregation") => Ok(vec![node!(ctx, {
            "type_": "state of aggregation subscript",
            "p1": engine::go_machine(ctx, &token_string(ctx, m)?, "o")?,
        })]),
        ("pq", "color-output") | ("bd", "color-output") => {
            let MatchToken::A(parts) = m else {
                return Err(MhchemError::msg("color bd/pq"));
            };
            let sm = if machine == "pq" { "pq" } else { "bd" };
            Ok(vec![node!(ctx, {
                "type_": "color",
                "color1": &parts[0],
                "color2": engine::go_machine(ctx, &parts[1], sm)?,
            })])
        }

        ("oxidation", "roman-numeral") => Ok(vec![node!(ctx, {
            "type_": "roman numeral",
            "p1": match_str(m).unwrap_or_default(),
        })]),

        ("tex-math", "output") | ("tex-math tight", "output") => {
            if let Some(o) = buffer.o.take() {
                buffer.clear_all();
                Ok(vec![node!(ctx, {"type_": "tex-math", "p1": o})])
            } else {
                Ok(vec![])
            }
        }
        ("tex-math tight", "tight operator") => {
            let op = match_str(m).unwrap_or_default();
            let cur = buffer.o.get_or_insert_with(String::new);
            ctx.resources.append(cur, "{")?;
            ctx.resources.append(cur, op)?;
            ctx.resources.append(cur, "}")?;
            Ok(vec![])
        }

        ("9,9", "comma") => Ok(vec![node!(ctx, {"type_": "commaDecimal"})]),

        ("pu", "enumber") => pu_enumber(ctx, m),
        ("pu", "number^") => pu_number_pow(ctx, m),
        ("pu", "space") => Ok(vec![node!(ctx, {"type_": "pu-space-1"})]),
        ("pu", "output") => pu_output(ctx, buffer),

        ("pu-2", "cdot") => Ok(vec![node!(ctx, {"type_": "tight cdot"})]),
        ("pu-2", "^(-1)") => {
            let pow = match_str(m).unwrap_or_default();
            let cur = buffer.rm.get_or_insert_with(String::new);
            ctx.resources.append(cur, "^{")?;
            ctx.resources.append(cur, pow)?;
            ctx.resources.append(cur, "}")?;
            Ok(vec![])
        }
        ("pu-2", "space") => Ok(vec![node!(ctx, {"type_": "pu-space-2"})]),
        ("pu-2", "output") => pu2_output(ctx, buffer),

        ("pu-9,9", "comma") => Ok(vec![node!(ctx, {"type_": "commaDecimal"})]),
        ("pu-9,9", "output-0") => pu99_out(ctx, buffer, false),
        ("pu-9,9", "output-o") => pu99_out(ctx, buffer, true),

        _ => global_action(ctx, machine, buffer, m, spec),
    }
}

fn match_str<'a>(m: &'a MatchToken<'_>) -> Option<&'a str> {
    match m {
        MatchToken::S(s) => Some(s.as_ref()),
        MatchToken::A(v) => v.first().map(|s| s.as_ref()),
    }
}
fn token_string<'a>(ctx: &RuntimeCtx, m: &'a MatchToken<'_>) -> MhchemResult<Cow<'a, str>> {
    match m {
        MatchToken::S(s) => Ok(Cow::Borrowed(s.as_ref())),
        MatchToken::A(v) => {
            let mut out = String::new();
            for text in v {
                ctx.resources.append(&mut out, text)?;
            }
            Ok(Cow::Owned(out))
        }
    }
}
fn owned_token(ctx: &RuntimeCtx, m: &MatchToken<'_>) -> MhchemResult<String> {
    match token_string(ctx, m)? {
        Cow::Borrowed(s) => Ok(ctx.resources.text(s)?),
        Cow::Owned(s) => Ok(s),
    }
}

fn ce_comma(ctx: &RuntimeCtx, buffer: &mut Buffer, m: &MatchToken) -> MhchemResult<Vec<Value>> {
    let raw = token_string(ctx, m)?;
    let trimmed = raw.trim_end();
    let with_space = trimmed != raw;
    let ty = if with_space && buffer.parenthesis_level == 0 {
        "comma enumeration L"
    } else {
        "comma enumeration M"
    };
    Ok(vec![node!(ctx, {"type_": ty, "p1": trimmed})])
}

fn ce_o_after_d(ctx: &RuntimeCtx, buffer: &mut Buffer, m: &MatchToken) -> MhchemResult<Vec<Value>> {
    let mut ret = vec![];
    let digits_only = buffer
        .d
        .as_deref()
        .map(|d| RE_DIGITS_ONLY.is_match(d))
        .unwrap_or(false);
    if digits_only {
        let tmp = buffer.d.take().unwrap();
        buffer.d = None;
        ret.extend(ce_output(ctx, buffer, None)?);
        Buffer::set_slot(&mut buffer.b, tmp);
    } else {
        ret.extend(ce_output(ctx, buffer, None)?);
    }
    cat(ctx, &mut buffer.o, m)?;
    Ok(ret)
}

fn ce_charge_or_bond(
    ctx: &RuntimeCtx,
    buffer: &mut Buffer,
    m: &MatchToken,
) -> MhchemResult<Vec<Value>> {
    if buffer.begins_with_bond {
        let mut ret = ce_output(ctx, buffer, None)?;
        ret.push(node!(ctx, {"type_": "bond", "kind_": "-"}));
        Ok(ret)
    } else {
        Buffer::set_slot(&mut buffer.d, owned_token(ctx, m)?);
        Ok(vec![])
    }
}

fn ce_after_od(
    ctx: &RuntimeCtx,
    buffer: &mut Buffer,
    m: &MatchToken,
    is_after_d: bool,
) -> MhchemResult<Vec<Value>> {
    let dash = token_string(ctx, m)?;
    let o = buffer.o.as_deref().unwrap_or("");
    let c1 = match_pattern(ctx.data, "orbital", o, &ctx.resources)?;
    let c2 = match_pattern(ctx.data, "one lowercase greek letter $", o, &ctx.resources)?;
    let c3 = match_pattern(ctx.data, "one lowercase latin letter $", o, &ctx.resources)?;
    let c4 = match_pattern(
        ctx.data,
        "$one lowercase latin letter$ $",
        o,
        &ctx.resources,
    )?;
    let h_orb = c1.as_ref().map(|h| h.remainder.is_empty()).unwrap_or(false);
    let hyphen_follows = dash == "-" && (h_orb || c2.is_some() || c3.is_some() || c4.is_some());

    if hyphen_follows
        && Buffer::is_slot_empty(&buffer.a)
        && Buffer::is_slot_empty(&buffer.b)
        && Buffer::is_slot_empty(&buffer.p)
        && Buffer::is_slot_empty(&buffer.d)
        && Buffer::is_slot_empty(&buffer.q)
        && c1.is_none()
        && c3.is_some()
    {
        let oo = buffer.o.take().unwrap_or_default();
        Buffer::set_slot(&mut buffer.o, ctx.resources.format(format_args!("${oo}$"))?);
    }

    let mut ret = vec![];
    if hyphen_follows {
        ret.extend(ce_output(ctx, buffer, None)?);
        ret.push(node!(ctx, {"type_": "hyphen"}));
        return Ok(ret);
    }

    let digits_d = match_pattern(
        ctx.data,
        "digits",
        buffer.d.as_deref().unwrap_or(""),
        &ctx.resources,
    )?;
    let d_only = digits_d.map(|h| h.remainder.is_empty()).unwrap_or(false);
    if is_after_d && d_only {
        cat(ctx, &mut buffer.d, m)?;
        ret.extend(ce_output(ctx, buffer, None)?);
    } else {
        ret.extend(ce_output(ctx, buffer, None)?);
        ret.push(node!(ctx, {"type_": "bond", "kind_": "-"}));
    }
    Ok(ret)
}

fn ce_output(
    ctx: &RuntimeCtx,
    buffer: &mut Buffer,
    entity: Option<&Value>,
) -> MhchemResult<Vec<Value>> {
    let entity_follows = entity.and_then(|v| {
        if let Some(n) = v.as_u64() {
            Some(n as i32)
        } else {
            v.as_i64().map(|x| x as i32)
        }
    });

    if Buffer::is_slot_empty(&buffer.r) {
        let empty_piece = Buffer::is_slot_empty(&buffer.a)
            && Buffer::is_slot_empty(&buffer.b)
            && Buffer::is_slot_empty(&buffer.p)
            && Buffer::is_slot_empty(&buffer.o)
            && Buffer::is_slot_empty(&buffer.q)
            && Buffer::is_slot_empty(&buffer.d)
            && entity_follows.is_none();
        if empty_piece {
            buffer.clear_soft();
            return Ok(vec![]);
        }

        let mut ret: Vec<Value> = vec![];
        if buffer.sb {
            ret.push(node!(ctx, {"type_": "entitySkip"}));
        }

        let mut d_type = buffer.d_type.take();
        if !Buffer::is_slot_empty(&buffer.o)
            && d_type.as_deref() == Some("kv")
            && match_pattern(
                ctx.data,
                "d-oxidation$",
                buffer.d.as_deref().unwrap_or(""),
                &ctx.resources,
            )?
            .is_some()
        {
            d_type = Some(ctx.resources.text("oxidation")?);
        } else if !Buffer::is_slot_empty(&buffer.o)
            && d_type.as_deref() == Some("kv")
            && Buffer::is_slot_empty(&buffer.q)
        {
            d_type = None;
        }

        if Buffer::is_slot_empty(&buffer.o)
            && Buffer::is_slot_empty(&buffer.q)
            && Buffer::is_slot_empty(&buffer.d)
            && Buffer::is_slot_empty(&buffer.b)
            && Buffer::is_slot_empty(&buffer.p)
            && entity_follows != Some(2)
        {
            buffer.o = buffer.a.take();
        } else if Buffer::is_slot_empty(&buffer.o)
            && Buffer::is_slot_empty(&buffer.q)
            && Buffer::is_slot_empty(&buffer.d)
            && (!Buffer::is_slot_empty(&buffer.b) || !Buffer::is_slot_empty(&buffer.p))
        {
            buffer.o = buffer.a.take();
            buffer.d = buffer.b.take();
            buffer.q = buffer.p.take();
        }

        let a = engine::go_machine(ctx, buffer.a.take().unwrap_or_default().as_str(), "a")?;
        let b = engine::go_machine(ctx, buffer.b.take().unwrap_or_default().as_str(), "bd")?;
        let p = engine::go_machine(ctx, buffer.p.take().unwrap_or_default().as_str(), "pq")?;
        let o = engine::go_machine(ctx, buffer.o.take().unwrap_or_default().as_str(), "o")?;
        let q = engine::go_machine(ctx, buffer.q.take().unwrap_or_default().as_str(), "pq")?;
        let d_sm = if d_type.as_deref() == Some("oxidation") {
            "oxidation"
        } else {
            "bd"
        };
        let d = engine::go_machine(ctx, buffer.d.take().unwrap_or_default().as_str(), d_sm)?;

        let mut chem = ctx.object()?;
        ctx.member(&mut chem, "type_", "chemfive")?;
        ctx.member(&mut chem, "a", a)?;
        ctx.member(&mut chem, "b", b)?;
        ctx.member(&mut chem, "p", p)?;
        ctx.member(&mut chem, "o", o)?;
        ctx.member(&mut chem, "q", q)?;
        ctx.member(&mut chem, "d", d)?;
        if let Some(dt) = d_type {
            ctx.member(&mut chem, "dType", dt)?;
        }
        ret.push(Value::Object(chem));
        buffer.clear_soft();
        Ok(ret)
    } else {
        let r = buffer.r.take().unwrap_or_default();
        let rdt = buffer.rdt.as_deref();
        let rd = buffer.rd.take().unwrap_or_default();
        let rd_ast = if rdt == Some("M") {
            engine::go_machine(ctx, &rd, "tex-math")?
        } else if rdt == Some("T") {
            vec![node!(ctx, {"type_": "text", "p1": rd})]
        } else {
            engine::go_machine(ctx, &rd, "ce")?
        };

        let rqt = buffer.rqt.as_deref();
        let rq = buffer.rq.take().unwrap_or_default();
        let rq_ast = if rqt == Some("M") {
            engine::go_machine(ctx, &rq, "tex-math")?
        } else if rqt == Some("T") {
            vec![node!(ctx, {"type_": "text", "p1": rq})]
        } else {
            engine::go_machine(ctx, &rq, "ce")?
        };

        let node = node!(ctx, {
            "type_": "arrow",
            "r": r,
            "rd": rd_ast,
            "rq": rq_ast,
        });
        buffer.clear_soft();
        Ok(vec![node])
    }
}

fn pu_enumber(ctx: &RuntimeCtx, m: &MatchToken) -> MhchemResult<Vec<Value>> {
    let MatchToken::A(parts) = m else {
        return Ok(vec![]);
    };
    let mut ret: Vec<Value> = vec![];
    let g0 = parts.first().map(|s| s.as_ref()).unwrap_or("");
    if g0 == "+-" || g0 == "+/-" {
        ret.push(ctx.value("\\pm ")?);
    } else if !g0.is_empty() {
        ret.push(ctx.value(g0)?);
    }
    if parts.get(1).map(|s| !s.is_empty()).unwrap_or(false) {
        ret.extend(engine::go_machine(ctx, &parts[1], "pu-9,9")?);
        if let Some(p2) = parts.get(2) {
            if !p2.is_empty() {
                if p2.contains(',') || p2.contains('.') {
                    ret.extend(engine::go_machine(ctx, p2, "pu-9,9")?);
                } else {
                    ret.push(ctx.value(p2)?);
                }
            }
        }
        // Regex group 5 = `*` / `×` branch; group 4 = `e`/`E` or `\s*...\s*10\^`. When `e` matches,
        // group 5 is present as empty string — `Some("")` so `.or_else(|| g4)` must not be used;
        // pick non-empty branch like KaTeX `m[5] || m[4]`.
        let g_star = parts.get(4).map(|s| s.as_ref()).unwrap_or_default();
        let g_e_or_times = parts.get(3).map(|s| s.as_ref()).unwrap_or_default();
        let mult = {
            let a = g_star.trim();
            let b = g_e_or_times.trim();
            if !a.is_empty() {
                a
            } else {
                b
            }
        };
        // KaTeX mhchem: lowercase `e` → \\cdot, uppercase `E` → \\times (see golden 0100/0101 vs 0102/0103).
        if !mult.is_empty() {
            if mult == "e" || mult.starts_with('*') {
                ret.push(node!(ctx, {"type_": "cdot"}));
            } else {
                ret.push(node!(ctx, {"type_": "times"}));
            }
        }
    }
    if let Some(exp) = parts.get(5).filter(|s| !s.is_empty()) {
        ret.push(ctx.value(ctx.resources.format(format_args!("10^{{{exp}}}"))?)?);
    }
    Ok(ret)
}

fn pu_number_pow(ctx: &RuntimeCtx, m: &MatchToken) -> MhchemResult<Vec<Value>> {
    let MatchToken::A(parts) = m else {
        return Ok(vec![]);
    };
    let mut ret: Vec<Value> = vec![];
    let g0 = parts.first().map(|s| s.as_ref()).unwrap_or("");
    if g0 == "+-" || g0 == "+/-" {
        ret.push(ctx.value("\\pm ")?);
    } else if !g0.is_empty() {
        ret.push(ctx.value(g0)?);
    }
    if let Some(base) = parts.get(1) {
        ret.extend(engine::go_machine(ctx, base, "pu-9,9")?);
    }
    if let Some(exp) = parts.get(2) {
        ret.push(ctx.value(ctx.resources.format(format_args!("^{{{exp}}}"))?)?);
    }
    Ok(ret)
}

fn pu_output(ctx: &RuntimeCtx, buffer: &mut Buffer) -> MhchemResult<Vec<Value>> {
    let mut d = buffer.d.take().unwrap_or_default();
    if let Some(md) = match_pattern(ctx.data, "{(...)}", &d, &ctx.resources)? {
        if md.remainder.is_empty() {
            if let MatchToken::S(inner) = md.token {
                d = ctx.resources.text(&inner)?;
            }
        }
    }
    let mut qv = buffer.q.take().unwrap_or_default();
    if let Some(mq) = match_pattern(ctx.data, "{(...)}", &qv, &ctx.resources)? {
        if mq.remainder.is_empty() {
            if let MatchToken::S(inner) = mq.token {
                qv = ctx.resources.text(&inner)?;
            }
        }
    }
    d = replace_regex(ctx, &RE_CELSIUS_C, &d, "{}^{\\circ}C")?;
    qv = replace_regex(ctx, &RE_CELSIUS_C, &qv, "{}^{\\circ}C")?;
    let d = replace_regex(ctx, &RE_CELSIUS_F, &d, "{}^{\\circ}F")?;
    let qv = replace_regex(ctx, &RE_CELSIUS_F, &qv, "{}^{\\circ}F")?;

    let res = if !qv.is_empty() {
        let b5d = engine::go_machine(ctx, &d, "pu")?;
        let b5q = engine::go_machine(ctx, &qv, "pu")?;
        if buffer.o.as_deref() == Some("//") {
            vec![node!(ctx, {"type_": "pu-frac", "p1": b5d, "p2": b5q})]
        } else {
            let blen = b5d.len();
            let mut v = b5d;
            if blen > 1 || b5q.len() > 1 {
                v.push(node!(ctx, {"type_": " / "}));
            } else {
                v.push(node!(ctx, {"type_": "/"}));
            }
            v.extend(b5q);
            v
        }
    } else {
        engine::go_machine(ctx, &d, "pu-2")?
    };
    buffer.clear_all();
    Ok(res)
}

fn pu2_output(ctx: &RuntimeCtx, buffer: &mut Buffer) -> MhchemResult<Vec<Value>> {
    let res = if let Some(rm) = buffer.rm.take() {
        if let Some(mrm) = match_pattern(ctx.data, "{(...)}", &rm, &ctx.resources)? {
            if mrm.remainder.is_empty() {
                if let MatchToken::S(inner) = mrm.token {
                    engine::go_machine(ctx, &inner, "pu")?
                } else {
                    vec![]
                }
            } else {
                vec![node!(ctx, {"type_": "rm", "p1": rm})]
            }
        } else {
            vec![node!(ctx, {"type_": "rm", "p1": rm})]
        }
    } else {
        vec![]
    };
    buffer.clear_all();
    Ok(res)
}

fn pu99_out(
    ctx: &RuntimeCtx,
    buffer: &mut Buffer,
    reverse_triplets: bool,
) -> MhchemResult<Vec<Value>> {
    let t = buffer.text_.take().unwrap_or_default();
    let mut ret: Vec<Value> = vec![];
    if t.len() > 4 {
        if reverse_triplets {
            let mut a = t.len() % 3;
            if a == 0 {
                a = 3;
            }
            let mut i = t.len() as i32 - 3;
            while i > 0 {
                ret.push(ctx.value(&t[i as usize..i as usize + 3])?);
                ret.push(node!(ctx, {"type_": "1000 separator"}));
                i -= 3;
            }
            ret.push(ctx.value(&t[..a])?);
            ret.reverse();
        } else {
            let a = t.len() - 3;
            let mut i = 0;
            while i < a {
                ret.push(ctx.value(&t[i..i + 3])?);
                ret.push(node!(ctx, {"type_": "1000 separator"}));
                i += 3;
            }
            ret.push(ctx.value(&t[i..])?);
        }
    } else {
        ret.push(ctx.value(t)?);
    }
    buffer.clear_all();
    Ok(ret)
}

fn global_action(
    ctx: &RuntimeCtx,
    machine: &str,
    buffer: &mut Buffer,
    m: &MatchToken,
    spec: &ActionSpec,
) -> MhchemResult<Vec<Value>> {
    match spec.type_.as_str() {
        "a=" => {
            cat(ctx, &mut buffer.a, m)?;
            Ok(vec![])
        }
        "b=" => {
            cat(ctx, &mut buffer.b, m)?;
            Ok(vec![])
        }
        "p=" => {
            cat(ctx, &mut buffer.p, m)?;
            Ok(vec![])
        }
        "o=" => {
            cat(ctx, &mut buffer.o, m)?;
            Ok(vec![])
        }
        "q=" => {
            cat(ctx, &mut buffer.q, m)?;
            Ok(vec![])
        }
        "d=" => {
            cat(ctx, &mut buffer.d, m)?;
            Ok(vec![])
        }
        "rm=" => {
            cat(ctx, &mut buffer.rm, m)?;
            Ok(vec![])
        }
        "text=" => {
            cat(ctx, &mut buffer.text_, m)?;
            Ok(vec![])
        }
        "r=" => {
            Buffer::set_slot(&mut buffer.r, owned_token(ctx, m)?);
            Ok(vec![])
        }
        "rdt=" | "rqt=" => {
            if spec.type_ == "rdt=" {
                Buffer::set_slot(&mut buffer.rdt, owned_token(ctx, m)?);
            } else {
                Buffer::set_slot(&mut buffer.rqt, owned_token(ctx, m)?);
            }
            Ok(vec![])
        }
        "rd=" | "rq=" => {
            if spec.type_ == "rd=" {
                Buffer::set_slot(&mut buffer.rd, owned_token(ctx, m)?);
            } else {
                Buffer::set_slot(&mut buffer.rq, owned_token(ctx, m)?);
            }
            Ok(vec![])
        }
        "insert" => {
            let opt = spec.option.as_ref().and_then(|v| v.as_str()).unwrap_or("");
            Ok(vec![node!(ctx, {"type_": opt})])
        }
        "insert+p1" => Ok(vec![node!(ctx, {
            "type_": spec.option.as_ref().and_then(|v| v.as_str()).unwrap_or(""),
            "p1": match_str(m).unwrap_or_default(),
        })]),
        "insert+p1+p2" => {
            let MatchToken::A(v) = m else {
                return Err(MhchemError::msg("insert+p1+p2"));
            };
            if v.len() < 2 {
                return Err(MhchemError::msg("insert+p1+p2 short"));
            }
            Ok(vec![node!(ctx, {
                "type_": spec.option.as_ref().and_then(|x| x.as_str()).unwrap_or(""),
                "p1": &v[0],
                "p2": &v[1],
            })])
        }
        "copy" => Ok(vec![ctx.value(token_string(ctx, m)?)?]),
        "rm" => Ok(vec![
            node!(ctx, {"type_": "rm", "p1": match_str(m).unwrap_or_default()}),
        ]),
        "text" => Ok(engine::go_machine(ctx, &token_string(ctx, m)?, "text")?),
        "{text}" => {
            let mut v = vec![ctx.value("{")?];
            v.extend(engine::go_machine(ctx, &token_string(ctx, m)?, "text")?);
            v.push(ctx.value("}")?);
            Ok(v)
        }
        "tex-math" => Ok(engine::go_machine(ctx, &token_string(ctx, m)?, "tex-math")?),
        "tex-math tight" => Ok(engine::go_machine(
            ctx,
            &token_string(ctx, m)?,
            "tex-math tight",
        )?),
        "bond" => {
            let kind = spec
                .option
                .as_ref()
                .and_then(|v| v.as_str())
                .or_else(|| match_str(m))
                .unwrap_or("-");
            Ok(vec![node!(ctx, { "type_": "bond", "kind_": kind })])
        }
        "color0-output" => {
            let MatchToken::A(v) = m else {
                return Err(MhchemError::msg("color0-output"));
            };
            Ok(vec![
                node!(ctx, {"type_": "color0", "color": v.first().map(|s| s.as_ref()).unwrap_or_default()}),
            ])
        }
        "ce" => Ok(engine::go_machine(ctx, &token_string(ctx, m)?, "ce")?),
        "1/2" => half_action(ctx, m),
        "9,9" => Ok(engine::go_machine(ctx, &token_string(ctx, m)?, "9,9")?),
        "operator" => {
            let kind = spec
                .option
                .as_ref()
                .and_then(|v| v.as_str())
                .or_else(|| match_str(m))
                .unwrap_or_default();
            Ok(vec![node!(ctx, { "type_": "operator", "kind_": kind })])
        }
        _ => Err(MhchemError::msg(format!(
            "mhchem bug A: {} / {}",
            machine, spec.type_
        ))),
    }
}

fn cat(ctx: &RuntimeCtx, slot: &mut Option<String>, m: &MatchToken<'_>) -> MhchemResult<()> {
    let out = slot.get_or_insert_with(String::new);
    match m {
        MatchToken::S(s) => ctx.resources.append(out, s)?,
        MatchToken::A(v) => {
            for s in v {
                ctx.resources.append(out, s)?;
            }
        }
    }
    Ok(())
}

fn half_action(ctx: &RuntimeCtx, m: &MatchToken) -> MhchemResult<Vec<Value>> {
    let text = token_string(ctx, m)?;
    let mut s = text.as_ref();
    let mut ret: Vec<Value> = vec![];
    if s.starts_with('+') || s.starts_with('-') {
        ret.push(ctx.value(&s[..1])?);
        s = &s[1..];
    }
    let Some(c) = RE_HALF.captures(s) else {
        return Ok(vec![]);
    };
    let n1 = remove_dollars(ctx, c.get(1).unwrap().as_str())?;
    let n2 = c.get(2).unwrap().as_str();
    ret.push(node!(ctx, {"type_": "frac", "p1": n1, "p2": n2}));
    if let Some(t) = c.get(3) {
        let t3 = remove_dollars(ctx, t.as_str())?;
        ret.push(node!(ctx, {"type_": "tex-math", "p1": t3}));
    }
    Ok(ret)
}

fn replace_regex(
    ctx: &RuntimeCtx,
    re: &Regex,
    input: &str,
    replacement: &str,
) -> MhchemResult<String> {
    let mut out = String::new();
    let mut end = 0;
    for m in re.find_iter(input) {
        ctx.resources.append(&mut out, &input[end..m.start()])?;
        ctx.resources.append(&mut out, replacement)?;
        end = m.end();
    }
    ctx.resources.append(&mut out, &input[end..])?;
    Ok(out)
}
fn remove_dollars(ctx: &RuntimeCtx, input: &str) -> MhchemResult<String> {
    let mut out = String::new();
    for s in input.split('$') {
        ctx.resources.append(&mut out, s)?;
    }
    Ok(out)
}
