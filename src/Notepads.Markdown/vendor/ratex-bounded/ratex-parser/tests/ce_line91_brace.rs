//! Debug: lexer brace balance for nested \\ce in \\underset (golden case 0091).

use ratex_lexer::Lexer;
use ratex_parser::macro_expander::MacroExpander;
use ratex_parser::mhchem::chem_parse_str;
use ratex_parser::parse_node::Mode;
use ratex_parser::parser::parse;

#[test]
fn lexer_balances_outer_ce_braces() {
    let s = r"\ce{Hg^2+ ->[I-]  $\underset{\mathrm{red}}{\ce{HgI2}}$  ->[I-]  $\underset{\mathrm{red}}{\ce{[Hg^{II}I4]^2-}}$}";
    let mut lex = Lexer::new(s);
    assert_eq!(lex.lex().text, "\\ce");
    assert_eq!(lex.lex().text, "{");
    let mut depth: i32 = 1;
    loop {
        let t = lex.lex();
        if t.is_eof() {
            panic!("EOF with depth {depth}");
        }
        match t.text.as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            break;
        }
    }
}

#[test]
fn macro_expander_consume_arg_after_ce() {
    let s = r"\ce{Hg^2+ ->[I-]  $\underset{\mathrm{red}}{\ce{HgI2}}$  ->[I-]  $\underset{\mathrm{red}}{\ce{[Hg^{II}I4]^2-}}$}";
    let mut g = MacroExpander::new(s, Mode::Math);
    assert_eq!(g.pop_token().text, "\\ce");
    g.consume_arg(None).expect("consume \\ce arg");
}

#[test]
fn chem_parse_output_then_parse_full_tex() {
    let inner = r"Hg^2+ ->[I-]  $\underset{\mathrm{red}}{\ce{HgI2}}$  ->[I-]  $\underset{\mathrm{red}}{\ce{[Hg^{II}I4]^2-}}$";
    let tex = chem_parse_str(inner, "ce").expect("mhchem");
    parse(&tex).expect("expanded ce should parse");
}
