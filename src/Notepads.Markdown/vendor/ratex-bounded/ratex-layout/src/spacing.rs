/// TeX math atom classes, determining inter-element spacing.
///
/// From TeXbook pp. 170-171. Also matches KaTeX's DomEnum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MathClass {
    Ord,   // ordinary
    Op,    // large operator
    Bin,   // binary operation
    Rel,   // relation
    Open,  // opening delimiter
    Close, // closing delimiter
    Punct, // punctuation
    Inner, // inner (fractions, etc.)
}

/// Spacing between adjacent math atoms, in mu (1mu = 1/18 em).
///
/// From TeXbook p. 170, Table 18.
/// 0 = no space, 3 = thin space, 4 = medium space, 5 = thick space.
/// Negative values should never appear here.
///
/// Rows = left atom class, Columns = right atom class.
/// Order: Ord, Op, Bin, Rel, Open, Close, Punct, Inner
#[rustfmt::skip]
const SPACING_TABLE: [[i8; 8]; 8] = [
//       Ord  Op  Bin  Rel Open Close Punct Inner
/*Ord*/  [ 0,  3,  4,   5,  0,   0,   0,    3],
/*Op*/   [ 3,  4,  0,   5,  0,   0,   0,    3],
/*Bin*/  [ 4,  4,  0,   0,  4,   0,   0,    4],
/*Rel*/  [ 5,  5,  0,   0,  5,   0,   0,    5],
/*Open*/ [ 0,  0,  0,   0,  0,   0,   0,    0],
/*Close*/[ 0,  3,  4,   5,  0,   0,   0,    3],
/*Punct*/[ 3,  3,  0,   5,  3,   3,   3,    3],
/*Inner*/[ 3,  3,  4,   5,  3,   0,   3,    3],
];

/// Same table but for tight (script/scriptscript) styles.
/// In tight mode, only thin spaces (3mu) between Op-Ord and Op-Op are kept.
#[rustfmt::skip]
const TIGHT_SPACING_TABLE: [[i8; 8]; 8] = [
//       Ord  Op  Bin  Rel Open Close Punct Inner
/*Ord*/  [ 0,  3,  0,   0,  0,   0,   0,    0],
/*Op*/   [ 3,  3,  0,   0,  0,   0,   0,    0],
/*Bin*/  [ 0,  0,  0,   0,  0,   0,   0,    0],
/*Rel*/  [ 0,  0,  0,   0,  0,   0,   0,    0],
/*Open*/ [ 0,  0,  0,   0,  0,   0,   0,    0],
/*Close*/[ 0,  3,  0,   0,  0,   0,   0,    0],
/*Punct*/[ 0,  0,  0,   0,  0,   0,   0,    0],
/*Inner*/[ 0,  3,  0,   0,  0,   0,   0,    0],
];

impl MathClass {
    fn index(self) -> usize {
        match self {
            Self::Ord => 0,
            Self::Op => 1,
            Self::Bin => 2,
            Self::Rel => 3,
            Self::Open => 4,
            Self::Close => 5,
            Self::Punct => 6,
            Self::Inner => 7,
        }
    }
}

/// Get spacing (in mu) between two adjacent math atoms.
///
/// `tight` should be true for script and scriptscript styles.
/// Returns the spacing in mu units (1mu = 1/18 em).
pub fn atom_spacing(left: MathClass, right: MathClass, tight: bool) -> f64 {
    let table = if tight {
        &TIGHT_SPACING_TABLE
    } else {
        &SPACING_TABLE
    };
    table[left.index()][right.index()] as f64
}

/// Convert mu to em. 1mu = 1/18 em (at the current style's quad width).
pub fn mu_to_em(mu: f64, quad: f64) -> f64 {
    mu * quad / 18.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ord_bin_spacing() {
        assert_eq!(atom_spacing(MathClass::Ord, MathClass::Bin, false), 4.0);
    }

    #[test]
    fn test_ord_rel_spacing() {
        assert_eq!(atom_spacing(MathClass::Ord, MathClass::Rel, false), 5.0);
    }

    #[test]
    fn test_ord_ord_no_spacing() {
        assert_eq!(atom_spacing(MathClass::Ord, MathClass::Ord, false), 0.0);
    }

    #[test]
    fn test_tight_eliminates_most_spacing() {
        assert_eq!(atom_spacing(MathClass::Ord, MathClass::Bin, true), 0.0);
        assert_eq!(atom_spacing(MathClass::Ord, MathClass::Rel, true), 0.0);
    }

    #[test]
    fn test_tight_keeps_op_spacing() {
        assert_eq!(atom_spacing(MathClass::Ord, MathClass::Op, true), 3.0);
        assert_eq!(atom_spacing(MathClass::Op, MathClass::Ord, true), 3.0);
    }

    #[test]
    fn test_mu_to_em() {
        let quad = 1.0;
        assert!((mu_to_em(3.0, quad) - 3.0 / 18.0).abs() < 1e-10);
        assert!((mu_to_em(4.0, quad) - 4.0 / 18.0).abs() < 1e-10);
        assert!((mu_to_em(5.0, quad) - 5.0 / 18.0).abs() < 1e-10);
    }
}
