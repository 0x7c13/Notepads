/// Comprehensive test suite for ratex-parser, organized by feature area.
/// Tests are designed to validate against KaTeX's parsing behavior.
#[cfg(test)]
mod core_parsing {
    use crate::parse_node::ParseNode;
    use crate::parser::parse;

    // ── Basic characters ─────────────────────────────────

    #[test]
    fn single_letter() {
        let ast = parse("x").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "mathord");
        assert_eq!(ast[0].symbol_text(), Some("x"));
    }

    #[test]
    fn multiple_letters() {
        let ast = parse("abc").unwrap();
        assert_eq!(ast.len(), 3);
        assert!(ast.iter().all(|n| n.type_name() == "mathord"));
    }

    #[test]
    fn digit() {
        let ast = parse("5").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "textord");
        assert_eq!(ast[0].symbol_text(), Some("5"));
    }

    #[test]
    fn unicode_white_circle_is_math_textord() {
        let ast = parse("○").unwrap();
        assert_eq!(ast.len(), 1);
        assert!(matches!(
            &ast[0],
            ParseNode::TextOrd {
                mode: crate::Mode::Math,
                text,
                ..
            } if text == "○"
        ));
    }

    #[test]
    fn empty_input() {
        let ast = parse("").unwrap();
        assert!(ast.is_empty());
    }

    // ── Operators and symbols ────────────────────────────

    #[test]
    fn binary_operator_plus() {
        let ast = parse("a+b").unwrap();
        assert_eq!(ast.len(), 3);
        assert_eq!(ast[1].type_name(), "atom");
        if let ParseNode::Atom { family, text, .. } = &ast[1] {
            assert_eq!(*family, crate::parse_node::AtomFamily::Bin);
            assert_eq!(text, "+");
        }
    }

    #[test]
    fn relation_equals() {
        let ast = parse("a=b").unwrap();
        assert_eq!(ast.len(), 3);
        assert_eq!(ast[1].type_name(), "atom");
        if let ParseNode::Atom { family, .. } = &ast[1] {
            assert_eq!(*family, crate::parse_node::AtomFamily::Rel);
        }
    }

    #[test]
    fn dotsc_before_comma_omits_thinspace() {
        let ast = parse("x,\\dotsc,y").unwrap();
        assert!(!ast
            .iter()
            .any(|node| matches!(node, ParseNode::Kern { .. })));
        assert_eq!(ast.len(), 5);
        assert_eq!(ast[2].symbol_text(), Some("\\ldots"));
        assert_eq!(ast[3].symbol_text(), Some(","));
    }

    #[test]
    fn dotsc_before_selected_punctuation_keeps_thinspace() {
        let ast = parse("x,\\dotsc;y").unwrap();
        assert_eq!(ast.len(), 6);
        assert_eq!(ast[2].symbol_text(), Some("\\ldots"));
        assert!(matches!(ast[3], ParseNode::Kern { .. }));
        assert_eq!(ast[4].symbol_text(), Some(";"));
    }

    #[test]
    fn dots_selects_low_dots_before_comma() {
        let ast = parse("x,\\dots,y").unwrap();
        assert_eq!(ast[2].symbol_text(), Some("\\ldots"));
        assert!(!matches!(ast[3], ParseNode::Kern { .. }));
    }

    #[test]
    fn dots_selects_centered_dots_before_binary_operator() {
        let ast = parse("x\\dots+y").unwrap();
        assert_eq!(ast[1].symbol_text(), Some("\\@cdots"));
    }

    #[test]
    fn dots_selects_integral_spacing_after_one_expansion() {
        let ast = parse("\\def\\next{\\int}x\\dots\\next y").unwrap();
        assert!(matches!(ast[1], ParseNode::Kern { .. }));
        assert_eq!(ast[2].symbol_text(), Some("\\@cdots"));
        assert_eq!(ast[3].type_name(), "op");
    }

    #[test]
    fn explicit_cdots_adds_thinspace_before_comma() {
        let ast = parse("x\\cdots,y").unwrap();
        assert_eq!(ast[1].symbol_text(), Some("\\@cdots"));
        assert!(matches!(ast[2], ParseNode::Kern { .. }));
    }

    #[test]
    fn open_paren() {
        let ast = parse("(").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "atom");
        if let ParseNode::Atom { family, .. } = &ast[0] {
            assert_eq!(*family, crate::parse_node::AtomFamily::Open);
        }
    }

    #[test]
    fn close_paren() {
        let ast = parse(")").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::Atom { family, .. } = &ast[0] {
            assert_eq!(*family, crate::parse_node::AtomFamily::Close);
        }
    }

    // ── Grouping ─────────────────────────────────────────

    #[test]
    fn braced_group() {
        let ast = parse("{a+b}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "ordgroup");
        if let ParseNode::OrdGroup { body, .. } = &ast[0] {
            assert_eq!(body.len(), 3);
        }
    }

    #[test]
    fn nested_groups() {
        let ast = parse("{{x}}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::OrdGroup { body, .. } = &ast[0] {
            assert_eq!(body.len(), 1);
            assert_eq!(body[0].type_name(), "ordgroup");
        }
    }

    #[test]
    fn empty_group() {
        let ast = parse("{}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::OrdGroup { body, .. } = &ast[0] {
            assert!(body.is_empty());
        }
    }

    // ── Super/subscripts ─────────────────────────────────

    #[test]
    fn superscript() {
        let ast = parse("x^2").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::SupSub { base, sup, sub, .. } = &ast[0] {
            assert!(base.is_some());
            assert!(sup.is_some());
            assert!(sub.is_none());
            assert_eq!(sup.as_ref().unwrap().symbol_text(), Some("2"));
        }
    }

    #[test]
    fn subscript() {
        let ast = parse("a_i").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::SupSub { base, sup, sub, .. } = &ast[0] {
            assert!(base.is_some());
            assert!(sup.is_none());
            assert!(sub.is_some());
        }
    }

    #[test]
    fn both_sup_sub() {
        let ast = parse("x^2_i").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::SupSub { sup, sub, .. } = &ast[0] {
            assert!(sup.is_some());
            assert!(sub.is_some());
        }
    }

    #[test]
    fn sub_then_sup() {
        let ast = parse("x_i^2").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::SupSub { sup, sub, .. } = &ast[0] {
            assert!(sup.is_some());
            assert!(sub.is_some());
        }
    }

    #[test]
    fn grouped_superscript() {
        let ast = parse("x^{2+3}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::SupSub { sup, .. } = &ast[0] {
            let s = sup.as_ref().unwrap();
            assert_eq!(s.type_name(), "ordgroup");
        }
    }

    #[test]
    fn double_superscript_error() {
        assert!(parse("x^2^3").is_err());
    }

    #[test]
    fn double_subscript_error() {
        assert!(parse("x_2_3").is_err());
    }
}

#[cfg(test)]
mod fractions_and_radicals {
    use crate::parse_node::ParseNode;
    use crate::parser::parse;

    #[test]
    fn simple_frac() {
        let ast = parse("\\frac{a}{b}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "genfrac");
        if let ParseNode::GenFrac {
            has_bar_line,
            numer,
            denom,
            ..
        } = &ast[0]
        {
            assert!(has_bar_line);
            // numer and denom are wrapped in OrdGroup
            if let ParseNode::OrdGroup { body, .. } = numer.as_ref() {
                assert_eq!(body.len(), 1);
                assert_eq!(body[0].symbol_text(), Some("a"));
            }
            if let ParseNode::OrdGroup { body, .. } = denom.as_ref() {
                assert_eq!(body.len(), 1);
                assert_eq!(body[0].symbol_text(), Some("b"));
            }
        }
    }

    #[test]
    fn frac_with_expressions() {
        let ast = parse("\\frac{a^2 + b}{c}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::GenFrac { numer, .. } = &ast[0] {
            if let ParseNode::OrdGroup { body, .. } = numer.as_ref() {
                assert!(body.len() >= 3); // a^2, +, b (with supsub)
            }
        }
    }

    #[test]
    fn dfrac() {
        let ast = parse("\\dfrac{a}{b}").unwrap();
        // dfrac wraps in styling node
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "styling");
    }

    #[test]
    fn tfrac() {
        let ast = parse("\\tfrac{a}{b}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "styling");
    }

    #[test]
    fn binom() {
        let ast = parse("\\binom{n}{k}").unwrap();
        assert_eq!(ast.len(), 1);
        // binom has delimiters and no bar line
        let check = |node: &ParseNode| {
            if let ParseNode::GenFrac {
                has_bar_line,
                left_delim,
                right_delim,
                ..
            } = node
            {
                assert!(!has_bar_line);
                assert_eq!(left_delim.as_deref(), Some("("));
                assert_eq!(right_delim.as_deref(), Some(")"));
            }
        };
        // Might be wrapped in styling
        if ast[0].type_name() == "genfrac" {
            check(&ast[0]);
        } else if let ParseNode::Styling { body, .. } = &ast[0] {
            check(&body[0]);
        }
    }

    #[test]
    fn sqrt_simple() {
        let ast = parse("\\sqrt{x}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "sqrt");
        if let ParseNode::Sqrt { index, .. } = &ast[0] {
            assert!(index.is_none());
        }
    }

    #[test]
    fn sqrt_with_index() {
        let ast = parse("\\sqrt[3]{x}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::Sqrt { index, body, .. } = &ast[0] {
            assert!(index.is_some());
            // body is an OrdGroup wrapping x
            assert_eq!(body.type_name(), "ordgroup");
        }
    }

    #[test]
    fn nested_frac_sqrt() {
        let ast = parse("\\frac{\\sqrt{a^2+b^2}}{c}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "genfrac");
        if let ParseNode::GenFrac { numer, .. } = &ast[0] {
            if let ParseNode::OrdGroup { body, .. } = numer.as_ref() {
                assert_eq!(body[0].type_name(), "sqrt");
            }
        }
    }
}

#[cfg(test)]
mod operators {
    use crate::parse_node::ParseNode;
    use crate::parser::parse;

    #[test]
    fn sum_symbol() {
        let ast = parse("\\sum").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "op");
        if let ParseNode::Op {
            symbol,
            limits,
            name,
            ..
        } = &ast[0]
        {
            assert!(symbol);
            assert!(limits);
            assert_eq!(name.as_deref(), Some("\\sum"));
        }
    }

    #[test]
    fn unicode_big_operators_normalize_to_command_forms() {
        fn op_name(node: &ParseNode) -> Option<&str> {
            match node {
                ParseNode::Op {
                    name,
                    symbol: true,
                    limits: true,
                    ..
                } => name.as_deref(),
                _ => None,
            }
        }

        for (unicode, command) in [
            ("∏", "\\prod"),
            ("∐", "\\coprod"),
            ("∑", "\\sum"),
            ("⋀", "\\bigwedge"),
            ("⋁", "\\bigvee"),
            ("⋂", "\\bigcap"),
            ("⋃", "\\bigcup"),
            ("⨀", "\\bigodot"),
            ("⨁", "\\bigoplus"),
            ("⨂", "\\bigotimes"),
            ("⨄", "\\biguplus"),
            ("⨆", "\\bigsqcup"),
        ] {
            let unicode_ast = parse(unicode).unwrap();
            let command_ast = parse(command).unwrap();

            assert_eq!(op_name(&unicode_ast[0]), Some(command));
            assert_eq!(op_name(&command_ast[0]), Some(command));
        }
    }

    #[test]
    fn int_symbol() {
        let ast = parse("\\int").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::Op { symbol, limits, .. } = &ast[0] {
            assert!(symbol);
            assert!(!limits); // integrals don't use limits by default
        }
    }

    #[test]
    fn lim_text_op() {
        let ast = parse("\\lim").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::Op {
            symbol,
            limits,
            name,
            ..
        } = &ast[0]
        {
            assert!(!symbol);
            assert!(limits);
            assert_eq!(name.as_deref(), Some("\\lim"));
        }
    }

    #[test]
    fn sin_text_op() {
        let ast = parse("\\sin").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::Op { symbol, limits, .. } = &ast[0] {
            assert!(!symbol);
            assert!(!limits);
        }
    }

    #[test]
    fn sum_with_limits() {
        let ast = parse("\\sum_{i=0}^{n}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "supsub");
        if let ParseNode::SupSub { base, sup, sub, .. } = &ast[0] {
            assert_eq!(base.as_ref().unwrap().type_name(), "op");
            assert!(sup.is_some());
            assert!(sub.is_some());
        }
    }

    #[test]
    fn sum_with_explicit_limits_forces_supsub_handling() {
        let ast = parse("\\sum\\limits_{i=0}^{n}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::SupSub { base, .. } = &ast[0] {
            if let ParseNode::Op {
                limits,
                always_handle_sup_sub,
                ..
            } = base.as_ref().unwrap().as_ref()
            {
                assert!(*limits);
                assert_eq!(*always_handle_sup_sub, Some(true));
            } else {
                panic!("expected op base");
            }
        } else {
            panic!("expected supsub");
        }
    }

    #[test]
    fn sum_with_explicit_nolimits_does_not_force_supsub_handling() {
        let ast = parse("\\sum\\nolimits_{i=0}^{n}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::SupSub { base, .. } = &ast[0] {
            if let ParseNode::Op {
                limits,
                always_handle_sup_sub,
                ..
            } = base.as_ref().unwrap().as_ref()
            {
                assert!(!*limits);
                assert_eq!(*always_handle_sup_sub, Some(false));
            } else {
                panic!("expected op base");
            }
        } else {
            panic!("expected supsub");
        }
    }
}

#[cfg(test)]
mod accents_and_fonts {
    use crate::parse_node::ParseNode;
    use crate::parser::parse;

    #[test]
    fn hat_accent() {
        let ast = parse("\\hat{x}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "accent");
        if let ParseNode::Accent {
            label, is_stretchy, ..
        } = &ast[0]
        {
            assert_eq!(label, "\\hat");
            assert_eq!(*is_stretchy, Some(false));
        }
    }

    #[test]
    fn widehat_accent() {
        let ast = parse("\\widehat{ABC}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::Accent {
            label, is_stretchy, ..
        } = &ast[0]
        {
            assert_eq!(label, "\\widehat");
            assert_eq!(*is_stretchy, Some(true));
        }
    }

    #[test]
    fn multidot_accents_have_dedicated_ast_nodes() {
        for (formula, expected_label) in [("\\dddot{x}", "\\dddot"), ("\\ddddot{x}", "\\ddddot")] {
            let ast = parse(formula).unwrap();
            assert_eq!(ast.len(), 1);
            if let ParseNode::Accent { label, .. } = &ast[0] {
                assert_eq!(label, expected_label);
            } else {
                panic!("expected dedicated accent node for {formula}");
            }
        }
    }

    #[test]
    fn mathbf() {
        let ast = parse("\\mathbf{A}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "font");
        if let ParseNode::Font { font, .. } = &ast[0] {
            assert_eq!(font, "mathbf");
        }
    }

    #[test]
    fn mathit() {
        let ast = parse("\\mathit{x}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::Font { font, .. } = &ast[0] {
            assert_eq!(font, "mathit");
        }
    }

    #[test]
    fn text_function() {
        let ast = parse("\\text{hello}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "text");
        if let ParseNode::Text { body, .. } = &ast[0] {
            assert!(!body.is_empty());
        }
    }
}

#[cfg(test)]
mod delimiters {
    use crate::parse_node::ParseNode;
    use crate::parser::parse;

    #[test]
    fn left_right_parens() {
        let ast = parse("\\left( x \\right)").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "leftright");
        if let ParseNode::LeftRight {
            left, right, body, ..
        } = &ast[0]
        {
            assert_eq!(left, "(");
            assert_eq!(right, ")");
            assert!(!body.is_empty());
        }
    }

    #[test]
    fn left_right_with_frac() {
        let ast = parse("\\left( \\frac{a}{b} \\right)").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::LeftRight { body, .. } = &ast[0] {
            assert_eq!(body[0].type_name(), "genfrac");
        }
    }

    #[test]
    fn right_without_left_error() {
        assert!(parse("\\right)").is_err());
    }
}

#[cfg(test)]
mod colors_and_sizing {
    use crate::parse_node::ParseNode;
    use crate::parser::parse;

    #[test]
    fn textcolor() {
        let ast = parse("\\textcolor{red}{x}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "color");
        if let ParseNode::Color { color, body, .. } = &ast[0] {
            assert_eq!(color, "red");
            assert!(!body.is_empty());
        }
    }

    #[test]
    fn overline() {
        let ast = parse("\\overline{x}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "overline");
    }

    #[test]
    fn underline() {
        let ast = parse("\\underline{x}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "underline");
    }

    #[test]
    fn htmlstyle() {
        let ast = parse("\\htmlStyle{color: blue; font-size: 20px;}{x^2}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "html");
        if let ParseNode::Html {
            attributes, body, ..
        } = &ast[0]
        {
            assert_eq!(
                attributes.get("style").unwrap(),
                "color: blue; font-size: 20px;"
            );
            assert!(!body.is_empty());
        } else {
            panic!("Expected html node");
        }
    }
}

#[cfg(test)]
mod complex_expressions {
    use crate::parser::parse;

    #[test]
    fn quadratic_formula() {
        let ast = parse("\\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "genfrac");
    }

    #[test]
    fn euler_identity() {
        let ast = parse("e^{i\\pi} + 1 = 0").unwrap();
        assert!(ast.len() >= 4); // supsub, +, 1, =, 0
    }

    #[test]
    fn sin_squared() {
        let ast = parse("\\sin^2(x) + \\cos^2(x) = 1").unwrap();
        assert!(ast.len() >= 5);
    }

    #[test]
    fn nested_fractions() {
        let ast = parse("\\frac{1}{1+\\frac{1}{x}}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "genfrac");
    }

    #[test]
    fn multiple_superscripts_in_expr() {
        let ast = parse("a^2 + b^2 = c^2").unwrap();
        let supsub_count = ast.iter().filter(|n| n.type_name() == "supsub").count();
        assert_eq!(supsub_count, 3);
    }
}

#[cfg(test)]
mod error_handling {
    use crate::parser::parse;

    #[test]
    fn unclosed_brace() {
        assert!(parse("{x").is_err());
    }

    #[test]
    fn extra_close_brace() {
        assert!(parse("x}").is_err());
    }

    #[test]
    fn missing_frac_arg() {
        assert!(parse("\\frac{a}").is_err());
    }

    #[test]
    fn double_superscript() {
        assert!(parse("x^1^2").is_err());
    }

    #[test]
    fn double_subscript() {
        assert!(parse("x_1_2").is_err());
    }

    #[test]
    fn undefined_command() {
        assert!(parse("\\nonexistentcommand").is_err());
    }
}

#[cfg(test)]
mod json_serialization {
    use crate::parser::parse;

    #[test]
    fn basic_json_output() {
        let ast = parse("x").unwrap();
        let json = serde_json::to_value(&ast).unwrap();
        let arr = json.as_array().unwrap();
        assert_eq!(arr[0]["type"], "mathord");
        assert_eq!(arr[0]["mode"], "math");
        assert_eq!(arr[0]["text"], "x");
    }

    #[test]
    fn supsub_json_structure() {
        let ast = parse("x^2").unwrap();
        let json = serde_json::to_value(&ast).unwrap();
        let node = &json[0];
        assert_eq!(node["type"], "supsub");
        assert_eq!(node["base"]["type"], "mathord");
        assert_eq!(node["sup"]["type"], "textord");
        assert_eq!(node["sup"]["text"], "2");
    }

    #[test]
    fn frac_json_structure() {
        let ast = parse("\\frac{a}{b}").unwrap();
        let json = serde_json::to_value(&ast).unwrap();
        let node = &json[0];
        assert_eq!(node["type"], "genfrac");
        assert_eq!(node["hasBarLine"], true);
        assert_eq!(node["numer"]["type"], "ordgroup");
        assert_eq!(node["denom"]["type"], "ordgroup");
    }

    #[test]
    fn atom_json_structure() {
        let ast = parse("+").unwrap();
        let json = serde_json::to_value(&ast).unwrap();
        let node = &json[0];
        assert_eq!(node["type"], "atom");
        assert_eq!(node["family"], "bin");
        assert_eq!(node["text"], "+");
    }
}

// ── Environment tests ────────────────────────────────────────────────────

#[cfg(test)]
mod environments {
    use crate::parse_node::ParseNode;
    use crate::parser::parse;

    #[test]
    fn simple_matrix() {
        let ast = parse("\\begin{matrix} a & b \\\\ c & d \\end{matrix}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "array");
        if let ParseNode::Array { body, .. } = &ast[0] {
            assert_eq!(body.len(), 2);
            assert_eq!(body[0].len(), 2);
            assert_eq!(body[1].len(), 2);
        } else {
            panic!("Expected Array node");
        }
    }

    #[test]
    fn pmatrix_wraps_in_leftright() {
        let ast = parse("\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "leftright");
        if let ParseNode::LeftRight {
            left, right, body, ..
        } = &ast[0]
        {
            assert_eq!(left, "(");
            assert_eq!(right, ")");
            assert_eq!(body.len(), 1);
            assert_eq!(body[0].type_name(), "array");
        } else {
            panic!("Expected LeftRight node");
        }
    }

    #[test]
    fn bmatrix_wraps_in_leftright() {
        let ast = parse("\\begin{bmatrix} 1 & 2 \\\\ 3 & 4 \\end{bmatrix}").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::LeftRight { left, right, .. } = &ast[0] {
            assert_eq!(left, "[");
            assert_eq!(right, "]");
        } else {
            panic!("Expected LeftRight node");
        }
    }

    #[test]
    fn vmatrix_wraps_in_leftright() {
        let ast = parse("\\begin{vmatrix} a & b \\\\ c & d \\end{vmatrix}").unwrap();
        if let ParseNode::LeftRight { left, right, .. } = &ast[0] {
            assert_eq!(left, "|");
            assert_eq!(right, "|");
        } else {
            panic!("Expected LeftRight node");
        }
    }

    #[test]
    fn big_bmatrix_wraps_in_leftright() {
        let ast = parse("\\begin{Bmatrix} a \\\\ b \\end{Bmatrix}").unwrap();
        if let ParseNode::LeftRight { left, right, .. } = &ast[0] {
            assert_eq!(left, "\\{");
            assert_eq!(right, "\\}");
        } else {
            panic!("Expected LeftRight node");
        }
    }

    #[test]
    fn cases_environment() {
        let ast =
            parse("\\begin{cases} x & \\text{if } x > 0 \\\\ -x & \\text{otherwise} \\end{cases}")
                .unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "leftright");
        if let ParseNode::LeftRight {
            left, right, body, ..
        } = &ast[0]
        {
            assert_eq!(left, "\\{");
            assert_eq!(right, ".");
            assert_eq!(body.len(), 1);
            if let ParseNode::Array { body: rows, .. } = &body[0] {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].len(), 2);
                assert_eq!(rows[1].len(), 2);
            } else {
                panic!("Expected Array inside LeftRight");
            }
        } else {
            panic!("Expected LeftRight node for cases");
        }
    }

    #[test]
    fn align_environment() {
        let ast = parse("\\begin{aligned} x &= 1 \\\\ y &= 2 \\end{aligned}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "array");
        if let ParseNode::Array {
            body,
            col_separation_type,
            add_jot,
            ..
        } = &ast[0]
        {
            assert_eq!(body.len(), 2);
            assert!(add_jot.unwrap_or(false));
            assert_eq!(col_separation_type.as_deref(), Some("align"));
        } else {
            panic!("Expected Array node for aligned");
        }
    }

    #[test]
    fn tag_primitive_parses_argument() {
        let ast = parse("\\tag{1}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "tag");
    }

    #[test]
    fn align_with_tag_strips_tag_and_sets_array_tags() {
        use crate::parse_node::ArrayTag;
        let ast = parse("\\begin{aligned} x &= 1 \\tag{1} \\\\ y &= 2 \\end{aligned}").unwrap();
        if let ParseNode::Array { body, tags, .. } = &ast[0] {
            assert_eq!(body.len(), 2);
            let row0_last = &body[0][body[0].len() - 1];
            let inner = match row0_last {
                ParseNode::Styling { body: sb, .. } => &sb[0],
                n => n,
            };
            let ob = match inner {
                ParseNode::OrdGroup { body, .. } => body,
                _ => panic!("expected ordgroup"),
            };
            assert!(!ob.iter().any(|n| n.type_name() == "tag"));
            let tags = tags.as_ref().expect("tags");
            assert_eq!(tags.len(), 2);
            assert!(matches!(&tags[0], ArrayTag::Explicit(v) if !v.is_empty()));
            assert!(matches!(&tags[1], ArrayTag::Auto(false)));
        } else {
            panic!("Expected Array node");
        }
    }

    #[test]
    fn matrix_single_row() {
        let ast = parse("\\begin{matrix} a & b & c \\end{matrix}").unwrap();
        if let ParseNode::Array { body, .. } = &ast[0] {
            assert_eq!(body.len(), 1);
            assert_eq!(body[0].len(), 3);
        } else {
            panic!("Expected Array node");
        }
    }

    #[test]
    fn matrix_3x3() {
        let ast =
            parse("\\begin{matrix} 1 & 2 & 3 \\\\ 4 & 5 & 6 \\\\ 7 & 8 & 9 \\end{matrix}").unwrap();
        if let ParseNode::Array { body, cols, .. } = &ast[0] {
            assert_eq!(body.len(), 3);
            for row in body {
                assert_eq!(row.len(), 3);
            }
            let cols = cols.as_ref().unwrap();
            assert_eq!(cols.len(), 3);
        } else {
            panic!("Expected Array node");
        }
    }

    #[test]
    fn env_name_mismatch_error() {
        let result = parse("\\begin{matrix} a \\end{pmatrix}");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Mismatch"));
    }

    #[test]
    fn unknown_environment_error() {
        let result = parse("\\begin{foobar} a \\end{foobar}");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No such environment"));
    }

    #[test]
    fn gathered_environment() {
        let ast = parse("\\begin{gathered} a \\\\ b \\\\ c \\end{gathered}").unwrap();
        assert_eq!(ast[0].type_name(), "array");
        if let ParseNode::Array { body, .. } = &ast[0] {
            assert_eq!(body.len(), 3);
            for row in body {
                assert_eq!(row.len(), 1);
            }
        } else {
            panic!("Expected Array node");
        }
    }

    #[test]
    fn smallmatrix_environment() {
        let ast = parse("\\begin{smallmatrix} a & b \\\\ c & d \\end{smallmatrix}").unwrap();
        assert_eq!(ast[0].type_name(), "array");
        if let ParseNode::Array {
            arraystretch,
            col_separation_type,
            body,
            ..
        } = &ast[0]
        {
            assert_eq!(*arraystretch, 0.5);
            assert_eq!(col_separation_type.as_deref(), Some("small"));
            assert_eq!(body.len(), 2);
        } else {
            panic!("Expected Array node");
        }
    }

    #[test]
    fn matrix_json_structure() {
        let ast = parse("\\begin{matrix} a & b \\\\ c & d \\end{matrix}").unwrap();
        let json = serde_json::to_value(&ast).unwrap();
        let node = &json[0];
        assert_eq!(node["type"], "array");
        let body = node["body"].as_array().unwrap();
        assert_eq!(body.len(), 2);
        assert_eq!(body[0].as_array().unwrap().len(), 2);
    }

    #[test]
    fn pmatrix_json_structure() {
        let ast = parse("\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}").unwrap();
        let json = serde_json::to_value(&ast).unwrap();
        let node = &json[0];
        assert_eq!(node["type"], "leftright");
        assert_eq!(node["left"], "(");
        assert_eq!(node["right"], ")");
        let inner = &node["body"][0];
        assert_eq!(inner["type"], "array");
    }

    #[test]
    fn cases_json_structure() {
        let ast = parse("\\begin{cases} a \\\\ b \\end{cases}").unwrap();
        let json = serde_json::to_value(&ast).unwrap();
        let node = &json[0];
        assert_eq!(node["type"], "leftright");
        assert_eq!(node["left"], "\\{");
        assert_eq!(node["right"], ".");
    }

    #[test]
    fn nested_frac_in_matrix() {
        let ast = parse("\\begin{pmatrix} \\frac{1}{2} & 0 \\\\ 0 & \\frac{3}{4} \\end{pmatrix}")
            .unwrap();
        assert_eq!(ast[0].type_name(), "leftright");
    }

    #[test]
    fn matrix_with_expressions() {
        let ast = parse("\\begin{bmatrix} a+b & c^2 \\\\ \\sqrt{d} & e_i \\end{bmatrix}").unwrap();
        assert_eq!(ast[0].type_name(), "leftright");
    }

    #[test]
    fn rcases_environment() {
        let ast = parse("\\begin{rcases} a \\\\ b \\end{rcases}").unwrap();
        if let ParseNode::LeftRight { left, right, .. } = &ast[0] {
            assert_eq!(left, ".");
            assert_eq!(right, "\\}");
        } else {
            panic!("Expected LeftRight node for rcases");
        }
    }

    #[test]
    fn prooftree_unary_with_label_and_abbreviations() {
        let ast = parse("\\begin{prooftree}\\AXC{P}\\RL{r}\\UIC{Q}\\end{prooftree}").unwrap();
        assert_eq!(ast.len(), 1);
        assert_eq!(ast[0].type_name(), "proofTree");
        if let ParseNode::ProofTree { tree, .. } = &ast[0] {
            assert_eq!(tree.premises.len(), 1);
            assert!(tree.left_label.is_none());
            assert!(tree.right_label.is_some());
            assert_eq!(tree.conclusion.len(), 1);
        } else {
            panic!("Expected ProofTree node");
        }
    }

    #[test]
    fn prooftree_binary_dashed_line() {
        let ast = parse(
            "\\begin{prooftree}\\AxiomC{P}\\AxiomC{Q}\\dashedLine\\BinaryInfC{R}\\end{prooftree}",
        )
        .unwrap();
        if let ParseNode::ProofTree { tree, .. } = &ast[0] {
            assert_eq!(tree.premises.len(), 2);
            assert!(matches!(
                tree.line_style,
                crate::parse_node::ProofLineStyle::Dashed
            ));
        } else {
            panic!("Expected ProofTree node");
        }
    }

    #[test]
    fn prooftree_errors_on_short_stack() {
        let result = parse("\\begin{prooftree}\\AxiomC{P}\\BinaryInfC{Q}\\end{prooftree}");
        assert!(result.is_err());
    }

    #[test]
    fn prooftree_fcenter_renders_visible_symbol() {
        let ast =
            parse("\\begin{prooftree}\\AxiomC{A \\fCenter B}\\UIC{C}\\end{prooftree}").unwrap();
        if let ParseNode::ProofTree { tree, .. } = &ast[0] {
            // The axiom's conclusion contains A \fCenter B, stored in premises[0].conclusion
            let axiom = &tree.premises[0];
            let has_arrow_rel = axiom.conclusion.iter().any(|n| match n {
                ParseNode::Atom { family, text, .. } => {
                    matches!(family, crate::parse_node::AtomFamily::Rel) && text == "\\Rightarrow"
                }
                _ => false,
            });
            assert!(
                has_arrow_rel,
                "\\fCenter should produce a relation arrow (\\\\Rightarrow) in the axiom"
            );
        } else {
            panic!("Expected ProofTree node");
        }
    }

    #[test]
    fn prooftree_root_at_top_flag() {
        let ast =
            parse("\\begin{prooftree}\\AxiomC{P}\\rootAtTop\\UIC{Q}\\end{prooftree}").unwrap();
        if let ParseNode::ProofTree { tree, .. } = &ast[0] {
            assert!(tree.root_at_top, "\\rootAtTop should set root_at_top flag");
        } else {
            panic!("Expected ProofTree node");
        }
    }

    #[test]
    fn prooftree_root_at_bottom_is_default() {
        let ast = parse("\\begin{prooftree}\\AxiomC{P}\\UIC{Q}\\end{prooftree}").unwrap();
        if let ParseNode::ProofTree { tree, .. } = &ast[0] {
            assert!(!tree.root_at_top, "root_at_top should default to false");
        } else {
            panic!("Expected ProofTree node");
        }
    }

    #[test]
    fn prooftree_orphan_label_errors() {
        let result = parse("\\begin{prooftree}\\AxiomC{P}\\LeftLabel{L}\\end{prooftree}");
        assert!(
            result.is_err(),
            "orphan \\LeftLabel should produce an error"
        );
    }

    #[test]
    fn vmatrix_double_wraps() {
        let ast = parse("\\begin{Vmatrix} a & b \\\\ c & d \\end{Vmatrix}").unwrap();
        if let ParseNode::LeftRight { left, right, .. } = &ast[0] {
            assert_eq!(left, "\\Vert");
            assert_eq!(right, "\\Vert");
        } else {
            panic!("Expected LeftRight node");
        }
    }
}

#[cfg(test)]
mod verb {
    use crate::parse_node::ParseNode;
    use crate::parser::parse;

    #[test]
    fn ascii_delimiter() {
        let ast = parse("\\verb|hello|").unwrap();
        assert_eq!(ast.len(), 1);
        if let ParseNode::Verb { body, star, .. } = &ast[0] {
            assert_eq!(body, "hello");
            assert!(!star);
        } else {
            panic!("Expected Verb node");
        }
    }

    #[test]
    fn starred_ascii_delimiter() {
        let ast = parse("\\verb*|hello world|").unwrap();
        if let ParseNode::Verb { body, star, .. } = &ast[0] {
            assert_eq!(body, "hello world");
            assert!(star);
        } else {
            panic!("Expected Verb node");
        }
    }

    #[test]
    fn multibyte_delimiter_does_not_panic() {
        let ast = parse("\\verbéxé").unwrap();
        if let ParseNode::Verb { body, star, .. } = &ast[0] {
            assert_eq!(body, "x");
            assert!(!star);
        } else {
            panic!("Expected Verb node");
        }
    }

    #[test]
    fn starred_multibyte_delimiter_does_not_panic() {
        let ast = parse("\\verb*éxé").unwrap();
        if let ParseNode::Verb { body, star, .. } = &ast[0] {
            assert_eq!(body, "x");
            assert!(star);
        } else {
            panic!("Expected Verb node");
        }
    }

    #[test]
    fn too_short_returns_error() {
        assert!(parse("\\verbé").is_err());
    }
}

#[cfg(test)]
mod recursion_limit {
    use crate::error::ParseError;
    use crate::parser::parse;

    fn nested_braces(n: usize) -> String {
        format!("{}{}{}", "{".repeat(n), "x", "}".repeat(n))
    }

    fn assert_recursion_limit_err(input: &str) {
        let err = parse(input).unwrap_err();
        assert!(
            err.to_string().contains("Recursion limit exceeded"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn recursion_limit_error_message() {
        let err = ParseError::recursion_limit_exceeded();
        assert!(err.to_string().contains("Recursion limit exceeded"));
    }

    #[test]
    fn nested_braces_at_limit_succeeds() {
        assert!(parse(&nested_braces(32)).is_ok());
    }

    #[test]
    fn nested_braces_over_limit_fails() {
        assert_recursion_limit_err(&nested_braces(33));
    }

    #[test]
    fn poc_deep_nesting_does_not_abort() {
        assert_recursion_limit_err(&nested_braces(300));
    }

    #[test]
    fn contextual_dots_expansion_depth_is_bounded() {
        assert!(parse(&r"\dots".repeat(32)).is_ok());
        assert_recursion_limit_err(&r"\dots".repeat(33));
        assert_recursion_limit_err(&r"\dots".repeat(300));
    }

    #[test]
    fn runtime_depth_ignores_non_structural_braces() {
        assert!(parse(&format!(r"\verb|{}|", "{".repeat(300))).is_ok());
        assert!(parse(&format!(r"\verbéà{}é", "{".repeat(300))).is_ok());
        assert!(parse(&format!("% {}\nx", "{".repeat(300))).is_ok());
        assert!(parse(&r"\{".repeat(300)).is_ok());
        assert!(parse(&format!(r"{}x", r"\def\foo{\left(}".repeat(32))).is_ok());
    }

    #[test]
    fn unicode_accent_depth_is_bounded_in_math_and_text() {
        let accents_31 = "\u{301}".repeat(31);
        let accents_32 = "\u{301}".repeat(32);
        let accents_33 = "\u{301}".repeat(33);
        let accents_4200 = "\u{301}".repeat(4_200);

        assert!(parse(&format!("x{accents_32}")).is_ok());
        assert!(parse(&format!(r"\text{{x{accents_31}}}")).is_ok());
        assert_recursion_limit_err(&format!(r"\text{{x{accents_32}}}"));
        assert_recursion_limit_err(&format!("x{accents_33}"));
        assert_recursion_limit_err(&format!(r"\text{{x{accents_33}}}"));
        assert_recursion_limit_err(&format!("x{accents_4200}"));
        assert_recursion_limit_err(&format!(r"\text{{x{accents_4200}}}"));
    }

    #[test]
    fn unicode_accent_depth_accounts_for_enclosing_groups() {
        let accent = "\u{301}";

        assert!(parse(&format!("{}x{accent}{}", "{".repeat(31), "}".repeat(31))).is_ok());
        assert_recursion_limit_err(&format!("{}x{accent}{}", "{".repeat(32), "}".repeat(32)));
    }

    #[test]
    fn non_latin_combining_marks_are_not_accent_trees() {
        let accents_33 = "\u{301}".repeat(33);
        let accents_4200 = "\u{301}".repeat(4_200);

        assert!(parse(&format!("α{accents_33}")).is_ok());
        assert!(parse(&format!("α{accents_4200}")).is_ok());
    }

    fn unary_prooftree(inferences: usize) -> String {
        format!(
            r"\begin{{prooftree}}\AxiomC{{P}}{}\end{{prooftree}}",
            r"\UnaryInfC{P}".repeat(inferences)
        )
    }

    fn braced_body(depth: usize) -> String {
        format!("{}x{}", "{".repeat(depth), "}".repeat(depth))
    }

    fn nested_ce(depth: usize) -> String {
        format!("{}H{}", r"\ce{".repeat(depth), "}".repeat(depth))
    }

    fn nested_tag(depth: usize) -> String {
        format!("{}{{x}}", r"\tag".repeat(depth))
    }

    fn unbraced_sqrt(depth: usize) -> String {
        format!("{}x", r"\sqrt".repeat(depth))
    }

    fn unbraced_delimiter_chain(command: &str, depth: usize) -> String {
        format!("{}(", command.repeat(depth))
    }

    fn nested_raisebox(depth: usize) -> String {
        let mut body = "x".to_owned();
        for _ in 0..depth {
            body = format!(r"\raisebox{{0pt}}{{{body}}}");
        }
        body
    }

    fn environment_with_explicit_tag(environment: &str, tag: &str) -> String {
        let body = if environment == "align" {
            r"x &= y"
        } else {
            "x"
        };
        format!(r"\begin{{{environment}}}{body}\tag{{{tag}}}\end{{{environment}}}")
    }

    fn nested_braket(depth: usize) -> String {
        let mut input = "x".to_string();
        for _ in 0..depth {
            input = format!(r"\Braket{{{input}}}");
        }
        input
    }

    fn prooftree_axiom_payload(payload_depth: usize) -> String {
        format!(
            r"\begin{{prooftree}}\AxiomC{{{}}}\end{{prooftree}}",
            braced_body(payload_depth)
        )
    }

    fn prooftree_left_label_payload(payload_depth: usize) -> String {
        format!(
            r"\begin{{prooftree}}\LeftLabel{{{}}}\AxiomC{{P}}\UnaryInfC{{Q}}\end{{prooftree}}",
            braced_body(payload_depth)
        )
    }

    fn macro_expanded_nested_ce(enclosing_depth: usize, ce_depth: usize) -> String {
        format!(
            r"\def\foo{{{}}}{}{}{}",
            nested_ce(ce_depth),
            "{".repeat(enclosing_depth),
            r"\foo",
            "}".repeat(enclosing_depth)
        )
    }

    #[test]
    fn prooftree_depth_is_bounded() {
        assert!(parse(&unary_prooftree(31)).is_ok());
        assert_recursion_limit_err(&unary_prooftree(32));
    }

    #[test]
    fn prooftree_depth_accounts_for_enclosing_groups() {
        assert!(parse(&format!(
            "{}{}{}",
            "{".repeat(31),
            unary_prooftree(0),
            "}".repeat(31)
        ))
        .is_ok());
        assert_recursion_limit_err(&format!(
            "{}{}{}",
            "{".repeat(31),
            unary_prooftree(1),
            "}".repeat(31)
        ));
    }

    #[test]
    fn prooftree_depth_includes_conclusion_and_label_payloads() {
        assert!(parse(&prooftree_axiom_payload(31)).is_ok());
        assert_recursion_limit_err(&prooftree_axiom_payload(32));

        assert!(parse(&prooftree_left_label_payload(31)).is_ok());
        assert_recursion_limit_err(&prooftree_left_label_payload(32));
    }

    #[test]
    fn unbraced_structural_arguments_are_bounded() {
        assert!(parse(&unbraced_sqrt(4_200)).is_err());

        assert!(parse(&nested_tag(32)).is_ok());
        assert_recursion_limit_err(&nested_tag(33));
        assert_recursion_limit_err(&nested_tag(4_200));
    }

    #[test]
    fn unbraced_delimiter_function_chains_fail_without_recursing() {
        assert!(parse(&unbraced_delimiter_chain(r"\bigl", 4_200)).is_err());
        assert!(parse(&unbraced_delimiter_chain(r"\left", 4_200)).is_err());
    }

    #[test]
    fn explicit_array_tags_are_included_in_final_ast_depth() {
        let tag = nested_raisebox(12);
        assert_recursion_limit_err(&environment_with_explicit_tag("equation", &tag));
        assert_recursion_limit_err(&environment_with_explicit_tag("align", &tag));
    }

    #[test]
    fn nested_braket_expansion_uses_the_shared_budget() {
        assert!(parse(&nested_braket(16)).is_ok());
        assert_recursion_limit_err(&nested_braket(33));
    }

    #[test]
    fn flat_macro_prefix_chains_are_iterative() {
        let input = format!(r"{}\def\foo{{x}}\foo", r"\global".repeat(4_200));
        assert!(parse(&input).is_ok());
    }

    #[test]
    fn mhchem_depth_matches_the_public_boundary() {
        assert!(parse(&nested_ce(31)).is_ok());
        assert_recursion_limit_err(&nested_ce(32));
    }

    #[test]
    fn mhchem_depth_accounts_for_macro_expanded_enclosing_groups() {
        assert!(parse(&macro_expanded_nested_ce(10, 10)).is_ok());
        assert_recursion_limit_err(&macro_expanded_nested_ce(20, 20));
    }

    #[test]
    fn runtime_depth_ignores_latex_macro_definition_bodies() {
        let deep_body = braced_body(40);
        assert!(parse(&format!(r"\newcommand{{\foo}}[1]{{{deep_body}}}x")).is_ok());
        assert!(parse(&format!(
            r"\def\foo{{x}}\renewcommand{{\foo}}{{{deep_body}}}x"
        ))
        .is_ok());
        assert!(parse(&format!(r"\providecommand{{\foo}}{{{deep_body}}}x")).is_ok());
    }

    #[test]
    fn runtime_depth_ignores_flat_string_arguments() {
        let deep_body = braced_body(40);

        assert!(parse(&format!(r"\url{{{deep_body}}}")).is_ok());
        assert!(parse(&format!(r"\href{{{deep_body}}}{{x}}")).is_ok());
        assert!(parse(&format!(r"\htmlStyle{{{deep_body}}}{{x}}")).is_ok());
    }
}
