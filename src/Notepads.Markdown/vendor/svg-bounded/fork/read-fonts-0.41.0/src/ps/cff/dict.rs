// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Parsing for PostScript DICTs.

use super::{
    blend::BlendState,
    stack::{Number, Stack},
};
use crate::control::{self, OrdinaryControl, PsCtl};
use crate::{
    ps::{
        error::Error,
        hinting::{Blues, StemSnaps},
        num::{self, BcdComponents},
        string::Sid,
        transform::ScaledFontMatrix,
    },
    types::Fixed,
    Cursor, ReadError,
};
use markdown_svg_budget::WorkControl;
use std::ops::Range;

/// PostScript DICT operator.
///
/// See "Table 9 Top DICT Operator Entries" and "Table 23 Private DICT
/// Operators" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf>
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Operator {
    Version,
    Notice,
    FullName,
    FamilyName,
    Weight,
    FontBbox,
    CharstringsOffset,
    PrivateDictRange,
    VariationStoreOffset,
    Copyright,
    IsFixedPitch,
    ItalicAngle,
    UnderlinePosition,
    UnderlineThickness,
    PaintType,
    CharstringType,
    FontMatrix,
    StrokeWidth,
    FdArrayOffset,
    FdSelectOffset,
    BlueValues,
    OtherBlues,
    FamilyBlues,
    FamilyOtherBlues,
    SubrsOffset,
    VariationStoreIndex,
    BlueScale,
    BlueShift,
    BlueFuzz,
    LanguageGroup,
    ExpansionFactor,
    Encoding,
    Charset,
    UniqueId,
    Xuid,
    SyntheticBase,
    PostScript,
    BaseFontName,
    BaseFontBlend,
    Ros,
    CidFontVersion,
    CidFontRevision,
    CidFontType,
    CidCount,
    UidBase,
    FontName,
    StdHw,
    StdVw,
    DefaultWidthX,
    NominalWidthX,
    Blend,
    StemSnapH,
    StemSnapV,
    ForceBold,
    InitialRandomSeed,
}

impl Operator {
    fn from_opcode(opcode: u8) -> Option<Self> {
        use Operator::*;
        Some(match opcode {
            // Top DICT operators
            0 => Version,
            1 => Notice,
            2 => FullName,
            3 => FamilyName,
            4 => Weight,
            5 => FontBbox,
            13 => UniqueId,
            14 => Xuid,
            15 => Charset,
            16 => Encoding,
            17 => CharstringsOffset,
            18 => PrivateDictRange,
            24 => VariationStoreOffset,
            // Private DICT operators
            6 => BlueValues,
            7 => OtherBlues,
            8 => FamilyBlues,
            9 => FamilyOtherBlues,
            10 => StdHw,
            11 => StdVw,
            19 => SubrsOffset,
            20 => DefaultWidthX,
            21 => NominalWidthX,
            22 => VariationStoreIndex,
            23 => Blend,
            // Font DICT only uses PrivateDictRange
            _ => return None,
        })
    }

    fn from_extended_opcode(opcode: u8) -> Option<Self> {
        use Operator::*;
        Some(match opcode {
            // Top DICT operators
            0 => Copyright,
            1 => IsFixedPitch,
            2 => ItalicAngle,
            3 => UnderlinePosition,
            4 => UnderlineThickness,
            5 => PaintType,
            6 => CharstringType,
            7 => FontMatrix,
            8 => StrokeWidth,
            20 => SyntheticBase,
            21 => PostScript,
            22 => BaseFontName,
            23 => BaseFontBlend,
            30 => Ros,
            31 => CidFontVersion,
            32 => CidFontRevision,
            33 => CidFontType,
            34 => CidCount,
            35 => UidBase,
            36 => FdArrayOffset,
            37 => FdSelectOffset,
            38 => FontName,
            // Private DICT operators
            9 => BlueScale,
            10 => BlueShift,
            11 => BlueFuzz,
            12 => StemSnapH,
            13 => StemSnapV,
            14 => ForceBold,
            17 => LanguageGroup,
            18 => ExpansionFactor,
            19 => InitialRandomSeed,
            _ => return None,
        })
    }
}

/// Either a PostScript DICT operator or a (numeric) operand.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Token {
    /// An operator parsed from a DICT.
    Operator(Operator),
    /// A number parsed from a DICT. If the source was in
    /// binary coded decimal format, then the second field
    /// contains the parsed components.
    Operand(Number, Option<BcdComponents>),
}

impl From<Operator> for Token {
    fn from(value: Operator) -> Self {
        Self::Operator(value)
    }
}

impl<T> From<T> for Token
where
    T: Into<Number>,
{
    fn from(value: T) -> Self {
        Self::Operand(value.into(), None)
    }
}

/// Given a byte slice containing DICT data, returns an iterator yielding
/// raw operands and operators.
///
/// This does not perform any additional processing such as type conversion,
/// delta decoding or blending.
pub fn tokens(dict_data: &[u8]) -> impl Iterator<Item = Result<Token, Error>> + '_ + Clone {
    let mut cursor = Tokens {
        cursor: crate::FontData::new(dict_data).cursor(),
    };
    std::iter::from_fn(move || control::ordinary(cursor.try_next(&mut OrdinaryControl::new())))
}

fn try_parse_token<C: WorkControl>(cursor: &mut Cursor, control: &mut C) -> PsCtl<Token, C> {
    // Escape opcode for accessing extensions.
    const ESCAPE: u8 = 12;
    control.checkpoint(1)?;
    let b0 = control::font_try!(cursor.read::<u8>());
    let result = if b0 == ESCAPE {
        let b1 = control::font_try!(cursor.read::<u8>());
        Token::Operator(control::font_try!(
            Operator::from_extended_opcode(b1).ok_or(Error::InvalidDictOperator(b1))
        ))
    } else {
        // See <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#table-3-operand-encoding>
        match b0 {
            28 | 29 | 32..=254 => {
                Token::Operand(control::font_try!(num::parse_int(cursor, b0)).into(), None)
            }
            30 => {
                let components = control::font_try!(BcdComponents::try_parse(cursor, control)?);
                Token::Operand(components.value(false).into(), Some(components))
            }
            _ => Token::Operator(control::font_try!(
                Operator::from_opcode(b0).ok_or(Error::InvalidDictOperator(b0))
            )),
        }
    };
    control.checkpoint(0)?;
    Ok(Ok(result))
}

/// PostScript DICT Operator with its associated operands.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Entry {
    Version(Sid),
    Notice(Sid),
    FullName(Sid),
    FamilyName(Sid),
    Weight(Sid),
    FontBbox([Fixed; 4]),
    CharstringsOffset(usize),
    PrivateDictRange(Range<usize>),
    VariationStoreOffset(usize),
    Copyright(Sid),
    IsFixedPitch(bool),
    ItalicAngle(Fixed),
    UnderlinePosition(Fixed),
    UnderlineThickness(Fixed),
    PaintType(i32),
    CharstringType(i32),
    FontMatrix(ScaledFontMatrix),
    StrokeWidth(Fixed),
    FdArrayOffset(usize),
    FdSelectOffset(usize),
    BlueValues(Blues),
    OtherBlues(Blues),
    FamilyBlues(Blues),
    FamilyOtherBlues(Blues),
    SubrsOffset(usize),
    VariationStoreIndex(u16),
    BlueScale(Fixed),
    BlueShift(Fixed),
    BlueFuzz(Fixed),
    LanguageGroup(i32),
    ExpansionFactor(Fixed),
    Encoding(usize),
    Charset(usize),
    UniqueId(i32),
    Xuid,
    SyntheticBase(i32),
    PostScript(Sid),
    BaseFontName(Sid),
    BaseFontBlend,
    Ros {
        registry: Sid,
        ordering: Sid,
        supplement: Fixed,
    },
    CidFontVersion(Fixed),
    CidFontRevision(Fixed),
    CidFontType(i32),
    CidCount(u32),
    UidBase(i32),
    FontName(Sid),
    StdHw(Fixed),
    StdVw(Fixed),
    DefaultWidthX(Fixed),
    NominalWidthX(Fixed),
    StemSnapH(StemSnaps),
    StemSnapV(StemSnaps),
    ForceBold(bool),
    InitialRandomSeed(i32),
}

/// Given a byte slice containing DICT data, returns an iterator yielding
/// each operator with its associated operands.
///
/// This performs appropriate type conversions, decodes deltas and applies
/// blending.
///
/// If processing a Private DICT from a CFF2 table and an item variation
/// store is present, then `blend_state` must be provided.
pub fn entries<'a>(
    dict_data: &'a [u8],
    blend_state: Option<BlendState<'a>>,
) -> impl Iterator<Item = Result<Entry, Error>> + 'a {
    let mut entries = try_entries(dict_data, blend_state);
    std::iter::from_fn(move || control::ordinary(entries.try_next(&mut OrdinaryControl::new())))
}
/// DICT execution keeps font recovery separate from the outside control fault.
pub struct Entries<'a> {
    dict_data: &'a [u8],
    blend_state: Option<BlendState<'a>>,
    stack: Stack,
    last_bcd_components: Option<BcdComponents>,
    cursor: Cursor<'a>,
    cursor_pos: usize,
}
pub fn try_entries<'a>(dict_data: &'a [u8], blend_state: Option<BlendState<'a>>) -> Entries<'a> {
    Entries {
        dict_data,
        blend_state,
        stack: Stack::new(),
        last_bcd_components: None,
        cursor: crate::FontData::new(dict_data).cursor(),
        cursor_pos: 0,
    }
}
impl Entries<'_> {
    pub fn try_next<C: WorkControl>(
        &mut self,
        cx: &mut C,
    ) -> Result<Option<Result<Entry, Error>>, C::Error> {
        cx.checkpoint(0)?;
        let result = self.next_impl(cx)?;
        cx.checkpoint(0)?;
        Ok(result)
    }
    fn next_impl<C: WorkControl>(
        &mut self,
        cx: &mut C,
    ) -> Result<Option<Result<Entry, Error>>, C::Error> {
        loop {
            cx.checkpoint(1)?;
            if self.cursor.remaining_bytes() == 0 {
                return Ok(None);
            }
            let token = match try_parse_token(&mut self.cursor, cx)? {
                Ok(token) => token,
                Err(Error::InvalidDictOperator(_)) => {
                    self.stack.clear();
                    continue;
                }
                Err(error) => return Ok(Some(Err(error))),
            };
            match token {
                Token::Operand(number, bcd_components) => {
                    self.last_bcd_components = bcd_components;
                    match self.stack.push(number) {
                        Ok(_) => continue,
                        Err(e) => return Ok(Some(Err(e))),
                    }
                }
                Token::Operator(op) => {
                    if op == Operator::Blend || op == Operator::VariationStoreIndex {
                        let state = match self.blend_state.as_mut() {
                            Some(s) => s,
                            None => return Ok(Some(Err(Error::MissingBlendState))),
                        };
                        if op == Operator::VariationStoreIndex {
                            let ix = match self.stack.get_i32(0) {
                                Ok(ix) => ix,
                                Err(e) => return Ok(Some(Err(e))),
                            };
                            if let Err(e) = state.try_set_store_index(ix as u16, cx)? {
                                return Ok(Some(Err(e)));
                            }
                        }
                        if op == Operator::Blend {
                            match self.stack.try_apply_blend(state, cx)? {
                                Ok(_) => continue,
                                Err(e) => return Ok(Some(Err(e))),
                            }
                        }
                    }
                    if op == Operator::BlueScale {
                        if let Some(bcd) = self.last_bcd_components.take() {
                            if self.stack.pop_fixed().is_err() {
                                return Ok(None);
                            }
                            if self.stack.push(bcd.value(true)).is_err() {
                                return Ok(None);
                            }
                        }
                    }
                    if op == Operator::FontMatrix {
                        self.stack.clear();
                        self.last_bcd_components = None;
                        // The original dynamic-scaling second pass uses the same control.
                        let mut cursor = crate::FontData::new(self.dict_data).cursor();
                        cursor.advance_by(self.cursor_pos);
                        if let Some(matrix) = ScaledFontMatrix::try_parse(&mut cursor, cx)? {
                            return Ok(Some(Ok(Entry::FontMatrix(matrix))));
                        }
                        continue;
                    }
                    self.last_bcd_components = None;
                    let entry = parse_entry(op, &mut self.stack, cx)?;
                    self.stack.clear();
                    self.cursor_pos = self.cursor.position().unwrap_or_default();
                    return Ok(Some(entry));
                }
            }
        }
    }
}

fn parse_entry<C: WorkControl>(op: Operator, stack: &mut Stack, cx: &mut C) -> PsCtl<Entry, C> {
    cx.checkpoint(0)?;
    use Operator::*;
    let entry = match op {
        Version => Entry::Version(control::font_try!(stack.pop_i32()).into()),
        Notice => Entry::Notice(control::font_try!(stack.pop_i32()).into()),
        FullName => Entry::FullName(control::font_try!(stack.pop_i32()).into()),
        FamilyName => Entry::FamilyName(control::font_try!(stack.pop_i32()).into()),
        Weight => Entry::Weight(control::font_try!(stack.pop_i32()).into()),
        FontBbox => Entry::FontBbox([
            control::font_try!(stack.get_fixed(0)),
            control::font_try!(stack.get_fixed(1)),
            control::font_try!(stack.get_fixed(2)),
            control::font_try!(stack.get_fixed(3)),
        ]),
        CharstringsOffset => Entry::CharstringsOffset(control::font_try!(stack.pop_i32()) as usize),
        PrivateDictRange => {
            let len = control::font_try!(stack.get_i32(0)) as usize;
            let start = control::font_try!(stack.get_i32(1)) as usize;
            let end = control::font_try!(start.checked_add(len).ok_or(ReadError::OutOfBounds));
            Entry::PrivateDictRange(start..end)
        }
        VariationStoreOffset => {
            Entry::VariationStoreOffset(control::font_try!(stack.pop_i32()) as usize)
        }
        Copyright => Entry::Copyright(control::font_try!(stack.pop_i32()).into()),
        IsFixedPitch => Entry::IsFixedPitch(control::font_try!(stack.pop_i32()) != 0),
        ItalicAngle => Entry::ItalicAngle(control::font_try!(stack.pop_fixed())),
        UnderlinePosition => Entry::UnderlinePosition(control::font_try!(stack.pop_fixed())),
        UnderlineThickness => Entry::UnderlineThickness(control::font_try!(stack.pop_fixed())),
        PaintType => Entry::PaintType(control::font_try!(stack.pop_i32())),
        CharstringType => Entry::CharstringType(control::font_try!(stack.pop_i32())),
        FontMatrix => unreachable!(),
        StrokeWidth => Entry::StrokeWidth(control::font_try!(stack.pop_fixed())),
        FdArrayOffset => Entry::FdArrayOffset(control::font_try!(stack.pop_i32()) as usize),
        FdSelectOffset => Entry::FdSelectOffset(control::font_try!(stack.pop_i32()) as usize),
        BlueValues => {
            stack.try_apply_delta_prefix_sum(cx)?;
            Entry::BlueValues(Blues::new(stack.fixed_values()))
        }
        OtherBlues => {
            stack.try_apply_delta_prefix_sum(cx)?;
            Entry::OtherBlues(Blues::new(stack.fixed_values()))
        }
        FamilyBlues => {
            stack.try_apply_delta_prefix_sum(cx)?;
            Entry::FamilyBlues(Blues::new(stack.fixed_values()))
        }
        FamilyOtherBlues => {
            stack.try_apply_delta_prefix_sum(cx)?;
            Entry::FamilyOtherBlues(Blues::new(stack.fixed_values()))
        }
        SubrsOffset => Entry::SubrsOffset(control::font_try!(stack.pop_i32()) as usize),
        VariationStoreIndex => {
            Entry::VariationStoreIndex(control::font_try!(stack.pop_i32()) as u16)
        }
        BlueScale => Entry::BlueScale(control::font_try!(stack.pop_fixed())),
        BlueShift => Entry::BlueShift(control::font_try!(stack.pop_fixed())),
        BlueFuzz => Entry::BlueFuzz(control::font_try!(stack.pop_fixed())),
        LanguageGroup => Entry::LanguageGroup(control::font_try!(stack.pop_i32())),
        ExpansionFactor => Entry::ExpansionFactor(control::font_try!(stack.pop_fixed())),
        Encoding => Entry::Encoding(control::font_try!(stack.pop_i32()) as usize),
        Charset => Entry::Charset(control::font_try!(stack.pop_i32()) as usize),
        UniqueId => Entry::UniqueId(control::font_try!(stack.pop_i32())),
        Xuid => Entry::Xuid,
        SyntheticBase => Entry::SyntheticBase(control::font_try!(stack.pop_i32())),
        PostScript => Entry::PostScript(control::font_try!(stack.pop_i32()).into()),
        BaseFontName => Entry::BaseFontName(control::font_try!(stack.pop_i32()).into()),
        BaseFontBlend => Entry::BaseFontBlend,
        Ros => Entry::Ros {
            registry: control::font_try!(stack.get_i32(0)).into(),
            ordering: control::font_try!(stack.get_i32(1)).into(),
            supplement: control::font_try!(stack.get_fixed(2)),
        },
        CidFontVersion => Entry::CidFontVersion(control::font_try!(stack.pop_fixed())),
        CidFontRevision => Entry::CidFontRevision(control::font_try!(stack.pop_fixed())),
        CidFontType => Entry::CidFontType(control::font_try!(stack.pop_i32())),
        CidCount => Entry::CidCount(control::font_try!(stack.pop_i32()) as u32),
        UidBase => Entry::UidBase(control::font_try!(stack.pop_i32())),
        FontName => Entry::FontName(control::font_try!(stack.pop_i32()).into()),
        StdHw => Entry::StdHw(control::font_try!(stack.pop_fixed())),
        StdVw => Entry::StdVw(control::font_try!(stack.pop_fixed())),
        DefaultWidthX => Entry::DefaultWidthX(control::font_try!(stack.pop_fixed())),
        NominalWidthX => Entry::NominalWidthX(control::font_try!(stack.pop_fixed())),
        StemSnapH => {
            stack.try_apply_delta_prefix_sum(cx)?;
            Entry::StemSnapH(StemSnaps::new(stack.fixed_values()))
        }
        StemSnapV => {
            stack.try_apply_delta_prefix_sum(cx)?;
            Entry::StemSnapV(StemSnaps::new(stack.fixed_values()))
        }
        ForceBold => Entry::ForceBold(control::font_try!(stack.pop_i32()) != 0),
        InitialRandomSeed => Entry::InitialRandomSeed(control::font_try!(stack.pop_i32())),
        // Blend is handled at the layer above
        Blend => unreachable!(),
    };
    cx.checkpoint(0)?;
    Ok(Ok(entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tables::variations::ItemVariationStore, types::F2Dot14, FontData, FontRead, FontRef,
        TableProvider,
    };
    use font_test_data::bebuffer::BeBuffer;

    #[test]
    fn example_top_dict_tokens() {
        use Operator::*;
        let top_dict_data = &font_test_data::cff2::EXAMPLE[5..12];
        let tokens: Vec<_> = tokens(top_dict_data).map(|entry| entry.unwrap()).collect();
        let expected: &[Token] = &[
            68.into(),
            FdArrayOffset.into(),
            56.into(),
            CharstringsOffset.into(),
            16.into(),
            VariationStoreOffset.into(),
        ];
        assert_eq!(&tokens, expected);
    }

    #[test]
    fn example_top_dict_entries() {
        use Entry::*;
        let top_dict_data = &font_test_data::cff2::EXAMPLE[0x5..=0xB];
        let entries: Vec<_> = entries(top_dict_data, None)
            .map(|entry| entry.unwrap())
            .collect();
        let expected: &[Entry] = &[
            FdArrayOffset(68),
            CharstringsOffset(56),
            VariationStoreOffset(16),
        ];
        assert_eq!(&entries, expected);
    }

    #[test]
    fn example_private_dict_entries() {
        use Entry::*;
        let private_dict_data = &font_test_data::cff2::EXAMPLE[0x4f..=0xc0];
        let store =
            ItemVariationStore::read(FontData::new(&font_test_data::cff2::EXAMPLE[18..])).unwrap();
        let coords = &[F2Dot14::from_f32(0.0)];
        let blend_state = BlendState::new(store, coords, 0).unwrap();
        let entries: Vec<_> = entries(private_dict_data, Some(blend_state))
            .map(|entry| entry.unwrap())
            .collect();
        fn make_blues(values: &[f64]) -> Blues {
            Blues::new(values.iter().copied().map(Fixed::from_f64))
        }
        fn make_stem_snaps(values: &[f64]) -> StemSnaps {
            StemSnaps::new(values.iter().copied().map(Fixed::from_f64))
        }
        let expected: &[Entry] = &[
            BlueValues(make_blues(&[
                -20.0, 0.0, 472.0, 490.0, 525.0, 540.0, 645.0, 660.0, 670.0, 690.0, 730.0, 750.0,
            ])),
            OtherBlues(make_blues(&[-250.0, -240.0])),
            FamilyBlues(make_blues(&[
                -20.0, 0.0, 473.0, 491.0, 525.0, 540.0, 644.0, 659.0, 669.0, 689.0, 729.0, 749.0,
            ])),
            FamilyOtherBlues(make_blues(&[-249.0, -239.0])),
            BlueScale(Fixed::from_f64(0.037506103515625)),
            BlueFuzz(Fixed::ZERO),
            StdHw(Fixed::from_f64(55.0)),
            StdVw(Fixed::from_f64(80.0)),
            StemSnapH(make_stem_snaps(&[40.0, 55.0])),
            StemSnapV(make_stem_snaps(&[80.0, 90.0])),
            SubrsOffset(114),
        ];
        assert_eq!(&entries, expected);
    }

    #[test]
    fn noto_serif_display_top_dict_entries() {
        use Entry::*;
        let top_dict_data = FontRef::new(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED)
            .unwrap()
            .cff()
            .unwrap()
            .top_dicts()
            .get(0)
            .unwrap();
        let entries: Vec<_> = entries(top_dict_data, None)
            .map(|entry| entry.unwrap())
            .collect();
        let expected = &[
            Version(Sid::new(391)),
            Notice(Sid::new(392)),
            Copyright(Sid::new(393)),
            FullName(Sid::new(394)),
            FamilyName(Sid::new(395)),
            FontBbox([-693.0, -470.0, 2797.0, 1048.0].map(Fixed::from_f64)),
            Charset(517),
            PrivateDictRange(549..587),
            CharstringsOffset(521),
        ];
        assert_eq!(&entries, expected);
    }

    // Fuzzer caught add with overflow when constructing private DICT
    // range.
    // See <https://bugs.chromium.org/p/oss-fuzz/issues/detail?id=71746>
    // and <https://oss-fuzz.com/testcase?key=4591358306746368>
    #[test]
    fn private_dict_range_avoid_overflow() {
        // A Private DICT that tries to construct a range from -1..(-1 + -1)
        // which overflows when converted to usize
        let private_dict = BeBuffer::new()
            .push(29u8) // integer operator
            .push(-1i32) // integer value
            .push(29u8) // integer operator
            .push(-1i32) // integer value
            .push(18u8) // PrivateDICT operator
            .to_vec();
        // Just don't panic
        let _ = entries(&private_dict, None).count();
    }

    #[test]
    fn read_font_matrix() {
        let dict_data = [
            30u8, 10, 0, 31, 139, 30, 10, 0, 1, 103, 255, 30, 10, 0, 31, 139, 139, 12, 7,
        ];
        let Entry::FontMatrix(matrix) = entries(&dict_data, None).next().unwrap().unwrap() else {
            panic!("This was totally a font matrix");
        };
        // From ttx: <FontMatrix value="0.001 0 0.000167 0.001 0 0"/>
        // But scaled by 1000 because that's how FreeType does it
        assert_eq!(
            matrix.matrix.elements(),
            [
                Fixed::ONE,
                Fixed::ZERO,
                Fixed::from_f64(0.167007446289062),
                Fixed::ONE,
                Fixed::ZERO,
                Fixed::ZERO,
            ]
        );
    }

    #[test]
    fn parse_degenerate_font_matrix() {
        let dict_data = [
            30u8, 0x0F, 30, 0x0F, 30, 0x0F, 30, 0x0F, 30, 0x0F, 30, 0x0F, 12, 7,
        ];
        // Don't return a degenerate matrix at all
        assert!(entries(&dict_data, None).next().is_none());
    }
}

/// Borrowed token cursor: EOF and font errors cannot swallow outside control.
#[derive(Clone)]
pub struct Tokens<'a> {
    cursor: Cursor<'a>,
}
impl Tokens<'_> {
    pub fn try_next<C: WorkControl>(
        &mut self,
        control: &mut C,
    ) -> Result<Option<Result<Token, Error>>, C::Error> {
        control.checkpoint(0)?;
        if self.cursor.remaining_bytes() == 0 {
            return Ok(None);
        }
        let result = try_parse_token(&mut self.cursor, control)?;
        control.checkpoint(0)?;
        Ok(Some(result))
    }
}
pub fn try_tokens(data: &[u8]) -> Tokens<'_> {
    Tokens {
        cursor: crate::FontData::new(data).cursor(),
    }
}
