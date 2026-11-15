// auto-generated: "lalrpop 0.23.2"
// sha3: 1f7da3837f2093b65fbe46ae8a57c7406b2d31e63d026bc190c7337b64b1ce4d
// features:
use crate::diagrams::er::{Action, Attribute, ErActionArena, ErActionList, RelSpec, SpannedDirection, SpannedIdList, Tok};
#[allow(unused_extern_crates)]
extern crate lalrpop_util as __lalrpop_util;
#[allow(unused_imports)]
use self::__lalrpop_util::state_machine as __state_machine;
#[allow(unused_extern_crates)]
extern crate alloc;

#[rustfmt::skip]
#[allow(explicit_outlives_requirements, non_snake_case, non_camel_case_types, unused_mut, unused_variables, unused_imports, unused_parens, clippy::needless_lifetimes, clippy::type_complexity, clippy::needless_return, clippy::too_many_arguments, clippy::match_single_binding, clippy::clone_on_copy, clippy::unit_arg)]
mod __parse__Actions {

    use crate::diagrams::er::{Action, Attribute, ErActionArena, ErActionList, RelSpec, SpannedDirection, SpannedIdList, Tok};
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
        Variant2(SpannedIdList),
        Variant3(SpannedDirection),
        Variant4(Action),
        Variant5(ErActionList),
        Variant6(Attribute),
        Variant7(Vec<String>),
        Variant8(Vec<Attribute>),
        Variant9(Vec<Action>),
        Variant10(()),
        Variant11(RelSpec),
        Variant12((String, String, String)),
    }
    const __ACTION: &[i8] = &[
        // State 0
        2, -32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 1
        2, -32, -32, 0, 0, 0, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, -32, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 2
        2, 0, -32, 0, 0, 0, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 3
        0, 0, 10, 0, 0, 0, 51, 49, 50, 52, 0, 0, 0, 0, 0, 0, 0, 0, 8, 7, 6, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 4
        2, 0, -32, 0, 0, 0, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 5
        0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 6
        0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 7
        0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 8
        0, 0, 57, 58, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 9
        -26, 0, -26, 0, 0, 0, -26, -26, -26, -26, 15, 0, 63, 0, 16, 0, 0, 0, -26, -26, -26, -26, -26, 65, 64, 61, 62, 60, 0, 0, 0, 0, 0,
        // State 10
        0, 0, 10, 0, 0, 0, 51, 49, 50, 52, 0, 0, 0, 0, 0, 0, 0, 0, 8, 7, 6, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 11
        0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 12
        18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 13
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 70, 71, 0, 0, 0,
        // State 14
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 75, 0, 0,
        // State 15
        0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 16
        0, 0, 10, 0, 0, 0, 51, 49, 50, 52, 0, 0, 0, 0, 0, 0, 0, 0, 8, 7, 6, 9, 78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 17
        2, 0, -32, 0, 0, 0, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, -32, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 18
        0, 0, 81, 82, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 19
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 65, 64, 61, 62, 60, 0, 0, 0, 0, 0,
        // State 20
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 75, 0, 0,
        // State 21
        -25, 0, -25, 0, 0, 0, -25, -25, -25, -25, 28, 0, 0, 0, 0, 0, 0, 0, -25, -25, -25, -25, -25, 65, 64, 61, 62, 60, 0, 0, 0, 0, 0,
        // State 22
        2, 0, -32, 0, 0, 0, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, -32, -32, -32, -32, -32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 23
        0, 0, 81, 82, 0, 0, 0, 0, 0, 0, 0, 0, 0, 90, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 24
        0, 0, 92, 93, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 25
        0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 26
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -5, 96, 97,
        // State 27
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 75, 0, 0,
        // State 28
        0, 0, 10, 0, 0, 0, 51, 49, 50, 52, 0, 0, 0, 0, 0, 0, 0, 0, 8, 7, 6, 9, 78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 29
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 75, 0, 0,
        // State 30
        0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 31
        0, 0, 92, 93, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 32
        0, 0, 92, 93, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 33
        0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 34
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 75, 0, 0,
        // State 35
        0, 0, 92, 93, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 36
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 37
        0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 38
        0, -33, -33, 0, 0, 0, -33, -33, -33, -33, 0, 0, 0, 0, 0, 0, 0, 0, -33, -33, -33, -33, -33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 39
        -46, 0, -46, 0, 0, 0, -46, -46, -46, -46, 0, 0, 0, 0, 0, 0, 0, 0, -46, -46, -46, -46, -46, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 40
        -48, 0, -48, 0, 0, 0, -48, -48, -48, -48, 0, 0, 0, 0, 0, 0, 0, 0, -48, -48, -48, -48, -48, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 41
        -49, 0, -49, 0, 0, 0, -49, -49, -49, -49, 0, 0, 0, 0, 0, 0, 0, 0, -49, -49, -49, -49, -49, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 42
        -47, 0, -47, 0, 0, 0, -47, -47, -47, -47, 0, 0, 0, 0, 0, 0, 0, 0, -47, -47, -47, -47, -47, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 43
        -45, 0, -45, 0, 0, 0, -45, -45, -45, -45, 0, 0, 0, 0, 0, 0, 0, 0, -45, -45, -45, -45, -45, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 44
        -44, 0, -44, 0, 0, 0, -44, -44, -44, -44, 0, 0, 0, 0, 0, 0, 0, 0, -44, -44, -44, -44, -44, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 45
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 46
        -50, 0, -50, 0, 0, 0, -50, -50, -50, -50, 0, 0, 0, 0, 0, 0, 0, 0, -50, -50, -50, -50, -50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 47
        -51, 0, -51, 0, 0, 0, -51, -51, -51, -51, 0, 0, 0, 0, 0, 0, 0, 0, -51, -51, -51, -51, -51, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 48
        -2, 0, -2, 0, 0, 0, -2, -2, -2, -2, 0, 0, 0, 0, 0, 0, 0, 0, -2, -2, -2, -2, -2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 49
        -3, 0, -3, 0, 0, 0, -3, -3, -3, -3, 0, 0, 0, 0, 0, 0, 0, 0, -3, -3, -3, -3, -3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 50
        -1, 0, -1, 0, 0, 0, -1, -1, -1, -1, 0, 0, 0, 0, 0, 0, 0, 0, -1, -1, -1, -1, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 51
        -22, 0, -22, 0, 0, 0, -22, -22, -22, -22, 0, 0, 0, 0, 0, 0, 0, 0, -22, -22, -22, -22, -22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 52
        -31, 0, -31, 0, -31, -31, -31, -31, -31, -31, -31, 0, 0, 0, 0, -31, 0, 0, -31, -31, -31, -31, -31, -31, -31, -31, -31, -31, 0, 0, 0, 0, 0,
        // State 53
        0, 0, 0, 0, 0, 68, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 54
        0, 0, 0, 0, 0, 69, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 55
        -57, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 56
        -59, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -59, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 57
        -60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 58
        0, 0, 72, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 59
        0, 0, -19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -19, -19, 0, 0, 0,
        // State 60
        0, 0, -17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -17, -17, 0, 0, 0,
        // State 61
        0, 0, -18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -18, -18, 0, 0, 0,
        // State 62
        0, 0, 76, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 63
        0, 0, -16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -16, -16, 0, 0, 0,
        // State 64
        0, 0, -15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -15, -15, 0, 0, 0,
        // State 65
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 66
        -21, 0, -21, 0, 0, 0, -21, -21, -21, -21, 0, 0, 0, 0, 0, 0, 0, 0, -21, -21, -21, -21, -21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 67
        -20, 0, -20, 0, 0, 0, -20, -20, -20, -20, 0, 0, 0, 0, 0, 0, 0, 0, -20, -20, -20, -20, -20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 68
        -54, 0, -54, 0, 0, 0, -54, -54, -54, -54, 0, 0, 0, 0, 0, 0, 0, 0, -54, -54, -54, -54, -54, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 69
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -36, -36, -36, -36, -36, 0, 0, 0, 0, 0,
        // State 70
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -37, -37, -37, -37, -37, 0, 0, 0, 0, 0,
        // State 71
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 72
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 27, 0, 0,
        // State 73
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 85, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 74
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 86, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -11, 0, 0,
        // State 75
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 87, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 76
        -61, 0, -61, 0, 0, 0, -61, -61, -61, -61, 0, 0, 0, 0, 0, 0, 0, 0, -61, -61, -61, -61, -61, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 77
        -55, 0, -55, 0, 0, 0, -55, -55, -55, -55, 0, 0, 0, 0, 0, 0, 0, 0, -55, -55, -55, -55, -55, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 78
        0, 0, -34, 0, 0, 0, -34, -34, -34, -34, 0, 0, 0, 0, 0, 0, 0, 0, -34, -34, -34, -34, -34, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 79
        0, 0, -62, -62, 0, 0, 0, 0, 0, 0, 0, 0, 0, -62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 80
        0, 0, -64, -64, 0, 0, 0, 0, 0, 0, 0, 0, 0, -64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 81
        0, 0, -65, -65, 0, 0, 0, 0, 0, 0, 0, 0, 0, -65, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 82
        0, 0, -35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 83
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 84
        -23, 0, -23, 0, 0, 0, -23, -23, -23, -23, 0, 0, 0, 0, 0, 0, 0, 0, -23, -23, -23, -23, -23, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 85
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -12, 0, 0,
        // State 86
        -28, 0, -28, 0, 0, 0, -28, -28, -28, -28, 30, 0, 0, 0, 31, 0, 0, 0, -28, -28, -28, -28, -28, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 87
        0, 0, 98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 88
        0, 0, -63, -63, 0, 0, 0, 0, 0, 0, 0, 0, 0, -63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 89
        -58, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 90
        -38, 0, -38, 0, 0, 0, -38, -38, -38, -38, 0, 0, 0, 0, 0, 0, 0, 0, -38, -38, -38, -38, -38, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 91
        -42, 0, -42, 0, 0, 0, -42, -42, -42, -42, 0, 0, 0, 0, 0, 0, 0, 0, -42, -42, -42, -42, -42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 92
        -43, 0, -43, 0, 0, 0, -43, -43, -43, -43, 0, 0, 0, 0, 0, 0, 0, 0, -43, -43, -43, -43, -43, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 93
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 94
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -6, 0, 0, 0, 0, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -6, 0, 102,
        // State 95
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -9, 0, 0, 0, 0, -9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -9, 0, -9,
        // State 96
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -7, 0, 0,
        // State 97
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 98
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 105, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 99
        -56, 0, -56, 0, 0, 0, -56, -56, -56, -56, 0, 0, 0, 0, 0, 0, 0, 0, -56, -56, -56, -56, -56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 100
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 107, 0,
        // State 101
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -8, 0, 0,
        // State 102
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 108, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 103
        -30, 0, -30, 0, 0, 0, -30, -30, -30, -30, 35, 0, 0, 0, 0, 0, 0, 0, -30, -30, -30, -30, -30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 104
        -24, 0, -24, 0, 0, 0, -24, -24, -24, -24, 0, 0, 0, 0, 0, 0, 0, 0, -24, -24, -24, -24, -24, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 105
        -41, 0, -41, 0, 0, 0, -41, -41, -41, -41, 0, 0, 0, 0, 0, 0, 0, 0, -41, -41, -41, -41, -41, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 106
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -10, 0, 0, 0, 0, -10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -10, 0, -10,
        // State 107
        -27, 0, -27, 0, 0, 0, -27, -27, -27, -27, 0, 0, 0, 0, 0, 0, 0, 0, -27, -27, -27, -27, -27, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 108
        -40, 0, -40, 0, 0, 0, -40, -40, -40, -40, 0, 0, 0, 0, 0, 0, 0, 0, -40, -40, -40, -40, -40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 109
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 36, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 110
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 112, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 111
        -29, 0, -29, 0, 0, 0, -29, -29, -29, -29, 0, 0, 0, 0, 0, 0, 0, 0, -29, -29, -29, -29, -29, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 112
        -39, 0, -39, 0, 0, 0, -39, -39, -39, -39, 0, 0, 0, 0, 0, 0, 0, 0, -39, -39, -39, -39, -39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    fn __action(state: i8, integer: usize) -> i8 {
        __ACTION[(state as usize) * 33 + integer]
    }
    const __EOF_ACTION: &[i8] = &[
        // State 0
        0,
        // State 1
        -32,
        // State 2
        -32,
        // State 3
        -52,
        // State 4
        -32,
        // State 5
        0,
        // State 6
        0,
        // State 7
        0,
        // State 8
        0,
        // State 9
        -26,
        // State 10
        -52,
        // State 11
        0,
        // State 12
        0,
        // State 13
        0,
        // State 14
        0,
        // State 15
        0,
        // State 16
        0,
        // State 17
        0,
        // State 18
        0,
        // State 19
        0,
        // State 20
        0,
        // State 21
        -25,
        // State 22
        0,
        // State 23
        0,
        // State 24
        0,
        // State 25
        0,
        // State 26
        0,
        // State 27
        0,
        // State 28
        0,
        // State 29
        0,
        // State 30
        0,
        // State 31
        0,
        // State 32
        0,
        // State 33
        0,
        // State 34
        0,
        // State 35
        0,
        // State 36
        -66,
        // State 37
        0,
        // State 38
        -33,
        // State 39
        -46,
        // State 40
        -48,
        // State 41
        -49,
        // State 42
        -47,
        // State 43
        -45,
        // State 44
        -44,
        // State 45
        -4,
        // State 46
        -50,
        // State 47
        -51,
        // State 48
        -2,
        // State 49
        -3,
        // State 50
        -1,
        // State 51
        -22,
        // State 52
        -31,
        // State 53
        0,
        // State 54
        0,
        // State 55
        0,
        // State 56
        0,
        // State 57
        0,
        // State 58
        0,
        // State 59
        0,
        // State 60
        0,
        // State 61
        0,
        // State 62
        0,
        // State 63
        0,
        // State 64
        0,
        // State 65
        -53,
        // State 66
        -21,
        // State 67
        -20,
        // State 68
        -54,
        // State 69
        0,
        // State 70
        0,
        // State 71
        0,
        // State 72
        0,
        // State 73
        0,
        // State 74
        0,
        // State 75
        0,
        // State 76
        -61,
        // State 77
        -55,
        // State 78
        0,
        // State 79
        0,
        // State 80
        0,
        // State 81
        0,
        // State 82
        0,
        // State 83
        0,
        // State 84
        -23,
        // State 85
        0,
        // State 86
        -28,
        // State 87
        0,
        // State 88
        0,
        // State 89
        0,
        // State 90
        -38,
        // State 91
        -42,
        // State 92
        -43,
        // State 93
        0,
        // State 94
        0,
        // State 95
        0,
        // State 96
        0,
        // State 97
        0,
        // State 98
        0,
        // State 99
        -56,
        // State 100
        0,
        // State 101
        0,
        // State 102
        0,
        // State 103
        -30,
        // State 104
        -24,
        // State 105
        -41,
        // State 106
        0,
        // State 107
        -27,
        // State 108
        -40,
        // State 109
        0,
        // State 110
        0,
        // State 111
        -29,
        // State 112
        -39,
    ];
    fn __goto(state: i8, nt: usize) -> i8 {
        match nt {
            0 => 39,
            1 => 36,
            2 => 20,
            3 => 94,
            4 => 72,
            5 => match state {
                20 => 83,
                27 => 98,
                29 => 102,
                34 => 110,
                _ => 73,
            },
            6 => match state {
                19 => 82,
                _ => 13,
            },
            7 => 40,
            8 => 41,
            9 => 42,
            10 => 43,
            11 => match state {
                15 => 21,
                6 => 53,
                7 => 54,
                11 => 66,
                25 => 93,
                30 => 103,
                33 => 109,
                _ => 11,
            },
            12 => match state {
                4 => 10,
                22 => 28,
                0 => 37,
                1 => 38,
                17 => 78,
                _ => 3,
            },
            13 => 16,
            14 => match state {
                21 => 87,
                _ => 58,
            },
            15 => 19,
            16 => 44,
            17 => match state {
                31 => 105,
                32 => 108,
                35 => 112,
                _ => 90,
            },
            18 => match state {
                16 | 28 => 22,
                _ => 4,
            },
            19 => match state {
                10 => 65,
                _ => 45,
            },
            20 => 46,
            21 => match state {
                28 => 99,
                _ => 76,
            },
            22 => 12,
            23 => 55,
            24 => 47,
            25 => 23,
            26 => match state {
                23 => 88,
                _ => 79,
            },
            _ => 0,
        }
    }
    #[allow(clippy::needless_raw_string_hashes)]
    const __TERMINAL: &[&str] = &[
        r###"Newline"###,
        r###""erDiagram""###,
        r###"Name"###,
        r###"Str"###,
        r###"IdList"###,
        r###"RestOfLine"###,
        r###"AccTitle"###,
        r###"AccDescr"###,
        r###"AccDescrMultiline"###,
        r###"Direction"###,
        r###"BlockStart"###,
        r###"BlockStop"###,
        r###"SquareStart"###,
        r###"SquareStop"###,
        r###"StyleSeparator"###,
        r###"Colon"###,
        r###"Comma"###,
        r###"Question"###,
        r###""style""###,
        r###""classDef""###,
        r###""class""###,
        r###""subgraph""###,
        r###""end""###,
        r###"ZeroOrOne"###,
        r###"ZeroOrMore"###,
        r###"OneOrMore"###,
        r###"OnlyOne"###,
        r###"MdParent"###,
        r###"Identifying"###,
        r###"NonIdentifying"###,
        r###"AttrWord"###,
        r###"AttrKey"###,
        r###"Comment"###,
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
    struct __StateMachine<'__0>
    where
    {
        arena: &'__0 mut ErActionArena,
        __phantom: core::marker::PhantomData<()>,
    }
    impl<'__0> __state_machine::ParserDefinition for __StateMachine<'__0>
    where
    {
        type Location = usize;
        type Error = crate::diagrams::er::LexError;
        type Token = Tok;
        type TokenIndex = usize;
        type Symbol = __Symbol<>;
        type Success = ErActionList;
        type StateIndex = i8;
        type Action = i8;
        type ReduceIndex = i8;
        type NonterminalIndex = usize;

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
            __action(state, 33 - 1)
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
                self.arena,
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
            Tok::Newline if true => Some(0),
            Tok::ErDiagram if true => Some(1),
            Tok::Name(_) if true => Some(2),
            Tok::Str(_) if true => Some(3),
            Tok::IdList(_) if true => Some(4),
            Tok::RestOfLine(_) if true => Some(5),
            Tok::AccTitle(_) if true => Some(6),
            Tok::AccDescr(_) if true => Some(7),
            Tok::AccDescrMultiline(_) if true => Some(8),
            Tok::Direction(_) if true => Some(9),
            Tok::BlockStart if true => Some(10),
            Tok::BlockStop if true => Some(11),
            Tok::SquareStart if true => Some(12),
            Tok::SquareStop if true => Some(13),
            Tok::StyleSeparator if true => Some(14),
            Tok::Colon if true => Some(15),
            Tok::Comma if true => Some(16),
            Tok::Question if true => Some(17),
            Tok::StyleKw if true => Some(18),
            Tok::ClassDefKw if true => Some(19),
            Tok::ClassKw if true => Some(20),
            Tok::SubgraphKw if true => Some(21),
            Tok::EndKw if true => Some(22),
            Tok::ZeroOrOne if true => Some(23),
            Tok::ZeroOrMore if true => Some(24),
            Tok::OneOrMore if true => Some(25),
            Tok::OnlyOne if true => Some(26),
            Tok::MdParent if true => Some(27),
            Tok::Identifying if true => Some(28),
            Tok::NonIdentifying if true => Some(29),
            Tok::AttrWord(_) if true => Some(30),
            Tok::AttrKey(_) if true => Some(31),
            Tok::Comment(_) if true => Some(32),
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
            0 | 1 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 21 | 22 | 23 | 24 | 25 | 26 | 27 | 28 | 29 => __Symbol::Variant0(__token),
            2 | 3 | 5 | 6 | 7 | 8 | 30 | 31 | 32 => match __token {
                Tok::Name(__tok0) | Tok::Str(__tok0) | Tok::RestOfLine(__tok0) | Tok::AccTitle(__tok0) | Tok::AccDescr(__tok0) | Tok::AccDescrMultiline(__tok0) | Tok::AttrWord(__tok0) | Tok::AttrKey(__tok0) | Tok::Comment(__tok0) if true => __Symbol::Variant1(__tok0),
                _ => unreachable!(),
            },
            4 => match __token {
                Tok::IdList(__tok0) if true => __Symbol::Variant2(__tok0),
                _ => unreachable!(),
            },
            9 => match __token {
                Tok::Direction(__tok0) if true => __Symbol::Variant3(__tok0),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
    fn __simulate_reduce<
        '__0,
    >(
        __reduce_index: i8,
        _: core::marker::PhantomData<()>,
    ) -> __state_machine::SimulatedReduce<__StateMachine<'__0>>
    {
        match __reduce_index {
            0 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 0,
                }
            }
            1 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 0,
                }
            }
            2 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 0,
                }
            }
            3 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 1,
                }
            }
            4 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 2,
                }
            }
            5 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 2,
                }
            }
            6 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 2,
                }
            }
            7 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 2,
                }
            }
            8 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 3,
                }
            }
            9 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 3,
                }
            }
            10 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 4,
                }
            }
            11 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 4,
                }
            }
            12 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 5,
                }
            }
            13 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 5,
                }
            }
            14 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 6,
                }
            }
            15 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 6,
                }
            }
            16 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 6,
                }
            }
            17 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 6,
                }
            }
            18 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 6,
                }
            }
            19 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 7,
                }
            }
            20 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 8,
                }
            }
            21 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 9,
                }
            }
            22 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 10,
                }
            }
            23 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 6,
                    nonterminal_produced: 10,
                }
            }
            24 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
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
                    states_to_pop: 7,
                    nonterminal_produced: 10,
                }
            }
            27 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 10,
                }
            }
            28 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 9,
                    nonterminal_produced: 10,
                }
            }
            29 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 6,
                    nonterminal_produced: 10,
                }
            }
            30 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 11,
                }
            }
            31 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 12,
                }
            }
            32 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 12,
                }
            }
            33 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 13,
                }
            }
            34 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 14,
                }
            }
            35 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 15,
                }
            }
            36 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 15,
                }
            }
            37 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 5,
                    nonterminal_produced: 16,
                }
            }
            38 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 9,
                    nonterminal_produced: 16,
                }
            }
            39 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 7,
                    nonterminal_produced: 16,
                }
            }
            40 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 7,
                    nonterminal_produced: 16,
                }
            }
            41 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 17,
                }
            }
            42 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 17,
                }
            }
            43 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 18,
                }
            }
            44 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 18,
                }
            }
            45 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 18,
                }
            }
            46 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 18,
                }
            }
            47 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 18,
                }
            }
            48 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 18,
                }
            }
            49 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 18,
                }
            }
            50 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 18,
                }
            }
            51 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 0,
                    nonterminal_produced: 19,
                }
            }
            52 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 19,
                }
            }
            53 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 3,
                    nonterminal_produced: 20,
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
                    states_to_pop: 3,
                    nonterminal_produced: 21,
                }
            }
            56 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 22,
                }
            }
            57 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 22,
                }
            }
            58 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 23,
                }
            }
            59 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 23,
                }
            }
            60 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 4,
                    nonterminal_produced: 24,
                }
            }
            61 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 25,
                }
            }
            62 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 2,
                    nonterminal_produced: 25,
                }
            }
            63 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 26,
                }
            }
            64 => {
                __state_machine::SimulatedReduce::Reduce {
                    states_to_pop: 1,
                    nonterminal_produced: 26,
                }
            }
            65 => __state_machine::SimulatedReduce::Accept,
            _ => panic!("invalid reduction index {__reduce_index}")
        }
    }
    pub struct ActionsParser {
        _priv: (),
    }

    impl Default for ActionsParser { fn default() -> Self { Self::new() } }
    impl ActionsParser {
        pub fn new() -> ActionsParser {
            ActionsParser {
                _priv: (),
            }
        }

        #[allow(dead_code)]
        pub fn parse<
            __TOKEN: __ToTriple<>,
            __TOKENS: IntoIterator<Item=__TOKEN>,
        >(
            &self,
            arena: &mut ErActionArena,
            __tokens0: __TOKENS,
        ) -> Result<ErActionList, __lalrpop_util::ParseError<usize, Tok, crate::diagrams::er::LexError>>
        {
            let __tokens = __tokens0.into_iter();
            let mut __tokens = __tokens.map(|t| __ToTriple::to_triple(t));
            __state_machine::Parser::drive(
                __StateMachine {
                    arena,
                    __phantom: core::marker::PhantomData::<()>,
                },
                __tokens,
            )
        }
    }
    fn __accepts<
        '__0,
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
        arena: &mut ErActionArena,
        __action: i8,
        __lookahead_start: Option<&usize>,
        __states: &mut alloc::vec::Vec<i8>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> Option<Result<ErActionList,__lalrpop_util::ParseError<usize, Tok, crate::diagrams::er::LexError>>>
    {
        let (__pop_states, __nonterminal) = match __action {
            0 => {
                __reduce0(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            1 => {
                __reduce1(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            2 => {
                __reduce2(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            3 => {
                __reduce3(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            4 => {
                __reduce4(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            5 => {
                __reduce5(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            6 => {
                __reduce6(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            7 => {
                __reduce7(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            8 => {
                __reduce8(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            9 => {
                __reduce9(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            10 => {
                __reduce10(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            11 => {
                __reduce11(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            12 => {
                __reduce12(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            13 => {
                __reduce13(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            14 => {
                __reduce14(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            15 => {
                __reduce15(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            16 => {
                __reduce16(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            17 => {
                __reduce17(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            18 => {
                __reduce18(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            19 => {
                __reduce19(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            20 => {
                __reduce20(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            21 => {
                __reduce21(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            22 => {
                __reduce22(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            23 => {
                __reduce23(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            24 => {
                __reduce24(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            25 => {
                __reduce25(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            26 => {
                __reduce26(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            27 => {
                __reduce27(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            28 => {
                __reduce28(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            29 => {
                __reduce29(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            30 => {
                __reduce30(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            31 => {
                __reduce31(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            32 => {
                __reduce32(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            33 => {
                __reduce33(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            34 => {
                __reduce34(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            35 => {
                __reduce35(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            36 => {
                __reduce36(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            37 => {
                __reduce37(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            38 => {
                __reduce38(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            39 => {
                __reduce39(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            40 => {
                __reduce40(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            41 => {
                __reduce41(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            42 => {
                __reduce42(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            43 => {
                __reduce43(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            44 => {
                __reduce44(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            45 => {
                __reduce45(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            46 => {
                __reduce46(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            47 => {
                __reduce47(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            48 => {
                __reduce48(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            49 => {
                __reduce49(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            50 => {
                __reduce50(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            51 => {
                __reduce51(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            52 => {
                __reduce52(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            53 => {
                __reduce53(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            54 => {
                __reduce54(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            55 => {
                __reduce55(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            56 => {
                __reduce56(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            57 => {
                __reduce57(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            58 => {
                __reduce58(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            59 => {
                __reduce59(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            60 => {
                __reduce60(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            61 => {
                __reduce61(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            62 => {
                __reduce62(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            63 => {
                __reduce63(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            64 => {
                __reduce64(arena, __lookahead_start, __symbols, core::marker::PhantomData::<()>)
            }
            65 => {
                // __Actions = Actions => ActionFn(0);
                let __sym0 = __pop_Variant5(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = super::__action0::<>(arena, __sym0);
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
    fn __pop_Variant10<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, (), usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant10(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant12<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, (String, String, String), usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant12(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant4<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Action, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant4(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant6<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Attribute, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant6(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant5<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, ErActionList, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant5(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant11<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, RelSpec, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant11(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant3<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, SpannedDirection, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant3(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant2<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, SpannedIdList, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant2(__v), __r)) => (__l, __v, __r),
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
    fn __pop_Variant9<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Vec<Action>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant9(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant8<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Vec<Attribute>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant8(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant7<
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>
    ) -> (usize, Vec<String>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant7(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __reduce0<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // AccStatement = AccTitle => ActionFn(26);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action26::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (1, 0)
    }
    fn __reduce1<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // AccStatement = AccDescr => ActionFn(27);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action27::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (1, 0)
    }
    fn __reduce2<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // AccStatement = AccDescrMultiline => ActionFn(28);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action28::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (1, 0)
    }
    fn __reduce3<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Actions = Newlines, "erDiagram", Newlines, Statements => ActionFn(1);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant5(__symbols);
        let __sym2 = __pop_Variant10(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant10(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action1::<>(arena, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (4, 1)
    }
    fn __reduce4<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Attribute = AttributeType, AttrWord => ActionFn(58);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action58::<>(arena, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (2, 2)
    }
    fn __reduce5<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Attribute = AttributeType, AttrWord, AttributeKeys => ActionFn(59);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant7(__symbols);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action59::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (3, 2)
    }
    fn __reduce6<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Attribute = AttributeType, AttrWord, Comment => ActionFn(60);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action60::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (3, 2)
    }
    fn __reduce7<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Attribute = AttributeType, AttrWord, AttributeKeys, Comment => ActionFn(61);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant1(__symbols);
        let __sym2 = __pop_Variant7(__symbols);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action61::<>(arena, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (4, 2)
    }
    fn __reduce8<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // AttributeKeys = AttrKey => ActionFn(64);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action64::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant7(__nt), __end));
        (1, 3)
    }
    fn __reduce9<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // AttributeKeys = AttributeKeys, Comma, AttrKey => ActionFn(65);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant7(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action65::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant7(__nt), __end));
        (3, 3)
    }
    fn __reduce10<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // AttributeType = AttrWord => ActionFn(62);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action62::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 4)
    }
    fn __reduce11<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // AttributeType = AttrWord, Question => ActionFn(63);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action63::<>(arena, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (2, 4)
    }
    fn __reduce12<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Attributes =  => ActionFn(56);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action56::<>(arena, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant8(__nt), __end));
        (0, 5)
    }
    fn __reduce13<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Attributes = Attribute, Attributes => ActionFn(57);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant8(__symbols);
        let __sym0 = __pop_Variant6(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action57::<>(arena, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant8(__nt), __end));
        (2, 5)
    }
    fn __reduce14<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Cardinality = ZeroOrOne => ActionFn(49);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action49::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 6)
    }
    fn __reduce15<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Cardinality = ZeroOrMore => ActionFn(50);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action50::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 6)
    }
    fn __reduce16<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Cardinality = OneOrMore => ActionFn(51);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action51::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 6)
    }
    fn __reduce17<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Cardinality = OnlyOne => ActionFn(52);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action52::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 6)
    }
    fn __reduce18<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Cardinality = MdParent => ActionFn(53);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action53::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 6)
    }
    fn __reduce19<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // ClassDefStatement = "classDef", IdNames, RestOfLine => ActionFn(31);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant7(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action31::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (3, 7)
    }
    fn __reduce20<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // ClassStatement = "class", IdNames, IdNames => ActionFn(32);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant7(__symbols);
        let __sym1 = __pop_Variant7(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action32::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (3, 8)
    }
    fn __reduce21<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // DirectionStatement = Direction => ActionFn(29);
        let __sym0 = __pop_Variant3(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action29::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (1, 9)
    }
    fn __reduce22<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EntityStatement = Name, BlockStart, Attributes, BlockStop => ActionFn(37);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant8(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action37::<>(arena, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (4, 10)
    }
    fn __reduce23<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EntityStatement = Name, StyleSeparator, IdNames, BlockStart, Attributes, BlockStop => ActionFn(38);
        assert!(__symbols.len() >= 6);
        let __sym5 = __pop_Variant0(__symbols);
        let __sym4 = __pop_Variant8(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant7(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym5.2.clone();
        let __nt = super::__action38::<>(arena, __sym0, __sym1, __sym2, __sym3, __sym4, __sym5);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (6, 10)
    }
    fn __reduce24<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EntityStatement = Name, StyleSeparator, IdNames => ActionFn(39);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant7(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action39::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (3, 10)
    }
    fn __reduce25<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EntityStatement = Name => ActionFn(40);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action40::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (1, 10)
    }
    fn __reduce26<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EntityStatement = Name, SquareStart, Name, SquareStop, BlockStart, Attributes, BlockStop => ActionFn(41);
        assert!(__symbols.len() >= 7);
        let __sym6 = __pop_Variant0(__symbols);
        let __sym5 = __pop_Variant8(__symbols);
        let __sym4 = __pop_Variant0(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym6.2.clone();
        let __nt = super::__action41::<>(arena, __sym0, __sym1, __sym2, __sym3, __sym4, __sym5, __sym6);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (7, 10)
    }
    fn __reduce27<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EntityStatement = Name, SquareStart, Name, SquareStop => ActionFn(42);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action42::<>(arena, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (4, 10)
    }
    fn __reduce28<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EntityStatement = Name, SquareStart, Name, SquareStop, StyleSeparator, IdNames, BlockStart, Attributes, BlockStop => ActionFn(43);
        assert!(__symbols.len() >= 9);
        let __sym8 = __pop_Variant0(__symbols);
        let __sym7 = __pop_Variant8(__symbols);
        let __sym6 = __pop_Variant0(__symbols);
        let __sym5 = __pop_Variant7(__symbols);
        let __sym4 = __pop_Variant0(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym8.2.clone();
        let __nt = super::__action43::<>(arena, __sym0, __sym1, __sym2, __sym3, __sym4, __sym5, __sym6, __sym7, __sym8);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (9, 10)
    }
    fn __reduce29<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // EntityStatement = Name, SquareStart, Name, SquareStop, StyleSeparator, IdNames => ActionFn(44);
        assert!(__symbols.len() >= 6);
        let __sym5 = __pop_Variant7(__symbols);
        let __sym4 = __pop_Variant0(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym5.2.clone();
        let __nt = super::__action44::<>(arena, __sym0, __sym1, __sym2, __sym3, __sym4, __sym5);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (6, 10)
    }
    fn __reduce30<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // IdNames = IdList => ActionFn(45);
        let __sym0 = __pop_Variant2(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action45::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant7(__nt), __end));
        (1, 11)
    }
    fn __reduce31<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Newlines =  => ActionFn(2);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action2::<>(arena, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant10(__nt), __end));
        (0, 12)
    }
    fn __reduce32<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Newlines = Newline, Newlines => ActionFn(3);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant10(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action3::<>(arena, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant10(__nt), __end));
        (2, 12)
    }
    fn __reduce33<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Newlines1 = Newline, Newlines => ActionFn(4);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant10(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action4::<>(arena, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant10(__nt), __end));
        (2, 13)
    }
    fn __reduce34<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // RelSpec = Cardinality, RelType, Cardinality => ActionFn(48);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action48::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant11(__nt), __end));
        (3, 14)
    }
    fn __reduce35<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // RelType = Identifying => ActionFn(54);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action54::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 15)
    }
    fn __reduce36<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // RelType = NonIdentifying => ActionFn(55);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action55::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 15)
    }
    fn __reduce37<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // RelationshipStatement = Name, RelSpec, Name, Colon, Role => ActionFn(33);
        assert!(__symbols.len() >= 5);
        let __sym4 = __pop_Variant1(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant11(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym4.2.clone();
        let __nt = super::__action33::<>(arena, __sym0, __sym1, __sym2, __sym3, __sym4);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (5, 16)
    }
    fn __reduce38<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // RelationshipStatement = Name, StyleSeparator, IdNames, RelSpec, Name, StyleSeparator, IdNames, Colon, Role => ActionFn(34);
        assert!(__symbols.len() >= 9);
        let __sym8 = __pop_Variant1(__symbols);
        let __sym7 = __pop_Variant0(__symbols);
        let __sym6 = __pop_Variant7(__symbols);
        let __sym5 = __pop_Variant0(__symbols);
        let __sym4 = __pop_Variant1(__symbols);
        let __sym3 = __pop_Variant11(__symbols);
        let __sym2 = __pop_Variant7(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym8.2.clone();
        let __nt = super::__action34::<>(arena, __sym0, __sym1, __sym2, __sym3, __sym4, __sym5, __sym6, __sym7, __sym8);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (9, 16)
    }
    fn __reduce39<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // RelationshipStatement = Name, StyleSeparator, IdNames, RelSpec, Name, Colon, Role => ActionFn(35);
        assert!(__symbols.len() >= 7);
        let __sym6 = __pop_Variant1(__symbols);
        let __sym5 = __pop_Variant0(__symbols);
        let __sym4 = __pop_Variant1(__symbols);
        let __sym3 = __pop_Variant11(__symbols);
        let __sym2 = __pop_Variant7(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym6.2.clone();
        let __nt = super::__action35::<>(arena, __sym0, __sym1, __sym2, __sym3, __sym4, __sym5, __sym6);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (7, 16)
    }
    fn __reduce40<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // RelationshipStatement = Name, RelSpec, Name, StyleSeparator, IdNames, Colon, Role => ActionFn(36);
        assert!(__symbols.len() >= 7);
        let __sym6 = __pop_Variant1(__symbols);
        let __sym5 = __pop_Variant0(__symbols);
        let __sym4 = __pop_Variant7(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant11(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym6.2.clone();
        let __nt = super::__action36::<>(arena, __sym0, __sym1, __sym2, __sym3, __sym4, __sym5, __sym6);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (7, 16)
    }
    fn __reduce41<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Role = Name => ActionFn(46);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action46::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 17)
    }
    fn __reduce42<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Role = Str => ActionFn(47);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action47::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 17)
    }
    fn __reduce43<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statement = RelationshipStatement => ActionFn(7);
        let __sym0 = __pop_Variant9(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action7::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 18)
    }
    fn __reduce44<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statement = EntityStatement => ActionFn(8);
        let __sym0 = __pop_Variant9(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action8::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 18)
    }
    fn __reduce45<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statement = AccStatement => ActionFn(9);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action9::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 18)
    }
    fn __reduce46<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statement = DirectionStatement => ActionFn(10);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action10::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 18)
    }
    fn __reduce47<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statement = ClassDefStatement => ActionFn(11);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action11::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 18)
    }
    fn __reduce48<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statement = ClassStatement => ActionFn(12);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action12::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 18)
    }
    fn __reduce49<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statement = StyleStatement => ActionFn(13);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action13::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 18)
    }
    fn __reduce50<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statement = SubgraphStatement => ActionFn(14);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action14::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 18)
    }
    fn __reduce51<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statements =  => ActionFn(5);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action5::<>(arena, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (0, 19)
    }
    fn __reduce52<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // Statements = Statement, Newlines, Statements => ActionFn(6);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant5(__symbols);
        let __sym1 = __pop_Variant10(__symbols);
        let __sym0 = __pop_Variant5(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action6::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (3, 19)
    }
    fn __reduce53<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // StyleStatement = "style", IdNames, RestOfLine => ActionFn(30);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant7(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action30::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (3, 20)
    }
    fn __reduce54<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphBody = "end" => ActionFn(16);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action16::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 21)
    }
    fn __reduce55<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphBody = Statement, Newlines, SubgraphBody => ActionFn(17);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant5(__symbols);
        let __sym1 = __pop_Variant10(__symbols);
        let __sym0 = __pop_Variant5(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action17::<>(arena, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (3, 21)
    }
    fn __reduce56<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphHeader = SubgraphName => ActionFn(18);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action18::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant12(__nt), __end));
        (1, 22)
    }
    fn __reduce57<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphHeader = SubgraphName, SquareStart, SubgraphTitle, SquareStop => ActionFn(19);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action19::<>(arena, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant12(__nt), __end));
        (4, 22)
    }
    fn __reduce58<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphName = Name => ActionFn(20);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action20::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 23)
    }
    fn __reduce59<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphName = Str => ActionFn(21);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action21::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 23)
    }
    fn __reduce60<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphStatement = "subgraph", SubgraphHeader, Newlines1, SubgraphBody => ActionFn(15);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant5(__symbols);
        let __sym2 = __pop_Variant10(__symbols);
        let __sym1 = __pop_Variant12(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action15::<>(arena, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (4, 24)
    }
    fn __reduce61<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphTitle = SubgraphTitlePart => ActionFn(22);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action22::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 25)
    }
    fn __reduce62<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphTitle = SubgraphTitle, SubgraphTitlePart => ActionFn(23);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action23::<>(arena, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (2, 25)
    }
    fn __reduce63<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphTitlePart = Name => ActionFn(24);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action24::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 26)
    }
    fn __reduce64<
    >(
        arena: &mut ErActionArena,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<>,usize)>,
        _: core::marker::PhantomData<()>,
    ) -> (usize, usize)
    {
        // SubgraphTitlePart = Str => ActionFn(25);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action25::<>(arena, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 26)
    }
}
#[allow(unused_imports)]
pub use self::__parse__Actions::ActionsParser;

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action0<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, ErActionList, usize),
) -> ErActionList
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action1<
>(
    arena: &mut ErActionArena,
    (_, _n, _): (usize, (), usize),
    (_, _, _): (usize, Tok, usize),
    (_, _n1, _): (usize, (), usize),
    (_, s, _): (usize, ErActionList, usize),
) -> ErActionList
{
    s
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action2<
>(
    arena: &mut ErActionArena,
    __lookbehind: &usize,
    __lookahead: &usize,
)
{
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action3<
>(
    arena: &mut ErActionArena,
    (_, _, _): (usize, Tok, usize),
    (_, rest, _): (usize, (), usize),
)
{
    rest
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action4<
>(
    arena: &mut ErActionArena,
    (_, _, _): (usize, Tok, usize),
    (_, _n, _): (usize, (), usize),
)
{
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action5<
>(
    arena: &mut ErActionArena,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> ErActionList
{
    ErActionList::default()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action6<
>(
    arena: &mut ErActionArena,
    (_, s, _): (usize, ErActionList, usize),
    (_, _n, _): (usize, (), usize),
    (_, rest, _): (usize, ErActionList, usize),
) -> ErActionList
{
    arena.append(s, rest)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action7<
>(
    arena: &mut ErActionArena,
    (_, v, _): (usize, Vec<Action>, usize),
) -> ErActionList
{
    arena.actions(v)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action8<
>(
    arena: &mut ErActionArena,
    (_, v, _): (usize, Vec<Action>, usize),
) -> ErActionList
{
    arena.actions(v)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action9<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, Action, usize),
) -> ErActionList
{
    arena.push(a)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action10<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, Action, usize),
) -> ErActionList
{
    arena.push(a)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action11<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, Action, usize),
) -> ErActionList
{
    arena.push(a)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action12<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, Action, usize),
) -> ErActionList
{
    arena.push(a)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action13<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, Action, usize),
) -> ErActionList
{
    arena.push(a)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action14<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, Action, usize),
) -> ErActionList
{
    arena.push(a)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action15<
>(
    arena: &mut ErActionArena,
    (_, _, _): (usize, Tok, usize),
    (_, header, _): (usize, (String, String, String), usize),
    (_, _n, _): (usize, (), usize),
    (_, body, _): (usize, ErActionList, usize),
) -> Action
{
    {
    let (id, title, label_type) = header;
    Action::AddSubgraph { id, title, label_type, body }
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action16<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, Tok, usize),
) -> ErActionList
{
    ErActionList::default()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action17<
>(
    arena: &mut ErActionArena,
    (_, s, _): (usize, ErActionList, usize),
    (_, _n, _): (usize, (), usize),
    (_, rest, _): (usize, ErActionList, usize),
) -> ErActionList
{
    arena.append(s, rest)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action18<
>(
    arena: &mut ErActionArena,
    (_, id, _): (usize, String, usize),
) -> (String, String, String)
{
    (id.clone(), id, "markdown".to_string())
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action19<
>(
    arena: &mut ErActionArena,
    (_, id, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, title, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
) -> (String, String, String)
{
    {
    (id, title, "markdown".to_string())
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action20<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> String
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action21<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> String
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action22<
>(
    arena: &mut ErActionArena,
    (_, part, _): (usize, String, usize),
) -> String
{
    part
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action23<
>(
    arena: &mut ErActionArena,
    (_, left, _): (usize, String, usize),
    (_, part, _): (usize, String, usize),
) -> String
{
    format!("{left} {part}")
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action24<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> String
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action25<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> String
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action26<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> Action
{
    Action::SetAccTitle(__0)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action27<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> Action
{
    Action::SetAccDescr(__0)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action28<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> Action
{
    Action::SetAccDescr(__0)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action29<
>(
    arena: &mut ErActionArena,
    (_, direction, _): (usize, SpannedDirection, usize),
) -> Action
{
    Action::SetDirection(direction.value)
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action30<
>(
    arena: &mut ErActionArena,
    (_, _, _): (usize, Tok, usize),
    (_, ids, _): (usize, Vec<String>, usize),
    (_, raw, _): (usize, String, usize),
) -> Action
{
    Action::AddCssStyles { entities: ids, raw }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action31<
>(
    arena: &mut ErActionArena,
    (_, _, _): (usize, Tok, usize),
    (_, ids, _): (usize, Vec<String>, usize),
    (_, raw, _): (usize, String, usize),
) -> Action
{
    Action::AddClassDef { classes: ids, raw }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action32<
>(
    arena: &mut ErActionArena,
    (_, _, _): (usize, Tok, usize),
    (_, entities, _): (usize, Vec<String>, usize),
    (_, classes, _): (usize, Vec<String>, usize),
) -> Action
{
    Action::SetClass { entities, classes }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action33<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, String, usize),
    (_, spec, _): (usize, RelSpec, usize),
    (_, b, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, role, _): (usize, String, usize),
) -> Vec<Action>
{
    vec![
    Action::AddRelationship { a, role, b, spec },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action34<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, classes_a, _): (usize, Vec<String>, usize),
    (_, spec, _): (usize, RelSpec, usize),
    (_, b, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, classes_b, _): (usize, Vec<String>, usize),
    (_, _, _): (usize, Tok, usize),
    (_, role, _): (usize, String, usize),
) -> Vec<Action>
{
    vec![
    Action::AddRelationship { a: a.clone(), role, b: b.clone(), spec },
    Action::SetClass { entities: vec![a], classes: classes_a },
    Action::SetClass { entities: vec![b], classes: classes_b },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action35<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, classes_a, _): (usize, Vec<String>, usize),
    (_, spec, _): (usize, RelSpec, usize),
    (_, b, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, role, _): (usize, String, usize),
) -> Vec<Action>
{
    vec![
    Action::AddRelationship { a: a.clone(), role, b: b.clone(), spec },
    Action::SetClass { entities: vec![a], classes: classes_a },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action36<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, String, usize),
    (_, spec, _): (usize, RelSpec, usize),
    (_, b, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, classes_b, _): (usize, Vec<String>, usize),
    (_, _, _): (usize, Tok, usize),
    (_, role, _): (usize, String, usize),
) -> Vec<Action>
{
    vec![
    Action::AddRelationship { a: a.clone(), role, b: b.clone(), spec },
    Action::SetClass { entities: vec![b], classes: classes_b },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action37<
>(
    arena: &mut ErActionArena,
    (_, name, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, attrs, _): (usize, Vec<Attribute>, usize),
    (_, _, _): (usize, Tok, usize),
) -> Vec<Action>
{
    vec![
    Action::AddEntity { name: name.clone(), alias: None },
    Action::AddAttributes { entity: name, attributes: attrs },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action38<
>(
    arena: &mut ErActionArena,
    (_, name, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, classes, _): (usize, Vec<String>, usize),
    (_, _, _): (usize, Tok, usize),
    (_, attrs, _): (usize, Vec<Attribute>, usize),
    (_, _, _): (usize, Tok, usize),
) -> Vec<Action>
{
    vec![
    Action::AddEntity { name: name.clone(), alias: None },
    Action::AddAttributes { entity: name.clone(), attributes: attrs },
    Action::SetClass { entities: vec![name], classes },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action39<
>(
    arena: &mut ErActionArena,
    (_, name, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, classes, _): (usize, Vec<String>, usize),
) -> Vec<Action>
{
    vec![
    Action::AddEntity { name: name.clone(), alias: None },
    Action::SetClass { entities: vec![name], classes },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action40<
>(
    arena: &mut ErActionArena,
    (_, name, _): (usize, String, usize),
) -> Vec<Action>
{
    vec![
    Action::AddEntity { name, alias: None },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action41<
>(
    arena: &mut ErActionArena,
    (_, name, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, alias, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, _, _): (usize, Tok, usize),
    (_, attrs, _): (usize, Vec<Attribute>, usize),
    (_, _, _): (usize, Tok, usize),
) -> Vec<Action>
{
    vec![
    Action::AddEntity { name: name.clone(), alias: Some(alias) },
    Action::AddAttributes { entity: name, attributes: attrs },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action42<
>(
    arena: &mut ErActionArena,
    (_, name, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, alias, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
) -> Vec<Action>
{
    vec![
    Action::AddEntity { name, alias: Some(alias) },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action43<
>(
    arena: &mut ErActionArena,
    (_, name, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, alias, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, _, _): (usize, Tok, usize),
    (_, classes, _): (usize, Vec<String>, usize),
    (_, _, _): (usize, Tok, usize),
    (_, attrs, _): (usize, Vec<Attribute>, usize),
    (_, _, _): (usize, Tok, usize),
) -> Vec<Action>
{
    vec![
    Action::AddEntity { name: name.clone(), alias: Some(alias) },
    Action::AddAttributes { entity: name.clone(), attributes: attrs },
    Action::SetClass { entities: vec![name], classes },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action44<
>(
    arena: &mut ErActionArena,
    (_, name, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, alias, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
    (_, _, _): (usize, Tok, usize),
    (_, classes, _): (usize, Vec<String>, usize),
) -> Vec<Action>
{
    vec![
    Action::AddEntity { name: name.clone(), alias: Some(alias) },
    Action::SetClass { entities: vec![name], classes },
  ]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action45<
>(
    arena: &mut ErActionArena,
    (_, ids, _): (usize, SpannedIdList, usize),
) -> Vec<String>
{
    ids.into_names()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action46<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> String
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action47<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> String
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action48<
>(
    arena: &mut ErActionArena,
    (_, left, _): (usize, String, usize),
    (_, t, _): (usize, String, usize),
    (_, right, _): (usize, String, usize),
) -> RelSpec
{
    RelSpec { card_a: right, card_b: left, rel_type: t }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action49<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, Tok, usize),
) -> String
{
    "ZERO_OR_ONE".to_string()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action50<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, Tok, usize),
) -> String
{
    "ZERO_OR_MORE".to_string()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action51<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, Tok, usize),
) -> String
{
    "ONE_OR_MORE".to_string()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action52<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, Tok, usize),
) -> String
{
    "ONLY_ONE".to_string()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action53<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, Tok, usize),
) -> String
{
    "MD_PARENT".to_string()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action54<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, Tok, usize),
) -> String
{
    "IDENTIFYING".to_string()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action55<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, Tok, usize),
) -> String
{
    "NON_IDENTIFYING".to_string()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action56<
>(
    arena: &mut ErActionArena,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Vec<Attribute>
{
    Vec::new()
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action57<
>(
    arena: &mut ErActionArena,
    (_, a, _): (usize, Attribute, usize),
    (_, rest, _): (usize, Vec<Attribute>, usize),
) -> Vec<Attribute>
{
    {
    let mut v = vec![a];
    v.extend(rest);
    v
  }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action58<
>(
    arena: &mut ErActionArena,
    (_, ty, _): (usize, String, usize),
    (_, name, _): (usize, String, usize),
) -> Attribute
{
    Attribute { ty, name, keys: Vec::new(), comment: String::new() }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action59<
>(
    arena: &mut ErActionArena,
    (_, ty, _): (usize, String, usize),
    (_, name, _): (usize, String, usize),
    (_, keys, _): (usize, Vec<String>, usize),
) -> Attribute
{
    Attribute { ty, name, keys, comment: String::new() }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action60<
>(
    arena: &mut ErActionArena,
    (_, ty, _): (usize, String, usize),
    (_, name, _): (usize, String, usize),
    (_, c, _): (usize, String, usize),
) -> Attribute
{
    Attribute { ty, name, keys: Vec::new(), comment: c }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action61<
>(
    arena: &mut ErActionArena,
    (_, ty, _): (usize, String, usize),
    (_, name, _): (usize, String, usize),
    (_, keys, _): (usize, Vec<String>, usize),
    (_, c, _): (usize, String, usize),
) -> Attribute
{
    Attribute { ty, name, keys, comment: c }
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action62<
>(
    arena: &mut ErActionArena,
    (_, __0, _): (usize, String, usize),
) -> String
{
    __0
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action63<
>(
    arena: &mut ErActionArena,
    (_, w, _): (usize, String, usize),
    (_, _, _): (usize, Tok, usize),
) -> String
{
    format!("{w}?")
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action64<
>(
    arena: &mut ErActionArena,
    (_, k, _): (usize, String, usize),
) -> Vec<String>
{
    vec![k]
}

#[allow(unused_variables)]
#[allow(clippy::too_many_arguments, clippy::needless_lifetimes, clippy::just_underscores_and_digits, clippy::extra_unused_type_parameters)]
fn __action65<
>(
    arena: &mut ErActionArena,
    (_, ks, _): (usize, Vec<String>, usize),
    (_, _, _): (usize, Tok, usize),
    (_, k, _): (usize, String, usize),
) -> Vec<String>
{
    { let mut v = ks; v.push(k); v }
}

#[allow(clippy::type_complexity, dead_code)]
pub trait __ToTriple<>
{
    fn to_triple(self) -> Result<(usize,Tok,usize), __lalrpop_util::ParseError<usize, Tok, crate::diagrams::er::LexError>>;
}

impl<> __ToTriple<> for (usize, Tok, usize)
{
    fn to_triple(self) -> Result<(usize,Tok,usize), __lalrpop_util::ParseError<usize, Tok, crate::diagrams::er::LexError>> {
        Ok(self)
    }
}
impl<> __ToTriple<> for Result<(usize, Tok, usize), crate::diagrams::er::LexError>
{
    fn to_triple(self) -> Result<(usize,Tok,usize), __lalrpop_util::ParseError<usize, Tok, crate::diagrams::er::LexError>> {
        self.map_err(|error| __lalrpop_util::ParseError::User { error })
    }
}
