// Code based on [https://github.com/defuz/sublimate/blob/master/src/core/syntax/scope.rs](https://github.com/defuz/sublimate/blob/master/src/core/syntax/scope.rs)
// released under the MIT license by @defuz
use serde_derive::{Deserialize, Serialize};
use std::{fmt, ops};

/// Foreground and background colors, with font style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Style {
    /// Foreground color
    pub foreground: Color,
    /// Background color
    pub background: Color,
    /// Style of the font
    pub font_style: FontStyle,
}

/// A change to a [`Style`] applied incrementally by a theme rule
///
/// Fields left empty (as `None`) will not modify the corresponding field on a `Style`
///
/// [`Style`]: struct.Style.html
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleModifier {
    /// Foreground color
    pub foreground: Option<Color>,
    /// Background color
    pub background: Option<Color>,
    /// Style of the font
    pub font_style: Option<FontStyle>,
}

/// RGBA color, directly from the theme
///
/// Because these numbers come directly from the theme, you might have to do your own color space
/// conversion if you're outputting a different color space from the theme. This can be a problem
/// because some Sublime themes use sRGB and some don't. This is specified in an attribute syntect
/// doesn't parse yet.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    /// Red component
    pub r: u8,
    /// Green component
    pub g: u8,
    /// Blue component
    pub b: u8,
    /// Alpha (transparency) component
    pub a: u8,
}

// More compact alternate debug representation by not using a separate line for each color field,
// also adapts the default debug representation to match.
impl std::fmt::Debug for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Color { r, g, b, a } = self;
        if f.alternate() {
            // when formatted with "{:#?}"
            write!(
                f,
                "Color {{ r/g/b/a: {: >3}/{: >3}/{: >3}/{: >3} }}",
                r, g, b, a
            )
        } else {
            // when formatted with "{:?}"
            write!(f, "Color {{ r/g/b/a: {}/{}/{}/{} }}", r, g, b, a)
        }
    }
}

/// The color-independent styling of a font - i.e. bold, italicized, and/or underlined
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FontStyle {
    bits: u8,
}

impl FontStyle {
    /// Bold font style
    pub const BOLD: Self = Self { bits: 1 };
    /// Underline font style
    pub const UNDERLINE: Self = Self { bits: 2 };
    /// Italic font style
    pub const ITALIC: Self = Self { bits: 4 };

    /// Returns an empty set of flags.
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Returns the set containing all flags.
    pub const fn all() -> Self {
        let bits = Self::BOLD.bits | Self::UNDERLINE.bits | Self::ITALIC.bits;
        Self { bits }
    }

    /// Returns the raw value of the flags currently stored.
    pub const fn bits(&self) -> u8 {
        self.bits
    }

    /// Convert from underlying bit representation, unless that
    /// representation contains bits that do not correspond to a flag.
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if (bits & !Self::all().bits()) == 0 {
            Some(Self { bits })
        } else {
            None
        }
    }

    /// Convert from underlying bit representation, dropping any bits
    /// that do not correspond to flags.
    pub const fn from_bits_truncate(bits: u8) -> Self {
        let bits = bits & Self::all().bits;
        Self { bits }
    }

    /// Convert from underlying bit representation, preserving all
    /// bits (even those not corresponding to a defined flag).
    ///
    /// # Safety
    ///
    /// The caller of the `bitflags!` macro can chose to allow or
    /// disallow extra bits for their bitflags type.
    ///
    /// The caller of `from_bits_unchecked()` has to ensure that
    /// all bits correspond to a defined flag or that extra bits
    /// are valid for this bitflags type.
    pub const unsafe fn from_bits_unchecked(bits: u8) -> Self {
        Self { bits }
    }

    /// Returns `true` if no flags are currently stored.
    pub const fn is_empty(&self) -> bool {
        self.bits() == Self::empty().bits()
    }

    /// Returns `true` if all flags are currently set.
    pub const fn is_all(&self) -> bool {
        self.bits() == Self::all().bits()
    }

    /// Returns `true` if there are flags common to both `self` and `other`.
    pub const fn intersects(&self, other: Self) -> bool {
        let bits = self.bits & other.bits;
        !(Self { bits }).is_empty()
    }

    /// Returns `true` if all of the flags in `other` are contained within `self`.
    pub const fn contains(&self, other: Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    /// Inserts the specified flags in-place.
    pub fn insert(&mut self, other: Self) {
        self.bits |= other.bits;
    }

    /// Removes the specified flags in-place.
    pub fn remove(&mut self, other: Self) {
        self.bits &= !other.bits;
    }

    /// Toggles the specified flags in-place.
    pub fn toggle(&mut self, other: Self) {
        self.bits ^= other.bits;
    }

    /// Inserts or removes the specified flags depending on the passed value.
    pub fn set(&mut self, other: Self, value: bool) {
        if value {
            self.insert(other);
        } else {
            self.remove(other);
        }
    }

    /// Returns the intersection between the flags in `self` and
    /// `other`.
    ///
    /// Specifically, the returned set contains only the flags which are
    /// present in *both* `self` *and* `other`.
    ///
    /// This is equivalent to using the `&` operator (e.g.
    /// [`ops::BitAnd`]), as in `flags & other`.
    ///
    /// [`ops::BitAnd`]: https://doc.rust-lang.org/std/ops/trait.BitAnd.html
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        let bits = self.bits & other.bits;
        Self { bits }
    }

    /// Returns the union of between the flags in `self` and `other`.
    ///
    /// Specifically, the returned set contains all flags which are
    /// present in *either* `self` *or* `other`, including any which are
    /// present in both (see [`Self::symmetric_difference`] if that
    /// is undesirable).
    ///
    /// This is equivalent to using the `|` operator (e.g.
    /// [`ops::BitOr`]), as in `flags | other`.
    ///
    /// [`ops::BitOr`]: https://doc.rust-lang.org/std/ops/trait.BitOr.html
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        let bits = self.bits | other.bits;
        Self { bits }
    }

    /// Returns the difference between the flags in `self` and `other`.
    ///
    /// Specifically, the returned set contains all flags present in
    /// `self`, except for the ones present in `other`.
    ///
    /// It is also conceptually equivalent to the "bit-clear" operation:
    /// `flags & !other` (and this syntax is also supported).
    ///
    /// This is equivalent to using the `-` operator (e.g.
    /// [`ops::Sub`]), as in `flags - other`.
    ///
    /// [`ops::Sub`]: https://doc.rust-lang.org/std/ops/trait.Sub.html
    pub const fn difference(self, other: Self) -> Self {
        let bits = self.bits & !other.bits;
        Self { bits }
    }

    /// Returns the [symmetric difference][sym-diff] between the flags
    /// in `self` and `other`.
    ///
    /// Specifically, the returned set contains the flags present which
    /// are present in `self` or `other`, but that are not present in
    /// both. Equivalently, it contains the flags present in *exactly
    /// one* of the sets `self` and `other`.
    ///
    /// This is equivalent to using the `^` operator (e.g.
    /// [`ops::BitXor`]), as in `flags ^ other`.
    ///
    /// [sym-diff]: https://en.wikipedia.org/wiki/Symmetric_difference
    /// [`ops::BitXor`]: https://doc.rust-lang.org/std/ops/trait.BitXor.html
    #[must_use]
    pub const fn symmetric_difference(self, other: Self) -> Self {
        let bits = self.bits ^ other.bits;
        Self { bits }
    }

    /// Returns the complement of this set of flags.
    ///
    /// Specifically, the returned set contains all the flags which are
    /// not set in `self`, but which are allowed for this type.
    ///
    /// Alternatively, it can be thought of as the set difference
    /// between [`Self::all()`] and `self` (e.g. `Self::all() - self`)
    ///
    /// This is equivalent to using the `!` operator (e.g.
    /// [`ops::Not`]), as in `!flags`.
    ///
    /// [`Self::all()`]: Self::all
    /// [`ops::Not`]: https://doc.rust-lang.org/std/ops/trait.Not.html
    #[must_use]
    pub const fn complement(self) -> Self {
        Self::from_bits_truncate(!self.bits)
    }
}

impl fmt::Debug for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut empty = true;

        let pairs = [
            (Self::BOLD, "BOLD"),
            (Self::UNDERLINE, "UNDERLINE"),
            (Self::ITALIC, "ITALIC"),
        ];
        for (flag, flag_str) in pairs {
            if self.contains(flag) {
                if !std::mem::take(&mut empty) {
                    f.write_str(" | ")?;
                }
                f.write_str(flag_str)?;
            }
        }

        let extra_bits = self.bits & !Self::all().bits();
        if extra_bits != 0 {
            if !std::mem::take(&mut empty) {
                f.write_str(" | ")?;
            }
            f.write_str("0x")?;
            fmt::LowerHex::fmt(&extra_bits, f)?;
        }

        if empty {
            f.write_str("(empty)")?;
        }

        Ok(())
    }
}

impl fmt::Binary for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Binary::fmt(&self.bits, f)
    }
}

impl fmt::Octal for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Octal::fmt(&self.bits, f)
    }
}

impl fmt::LowerHex for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(&self.bits, f)
    }
}

impl fmt::UpperHex for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::UpperHex::fmt(&self.bits, f)
    }
}

impl ops::BitOr for FontStyle {
    type Output = Self;
    /// Returns the union of the two sets of flags.
    fn bitor(self, other: FontStyle) -> Self {
        let bits = self.bits | other.bits;
        Self { bits }
    }
}

impl ops::BitOrAssign for FontStyle {
    /// Adds the set of flags.
    fn bitor_assign(&mut self, other: Self) {
        self.bits |= other.bits;
    }
}

impl ops::BitXor for FontStyle {
    type Output = Self;
    /// Returns the left flags, but with all the right flags toggled.
    fn bitxor(self, other: Self) -> Self {
        let bits = self.bits ^ other.bits;
        Self { bits }
    }
}

impl ops::BitXorAssign for FontStyle {
    /// Toggles the set of flags.
    fn bitxor_assign(&mut self, other: Self) {
        self.bits ^= other.bits;
    }
}

impl ops::BitAnd for FontStyle {
    type Output = Self;
    /// Returns the intersection between the two sets of flags.
    fn bitand(self, other: Self) -> Self {
        let bits = self.bits & other.bits;
        Self { bits }
    }
}

impl ops::BitAndAssign for FontStyle {
    /// Disables all flags disabled in the set.
    fn bitand_assign(&mut self, other: Self) {
        self.bits &= other.bits;
    }
}

impl ops::Sub for FontStyle {
    type Output = Self;
    /// Returns the set difference of the two sets of flags.
    fn sub(self, other: Self) -> Self {
        let bits = self.bits & !other.bits;
        Self { bits }
    }
}

impl ops::SubAssign for FontStyle {
    /// Disables all flags enabled in the set.
    fn sub_assign(&mut self, other: Self) {
        self.bits &= !other.bits;
    }
}

impl ops::Not for FontStyle {
    type Output = Self;
    /// Returns the complement of this set of flags.
    fn not(self) -> Self {
        Self { bits: !self.bits } & Self::all()
    }
}

impl Extend<FontStyle> for FontStyle {
    fn extend<T: IntoIterator<Item = Self>>(&mut self, iterator: T) {
        for item in iterator {
            self.insert(item)
        }
    }
}

impl FromIterator<FontStyle> for FontStyle {
    fn from_iter<T: IntoIterator<Item = Self>>(iterator: T) -> Self {
        let mut result = Self::empty();
        result.extend(iterator);
        result
    }
}

impl Color {
    /// The color black (`#000000`)
    pub const BLACK: Color = Color {
        r: 0x00,
        g: 0x00,
        b: 0x00,
        a: 0xFF,
    };

    /// The color white (`#FFFFFF`)
    pub const WHITE: Color = Color {
        r: 0xFF,
        g: 0xFF,
        b: 0xFF,
        a: 0xFF,
    };
}

impl Style {
    /// Applies a change to this style, yielding a new changed style
    pub fn apply(&self, modifier: StyleModifier) -> Style {
        Style {
            foreground: modifier.foreground.unwrap_or(self.foreground),
            background: modifier.background.unwrap_or(self.background),
            font_style: modifier.font_style.unwrap_or(self.font_style),
        }
    }
}

impl Default for Style {
    fn default() -> Style {
        Style {
            foreground: Color::BLACK,
            background: Color::WHITE,
            font_style: FontStyle::empty(),
        }
    }
}

impl StyleModifier {
    /// Applies the other modifier to this one, creating a new modifier.
    ///
    /// Values in `other` are preferred.
    pub fn apply(&self, other: StyleModifier) -> StyleModifier {
        StyleModifier {
            foreground: other.foreground.or(self.foreground),
            background: other.background.or(self.background),
            font_style: other.font_style.or(self.font_style),
        }
    }
}
