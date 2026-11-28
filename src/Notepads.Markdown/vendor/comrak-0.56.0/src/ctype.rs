#[rustfmt::skip]
const CMARK_CTYPE_CLASS: [u8; 256] = [
    /*      0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f */
    /* 0 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0,
    /* 1 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    /* 2 */ 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    /* 3 */ 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 2, 2, 2, 2, 2, 2,
    /* 4 */ 2, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    /* 5 */ 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 2, 2, 2, 2, 2,
    /* 6 */ 2, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    /* 7 */ 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 2, 2, 2, 2, 0,
    /* 8 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    /* 9 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    /* a */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    /* b */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    /* c */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    /* d */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    /* e */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    /* f */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub fn isspace(ch: u8) -> bool {
    CMARK_CTYPE_CLASS[ch as usize] == 1
}

pub fn isspace_char(ch: char) -> bool {
    (ch as u64) < 256 && CMARK_CTYPE_CLASS[ch as usize] == 1
}

pub fn ispunct(ch: u8) -> bool {
    CMARK_CTYPE_CLASS[ch as usize] == 2
}

pub fn ispunct_char(ch: char) -> bool {
    (ch as u64) < 256 && CMARK_CTYPE_CLASS[ch as usize] == 2
}

pub fn isdigit(ch: u8) -> bool {
    CMARK_CTYPE_CLASS[ch as usize] == 3
}

pub fn isalpha(ch: u8) -> bool {
    CMARK_CTYPE_CLASS[ch as usize] == 4
}

pub fn isalnum(ch: u8) -> bool {
    CMARK_CTYPE_CLASS[ch as usize] == 3 || CMARK_CTYPE_CLASS[ch as usize] == 4
}
