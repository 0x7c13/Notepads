//! The [morx (Extended Glyph Metamorphosis)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6morx.html) table.

use super::aat::{safe_read_array_to_end, ExtendedStateTable, LookupU16};
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::control::{
    self, OrdinaryControl, ReadCtl, TryFontRead, TryFontReadWithArgs, TryVarSize,
};
use markdown_svg_budget::WorkControl;

include!("../../generated/generated_morx.rs");

impl VarSize for Chain<'_> {
    type Size = u32;

    fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
        control::ordinary(Self::try_read_len_at(
            data,
            pos,
            &mut OrdinaryControl::new(),
        ))
    }
}
impl TryVarSize for Chain<'_> {
    fn try_read_len_at<C: WorkControl>(
        data: FontData<'_>,
        pos: usize,
        cx: &mut C,
    ) -> Result<Option<usize>, C::Error> {
        cx.checkpoint(0)?;
        // Size in a chain is second field beyond 4 byte `defaultFlags`
        let Some(offset) = pos.checked_add(u32::RAW_BYTE_LEN) else {
            return Ok(None);
        };
        Ok(data.read_at::<u32>(offset).ok().map(|size| size as usize))
    }
}

impl VarSize for Subtable<'_> {
    type Size = u32;

    fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
        control::ordinary(Self::try_read_len_at(
            data,
            pos,
            &mut OrdinaryControl::new(),
        ))
    }
}
impl TryVarSize for Subtable<'_> {
    fn try_read_len_at<C: WorkControl>(
        data: FontData<'_>,
        pos: usize,
        cx: &mut C,
    ) -> Result<Option<usize>, C::Error> {
        cx.checkpoint(0)?;
        // The default implementation assumes that the length field itself
        // is not included in the total size which is not true of this
        // table.
        Ok(data.read_at::<u32>(pos).ok().map(|size| size as usize))
    }
}

impl<'a> Subtable<'a> {
    /// If true, this subtable will process glyphs in logical order (or reverse
    /// logical order, depending on the value of bit 0x80000000).
    #[inline]
    pub fn is_logical(&self) -> bool {
        self.coverage() & 0x10000000 != 0
    }

    /// If true, this subtable will be applied to both horizontal and vertical
    /// text (i.e. the state of bit 0x80000000 is ignored).
    #[inline]
    pub fn is_all_directions(&self) -> bool {
        self.coverage() & 0x20000000 != 0
    }

    /// If true, this subtable will process glyphs in descending order.
    /// Otherwise, it will process the glyphs in ascending order.
    #[inline]
    pub fn is_backwards(&self) -> bool {
        self.coverage() & 0x40000000 != 0
    }

    /// If true, this subtable will only be applied to vertical text.
    /// Otherwise, this subtable will only be applied to horizontal
    /// text.
    #[inline]
    pub fn is_vertical(&self) -> bool {
        self.coverage() & 0x80000000 != 0
    }

    /// Returns an enum representing the actual subtable data.
    pub fn kind(&self) -> Result<SubtableKind<'a>, ReadError> {
        control::ordinary(self.try_kind(&mut OrdinaryControl::new()))
    }

    pub fn try_kind<C: WorkControl>(&self, cx: &mut C) -> ReadCtl<SubtableKind<'a>, C> {
        cx.checkpoint(0)?;
        SubtableKind::try_read_with_args(FontData::new(self.data()), &self.coverage(), cx)
    }
}

/// The various `morx` subtable formats.
#[derive(Clone)]
pub enum SubtableKind<'a> {
    Rearrangement(ExtendedStateTable<'a>),
    Contextual(ContextualSubtable<'a>),
    Ligature(LigatureSubtable<'a>),
    NonContextual(LookupU16<'a>),
    Insertion(InsertionSubtable<'a>),
}

impl ReadArgs for SubtableKind<'_> {
    type Args = u32;
}

impl<'a> FontReadWithArgs<'a> for SubtableKind<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        control::ordinary(Self::try_read_with_args(
            data,
            args,
            &mut OrdinaryControl::new(),
        ))
    }
}
impl control::sealed::Reader for SubtableKind<'_> {}
impl<'a> TryFontReadWithArgs<'a> for SubtableKind<'a> {
    fn try_read_with_args<C: WorkControl>(
        data: FontData<'a>,
        args: &Self::Args,
        cx: &mut C,
    ) -> ReadCtl<Self, C> {
        cx.checkpoint(0)?;
        // Format is low byte of coverage
        let format = *args & 0xFF;
        match format {
            0 => Ok(ExtendedStateTable::try_read(data, cx)?.map(Self::Rearrangement)),
            1 => Ok(ContextualSubtable::try_read(data, cx)?.map(Self::Contextual)),
            2 => Ok(LigatureSubtable::try_read(data, cx)?.map(Self::Ligature)),
            // 3 is reserved
            4 => Ok(LookupU16::try_read(data, cx)?.map(Self::NonContextual)),
            5 => Ok(InsertionSubtable::try_read(data, cx)?.map(Self::Insertion)),
            _ => Ok(Err(ReadError::InvalidFormat(format as _))),
        }
    }
}

/// Contextual glyph substitution subtable.
#[derive(Clone)]
pub struct ContextualSubtable<'a> {
    pub state_table: ExtendedStateTable<'a, ContextualEntryData>,
    /// List of lookups specifying substitutions. The index into this array
    /// is specified by the action in the state table.
    pub lookups: ArrayOfOffsets<'a, LookupU16<'a>, Offset32>,
}

impl<'a> FontRead<'a> for ContextualSubtable<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        control::ordinary(Self::try_read(data, &mut OrdinaryControl::new()))
    }
}
impl control::sealed::Reader for ContextualSubtable<'_> {}
impl<'a> TryFontRead<'a> for ContextualSubtable<'a> {
    fn try_read<C: WorkControl>(data: FontData<'a>, cx: &mut C) -> ReadCtl<Self, C> {
        let state_table = control::font_try!(ExtendedStateTable::try_read(data, cx)?);
        // Fixed cursor fields and slice views only. No element is scanned/copied.
        let result: Result<Self, ReadError> = (|| {
            let mut cursor = data.cursor();
            cursor.advance_by(ExtendedStateTable::<()>::HEADER_LEN);
            let offset = cursor.read::<u32>()? as usize;
            let end = data.len();
            let offsets_data = FontData::new(data.read_array(offset..end)?);
            let raw_offsets: &[BigEndian<Offset32>] = safe_read_array_to_end(&offsets_data, 0)?;
            let lookups = ArrayOfOffsets::new(raw_offsets, offsets_data, ());
            Ok(Self {
                state_table,
                lookups,
            })
        })();
        Ok(result)
    }
}

/// Ligature glyph substitution subtable.
#[derive(Clone)]
pub struct LigatureSubtable<'a> {
    pub state_table: ExtendedStateTable<'a, BigEndian<u16>>,
    /// Contains the set of ligature stack actions, one for each state.
    pub ligature_actions: &'a [BigEndian<u32>],
    /// Array of component indices which are summed to determine the index
    /// of the final ligature glyph.
    pub components: &'a [BigEndian<u16>],
    /// Output ligature glyphs.
    pub ligatures: &'a [BigEndian<GlyphId16>],
}

impl<'a> FontRead<'a> for LigatureSubtable<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        control::ordinary(Self::try_read(data, &mut OrdinaryControl::new()))
    }
}
impl control::sealed::Reader for LigatureSubtable<'_> {}
impl<'a> TryFontRead<'a> for LigatureSubtable<'a> {
    fn try_read<C: WorkControl>(data: FontData<'a>, cx: &mut C) -> ReadCtl<Self, C> {
        let state_table = control::font_try!(ExtendedStateTable::try_read(data, cx)?);
        // Fixed cursor fields and slice views only. No element is scanned/copied.
        let result: Result<Self, ReadError> = (|| {
            let mut cursor = data.cursor();
            cursor.advance_by(ExtendedStateTable::<()>::HEADER_LEN);
            // None of these arrays have associated sizes, so we just read until
            // the end of the data.
            let lig_action_offset = cursor.read::<u32>()? as usize;
            let component_offset = cursor.read::<u32>()? as usize;
            let ligature_offset = cursor.read::<u32>()? as usize;
            let ligature_actions = safe_read_array_to_end(&data, lig_action_offset)?;
            let components = safe_read_array_to_end(&data, component_offset)?;
            let ligatures = safe_read_array_to_end(&data, ligature_offset)?;
            Ok(Self {
                state_table,
                ligature_actions,
                components,
                ligatures,
            })
        })();
        Ok(result)
    }
}

/// Insertion glyph substitution subtable.
#[derive(Clone)]
pub struct InsertionSubtable<'a> {
    pub state_table: ExtendedStateTable<'a, InsertionEntryData>,
    /// Insertion glyph table. The index and count of glyphs to insert is
    /// determined by the state machine.
    pub glyphs: &'a [BigEndian<GlyphId16>],
}

impl<'a> FontRead<'a> for InsertionSubtable<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        control::ordinary(Self::try_read(data, &mut OrdinaryControl::new()))
    }
}
impl control::sealed::Reader for InsertionSubtable<'_> {}
impl<'a> TryFontRead<'a> for InsertionSubtable<'a> {
    fn try_read<C: WorkControl>(data: FontData<'a>, cx: &mut C) -> ReadCtl<Self, C> {
        let state_table = control::font_try!(ExtendedStateTable::try_read(data, cx)?);
        // Fixed cursor fields and slice views only. No element is scanned/copied.
        let result: Result<Self, ReadError> = (|| {
            let mut cursor = data.cursor();
            cursor.advance_by(ExtendedStateTable::<()>::HEADER_LEN);
            let glyphs_offset = cursor.read::<u32>()? as usize;
            let glyphs = safe_read_array_to_end(&data, glyphs_offset)?;
            Ok(Self {
                state_table,
                glyphs,
            })
        })();
        Ok(result)
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> SomeRecord<'a> for Chain<'a> {
    fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
        RecordResolver {
            name: "Chain",
            get_field: Box::new(move |idx, _data| match idx {
                0usize => Some(Field::new("default_flags", self.default_flags())),
                _ => None,
            }),
            data,
        }
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> SomeRecord<'a> for Subtable<'a> {
    fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
        RecordResolver {
            name: "Subtable",
            get_field: Box::new(move |idx, _data| match idx {
                0usize => Some(Field::new("coverage", self.coverage())),
                1usize => Some(Field::new("sub_feature_flags", self.sub_feature_flags())),
                _ => None,
            }),
            data,
        }
    }
}

#[cfg(test)]
// Literal bytes are grouped according to layout in the spec
// for readabiity
#[allow(clippy::unusual_byte_groupings)]
mod tests {
    use super::*;
    use crate::{FontRef, TableProvider};

    #[test]
    fn parse_chain_flags_features() {
        let font = FontRef::new(font_test_data::morx::FOUR).unwrap();
        let morx = font.morx().unwrap();
        let chain = morx.chains().iter().next().unwrap().unwrap();
        assert_eq!(chain.default_flags(), 1);
        let feature = chain.features()[0];
        assert_eq!(feature.feature_type(), 4);
        assert_eq!(feature.feature_settings(), 0);
        assert_eq!(feature.enable_flags(), 1);
        assert_eq!(feature.disable_flags(), 0xFFFFFFFF);
    }

    #[test]
    fn parse_rearrangement() {
        let font = FontRef::new(font_test_data::morx::FOUR).unwrap();
        let morx = font.morx().unwrap();
        let chain = morx.chains().iter().next().unwrap().unwrap();
        let subtable = chain.subtables().iter().next().unwrap().unwrap();
        assert_eq!(subtable.coverage(), 0x20_0000_00);
        // Rearrangement is just a state table
        let SubtableKind::Rearrangement(_kind) = subtable.kind().unwrap() else {
            panic!("expected rearrangement subtable!");
        };
    }

    #[test]
    fn parse_contextual() {
        let font = FontRef::new(font_test_data::morx::EIGHTEEN).unwrap();
        let morx = font.morx().unwrap();
        let chain = morx.chains().iter().next().unwrap().unwrap();
        let subtable = chain.subtables().iter().next().unwrap().unwrap();
        assert_eq!(subtable.coverage(), 0x20_0000_01);
        let SubtableKind::Contextual(kind) = subtable.kind().unwrap() else {
            panic!("expected contextual subtable!");
        };
        let lookup = kind.lookups.get(0).unwrap();
        let expected = [None, None, Some(7u16), Some(8), Some(9), Some(10), Some(11)];
        let values = (0..7).map(|gid| lookup.value(gid).ok()).collect::<Vec<_>>();
        assert_eq!(values, &expected);
    }

    #[test]
    fn parse_ligature() {
        let font = FontRef::new(font_test_data::morx::FORTY_ONE).unwrap();
        let morx = font.morx().unwrap();
        let chain = morx.chains().iter().next().unwrap().unwrap();
        let subtable = chain.subtables().iter().next().unwrap().unwrap();
        assert_eq!(subtable.coverage(), 0x20_0000_02);
        let SubtableKind::Ligature(kind) = subtable.kind().unwrap() else {
            panic!("expected ligature subtable!");
        };
        let expected_actions = [0x3FFFFFFE, 0xBFFFFFFE];
        // Note, we limit the number of elements because the arrays do not
        // have specified lengths in the table
        let actions = kind
            .ligature_actions
            .iter()
            .take(2)
            .map(|action| action.get())
            .collect::<Vec<_>>();
        assert_eq!(actions, &expected_actions);
        let expected_components = [0u16, 1, 0, 0];
        // See above explanation for the limit
        let components = kind
            .components
            .iter()
            .take(4)
            .map(|comp| comp.get())
            .collect::<Vec<_>>();
        assert_eq!(components, &expected_components);
        let expected_ligatures = [GlyphId16::new(5), GlyphId16::new(6)];
        let ligatures = kind
            .ligatures
            .iter()
            .map(|gid| gid.get())
            .collect::<Vec<_>>();
        assert_eq!(ligatures, &expected_ligatures);
    }

    #[test]
    fn parse_non_contextual() {
        let font = FontRef::new(font_test_data::morx::ONE).unwrap();
        let morx = font.morx().unwrap();
        let chain = morx.chains().iter().next().unwrap().unwrap();
        let subtable = chain.subtables().iter().next().unwrap().unwrap();
        assert_eq!(subtable.coverage(), 0x20_0000_04);
        let SubtableKind::NonContextual(kind) = subtable.kind().unwrap() else {
            panic!("expected non-contextual subtable!");
        };
        let expected_values = [None, None, Some(5u16), None, Some(7)];
        let values = (0..5).map(|gid| kind.value(gid).ok()).collect::<Vec<_>>();
        assert_eq!(values, &expected_values);
    }

    #[test]
    fn parse_insertion() {
        let font = FontRef::new(font_test_data::morx::THIRTY_FOUR).unwrap();
        let morx = font.morx().unwrap();
        let chain = morx.chains().iter().next().unwrap().unwrap();
        let subtable = chain.subtables().iter().next().unwrap().unwrap();
        assert_eq!(subtable.coverage(), 0x20_0000_05);
        let SubtableKind::Insertion(kind) = subtable.kind().unwrap() else {
            panic!("expected insertion subtable!");
        };
        let mut expected_glyphs = vec![];
        for _ in 0..9 {
            for gid in [3, 2] {
                expected_glyphs.push(GlyphId16::new(gid));
            }
        }
        assert_eq!(kind.glyphs, &expected_glyphs);
    }
}
