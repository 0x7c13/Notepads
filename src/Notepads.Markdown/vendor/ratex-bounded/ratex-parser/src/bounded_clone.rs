// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Generated explicit field-by-field bounded clone; no serde round trip or ambient context.
use crate::parse_node::*;
use ratex_types::{
    execution::Phase,
    storage::{BoundedClone, StorageError, StorageFamily, StorageLedger},
};
macro_rules! leaf_clone {
    ($($ty:ty),* $(,)?) => {$(impl BoundedClone for $ty {
        fn bounded_clone(&self, storage:&StorageLedger<'_>, phase:Phase, _storage_family:StorageFamily)
            ->Result<Self,StorageError>{ storage.check(phase)?; Ok(self.clone()) }
    })*};
}
leaf_clone!(Mode, StyleStr, AtomFamily, AlignType, ProofLineStyle);
// Declaration SHA256: 6e6733b3823e5239d8c3e33e996e85a5f6e865f1340b4342a99853458c4d06c7
impl BoundedClone for Measurement {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        storage_family: StorageFamily,
    ) -> Result<Self, StorageError> {
        storage.check(phase)?;
        Ok(Self {
            number: self.number.bounded_clone(storage, phase, storage_family)?,
            unit: self.unit.bounded_clone(storage, phase, storage_family)?,
        })
    }
}
impl BoundedClone for AlignSpec {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        storage_family: StorageFamily,
    ) -> Result<Self, StorageError> {
        storage.check(phase)?;
        Ok(Self {
            align_type: self
                .align_type
                .bounded_clone(storage, phase, storage_family)?,
            align: self.align.bounded_clone(storage, phase, storage_family)?,
            pregap: self.pregap.bounded_clone(storage, phase, storage_family)?,
            postgap: self.postgap.bounded_clone(storage, phase, storage_family)?,
        })
    }
}
impl BoundedClone for ArrayTag {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        storage_family: StorageFamily,
    ) -> Result<Self, StorageError> {
        storage.check(phase)?;
        Ok(match self {
            Self::Auto(value) => Self::Auto(*value),
            Self::Explicit(value) => {
                Self::Explicit(value.bounded_clone(storage, phase, storage_family)?)
            }
        })
    }
}
impl BoundedClone for ParseNode {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        storage_family: StorageFamily,
    ) -> Result<Self, StorageError> {
        storage.check(phase)?;
        Ok(match self {
            Self::Atom {
                mode,
                family,
                text,
                loc,
            } => Self::Atom {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                family: family.bounded_clone(storage, phase, storage_family)?,
                text: text.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::MathOrd { mode, text, loc } => Self::MathOrd {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                text: text.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::TextOrd { mode, text, loc } => Self::TextOrd {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                text: text.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::OpToken { mode, text, loc } => Self::OpToken {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                text: text.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::AccentToken { mode, text, loc } => Self::AccentToken {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                text: text.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::SpacingNode { mode, text, loc } => Self::SpacingNode {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                text: text.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::OrdGroup {
                mode,
                body,
                semisimple,
                loc,
            } => Self::OrdGroup {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                semisimple: semisimple.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::SupSub {
                mode,
                base,
                sup,
                sub,
                loc,
            } => Self::SupSub {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                base: base.bounded_clone(storage, phase, storage_family)?,
                sup: sup.bounded_clone(storage, phase, storage_family)?,
                sub: sub.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::GenFrac {
                mode,
                continued,
                numer,
                denom,
                has_bar_line,
                left_delim,
                right_delim,
                bar_size,
                loc,
            } => Self::GenFrac {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                continued: continued.bounded_clone(storage, phase, storage_family)?,
                numer: numer.bounded_clone(storage, phase, storage_family)?,
                denom: denom.bounded_clone(storage, phase, storage_family)?,
                has_bar_line: has_bar_line.bounded_clone(storage, phase, storage_family)?,
                left_delim: left_delim.bounded_clone(storage, phase, storage_family)?,
                right_delim: right_delim.bounded_clone(storage, phase, storage_family)?,
                bar_size: bar_size.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Sqrt {
                mode,
                body,
                index,
                loc,
            } => Self::Sqrt {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                index: index.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Accent {
                mode,
                label,
                is_stretchy,
                is_shifty,
                base,
                loc,
            } => Self::Accent {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                label: label.bounded_clone(storage, phase, storage_family)?,
                is_stretchy: is_stretchy.bounded_clone(storage, phase, storage_family)?,
                is_shifty: is_shifty.bounded_clone(storage, phase, storage_family)?,
                base: base.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::AccentUnder {
                mode,
                label,
                is_stretchy,
                is_shifty,
                base,
                loc,
            } => Self::AccentUnder {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                label: label.bounded_clone(storage, phase, storage_family)?,
                is_stretchy: is_stretchy.bounded_clone(storage, phase, storage_family)?,
                is_shifty: is_shifty.bounded_clone(storage, phase, storage_family)?,
                base: base.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Op {
                mode,
                limits,
                always_handle_sup_sub,
                suppress_base_shift,
                parent_is_sup_sub,
                symbol,
                name,
                body,
                loc,
            } => Self::Op {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                limits: limits.bounded_clone(storage, phase, storage_family)?,
                always_handle_sup_sub: always_handle_sup_sub.bounded_clone(
                    storage,
                    phase,
                    storage_family,
                )?,
                suppress_base_shift: suppress_base_shift.bounded_clone(
                    storage,
                    phase,
                    storage_family,
                )?,
                parent_is_sup_sub: parent_is_sup_sub.bounded_clone(
                    storage,
                    phase,
                    storage_family,
                )?,
                symbol: symbol.bounded_clone(storage, phase, storage_family)?,
                name: name.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::OperatorName {
                mode,
                body,
                always_handle_sup_sub,
                limits,
                parent_is_sup_sub,
                loc,
            } => Self::OperatorName {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                always_handle_sup_sub: always_handle_sup_sub.bounded_clone(
                    storage,
                    phase,
                    storage_family,
                )?,
                limits: limits.bounded_clone(storage, phase, storage_family)?,
                parent_is_sup_sub: parent_is_sup_sub.bounded_clone(
                    storage,
                    phase,
                    storage_family,
                )?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Font {
                mode,
                font,
                body,
                loc,
            } => Self::Font {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                font: font.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Text {
                mode,
                body,
                font,
                loc,
            } => Self::Text {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                font: font.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Color {
                mode,
                color,
                body,
                loc,
            } => Self::Color {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                color: color.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::ColorToken { mode, color, loc } => Self::ColorToken {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                color: color.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Size {
                mode,
                value,
                is_blank,
                loc,
            } => Self::Size {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                value: value.bounded_clone(storage, phase, storage_family)?,
                is_blank: is_blank.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Styling {
                mode,
                style,
                body,
                loc,
            } => Self::Styling {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                style: style.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Sizing {
                mode,
                size,
                body,
                loc,
            } => Self::Sizing {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                size: size.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::DelimSizing {
                mode,
                size,
                mclass,
                delim,
                loc,
            } => Self::DelimSizing {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                size: size.bounded_clone(storage, phase, storage_family)?,
                mclass: mclass.bounded_clone(storage, phase, storage_family)?,
                delim: delim.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::LeftRight {
                mode,
                body,
                left,
                right,
                right_color,
                loc,
            } => Self::LeftRight {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                left: left.bounded_clone(storage, phase, storage_family)?,
                right: right.bounded_clone(storage, phase, storage_family)?,
                right_color: right_color.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::LeftRightRight {
                mode,
                delim,
                color,
                loc,
            } => Self::LeftRightRight {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                delim: delim.bounded_clone(storage, phase, storage_family)?,
                color: color.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Middle { mode, delim, loc } => Self::Middle {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                delim: delim.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Overline { mode, body, loc } => Self::Overline {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Underline { mode, body, loc } => Self::Underline {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Rule {
                mode,
                shift,
                width,
                height,
                loc,
            } => Self::Rule {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                shift: shift.bounded_clone(storage, phase, storage_family)?,
                width: width.bounded_clone(storage, phase, storage_family)?,
                height: height.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Kern {
                mode,
                dimension,
                loc,
            } => Self::Kern {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                dimension: dimension.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Phantom { mode, body, loc } => Self::Phantom {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::VPhantom { mode, body, loc } => Self::VPhantom {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Smash {
                mode,
                body,
                smash_height,
                smash_depth,
                loc,
            } => Self::Smash {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                smash_height: smash_height.bounded_clone(storage, phase, storage_family)?,
                smash_depth: smash_depth.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::MClass {
                mode,
                mclass,
                body,
                is_character_box,
                loc,
            } => Self::MClass {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                mclass: mclass.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                is_character_box: is_character_box.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Array {
                mode,
                body,
                row_gaps,
                hlines_before_row,
                cols,
                col_separation_type,
                hskip_before_and_after,
                add_jot,
                arraystretch,
                tags,
                leqno,
                is_cd,
                loc,
            } => Self::Array {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                row_gaps: row_gaps.bounded_clone(storage, phase, storage_family)?,
                hlines_before_row: hlines_before_row.bounded_clone(
                    storage,
                    phase,
                    storage_family,
                )?,
                cols: cols.bounded_clone(storage, phase, storage_family)?,
                col_separation_type: col_separation_type.bounded_clone(
                    storage,
                    phase,
                    storage_family,
                )?,
                hskip_before_and_after: hskip_before_and_after.bounded_clone(
                    storage,
                    phase,
                    storage_family,
                )?,
                add_jot: add_jot.bounded_clone(storage, phase, storage_family)?,
                arraystretch: arraystretch.bounded_clone(storage, phase, storage_family)?,
                tags: tags.bounded_clone(storage, phase, storage_family)?,
                leqno: leqno.bounded_clone(storage, phase, storage_family)?,
                is_cd: is_cd.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Environment {
                mode,
                name,
                name_group,
                loc,
            } => Self::Environment {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                name: name.bounded_clone(storage, phase, storage_family)?,
                name_group: name_group.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Cr {
                mode,
                new_line,
                size,
                loc,
            } => Self::Cr {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                new_line: new_line.bounded_clone(storage, phase, storage_family)?,
                size: size.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Infix {
                mode,
                replace_with,
                size,
                loc,
            } => Self::Infix {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                replace_with: replace_with.bounded_clone(storage, phase, storage_family)?,
                size: size.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Internal { mode, loc } => Self::Internal {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Verb {
                mode,
                body,
                star,
                loc,
            } => Self::Verb {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                star: star.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Href {
                mode,
                href,
                body,
                loc,
            } => Self::Href {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                href: href.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Url { mode, url, loc } => Self::Url {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                url: url.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Raw { mode, string, loc } => Self::Raw {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                string: string.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::HBox { mode, body, loc } => Self::HBox {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::HorizBrace {
                mode,
                label,
                is_over,
                base,
                loc,
            } => Self::HorizBrace {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                label: label.bounded_clone(storage, phase, storage_family)?,
                is_over: is_over.bounded_clone(storage, phase, storage_family)?,
                base: base.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Enclose {
                mode,
                label,
                background_color,
                border_color,
                body,
                loc,
            } => Self::Enclose {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                label: label.bounded_clone(storage, phase, storage_family)?,
                background_color: background_color.bounded_clone(storage, phase, storage_family)?,
                border_color: border_color.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Lap {
                mode,
                alignment,
                body,
                loc,
            } => Self::Lap {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                alignment: alignment.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::MathChoice {
                mode,
                display,
                text,
                script,
                scriptscript,
                loc,
            } => Self::MathChoice {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                display: display.bounded_clone(storage, phase, storage_family)?,
                text: text.bounded_clone(storage, phase, storage_family)?,
                script: script.bounded_clone(storage, phase, storage_family)?,
                scriptscript: scriptscript.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::RaiseBox {
                mode,
                dy,
                body,
                loc,
            } => Self::RaiseBox {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                dy: dy.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::VCenter { mode, body, loc } => Self::VCenter {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::XArrow {
                mode,
                label,
                body,
                below,
                loc,
            } => Self::XArrow {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                label: label.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                below: below.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Pmb {
                mode,
                mclass,
                body,
                loc,
            } => Self::Pmb {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                mclass: mclass.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Tag {
                mode,
                body,
                tag,
                loc,
            } => Self::Tag {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                body: body.bounded_clone(storage, phase, storage_family)?,
                tag: tag.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::NoNumber { mode, loc } => Self::NoNumber {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::Html { .. } => return Err(storage.fail(StorageError::UnsupportedObject)),
            Self::HtmlMathMl {
                mode,
                html,
                mathml,
                loc,
            } => Self::HtmlMathMl {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                html: html.bounded_clone(storage, phase, storage_family)?,
                mathml: mathml.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::IncludeGraphics {
                mode,
                alt,
                width,
                height,
                totalheight,
                src,
                loc,
            } => Self::IncludeGraphics {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                alt: alt.bounded_clone(storage, phase, storage_family)?,
                width: width.bounded_clone(storage, phase, storage_family)?,
                height: height.bounded_clone(storage, phase, storage_family)?,
                totalheight: totalheight.bounded_clone(storage, phase, storage_family)?,
                src: src.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::CdLabel {
                mode,
                side,
                label,
                loc,
            } => Self::CdLabel {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                side: side.bounded_clone(storage, phase, storage_family)?,
                label: label.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::CdLabelParent {
                mode,
                fragment,
                loc,
            } => Self::CdLabelParent {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                fragment: fragment.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::CdArrow {
                mode,
                direction,
                label_above,
                label_below,
                loc,
            } => Self::CdArrow {
                mode: mode.bounded_clone(storage, phase, storage_family)?,
                direction: direction.bounded_clone(storage, phase, storage_family)?,
                label_above: label_above.bounded_clone(storage, phase, storage_family)?,
                label_below: label_below.bounded_clone(storage, phase, storage_family)?,
                loc: loc.bounded_clone(storage, phase, storage_family)?,
            },
            Self::ProofTree { .. } => return Err(storage.fail(StorageError::UnsupportedObject)),
        })
    }
}
const _: () = assert!(std::mem::size_of::<ParseNode>() <= 1024);
const _: () = assert!(std::mem::size_of::<ArrayTag>() <= 1024);
