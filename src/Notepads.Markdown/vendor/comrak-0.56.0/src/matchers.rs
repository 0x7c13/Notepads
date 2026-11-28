use std::sync::OnceLock;

use memchr_n::MemchrN;

pub fn cr_or_lf_matcher() -> &'static MemchrN {
    static MATCHER: OnceLock<MemchrN> = OnceLock::new();
    MATCHER.get_or_init(|| MemchrN::new(b"\r\n"))
}

pub fn backtick_matcher() -> &'static MemchrN {
    static MATCHER: OnceLock<MemchrN> = OnceLock::new();
    MATCHER.get_or_init(|| MemchrN::new(b"`"))
}

pub fn dollar_matcher() -> &'static MemchrN {
    static MATCHER: OnceLock<MemchrN> = OnceLock::new();
    MATCHER.get_or_init(|| MemchrN::new(b"$"))
}

pub fn backslash_matcher() -> &'static MemchrN {
    static MATCHER: OnceLock<MemchrN> = OnceLock::new();
    MATCHER.get_or_init(|| MemchrN::new(b"\\"))
}
