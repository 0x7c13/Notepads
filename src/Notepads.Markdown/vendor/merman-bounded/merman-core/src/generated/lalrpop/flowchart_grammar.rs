// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original generated grammar and upstream licenses are preserved.
// The selected generated parser uses explicit fallible grammar actions and MemoryScope stack hooks.
// auto-generated: "lalrpop 0.23.2"
// sha3: f72358728e6ffc954fb69e2901ec417d5079cc02f3f98c3af013622ef1867aaa
// features: 
use crate::diagrams::flowchart::{
  ArrowToken, ClassAssignStmt, ClassDefStmt, ClickStmt, DirectionStatementToken, Edge, FlowchartAst,
  FlowNodeProvenance, FlowNodeSyntax, LabeledText, LinkStyleStmt, LinkToken, Node, NodeLabelToken,
  StatementArena, StatementId, StatementList, Stmt, StyleStmt, SubgraphBlock, SubgraphHeader, TitleKind, Tok
};
use crate::SourceSpan;
#[allow(unused_extern_crates)]
extern crate lalrpop_util as __lalrpop_util;
#[allow(unused_imports)]
use self::__lalrpop_util::state_machine as __state_machine;
#[allow(unused_extern_crates)]
extern crate alloc;

#[rustfmt::skip]
#[allow(explicit_outlives_requirements, non_snake_case, non_camel_case_types, unused_mut, unused_variables, unused_imports, unused_parens, clippy::needless_lifetimes, clippy::type_complexity, clippy::needless_return, clippy::too_many_arguments, clippy::match_single_binding, clippy::clone_on_copy, clippy::unit_arg)]
mod __parse__FlowchartAst {

    use crate::diagrams::flowchart::{
  ArrowToken, ClassAssignStmt, ClassDefStmt, ClickStmt, DirectionStatementToken, Edge, FlowchartAst,
  FlowNodeProvenance, FlowNodeSyntax, LabeledText, LinkStyleStmt, LinkToken, Node, NodeLabelToken,
  StatementArena, StatementId, StatementList, Stmt, StyleStmt, SubgraphBlock, SubgraphHeader, TitleKind, Tok
};
    use crate::SourceSpan;
    #[allow(unused_extern_crates)]
    extern crate lalrpop_util as __lalrpop_util;
    #[allow(unused_imports)]
    use self::__lalrpop_util::state_machine as __state_machine;
    #[allow(unused_extern_crates)]
    extern crate alloc;
    use super::__ToTriple;
    #[allow(dead_code)]
    pub(crate) enum __Symbol<>
     {
        Variant0(Tok),
        Variant1(String),
        Variant2(DirectionStatementToken),
        Variant3(NodeLabelToken),
        Variant4(ArrowToken),
        Variant5(LabeledText),
        Variant6(SubgraphHeader),
        Variant7(StyleStmt),
        Variant8(ClassDefStmt),
        Variant9(ClassAssignStmt),
        Variant10(ClickStmt),
        Variant11(LinkStyleStmt),
        Variant12(usize),
        Variant13((Vec<Vec<Node>>, Vec<Vec<Edge>>)),
        Variant14(Vec<String>),
        Variant15(Option<String>),
        Variant16(Option<LabeledText>),
        Variant17((Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)),
        Variant18(Vec<(Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)>),
        Variant19(FlowchartAst),
        Variant20((String, Option<String>, SourceSpan)),
        Variant21(Vec<Node>),
        Variant22(Node),
        Variant23(()),
        Variant24(StatementId),
        Variant25(StatementList),
        Variant26(SubgraphBlock),
        Variant27(Option<SubgraphHeader>),
    }
    const __ACTION: &[i8] = &[
        // State 0
        -41, -41, -41, -41, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 1
        37, 35, 36, 38, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 2
        -41, -41, -41, -41, -41, -41, 3, 0, 0, 0, -41, -41, 0, 0, 0, 0, -41, -41, -41, -41, -41, 0, 0,
        // State 3
        0, 0, 0, 0, -41, 0, 3, 0, 0, 0, -41, -41, 0, 0, 0, 0, -41, -41, -41, -41, -41, 0, 0,
        // State 4
        0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 52, 10, 0, 0, 0, 0, 54, 50, 49, 51, 53, 0, 0,
        // State 5
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 56, 0,
        // State 6
        0, 0, 0, 0, 0, 0, 0, 14, 0, 0, 0, 0, 0, -30, 0, 0, 0, 0, 0, 0, 0, -30, 0,
        // State 7
        0, 0, 0, 0, 0, -41, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 8
        0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0, 18, 0, 0, 0, 0, 0, 0, 0,
        // State 9
        0, 0, 0, 0, 0, -5, -5, -5, 62, 0, 0, 0, 19, -5, 0, 0, 0, 0, 0, 0, 0, -5, 20,
        // State 10
        0, 0, 0, 0, 0, -3, -3, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 56, 0,
        // State 11
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 23, 0, 0, 22, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 12
        0, 0, 0, 0, 0, -28, -28, 14, 0, 0, 0, 0, 0, -31, 0, 0, 0, 0, 0, 0, 0, -31, 0,
        // State 13
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 23, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 14
        0, 0, 0, 0, -41, -41, 3, 0, 0, 0, -41, -41, 0, 0, 0, 0, -41, -41, -41, -41, -41, 0, 0,
        // State 15
        0, 0, 0, 0, 9, -59, 0, 0, 0, 0, 52, 10, 0, 0, 0, 0, 54, 50, 49, 51, 53, 0, 0,
        // State 16
        0, 0, 0, 0, -41, -41, 3, 0, 0, 0, -41, -41, 0, 0, 0, 0, -41, -41, -41, -41, -41, 0, 0,
        // State 17
        0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 18
        0, 0, 0, 0, 0, -5, -5, -5, 62, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, -5, 27,
        // State 19
        0, 0, 0, 0, 0, -47, -47, -5, 62, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, -5, 0,
        // State 20
        0, 0, 0, 0, 0, -30, -30, 14, 0, 0, 0, 0, 0, -30, 0, 0, 0, 0, 0, 0, 0, -30, 0,
        // State 21
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 23, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 22
        0, 0, 0, 0, 0, -5, -5, -5, 62, 0, 0, 0, 29, -5, 0, 0, 0, 0, 0, 0, 0, -5, 30,
        // State 23
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 23, 0, 0, 31, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 24
        0, 0, 0, 0, 9, -42, 0, 0, 0, 0, 52, 10, 0, 0, 0, 0, 54, 50, 49, 51, 53, 0, 0,
        // State 25
        0, 0, 0, 0, 9, -59, 0, 0, 0, 0, 52, 10, 0, 0, 0, 0, 54, 50, 49, 51, 53, 0, 0,
        // State 26
        0, 0, 0, 0, 0, -5, -5, -5, 62, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, -5, 0,
        // State 27
        0, 0, 0, 0, 0, -31, -31, 14, 0, 0, 0, 0, 0, -31, 0, 0, 0, 0, 0, 0, 0, -31, 0,
        // State 28
        0, 0, 0, 0, 0, -5, -5, -5, 62, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, -5, 33,
        // State 29
        0, 0, 0, 0, 0, -5, -5, -5, 62, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, -5, 0,
        // State 30
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 23, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 31
        0, 0, 0, 0, 0, -41, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 32
        0, 0, 0, 0, 0, -5, -5, -5, 62, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, -5, 0,
        // State 33
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 34
        0, 0, 0, 0, -22, 0, -22, 0, 0, 40, -22, -22, 0, 0, 0, 0, -22, -22, -22, -22, -22, 0, 0,
        // State 35
        0, 0, 0, 0, -24, 0, -24, 0, 0, 41, -24, -24, 0, 0, 0, 0, -24, -24, -24, -24, -24, 0, 0,
        // State 36
        0, 0, 0, 0, -20, 0, -20, 0, 0, 42, -20, -20, 0, 0, 0, 0, -20, -20, -20, -20, -20, 0, 0,
        // State 37
        0, 0, 0, 0, -26, 0, -26, 0, 0, 43, -26, -26, 0, 0, 0, 0, -26, -26, -26, -26, -26, 0, 0,
        // State 38
        -42, -42, -42, -42, -42, -42, 0, 0, 0, 0, -42, -42, 0, 0, 0, 0, -42, -42, -42, -42, -42, 0, 0,
        // State 39
        0, 0, 0, 0, -21, 0, -21, 0, 0, 0, -21, -21, 0, 0, 0, 0, -21, -21, -21, -21, -21, 0, 0,
        // State 40
        0, 0, 0, 0, -23, 0, -23, 0, 0, 0, -23, -23, 0, 0, 0, 0, -23, -23, -23, -23, -23, 0, 0,
        // State 41
        0, 0, 0, 0, -19, 0, -19, 0, 0, 0, -19, -19, 0, 0, 0, 0, -19, -19, -19, -19, -19, 0, 0,
        // State 42
        0, 0, 0, 0, -25, 0, -25, 0, 0, 0, -25, -25, 0, 0, 0, 0, -25, -25, -25, -25, -25, 0, 0,
        // State 43
        0, 0, 0, 0, 0, -46, -46, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 44
        0, 0, 0, 0, 0, -48, -48, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 45
        0, 0, 0, 0, 0, -49, -49, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 46
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 47
        0, 0, 0, 0, 0, -50, -50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 48
        0, 0, 0, 0, 0, -54, -54, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 49
        0, 0, 0, 0, 0, -53, -53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 50
        0, 0, 0, 0, 0, -55, -55, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 51
        0, 0, 0, 0, 0, -51, -51, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 52
        0, 0, 0, 0, 0, -56, -56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 53
        0, 0, 0, 0, 0, -52, -52, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 54
        0, 0, 0, 0, 0, -16, -16, 0, 0, 0, 0, 0, 0, -16, 0, 0, 0, 0, 0, 0, 0, -16, 0,
        // State 55
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 56
        0, 0, 0, 0, 0, -32, -32, -32, 0, 0, 0, 0, 0, -32, 0, 0, 0, 0, 0, 0, 0, -32, 0,
        // State 57
        0, 0, 0, 0, 0, -27, -27, 0, 0, 0, 0, 0, 0, -27, 0, 0, 0, 0, 0, 0, 0, -27, 0,
        // State 58
        0, 0, 0, 0, 0, -58, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 59
        0, 0, 0, 0, 0, -60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 60
        0, 0, 0, 0, 0, -36, -36, -40, 0, 0, 0, 0, 0, -40, 0, 0, 0, 0, 0, 0, 0, -40, 0,
        // State 61
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 71, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 62
        0, 0, 0, 0, 0, -17, -17, 0, 0, 0, 0, 0, 0, -17, 0, 0, 0, 0, 0, 0, 0, -17, 0,
        // State 63
        0, 0, 0, 0, 0, -15, -15, 0, 0, 0, 0, 0, 0, -15, 0, 0, 0, 0, 0, 0, 0, -15, 0,
        // State 64
        0, 0, 0, 0, 0, -33, -33, -33, 0, 0, 0, 0, 0, -33, 0, 0, 0, 0, 0, 0, 0, -33, 0,
        // State 65
        0, 0, 0, 0, 0, -29, -29, -29, 0, 0, 0, 0, 0, -29, 0, 0, 0, 0, 0, 0, 0, -29, 0,
        // State 66
        0, 0, 0, 0, 0, 75, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 67
        0, 0, 0, 0, -43, -43, 0, 0, 0, 0, -43, -43, 0, 0, 0, 0, -43, -43, -43, -43, -43, 0, 0,
        // State 68
        0, 0, 0, 0, 0, -35, -35, -38, 0, 0, 0, 0, 0, -38, 0, 0, 0, 0, 0, 0, 0, -38, 0,
        // State 69
        0, 0, 0, 0, 0, -39, -39, -39, 0, 0, 0, 0, 0, -39, 0, 0, 0, 0, 0, 0, 0, -39, 0,
        // State 70
        0, 0, 0, 0, 0, -4, -4, -4, 0, 0, 0, 0, 0, -4, 0, 0, 0, 0, 0, 0, 0, -4, 0,
        // State 71
        0, 0, 0, 0, 0, -14, -14, 0, 0, 0, 0, 0, 0, -14, 0, 0, 0, 0, 0, 0, 0, -14, 0,
        // State 72
        0, 0, 0, 0, 0, -40, -40, -40, 0, 0, 0, 0, 0, -40, 0, 0, 0, 0, 0, 0, 0, -40, 0,
        // State 73
        0, 0, 0, 0, 0, -13, -13, 0, 0, 0, 0, 0, 0, -13, 0, 0, 0, 0, 0, 0, 0, -13, 0,
        // State 74
        0, 0, 0, 0, 0, -62, -62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 75
        0, 0, 0, 0, 0, 81, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 76
        0, 0, 0, 0, 0, -34, -34, -37, 0, 0, 0, 0, 0, -37, 0, 0, 0, 0, 0, 0, 0, -37, 0,
        // State 77
        0, 0, 0, 0, 0, -38, -38, -38, 0, 0, 0, 0, 0, -38, 0, 0, 0, 0, 0, 0, 0, -38, 0,
        // State 78
        0, 0, 0, 0, 0, -12, -12, 0, 0, 0, 0, 0, 0, -12, 0, 0, 0, 0, 0, 0, 0, -12, 0,
        // State 79
        0, 0, 0, 0, 0, -57, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 80
        0, 0, 0, 0, 0, -61, -61, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 81
        0, 0, 0, 0, 0, -37, -37, -37, 0, 0, 0, 0, 0, -37, 0, 0, 0, 0, 0, 0, 0, -37, 0,
    ];
    fn __action(state: i8, integer: usize) -> i8 {
        __ACTION[(state as usize) * 23 + integer]
    }
    const __EOF_ACTION: &[i8] = &[
        // State 0
        0,
        // State 1
        0,
        // State 2
        -41,
        // State 3
        -41,
        // State 4
        -59,
        // State 5
        0,
        // State 6
        0,
        // State 7
        -41,
        // State 8
        0,
        // State 9
        -5,
        // State 10
        -3,
        // State 11
        0,
        // State 12
        -28,
        // State 13
        0,
        // State 14
        -41,
        // State 15
        0,
        // State 16
        0,
        // State 17
        0,
        // State 18
        -5,
        // State 19
        -47,
        // State 20
        -30,
        // State 21
        0,
        // State 22
        -5,
        // State 23
        0,
        // State 24
        -42,
        // State 25
        0,
        // State 26
        -5,
        // State 27
        -31,
        // State 28
        -5,
        // State 29
        -5,
        // State 30
        0,
        // State 31
        -41,
        // State 32
        -5,
        // State 33
        -65,
        // State 34
        -22,
        // State 35
        -24,
        // State 36
        -20,
        // State 37
        -26,
        // State 38
        -42,
        // State 39
        -21,
        // State 40
        -23,
        // State 41
        -19,
        // State 42
        -25,
        // State 43
        -46,
        // State 44
        -48,
        // State 45
        -49,
        // State 46
        -18,
        // State 47
        -50,
        // State 48
        -54,
        // State 49
        -53,
        // State 50
        -55,
        // State 51
        -51,
        // State 52
        -56,
        // State 53
        -52,
        // State 54
        -16,
        // State 55
        0,
        // State 56
        -32,
        // State 57
        -27,
        // State 58
        -58,
        // State 59
        -60,
        // State 60
        -36,
        // State 61
        0,
        // State 62
        -17,
        // State 63
        -15,
        // State 64
        -33,
        // State 65
        -29,
        // State 66
        0,
        // State 67
        0,
        // State 68
        -35,
        // State 69
        -39,
        // State 70
        -4,
        // State 71
        -14,
        // State 72
        -40,
        // State 73
        -13,
        // State 74
        -62,
        // State 75
        0,
        // State 76
        -34,
        // State 77
        -38,
        // State 78
        -12,
        // State 79
        -57,
        // State 80
        -61,
        // State 81
        -37,
    ];
    fn __goto(state: i8, nt: usize) -> i8 {
        match nt {
            2 => 43,
            3 => match state {
                9 => 60,
                18 => 68,
                22 => 72,
                26 => 76,
                28 => 77,
                32 => 81,
                _ => 69,
            },
            7 => match state {
                10 => 62,
                _ => 54,
            },
            8 => 10,
            9 => 33,
            10 => 3,
            11 => match state {
                11 => 63,
                21 => 71,
                23 => 73,
                30 => 78,
                _ => 5,
            },
            12 => 44,
            13 => match state {
                12 | 27 => 64,
                _ => 56,
            },
            14 => 57,
            15 => match state {
                20 => 27,
                _ => 12,
            },
            16 => 45,
            17 => match state {
                4 | 15 | 24..=25 => 6,
                13 => 65,
                _ => 20,
            },
            18 => match state {
                0 => 1,
                3 => 4,
                14 => 24,
                2 => 38,
                16 => 67,
                _ => 58,
            },
            19 => match state {
                17 => 25,
                _ => 15,
            },
            21 => match state {
                24 => 31,
                _ => 7,
            },
            22 => match state {
                31 => 79,
                _ => 59,
            },
            23 => match state {
                15 => 66,
                25 => 75,
                _ => 46,
            },
            24 => 47,
            _ => 0,
        }
    }
    #[allow(clippy::needless_raw_string_hashes)]
    const __TERMINAL: &[&str] = &[
        r###""graph""###,
        r###""flowchart""###,
        r###""flowchart-elk""###,
        r###""swimlane-beta""###,
        r###""subgraph""###,
        r###""end""###,
        r###"Sep"###,
        r###"Amp"###,
        r###"StyleSep"###,
        r###"Direction"###,
        r###"DirectionStmt"###,
        r###"Id"###,
        r###"NodeLabel"###,
        r###"Arrow"###,
        r###"EdgeLabel"###,
        r###"SubgraphHeader"###,
        r###"StyleStmt"###,
        r###"ClassDefStmt"###,
        r###"ClassAssignStmt"###,
        r###"ClickStmt"###,
        r###"LinkStyleStmt"###,
        r###"EdgeId"###,
        r###"ShapeData"###,
    ];
    fn __expected_tokens(__state: i8) -> alloc::vec::Vec<alloc::string::String> {
        __TERMINAL.iter().enumerate().filter_map(|(index, terminal)| {
            let next_state = __action(__state, index);
            if next_state == 0 {
                None
            } else {
                Some(alloc::string::ToString::to_string(terminal))
            }
        }).collect()
    }
    fn __expected_tokens_from_states<
        '__0,
        '__1,
    >(
        __states: &[i8],
        _: core::marker::PhantomData<()>,
    ) -> alloc::vec::Vec<alloc::string::String>
    {
        __TERMINAL.iter().enumerate().filter_map(|(index, terminal)| {
            if __accepts(None, __states, Some(index), core::marker::PhantomData::<()>) {
                Some(alloc::string::ToString::to_string(terminal))
            } else {
                None
            }
        }).collect()
    }
    struct __StateMachine<'__0, '__1>
    where 
    {
        statements: &'__0 mut StatementArena,
        control: &'__1 crate::OperationControl,
        __phantom: core::marker::PhantomData<()>,
    }
    impl<'__0, '__1> __state_machine::ParserDefinition for __StateMachine<'__0, '__1>
    where 
    {
        type Location = usize;
        type Error = crate::diagrams::flowchart::LexError;
        type Token = Tok;
        type TokenIndex = usize;
        type Symbol = __Symbol<>;
        type Success = FlowchartAst;
        type StateIndex = i8;
        type Action = i8;
        type ReduceIndex = i8;
        type NonterminalIndex = usize;

        #[inline]
        fn memory_scope(&self) -> Option<&markdown_memory::MemoryScope> {
            self.control.memory_scope()
        }

        fn memory_error(&self, error: markdown_memory::MemoryError) -> __state_machine::ParseError<Self> {
            if let Some(scope) = self.control.memory_scope() {
                scope.record_failure(error);
            }
            self.control.cancel();
            __lalrpop_util::ParseError::User { error: crate::diagrams::flowchart::LexError::terminal() }
        }

        fn memory_checkpoint(&self) -> Result<(), markdown_memory::MemoryError> {
            if let Some(error) = self.control.memory_scope().and_then(|scope| scope.failure()) {
                return Err(error);
            }
            self.control.checkpoint().map_err(|_| markdown_memory::MemoryError::Cancelled)
        }

        #[inline]
        fn start_location(&self) -> Self::Location {
              Default::default()
        }

        #[inline]
        fn start_state(&self) -> Self::StateIndex {
              0
        }

        #[inline]
        fn token_to_index(&self, token: &Self::Token) -> Option<usize> {
            __token_to_integer(token, core::marker::PhantomData::<()>)
        }

        #[inline]
        fn action(&self, state: i8, integer: usize) -> i8 {
            __action(state, integer)
        }

        #[inline]
        fn error_action(&self, state: i8) -> i8 {
            __action(state, 23 - 1)
        }

        #[inline]
        fn eof_action(&self, state: i8) -> i8 {
            __EOF_ACTION[state as usize]
        }

        #[inline]
        fn goto(&self, state: i8, nt: usize) -> i8 {
            __goto(state, nt)
        }

        fn token_to_symbol(&self, token_index: usize, token: Self::Token) -> Self::Symbol {
            __token_to_symbol(token_index, token, core::marker::PhantomData::<()>)
        }

        fn expected_tokens(&self, state: i8) -> alloc::vec::Vec<alloc::string::String> {
            __expected_tokens(state)
        }

        fn expected_tokens_from_states(&self, states: &[i8]) -> alloc::vec::Vec<alloc::string::String> {
            __expected_tokens_from_states(states, core::marker::PhantomData::<()>)
        }

        #[inline]
        fn uses_error_recovery(&self) -> bool {
            false
        }

        #[inline]
        fn error_recovery_symbol(
            &self,
            recovery: __state_machine::ErrorRecovery<Self>,
        ) -> Self::Symbol {
            panic!("error recovery not enabled for this grammar")
        }

        fn reduce(
            &mut self,
            action: i8,
            start_location: Option<&Self::Location>,
            states: &mut alloc::vec::Vec<i8>,
            symbols: &mut alloc::vec::Vec<__state_machine::SymbolTriple<Self>>,
        ) -> Option<__state_machine::ParseResult<Self>> {
            __reduce(
                self.statements,
                self.control,
                action,
                start_location,
                states,
                symbols,
                core::marker::PhantomData::<()>,
            )
        }

        fn simulate_reduce(&self, action: i8) -> __state_machine::SimulatedReduce<Self> {
            __simulate_reduce(action, core::marker::PhantomData::<()>)
        }
    }
    fn __token_to_integer<
    >(
        __token: &Tok,
        _: core::marker::PhantomData<()>,
    ) -> Option<usize>
    {
        #[warn(unused_variables)]
        match __token {
            Tok::KwGraph if true => Some(0),
            Tok::KwFlowchart if true => Some(1),
            Tok::KwFlowchartElk if true => Some(2),
            Tok::KwSwimlane if true => Some(3),
            Tok::KwSubgraph if true => Some(4),
            Tok::KwEnd if true => Some(5),
            Tok::Sep if true => Some(6),
            Tok::Amp if true => Some(7),
            Tok::StyleSep if true => Some(8),
            Tok::Direction(_) if true => Some(9),
            Tok::DirectionStmt(_) if true => Some(10),
            Tok::Id(_) if true => Some(11),
            Tok::NodeLabel(_) if true => Some(12),
            Tok::Arrow(_) if true => Some(13),
            Tok::EdgeLabel(_) if true => Some(14),
            Tok::SubgraphHeader(_) if true => Some(15),
            Tok::StyleStmt(_) if true => Some(16),
            Tok::ClassDefStmt(_) if true => Some(17),
            Tok::ClassAssignStmt(_) if true => Some(18),
            Tok::ClickStmt(_) if true => Some(19),
            Tok::LinkStyleStmt(_) if true => Some(20),
            Tok::EdgeId(_) if true => Some(21),
            Tok::ShapeData(_) if true => Some(22),
            _ => None,
        }
    }
    fn __token_to_symbol<
    >(
        __token_index: usize,
        __token: Tok,
        _: core::marker::PhantomData<()>,
    ) -> __Symbol<>
    {
        #[allow(clippy::manual_range_patterns)]match __token_index {
            0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 => __Symbol::Variant0(__token),
            9 | 11 | 21 | 22 => match __token {
                Tok::Direction(__tok0) | Tok::Id(__tok0) | Tok::EdgeId(__tok0) | Tok::ShapeData(__tok0) if true => __Symbol::Variant1(__tok0),
                _ => unreachable!(),
            },
            10 => match __token {
                Tok::DirectionStmt(__tok0) if true => __Symbol::Variant2(__tok0),
                _ => unreachable!(),
            },
            12 => match __token {
                Tok::NodeLabel(__tok0) if true => __Symbol::Variant3(__tok0),
                _ => unreachable!(),
            },
            13 => match __token {
                Tok::Arrow(__tok0) if true => __Symbol::Variant4(__tok0),
                _ => unreachable!(),
            },
            14 => match __token {
                Tok::EdgeLabel(__tok0) if true => __Symbol::Variant5(__tok0),
                _ => unreachable!(),
            },
            15 => match __token {
                Tok::SubgraphHeader(__tok0) if true => __Symbol::Variant6(__tok0),
                _ => unreachable!(),
            },
            16 => match __token {
                Tok::StyleStmt(__tok0) if true => __Symbol::Variant7(__tok0),
                _ => unreachable!(),
            },
            17 => match __token {
                Tok::ClassDefStmt(__tok0) if true => __Symbol::Variant8(__tok0),
                _ => unreachable!(),
            },
            18 => match __token {
                Tok::ClassAssignStmt(__tok0) if true => __Symbol::Variant9(__tok0),
                _ => unreachable!(),
            },
            19 => match __token {
                Tok::ClickStmt(__tok0) if true => __Symbol::Variant10(__tok0),
                _ => unreachable!(),
            },
            20 => match __token {
                Tok::LinkStyleStmt(__tok0) if true => __Symbol::Variant11(__tok0),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
    fn __simulate_reduce<
        '__0,
        '__1,
    >(
        __reduce_index: i8,
        _: core::marker::PhantomData<()>,
    ) -> __state_machine::SimulatedReduce<__StateMachine<'__0, '__1>>
    {
        match __reduce_index {
            0 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 0,
                }
            }
            1 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 1,
                }
            }
            2 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 2,
                }
            }
            3 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 3,
                }
            }
            4 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 3,
                }
            }
            5 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 4,
                }
            }
            6 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 4,
                }
            }
            7 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 5,
                }
            }
            8 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 5,
                }
            }
            9 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 6,
                }
            }
            10 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 6,
                }
            }
            11 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 7,
                }
            }
            12 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 7,
                }
            }
            13 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 7,
                }
            }
            14 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 7,
                }
            }
            15 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 8,
                }
            }
            16 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 8,
                }
            }
            17 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 9,
                }
            }
            18 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 10,
                }
            }
            19 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 10,
                }
            }
            20 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 10,
                }
            }
            21 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 10,
                }
            }
            22 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 10,
                }
            }
            23 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 10,
                }
            }
            24 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 10,
                }
            }
            25 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 10,
                }
            }
            26 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 11,
                }
            }
            27 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 12,
                }
            }
            28 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 13,
                }
            }
            29 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 14,
                }
            }
            30 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 14,
                }
            }
            31 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 15,
                }
            }
            32 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 15,
                }
            }
            33 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 16,
                }
            }
            34 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 16,
                }
            }
            35 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 16,
                }
            }
            36 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 17,
                }
            }
            37 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 17,
                }
            }
            38 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 17,
                }
            }
            39 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 17,
                }
            }
            40 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 18,
                }
            }
            41 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 18,
                }
            }
            42 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 19,
                }
            }
            43 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 20,
                }
            }
            44 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 20,
                }
            }
            45 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            46 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 21,
                }
            }
            47 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            48 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            49 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            50 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            51 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            52 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            53 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            54 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            55 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 21,
                }
            }
            56 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 22,
                }
            }
            57 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 22,
                }
            }
            58 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 23,
                }
            }
            59 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 23,
                }
            }
            60 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 5,
                    nonterminal_produced: 24,
                }
            }
            61 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 24,
                }
            }
            62 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 25,
                }
            }
            63 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 25,
                }
            }
            64 => __state_machine::SimulatedReduce::Accept,
            _ => panic!("invalid reduction index {__reduce_index}")
        }
    }
    pub struct FlowchartAstParser {
        _priv: (),
    }

    impl Default for FlowchartAstParser { fn default() -> Self { Self::new() } }
    impl FlowchartAstParser {
        pub fn new() -> FlowchartAstParser {
            FlowchartAstParser {
                _priv: (),
            }
        }

        #[allow(dead_code)]
        pub fn parse<
            __TOKEN: __ToTriple<>,
            __TOKENS: IntoIterator<Item=__TOKEN>,
        >(
            &self,
            statements: &mut StatementArena,
            control: &crate::OperationControl,
            __tokens0: __TOKENS,
        ) -> Result<FlowchartAst, __lalrpop_util::ParseError<usize, Tok, crate::diagrams::flowchart::LexError>>
        {
            let __tokens = __tokens0.into_iter();
            let mut __tokens = __tokens.map(|t| __ToTriple::to_triple(t));
            __state_machine::Parser::drive(
                __StateMachine {
                    statements,
                    control,
                    __phantom: core::marker::PhantomData::<()>,
                },
                __tokens,
            )
        }
    }
    fn __accepts<
        '__0,
        '__1,
    >(
        __error_state: Option<i8>,
        __states: &[i8],
        __opt_integer: Option<usize>,
        _: core::marker::PhantomData<()>,
    ) -> bool
    {
        let mut __states = __states.to_vec();
        __states.extend(__error_state);
        loop {
            let mut __states_len = __states.len();
            let __top = __states[__states_len - 1];
            let __action = match __opt_integer {
                None => __EOF_ACTION[__top as usize],
                Some(__integer) => __action(__top, __integer),
            };
            if __action == 0 { return false; }
            if __action > 0 { return true; }
            let (__to_pop, __nt) = match __simulate_reduce(-(__action + 1), core::marker::PhantomData::<()>) {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop, nonterminal_produced
                } => (states_to_pop, nonterminal_produced),
                __state_machine::SimulatedReduce::Accept => return true,
            };
            __states_len -= __to_pop;
            __states.truncate(__states_len);
            let __top = __states[__states_len - 1];
            let __next_state = __goto(__top, __nt);
            __states.push(__next_state);
        }
    }
    fn __reduce<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __action: i8,
        __lookahead_start: Option<&usize>,
        __states: &mut alloc::vec::Vec<i8>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> Option<Result<FlowchartAst,__lalrpop_util::ParseError<usize, Tok, crate::diagrams::flowchart::LexError>>>
    {
        let (__pop_states, __nonterminal) = match __action {
            0 => {
                __reduce0(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            1 => {
                __reduce1(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            2 => {
                // Chain = NodeGroup, EdgeSegments => ActionFn(23);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant18(__symbols);
                let __sym0 = __pop_Variant21(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action23::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant13(__nt), __end));
                (2, 2)
            }
            3 => {
                // ClassOpt = StyleSep, Id => ActionFn(41);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant1(__symbols);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action41::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant14(__nt), __end));
                (2, 3)
            }
            4 => {
                __reduce4(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            5 => {
                __reduce5(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            6 => {
                __reduce6(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            7 => {
                __reduce7(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            8 => {
                __reduce8(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            9 => {
                __reduce9(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            10 => {
                __reduce10(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            11 => {
                __reduce11(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            12 => {
                __reduce12(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            13 => {
                __reduce13(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            14 => {
                __reduce14(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            15 => {
                // EdgeSegments = EdgeSeg => ActionFn(24);
                let __sym0 = __pop_Variant17(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action24::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant18(__nt), __end));
                (1, 8)
            }
            16 => {
                // EdgeSegments = EdgeSegments, EdgeSeg => ActionFn(25);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant17(__symbols);
                let __sym0 = __pop_Variant18(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action25::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant18(__nt), __end));
                (2, 8)
            }
            17 => {
                __reduce17(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            18 => {
                // Header = "graph", Direction => ActionFn(75);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant1(__symbols);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action75::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant20(__nt), __end));
                (2, 10)
            }
            19 => {
                // Header = "graph" => ActionFn(76);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action76::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant20(__nt), __end));
                (1, 10)
            }
            20 => {
                // Header = "flowchart", Direction => ActionFn(77);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant1(__symbols);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action77::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant20(__nt), __end));
                (2, 10)
            }
            21 => {
                // Header = "flowchart" => ActionFn(78);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action78::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant20(__nt), __end));
                (1, 10)
            }
            22 => {
                // Header = "flowchart-elk", Direction => ActionFn(79);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant1(__symbols);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action79::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant20(__nt), __end));
                (2, 10)
            }
            23 => {
                // Header = "flowchart-elk" => ActionFn(80);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action80::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant20(__nt), __end));
                (1, 10)
            }
            24 => {
                // Header = "swimlane-beta", Direction => ActionFn(81);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant1(__symbols);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action81::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant20(__nt), __end));
                (2, 10)
            }
            25 => {
                // Header = "swimlane-beta" => ActionFn(82);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action82::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant20(__nt), __end));
                (1, 10)
            }
            26 => {
                // NodeGroup = NodeRefChain, NodeGroupRestList => ActionFn(29);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant21(__symbols);
                let __sym0 = __pop_Variant22(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action29::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant21(__nt), __end));
                (2, 11)
            }
            27 => {
                // NodeGroupOnly = NodeRefChain, NodeGroupRestNonempty => ActionFn(30);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant21(__symbols);
                let __sym0 = __pop_Variant22(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action30::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant21(__nt), __end));
                (2, 12)
            }
            28 => {
                __reduce28(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            29 => {
                __reduce29(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            30 => {
                __reduce30(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            31 => {
                // NodeGroupRestNonempty = NodeGroupRest => ActionFn(33);
                let __sym0 = __pop_Variant22(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action33::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant21(__nt), __end));
                (1, 15)
            }
            32 => {
                // NodeGroupRestNonempty = NodeGroupRestNonempty, NodeGroupRest => ActionFn(34);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant22(__symbols);
                let __sym0 = __pop_Variant21(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action34::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant21(__nt), __end));
                (2, 15)
            }
            33 => {
                __reduce33(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            34 => {
                __reduce34(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            35 => {
                __reduce35(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            36 => {
                __reduce36(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            37 => {
                __reduce37(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            38 => {
                __reduce38(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            39 => {
                __reduce39(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            40 => {
                __reduce40(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            41 => {
                __reduce41(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            42 => {
                __reduce42(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            43 => {
                __reduce43(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            44 => {
                __reduce44(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            45 => {
                // Statement = Chain => ActionFn(12);
                let __sym0 = __pop_Variant13(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action12::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            46 => {
                // Statement = Id, ShapeData => ActionFn(74);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant1(__symbols);
                let __sym0 = __pop_Variant1(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action74::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (2, 21)
            }
            47 => {
                // Statement = NodeGroupOnly => ActionFn(14);
                let __sym0 = __pop_Variant21(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action14::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            48 => {
                // Statement = NodeRef => ActionFn(15);
                let __sym0 = __pop_Variant22(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action15::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            49 => {
                // Statement = SubgraphBlock => ActionFn(16);
                let __sym0 = __pop_Variant26(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action16::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            50 => {
                // Statement = DirectionStmt => ActionFn(17);
                let __sym0 = __pop_Variant2(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action17::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            51 => {
                // Statement = StyleStmt => ActionFn(18);
                let __sym0 = __pop_Variant7(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action18::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            52 => {
                // Statement = ClassDefStmt => ActionFn(19);
                let __sym0 = __pop_Variant8(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action19::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            53 => {
                // Statement = ClassAssignStmt => ActionFn(20);
                let __sym0 = __pop_Variant9(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action20::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            54 => {
                // Statement = ClickStmt => ActionFn(21);
                let __sym0 = __pop_Variant10(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action21::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            55 => {
                // Statement = LinkStyleStmt => ActionFn(22);
                let __sym0 = __pop_Variant11(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action22::<>(statements, control, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant24(__nt), __end));
                (1, 21)
            }
            56 => {
                // StatementRest = Sep, Seps, Statement, StatementRest => ActionFn(8);
                assert!(__symbols.len() >= 4);
                let __sym3 = __pop_Variant25(__symbols);
                let __sym2 = __pop_Variant24(__symbols);
                let __sym1 = __pop_Variant23(__symbols);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym3.2.clone();
                let __nt = match super::__action8::<>(statements, control, __sym0, __sym1, __sym2, __sym3) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant25(__nt), __end));
                (4, 22)
            }
            57 => {
                __reduce57(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            58 => {
                __reduce58(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            59 => {
                // Statements = Statement, StatementRest => ActionFn(7);
                assert!(__symbols.len() >= 2);
                let __sym1 = __pop_Variant25(__symbols);
                let __sym0 = __pop_Variant24(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym1.2.clone();
                let __nt = match super::__action7::<>(statements, control, __sym0, __sym1) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant25(__nt), __end));
                (2, 23)
            }
            60 => {
                __reduce60(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            61 => {
                __reduce61(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            62 => {
                __reduce62(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            63 => {
                __reduce63(statements, control, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            64 => {
                // __FlowchartAst = FlowchartAst => ActionFn(0);
                let __sym0 = __pop_Variant19(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = super::__action0::<>(statements, control, __sym0);
                return Some(Ok(__nt));
            }
            _ => panic!("invalid action code {__action}")
        };
        let __states_len = __states.len();
        __states.truncate(__states_len - __pop_states);
        let __state = *__states.last().unwrap();
        let __next_state = __goto(__state, __nonterminal);
        __states.push(__next_state);
        None
    }
    #[inline(never)]
    fn __symbol_type_mismatch() -> ! {
        panic!("symbol type mismatch")
    }
    fn __pop_Variant23<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, (), usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant23(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant17<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>), usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant17(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant20<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, (String, Option<String>, SourceSpan), usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant20(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant13<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, (Vec<Vec<Node>>, Vec<Vec<Edge>>), usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant13(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant4<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, ArrowToken, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant4(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant9<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, ClassAssignStmt, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant9(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant8<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, ClassDefStmt, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant8(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant10<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, ClickStmt, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant10(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant2<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, DirectionStatementToken, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant2(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant19<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, FlowchartAst, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant19(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant5<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, LabeledText, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant5(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant11<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, LinkStyleStmt, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant11(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant22<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Node, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant22(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant3<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, NodeLabelToken, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant3(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant16<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Option<LabeledText>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant16(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant15<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Option<String>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant15(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant27<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Option<SubgraphHeader>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant27(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant24<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, StatementId, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant24(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant25<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, StatementList, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant25(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant1<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, String, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant1(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant7<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, StyleStmt, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant7(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant26<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, SubgraphBlock, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant26(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant6<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, SubgraphHeader, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant6(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant0<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Tok, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant0(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant18<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Vec<(Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant18(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant21<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Vec<Node>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant21(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant14<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Vec<String>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant14(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant12<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, usize, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant12(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __reduce0<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // @L =  => ActionFn(54);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action54::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant12(__nt), __end));
        (0, 0)
    }
    fn __reduce1<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // @R =  => ActionFn(51);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action51::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant12(__nt), __end));
        (0, 1)
    }
    fn __reduce4<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // ClassOpt =  => ActionFn(42);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action42::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant14(__nt), __end));
        (0, 3)
    }
    fn __reduce5<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Direction? = Direction => ActionFn(52);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action52::<>(statements, control, __sym0);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (1, 4)
    }
    fn __reduce6<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Direction? =  => ActionFn(53);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action53::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (0, 4)
    }
    fn __reduce7<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EdgeId? = EdgeId => ActionFn(47);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action47::<>(statements, control, __sym0);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (1, 5)
    }
    fn __reduce8<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EdgeId? =  => ActionFn(48);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action48::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (0, 5)
    }
    fn __reduce9<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EdgeLabel? = EdgeLabel => ActionFn(45);
        let __sym0 = __pop_Variant5(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action45::<>(statements, control, __sym0);
        __symbols.push((__start, __Symbol::Variant16(__nt), __end));
        (1, 6)
    }
    fn __reduce10<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EdgeLabel? =  => ActionFn(46);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action46::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant16(__nt), __end));
        (0, 6)
    }
    fn __reduce11<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EdgeSeg = EdgeId, Arrow, EdgeLabel, NodeGroup => ActionFn(85);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant21(__symbols);
        let __sym2 = __pop_Variant5(__symbols);
        let __sym1 = __pop_Variant4(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action85::<>(statements, control, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant17(__nt), __end));
        (4, 7)
    }
    fn __reduce12<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EdgeSeg = EdgeId, Arrow, NodeGroup => ActionFn(86);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant21(__symbols);
        let __sym1 = __pop_Variant4(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action86::<>(statements, control, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant17(__nt), __end));
        (3, 7)
    }
    fn __reduce13<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EdgeSeg = Arrow, EdgeLabel, NodeGroup => ActionFn(87);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant21(__symbols);
        let __sym1 = __pop_Variant5(__symbols);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action87::<>(statements, control, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant17(__nt), __end));
        (3, 7)
    }
    fn __reduce14<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EdgeSeg = Arrow, NodeGroup => ActionFn(88);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant21(__symbols);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action88::<>(statements, control, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant17(__nt), __end));
        (2, 7)
    }
    fn __reduce17<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // FlowchartAst = Seps, Header, Seps, Statements => ActionFn(1);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant25(__symbols);
        let __sym2 = __pop_Variant23(__symbols);
        let __sym1 = __pop_Variant20(__symbols);
        let __sym0 = __pop_Variant23(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action1::<>(statements, control, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant19(__nt), __end));
        (4, 9)
    }
    fn __reduce28<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeGroupRest = Amp, NodeRefChain => ActionFn(35);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant22(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action35::<>(statements, control, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (2, 13)
    }
    fn __reduce29<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeGroupRestList =  => ActionFn(31);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action31::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant21(__nt), __end));
        (0, 14)
    }
    fn __reduce30<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeGroupRestList = NodeGroupRestNonempty => ActionFn(32);
        let __sym0 = __pop_Variant21(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action32::<>(statements, control, __sym0);
        __symbols.push((__start, __Symbol::Variant21(__nt), __end));
        (1, 14)
    }
    fn __reduce33<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeRef = Id, NodeLabel, ShapeData, ClassOpt => ActionFn(89);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant14(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant3(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action89::<>(statements, control, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (4, 16)
    }
    fn __reduce34<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeRef = Id, NodeLabel, ClassOpt => ActionFn(90);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant14(__symbols);
        let __sym1 = __pop_Variant3(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action90::<>(statements, control, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (3, 16)
    }
    fn __reduce35<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeRef = Id, ClassOpt => ActionFn(70);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant14(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action70::<>(statements, control, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (2, 16)
    }
    fn __reduce36<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeRefChain = Id, NodeLabel, ShapeData, ClassOpt => ActionFn(91);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant14(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant3(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action91::<>(statements, control, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (4, 17)
    }
    fn __reduce37<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeRefChain = Id, NodeLabel, ClassOpt => ActionFn(92);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant14(__symbols);
        let __sym1 = __pop_Variant3(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action92::<>(statements, control, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (3, 17)
    }
    fn __reduce38<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeRefChain = Id, ShapeData, ClassOpt => ActionFn(72);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant14(__symbols);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action72::<>(statements, control, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (3, 17)
    }
    fn __reduce39<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // NodeRefChain = Id, ClassOpt => ActionFn(73);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant14(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action73::<>(statements, control, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (2, 17)
    }
    fn __reduce40<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Seps =  => ActionFn(10);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action10::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant23(__nt), __end));
        (0, 18)
    }
    fn __reduce41<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Seps = Sep, Seps => ActionFn(11);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant23(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action11::<>(statements, control, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant23(__nt), __end));
        (2, 18)
    }
    fn __reduce42<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Seps1 = Sep, Seps => ActionFn(26);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant23(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action26::<>(statements, control, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant23(__nt), __end));
        (2, 19)
    }
    fn __reduce43<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // ShapeData? = ShapeData => ActionFn(43);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action43::<>(statements, control, __sym0);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (1, 20)
    }
    fn __reduce44<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // ShapeData? =  => ActionFn(44);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action44::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (0, 20)
    }
    fn __reduce57<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // StatementRest = Seps => ActionFn(9);
        let __sym0 = __pop_Variant23(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action9::<>(statements, control, __sym0);
        __symbols.push((__start, __Symbol::Variant25(__nt), __end));
        (1, 22)
    }
    fn __reduce58<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statements =  => ActionFn(6);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action6::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant25(__nt), __end));
        (0, 23)
    }
    fn __reduce60<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphBlock = "subgraph", SubgraphHeader, Seps1, Statements, "end" => ActionFn(93);
        assert!(__symbols.len() >= 5);
        let __sym4 = __pop_Variant0(__symbols);
        let __sym3 = __pop_Variant25(__symbols);
        let __sym2 = __pop_Variant23(__symbols);
        let __sym1 = __pop_Variant6(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym4.2.clone();
        let __nt = super::__action93::<>(statements, control, __sym0, __sym1, __sym2, __sym3, __sym4);
        __symbols.push((__start, __Symbol::Variant26(__nt), __end));
        (5, 24)
    }
    fn __reduce61<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphBlock = "subgraph", Seps1, Statements, "end" => ActionFn(94);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant25(__symbols);
        let __sym1 = __pop_Variant23(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action94::<>(statements, control, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant26(__nt), __end));
        (4, 24)
    }
    fn __reduce62<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphHeader? = SubgraphHeader => ActionFn(49);
        let __sym0 = __pop_Variant6(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action49::<>(statements, control, __sym0);
        __symbols.push((__start, __Symbol::Variant27(__nt), __end));
        (1, 25)
    }
    fn __reduce63<
    >(
        statements: &mut StatementArena,
        control: &crate::OperationControl,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphHeader? =  => ActionFn(50);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action50::<>(statements, control, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant27(__nt), __end));
        (0, 25)
    }
}
#[allow(unused_imports)]
pub use self::__parse__FlowchartAst::FlowchartAstParser;

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action0<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, FlowchartAst, usize),
) -> FlowchartAst
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action1<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, _lead, _): (usize, (), usize),
    (_, h, _): (usize, (String, Option<String>, SourceSpan), usize),
    (_, _s, _): (usize, (), usize),
    (_, st, _): (usize, StatementList, usize),
) -> FlowchartAst
{
    {
    let (keyword, direction, header_span) = h;
    FlowchartAst { keyword, direction, header_span, statements: std::mem::take(statements), root: st }
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action2<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, _, _): (usize, Tok, usize),
    (_, d, _): (usize, Option<String>, usize),
    (_, r, _): (usize, usize, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    Ok((StatementArena::owned_keyword("graph", control)?, d, SourceSpan::new(l, r)))
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action3<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, _, _): (usize, Tok, usize),
    (_, d, _): (usize, Option<String>, usize),
    (_, r, _): (usize, usize, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    Ok((StatementArena::owned_keyword("flowchart", control)?, d, SourceSpan::new(l, r)))
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action4<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, _, _): (usize, Tok, usize),
    (_, d, _): (usize, Option<String>, usize),
    (_, r, _): (usize, usize, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    Ok((StatementArena::owned_keyword("flowchart-elk", control)?, d, SourceSpan::new(l, r)))
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action5<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, _, _): (usize, Tok, usize),
    (_, d, _): (usize, Option<String>, usize),
    (_, r, _): (usize, usize, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    Ok((StatementArena::owned_keyword("swimlane-beta", control)?, d, SourceSpan::new(l, r)))
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action6<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> StatementList
{
    StatementList::default()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action7<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, s, _): (usize, StatementId, usize),
    (_, rest, _): (usize, StatementList, usize),
) -> Result<StatementList,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.prepend(s, rest, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action8<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, _sep, _): (usize, Tok, usize),
    (_, _seps, _): (usize, (), usize),
    (_, s, _): (usize, StatementId, usize),
    (_, rest, _): (usize, StatementList, usize),
) -> Result<StatementList,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.prepend(s, rest, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action9<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, (), usize),
) -> StatementList
{
    StatementList::default()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action10<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
)
{
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action11<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, Tok, usize),
    (_, __1, _): (usize, (), usize),
)
{
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action12<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, c, _): (usize, (Vec<Vec<Node>>, Vec<Vec<Edge>>), usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::Chain { node_groups: c.0, edge_groups: c.1 }, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action13<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, id, _): (usize, String, usize),
    (_, r, _): (usize, usize, usize),
    (_, sd, _): (usize, String, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::ShapeData {
    target: id,
    target_span: Some(SourceSpan::new(l, r)),
    yaml: sd,
  }, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action14<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, g, _): (usize, Vec<Node>, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push_group(g, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action15<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, n, _): (usize, Node, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push_node(n, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action16<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, sg, _): (usize, SubgraphBlock, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::Subgraph(sg), control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action17<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, d, _): (usize, DirectionStatementToken, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::Direction(d.direction), control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action18<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, s, _): (usize, StyleStmt, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::Style(s), control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action19<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, c, _): (usize, ClassDefStmt, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::ClassDef(c), control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action20<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, c, _): (usize, ClassAssignStmt, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::ClassAssign(c), control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action21<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, c, _): (usize, ClickStmt, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::Click(c), control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action22<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, ls, _): (usize, LinkStyleStmt, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.push(Stmt::LinkStyle(ls), control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action23<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, start, _): (usize, Vec<Node>, usize),
    (_, segs, _): (usize, Vec<(Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)>, usize),
) -> Result<(Vec<Vec<Node>>, Vec<Vec<Edge>>),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    statements.chain(start, segs, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action24<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, segment, _): (usize, (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>), usize),
) -> Result<Vec<(Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)>,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    StatementArena::vector_one(segment, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action25<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, segments, _): (usize, Vec<(Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)>, usize),
    (_, segment, _): (usize, (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>), usize),
) -> Result<Vec<(Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)>,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    StatementArena::vector_push(segments, segment, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action26<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, Tok, usize),
    (_, __1, _): (usize, (), usize),
)
{
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action27<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, _, _): (usize, Tok, usize),
    (_, h, _): (usize, Option<SubgraphHeader>, usize),
    (_, _s, _): (usize, (), usize),
    (_, inner, _): (usize, StatementList, usize),
    (_, _, _): (usize, Tok, usize),
) -> SubgraphBlock
{
    SubgraphBlock { header: h.unwrap_or_default(), statements: inner }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action28<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, eid, _): (usize, Option<String>, usize),
    (_, a, _): (usize, ArrowToken, usize),
    (_, l, _): (usize, Option<LabeledText>, usize),
    (_, n, _): (usize, Vec<Node>, usize),
) -> (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)
{
    (eid, a.link, l, n)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action29<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, first, _): (usize, Node, usize),
    (_, rest, _): (usize, Vec<Node>, usize),
) -> Result<Vec<Node>,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    StatementArena::vector_prepend(first, rest, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action30<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, first, _): (usize, Node, usize),
    (_, rest, _): (usize, Vec<Node>, usize),
) -> Result<Vec<Node>,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    StatementArena::vector_prepend(first, rest, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action31<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Vec<Node>
{
    Vec::new()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action32<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, rest, _): (usize, Vec<Node>, usize),
) -> Vec<Node>
{
    rest
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action33<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, node, _): (usize, Node, usize),
) -> Result<Vec<Node>,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    StatementArena::vector_one(node, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action34<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, rest, _): (usize, Vec<Node>, usize),
    (_, node, _): (usize, Node, usize),
) -> Result<Vec<Node>,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    StatementArena::vector_push(rest, node, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action35<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, _, _): (usize, Tok, usize),
    (_, n, _): (usize, Node, usize),
) -> Node
{
    n
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action36<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, id, _): (usize, String, usize),
    (_, r, _): (usize, usize, usize),
    (_, nl, _): (usize, NodeLabelToken, usize),
    (_, sd, _): (usize, Option<String>, usize),
    (_, cls, _): (usize, Vec<String>, usize),
) -> Node
{
    {
    let NodeLabelToken { shape, text, .. } = nl;
    let label_span = text.span;
    let label_selection = text.selection;
    Node {
      id_span: Some(SourceSpan::new(l, r)),
      id,
      provenance: FlowNodeProvenance::Authored,
      syntax: FlowNodeSyntax::ExplicitDefinition,
      label: Some(text.text),
      label_type: text.kind,
      label_span,
      label_selection,
      shape: Some(shape),
      shape_data: sd,
      icon: None,
      form: None,
      pos: None,
      img: None,
      constraint: None,
      asset_width: None,
      asset_height: None,
      styles: Vec::new(),
      classes: cls,
      link: None,
      link_target: None,
      have_callback: false,
    }
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action37<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, id, _): (usize, String, usize),
    (_, r, _): (usize, usize, usize),
    (_, cls, _): (usize, Vec<String>, usize),
) -> Node
{
    Node {
    id_span: Some(SourceSpan::new(l, r)),
    id,
    provenance: FlowNodeProvenance::Authored,
    syntax: if cls.is_empty() {
      FlowNodeSyntax::BareReference
    } else {
      FlowNodeSyntax::ExplicitDefinition
    },
    label: None,
    label_type: TitleKind::Text,
    label_span: None,
    label_selection: None,
    shape: None,
    shape_data: None,
    icon: None,
    form: None,
    pos: None,
    img: None,
    constraint: None,
    asset_width: None,
    asset_height: None,
    styles: Vec::new(),
    classes: cls,
    link: None,
    link_target: None,
    have_callback: false,
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action38<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, id, _): (usize, String, usize),
    (_, r, _): (usize, usize, usize),
    (_, nl, _): (usize, NodeLabelToken, usize),
    (_, sd, _): (usize, Option<String>, usize),
    (_, cls, _): (usize, Vec<String>, usize),
) -> Node
{
    {
    let NodeLabelToken { shape, text, .. } = nl;
    let label_span = text.span;
    let label_selection = text.selection;
    Node {
      id_span: Some(SourceSpan::new(l, r)),
      id,
      provenance: FlowNodeProvenance::Authored,
      syntax: FlowNodeSyntax::ExplicitDefinition,
      label: Some(text.text),
      label_type: text.kind,
      label_span,
      label_selection,
      shape: Some(shape),
      shape_data: sd,
      icon: None,
      form: None,
      pos: None,
      img: None,
      constraint: None,
      asset_width: None,
      asset_height: None,
      styles: Vec::new(),
      classes: cls,
      link: None,
      link_target: None,
      have_callback: false,
    }
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action39<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, id, _): (usize, String, usize),
    (_, r, _): (usize, usize, usize),
    (_, sd, _): (usize, String, usize),
    (_, cls, _): (usize, Vec<String>, usize),
) -> Node
{
    Node {
    id_span: Some(SourceSpan::new(l, r)),
    id,
    provenance: FlowNodeProvenance::Authored,
    syntax: FlowNodeSyntax::ExplicitDefinition,
    label: None,
    label_type: TitleKind::Text,
    label_span: None,
    label_selection: None,
    shape: None,
    shape_data: Some(sd),
    icon: None,
    form: None,
    pos: None,
    img: None,
    constraint: None,
    asset_width: None,
    asset_height: None,
    styles: Vec::new(),
    classes: cls,
    link: None,
    link_target: None,
    have_callback: false,
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action40<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, l, _): (usize, usize, usize),
    (_, id, _): (usize, String, usize),
    (_, r, _): (usize, usize, usize),
    (_, cls, _): (usize, Vec<String>, usize),
) -> Node
{
    Node {
    id_span: Some(SourceSpan::new(l, r)),
    id,
    provenance: FlowNodeProvenance::Authored,
    syntax: if cls.is_empty() {
      FlowNodeSyntax::BareReference
    } else {
      FlowNodeSyntax::ExplicitDefinition
    },
    label: None,
    label_type: TitleKind::Text,
    label_span: None,
    label_selection: None,
    shape: None,
    shape_data: None,
    icon: None,
    form: None,
    pos: None,
    img: None,
    constraint: None,
    asset_width: None,
    asset_height: None,
    styles: Vec::new(),
    classes: cls,
    link: None,
    link_target: None,
    have_callback: false,
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action41<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, _, _): (usize, Tok, usize),
    (_, c, _): (usize, String, usize),
) -> Result<Vec<String>,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    StatementArena::vector_one(c, control)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action42<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Vec<String>
{
    Vec::new()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action43<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, String, usize),
) -> Option<String>
{
    Some(__0)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action44<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Option<String>
{
    None
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action45<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, LabeledText, usize),
) -> Option<LabeledText>
{
    Some(__0)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action46<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Option<LabeledText>
{
    None
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action47<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, String, usize),
) -> Option<String>
{
    Some(__0)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action48<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Option<String>
{
    None
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action49<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, SubgraphHeader, usize),
) -> Option<SubgraphHeader>
{
    Some(__0)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action50<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Option<SubgraphHeader>
{
    None
}

#[allow(unused_variables)]
#[allow(clippy::needless_lifetimes)]
fn __action51<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> usize
{
    *__lookbehind
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action52<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    (_, __0, _): (usize, String, usize),
) -> Option<String>
{
    Some(__0)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action53<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Option<String>
{
    None
}

#[allow(unused_variables)]
#[allow(clippy::needless_lifetimes)]
fn __action54<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> usize
{
    *__lookahead
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action55<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, Option<String>, usize),
    __2: (usize, usize, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action2(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action56<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, Option<String>, usize),
    __2: (usize, usize, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action3(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action57<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, Option<String>, usize),
    __2: (usize, usize, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action4(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action58<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, Option<String>, usize),
    __2: (usize, usize, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action5(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action59<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, usize, usize),
    __2: (usize, NodeLabelToken, usize),
    __3: (usize, Option<String>, usize),
    __4: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action36(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
        __3,
        __4,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action60<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, usize, usize),
    __2: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action37(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action61<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, usize, usize),
    __2: (usize, NodeLabelToken, usize),
    __3: (usize, Option<String>, usize),
    __4: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action38(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
        __3,
        __4,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action62<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, usize, usize),
    __2: (usize, String, usize),
    __3: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action39(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
        __3,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action63<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, usize, usize),
    __2: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action40(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action64<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, usize, usize),
    __2: (usize, String, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action54(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action13(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action65<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, Option<String>, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __1.2.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action55(
        statements,
        control,
        __0,
        __1,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action66<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, Option<String>, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __1.2.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action56(
        statements,
        control,
        __0,
        __1,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action67<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, Option<String>, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __1.2.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action57(
        statements,
        control,
        __0,
        __1,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action68<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, Option<String>, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __1.2.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action58(
        statements,
        control,
        __0,
        __1,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action69<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, NodeLabelToken, usize),
    __2: (usize, Option<String>, usize),
    __3: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action59(
        statements,
        control,
        __0,
        __temp0,
        __1,
        __2,
        __3,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action70<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action60(
        statements,
        control,
        __0,
        __temp0,
        __1,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action71<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, NodeLabelToken, usize),
    __2: (usize, Option<String>, usize),
    __3: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action61(
        statements,
        control,
        __0,
        __temp0,
        __1,
        __2,
        __3,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action72<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, String, usize),
    __2: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action62(
        statements,
        control,
        __0,
        __temp0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action73<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action63(
        statements,
        control,
        __0,
        __temp0,
        __1,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action74<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, String, usize),
) -> Result<StatementId,__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action51(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action64(
        statements,
        control,
        __0,
        __temp0,
        __1,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action75<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, String, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action52(
        statements,
        control,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action65(
        statements,
        control,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action76<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.2.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action53(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action65(
        statements,
        control,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action77<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, String, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action52(
        statements,
        control,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action66(
        statements,
        control,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action78<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.2.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action53(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action66(
        statements,
        control,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action79<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, String, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action52(
        statements,
        control,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action67(
        statements,
        control,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action80<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.2.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action53(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action67(
        statements,
        control,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action81<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, String, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action52(
        statements,
        control,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action68(
        statements,
        control,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action82<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
) -> Result<(String, Option<String>, SourceSpan),__lalrpop_util::ParseError<usize,Tok,crate::diagrams::flowchart::LexError>>
{
    let __start0 = __0.2.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action53(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action68(
        statements,
        control,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action83<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, ArrowToken, usize),
    __2: (usize, Option<LabeledText>, usize),
    __3: (usize, Vec<Node>, usize),
) -> (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)
{
    let __start0 = __0.0.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action47(
        statements,
        control,
        __0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action28(
        statements,
        control,
        __temp0,
        __1,
        __2,
        __3,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action84<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, ArrowToken, usize),
    __1: (usize, Option<LabeledText>, usize),
    __2: (usize, Vec<Node>, usize),
) -> (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action48(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action28(
        statements,
        control,
        __temp0,
        __0,
        __1,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action85<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, ArrowToken, usize),
    __2: (usize, LabeledText, usize),
    __3: (usize, Vec<Node>, usize),
) -> (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)
{
    let __start0 = __2.0.clone();
    let __end0 = __2.2.clone();
    let __temp0 = __action45(
        statements,
        control,
        __2,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action83(
        statements,
        control,
        __0,
        __1,
        __temp0,
        __3,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action86<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, ArrowToken, usize),
    __2: (usize, Vec<Node>, usize),
) -> (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)
{
    let __start0 = __1.2.clone();
    let __end0 = __2.0.clone();
    let __temp0 = __action46(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action83(
        statements,
        control,
        __0,
        __1,
        __temp0,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action87<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, ArrowToken, usize),
    __1: (usize, LabeledText, usize),
    __2: (usize, Vec<Node>, usize),
) -> (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action45(
        statements,
        control,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action84(
        statements,
        control,
        __0,
        __temp0,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action88<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, ArrowToken, usize),
    __1: (usize, Vec<Node>, usize),
) -> (Option<String>, LinkToken, Option<LabeledText>, Vec<Node>)
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action46(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action84(
        statements,
        control,
        __0,
        __temp0,
        __1,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action89<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, NodeLabelToken, usize),
    __2: (usize, String, usize),
    __3: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __2.0.clone();
    let __end0 = __2.2.clone();
    let __temp0 = __action43(
        statements,
        control,
        __2,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action69(
        statements,
        control,
        __0,
        __1,
        __temp0,
        __3,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action90<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, NodeLabelToken, usize),
    __2: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __1.2.clone();
    let __end0 = __2.0.clone();
    let __temp0 = __action44(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action69(
        statements,
        control,
        __0,
        __1,
        __temp0,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action91<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, NodeLabelToken, usize),
    __2: (usize, String, usize),
    __3: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __2.0.clone();
    let __end0 = __2.2.clone();
    let __temp0 = __action43(
        statements,
        control,
        __2,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action71(
        statements,
        control,
        __0,
        __1,
        __temp0,
        __3,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action92<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, String, usize),
    __1: (usize, NodeLabelToken, usize),
    __2: (usize, Vec<String>, usize),
) -> Node
{
    let __start0 = __1.2.clone();
    let __end0 = __2.0.clone();
    let __temp0 = __action44(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action71(
        statements,
        control,
        __0,
        __1,
        __temp0,
        __2,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action93<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, SubgraphHeader, usize),
    __2: (usize, (), usize),
    __3: (usize, StatementList, usize),
    __4: (usize, Tok, usize),
) -> SubgraphBlock
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action49(
        statements,
        control,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action27(
        statements,
        control,
        __0,
        __temp0,
        __2,
        __3,
        __4,
    )
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes,
    clippy::just_underscores_and_digits, clippy::clone_on_copy, clippy::unit_arg)]
fn __action94<
>(
    statements: &mut StatementArena,
    control: &crate::OperationControl,
    __0: (usize, Tok, usize),
    __1: (usize, (), usize),
    __2: (usize, StatementList, usize),
    __3: (usize, Tok, usize),
) -> SubgraphBlock
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action50(
        statements,
        control,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action27(
        statements,
        control,
        __0,
        __temp0,
        __1,
        __2,
        __3,
    )
}

#[allow(clippy::type_complexity, dead_code)]
pub trait __ToTriple<>
{
    fn to_triple(self) -> Result<(usize,Tok,usize), __lalrpop_util::ParseError<usize, Tok, crate::diagrams::flowchart::LexError>>;
}

impl<> __ToTriple<> for (usize, Tok, usize)
{
    fn to_triple(self) -> Result<(usize,Tok,usize), __lalrpop_util::ParseError<usize, Tok, crate::diagrams::flowchart::LexError>> {
        Ok(self)
    }
}
impl<> __ToTriple<> for Result<(usize, Tok, usize), crate::diagrams::flowchart::LexError>
{
    fn to_triple(self) -> Result<(usize,Tok,usize), __lalrpop_util::ParseError<usize, Tok, crate::diagrams::flowchart::LexError>> {
        self.map_err(|error| __lalrpop_util::ParseError::User { error })
    }
}
