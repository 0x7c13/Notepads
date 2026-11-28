use finl_unicode::categories::CharacterCategories;

pub(crate) trait FlankingCheckHelper
where
    Self: Sized + Copy,
{
    fn is_cjk_ambiguous_punctuation_candidate(&self) -> bool;
    fn is_ideographic_vs(&self) -> bool;
    fn is_cjk(&self) -> bool;
    #[inline]
    fn is_cjk_or_ideographic_vs(&self) -> bool {
        self.is_cjk() || self.is_ideographic_vs()
    }
    fn is_non_emoji_general_purpose_vs(&self) -> bool;
    fn is_cmark_punctuation(&self) -> bool;
}

impl FlankingCheckHelper for char {
    /// https://github.com/tats-u/markdown-cjk-friendly/blob/main/ranges.md#cjk-characters
    #[inline]
    fn is_cjk(&self) -> bool {
        // Fast path
        if *self < '\u{1100}' {
            return false;
        }
        // Snapshot as of Unicode 18
        matches!(
            u32::from(*self),
            0x1100..=0x11ff
              | 0x20a9
              | 0x2329..=0x232a
              | 0x2630..=0x2637
              | 0x268a..=0x268f
              | 0x2e80..=0x2e99
              | 0x2e9b..=0x2ef3
              | 0x2f00..=0x2fd5
              | 0x2ff0..=0x303e
              | 0x3041..=0x3096
              | 0x3099..=0x30ff
              | 0x3105..=0x312f
              | 0x3131..=0x318e
              | 0x3190..=0x31e5
              | 0x31ef..=0x321e
              | 0x3220..=0x3247
              | 0x3250..=0xa48c
              | 0xa490..=0xa4c6
              | 0xa960..=0xa97c
              | 0xac00..=0xd7a3
              | 0xd7b0..=0xd7c6
              | 0xd7cb..=0xd7fb
              | 0xf900..=0xfaff
              | 0xfe10..=0xfe19
              | 0xfe30..=0xfe52
              | 0xfe54..=0xfe66
              | 0xfe68..=0xfe6b
              | 0xff01..=0xffbe
              | 0xffc2..=0xffc7
              | 0xffca..=0xffcf
              | 0xffd2..=0xffd7
              | 0xffda..=0xffdc
              | 0xffe0..=0xffe6
              | 0xffe8..=0xffee
              | 0x16fe0..=0x16fe4
              | 0x16ff0..=0x16ff6
              | 0x17000..=0x18cda
              | 0x18cff..=0x18d20
              | 0x18d80..=0x18df2
              | 0x18e00..=0x19191
              | 0x191a0..=0x191d2
              | 0x1aff0..=0x1aff3
              | 0x1aff5..=0x1affb
              | 0x1affd..=0x1affe
              | 0x1b000..=0x1b128
              | 0x1b132
              | 0x1b150..=0x1b152
              | 0x1b155
              | 0x1b164..=0x1b168
              | 0x1b170..=0x1b2fb
              | 0x1d300..=0x1d356
              | 0x1d360..=0x1d376
              | 0x1f1ae
              | 0x1f200
              | 0x1f202
              | 0x1f210..=0x1f219
              | 0x1f21b..=0x1f22e
              | 0x1f230..=0x1f231
              | 0x1f237
              | 0x1f23b
              | 0x1f240..=0x1f248
              | 0x1f260..=0x1f265
              | 0x1f7da
              | 0x20000..=0x3fffd
        )
    }

    #[inline]
    fn is_non_emoji_general_purpose_vs(&self) -> bool {
        matches!(u32::from(*self), 0xFE00..=0xFE0E)
    }

    #[inline]
    fn is_ideographic_vs(&self) -> bool {
        matches!(u32::from(*self), 0xE0100..=0xE01EF)
    }
    #[inline]
    fn is_cmark_punctuation(&self) -> bool {
        self.is_punctuation() || self.is_symbol()
    }
    #[inline]
    fn is_cjk_ambiguous_punctuation_candidate(&self) -> bool {
        matches!(u32::from(*self), 0x2018 | 0x2019 | 0x201c | 0x201d)
    }
}
