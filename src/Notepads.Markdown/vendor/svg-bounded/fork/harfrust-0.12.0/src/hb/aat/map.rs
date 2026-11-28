// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::hb::common::{HB_FEATURE_GLOBAL_END, HB_FEATURE_GLOBAL_START};
use crate::Feature;

use crate::hb::storage::{Control, Storage as Vec};
use core::cmp::Ordering;

use super::layout::*;
use crate::hb::{hb_font_t, hb_mask_t, hb_tag_t};

/// HB: hb_aat_map_t
///
/// See <https://github.com/harfbuzz/harfbuzz/blob/2c22a65f0cb99544c36580b9703a43b5dc97a9e1/src/hb-aat-map.hh#L33>
#[doc(alias = "hb_aat_map_t")]
#[derive(Default)]
pub struct AatMap {
    pub chain_flags: Vec<Vec<RangeFlags>>,
}

/// HB: hb_aat_map_t::range_flags_t
///
/// See <https://github.com/harfbuzz/harfbuzz/blob/2c22a65f0cb99544c36580b9703a43b5dc97a9e1/src/hb-aat-map.hh#L38>
#[derive(Copy, Clone)]
pub struct RangeFlags {
    pub flags: hb_mask_t,
    pub cluster_first: u32,
    pub cluster_last: u32, // end - 1
}

/// HB: hb_aat_map_builder_t
///
/// See <https://github.com/harfbuzz/harfbuzz/blob/2c22a65f0cb99544c36580b9703a43b5dc97a9e1/src/hb-aat-map.hh#L49>
#[doc(alias = "hb_aat_map_builder_t")]
pub struct AatMapBuilder {
    pub current_features: Vec<FeatureInfo>,
    pub features: Vec<FeatureRange>,
    pub range_first: usize,
    pub range_last: usize,
}

impl Default for AatMapBuilder {
    fn default() -> Self {
        Self {
            range_first: HB_FEATURE_GLOBAL_START as usize,
            range_last: HB_FEATURE_GLOBAL_END as usize,
            current_features: Vec::default(),
            features: Vec::default(),
        }
    }
}

impl AatMapBuilder {
    pub(crate) fn new(control: &Control) -> Self {
        Self {
            current_features: Vec::new(control),
            features: Vec::new(control),
            ..Self::default()
        }
    }
    pub fn add_feature(&mut self, face: &hb_font_t, feature: &Feature) -> Option<()> {
        font_read!(face.control, face.control.step(0));
        let feat = face.aat_tables.feat.as_ref()?;

        if feature.tag == hb_tag_t::new(b"aalt") {
            let exposes_feature = font_read!(
                face.control,
                super::reader::feat_find(
                    feat,
                    FEATURE_TYPE_CHARACTER_ALTERNATIVES as u16,
                    &face.control
                )
            )
            .is_some_and(|f| f.n_settings() != 0);

            if !exposes_feature {
                return Some(());
            }

            let setting = if face.control.0.is_some() {
                match u16::try_from(feature.value) {
                    Ok(setting) => setting,
                    Err(_) => {
                        face.control.record(markdown_svg_budget::Error::new(
                            markdown_svg_budget::FailureKind::Quota,
                            markdown_svg_budget::LimitKind::Capacity,
                        ));
                        return None;
                    }
                }
            } else {
                u16::try_from(feature.value).unwrap()
            };

            self.features.push(FeatureRange {
                start: feature.start,
                end: feature.end,
                info: FeatureInfo {
                    kind: FEATURE_TYPE_CHARACTER_ALTERNATIVES as u16,
                    setting,
                    is_exclusive: true,
                },
            });
        }

        font_read!(
            face.control,
            face.control
                .step((usize::BITS - feature_mappings.len().leading_zeros() + 1) as u64)
        );
        let idx = feature_mappings
            .binary_search_by(|map| map.ot_feature_tag.cmp(&feature.tag))
            .ok()?;
        let mapping = &feature_mappings[idx];

        let mut feature_name = font_read!(
            face.control,
            super::reader::feat_find(feat, mapping.aat_feature_type as u16, &face.control)
        );

        match feature_name {
            Some(feature) if feature.n_settings() != 0 => {}
            _ => {
                // Special case: Chain::compile_flags will fall back to the deprecated version of
                // small-caps if necessary, so we need to check for that possibility.
                // https://github.com/harfbuzz/harfbuzz/issues/2307
                if mapping.aat_feature_type == FEATURE_TYPE_LOWER_CASE
                    && mapping.selector_to_enable == FEATURE_SELECTOR_LOWER_CASE_SMALL_CAPS
                {
                    feature_name = font_read!(
                        face.control,
                        super::reader::feat_find(
                            feat,
                            FEATURE_TYPE_LETTER_CASE as u16,
                            &face.control
                        )
                    );
                }
            }
        }

        match feature_name {
            Some(feature_name) if feature_name.n_settings() != 0 => {
                let setting = if feature.value != 0 {
                    mapping.selector_to_enable
                } else {
                    mapping.selector_to_disable
                } as u16;

                self.features.push(FeatureRange {
                    start: feature.start,
                    end: feature.end,
                    info: FeatureInfo {
                        kind: mapping.aat_feature_type as u16,
                        setting,
                        is_exclusive: feature_name.is_exclusive(),
                    },
                });
            }
            _ => {}
        }

        Some(())
    }

    pub fn compile(&mut self, face: &hb_font_t, m: &mut AatMap) {
        // Compute active features per range, and compile each.
        let mut feature_events = Vec::new(&face.control);
        for feature in &self.features {
            if face.control.step(1).is_err() {
                return;
            }
            if feature.start == feature.end {
                continue;
            }

            feature_events.push(FeatureEvent {
                index: feature.start as usize,
                start: true,
                feature: feature.info,
            });

            feature_events.push(FeatureEvent {
                index: feature.end as usize,
                start: false,
                feature: feature.info,
            });
        }

        feature_events.sort();
        if face.control.step(0).is_err() {
            return;
        }

        // Add a strategic final event.
        feature_events.push(FeatureEvent {
            index: u32::MAX as usize,
            start: false,
            feature: FeatureInfo::default(),
        });

        // Scan events and save features for each range.
        let mut active_features = Vec::new(&face.control);
        let mut last_index = 0;

        for event in &feature_events {
            if face.control.step(1).is_err() {
                return;
            }
            if event.index != last_index {
                // Save a snapshot of active features and the range.
                // Sort features and merge duplicates.
                self.current_features.clear();
                if self.current_features.try_extend(&active_features).is_err() {
                    return;
                }
                self.range_first = last_index;
                self.range_last = event.index.wrapping_sub(1);

                if !self.current_features.is_empty() {
                    self.current_features.sort();
                    if face.control.step(0).is_err() {
                        return;
                    }
                    let mut j = 0;
                    for i in 1..self.current_features.len() {
                        if face.control.step(1).is_err() {
                            return;
                        }
                        // Nonexclusive feature selectors come in even/odd pairs to turn a setting on/off
                        // respectively, so we mask out the low-order bit when checking for "duplicates"
                        // (selectors referring to the same feature setting) here.
                        let non_exclusive = !self.current_features[i].is_exclusive
                            && (self.current_features[i].setting & !1)
                                != (self.current_features[j].setting & !1);

                        if self.current_features[i].kind != self.current_features[j].kind
                            || non_exclusive
                        {
                            j += 1;
                            self.current_features[j] = self.current_features[i];
                        }
                    }
                    self.current_features.truncate(j + 1);
                }

                if super::layout_morx_table::compile_flags(face, self, m).is_none() {
                    return;
                }
                if face.control.step(0).is_err() {
                    return;
                }
                last_index = event.index;
            }

            if event.start {
                active_features.push(event.feature);
            } else {
                if let Some(index) = active_features
                    .iter()
                    .position(|&f| face.control.step(1).is_err() || f == event.feature)
                {
                    if face.control.step(0).is_err() {
                        return;
                    }
                    let len = active_features.len();
                    for i in index..len - 1 {
                        if face.control.step(1).is_err() {
                            return;
                        }
                        active_features[i] = active_features[i + 1];
                    }
                    active_features.pop();
                }
            }
        }

        if let Err(error) = finish_chain_flags(m, &face.control) {
            face.control.record(error);
        }
    }
}

fn finish_chain_flags(
    map: &mut AatMap,
    control: &Control,
) -> Result<(), markdown_svg_budget::Error> {
    for flags in &mut map.chain_flags {
        control.step(1)?;
        if let Some(last) = flags.last_mut() {
            last.cluster_last = HB_FEATURE_GLOBAL_END;
        }
    }
    control.step(0)
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    use crate::hb::glyph_array::tests::support::setup;
    use markdown_svg_budget::{Error, FailureKind, LimitKind};
    use std::sync::atomic::Ordering;
    #[test]
    fn repair_chain_tail_second_window_stops_before_rejected_chain() {
        let (r, state) = setup();
        {
            let control = Control(Some(r.clone()));
            let mut map = AatMap {
                chain_flags: Vec::new(&control),
            };
            for _ in 0..1024 {
                let mut flags = Vec::new(&control);
                flags
                    .try_push(RangeFlags {
                        flags: 1,
                        cluster_first: 0,
                        cluster_last: 7,
                    })
                    .unwrap();
                map.chain_flags.try_push(flags).unwrap();
            }
            state.reset_windows(257);
            let expected = Error::new(FailureKind::Cancelled, LimitKind::Work);
            assert_eq!(finish_chain_flags(&mut map, &control), Err(expected));
            assert_eq!(state.windows.load(Ordering::Relaxed), 257);
            assert!(map.chain_flags[..256]
                .iter()
                .all(|f| f[0].cluster_last == HB_FEATURE_GLOBAL_END));
            assert!(map.chain_flags[256..]
                .iter()
                .all(|f| f[0].cluster_last == 7));
        }
        assert_eq!(state.live(), 0);
        r.validate_retired().unwrap();
    }
}

/// HB: hb_aat_map_builder_t::feature_info_t
///
/// See <https://github.com/harfbuzz/harfbuzz/blob/2c22a65f0cb99544c36580b9703a43b5dc97a9e1/src/hb-aat-map.hh#L63>
#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub struct FeatureInfo {
    pub kind: u16,
    pub setting: u16,
    pub is_exclusive: bool,
}

impl Ord for FeatureInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl PartialOrd for FeatureInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.kind != other.kind {
            Some(self.kind.cmp(&other.kind))
        } else if !self.is_exclusive && (self.setting & !1) != (other.setting & !1) {
            Some(self.setting.cmp(&other.setting))
        } else {
            Some(Ordering::Equal)
        }
    }
}

/// HB: hb_aat_map_builder_t::feature_range_t
///
/// See <https://github.com/harfbuzz/harfbuzz/blob/2c22a65f0cb99544c36580b9703a43b5dc97a9e1/src/hb-aat-map.hh#L88>
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct FeatureRange {
    pub info: FeatureInfo,
    pub start: u32,
    pub end: u32,
}

/// HB: hb_aat_map_builder_t::feature_event_t
///
/// See <https://github.com/harfbuzz/harfbuzz/blob/2c22a65f0cb99544c36580b9703a43b5dc97a9e1/src/hb-aat-map.hh#L96>
#[derive(Copy, Clone, Eq, PartialEq)]
struct FeatureEvent {
    pub index: usize,
    pub start: bool,
    pub feature: FeatureInfo,
}

impl Ord for FeatureEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl PartialOrd for FeatureEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.index != other.index {
            Some(self.index.cmp(&other.index))
        } else if self.start != other.start {
            Some(self.start.cmp(&other.start))
        } else {
            Some(Ordering::Equal)
        }
    }
}
