//! Additional support for working with OpenType features.

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::control::{self, OrdinaryControl, ReadCtl, TryResolveOffset};
use markdown_svg_budget::WorkControl;

use super::{Feature, FeatureList, ReadError, TaggedElement};

impl<'a> FeatureList<'a> {
    /// Returns the tag and feature at the given index.
    pub fn get(&self, index: u16) -> Result<TaggedElement<Feature<'a>>, ReadError> {
        control::ordinary(self.try_get(index, &mut OrdinaryControl::new()))
    }

    pub fn try_get<C: WorkControl>(
        &self,
        index: u16,
        cx: &mut C,
    ) -> ReadCtl<TaggedElement<Feature<'a>>, C> {
        cx.checkpoint(0)?;
        let rec = control::font_try!(self
            .feature_records()
            .get(index as usize)
            .ok_or(ReadError::OutOfBounds));
        let feature = control::font_try!(rec.feature_offset().try_resolve_with_args(
            self.offset_data(),
            &rec.feature_tag(),
            cx
        )?);
        Ok(Ok(TaggedElement::new(rec.feature_tag(), feature)))
    }
}

#[cfg(test)]
mod tests {
    use crate::{FontRef, TableProvider, Tag};

    #[test]
    fn feature_list_get() {
        let font = FontRef::new(font_test_data::NOTOSERIF_AUTOHINT_SHAPING).unwrap();
        let gsub = font.gsub().unwrap();
        let feature_list = gsub.feature_list().unwrap();
        assert_eq!(feature_list.get(0).unwrap().tag, Tag::new(b"c2sc"));
        assert_eq!(feature_list.get(1).unwrap().tag, Tag::new(b"liga"));
        assert!(feature_list.get(2).is_err());
    }
}
