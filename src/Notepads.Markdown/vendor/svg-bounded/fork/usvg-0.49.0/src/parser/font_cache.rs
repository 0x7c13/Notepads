// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Concrete font representation caches. Request keys borrow axes on lookup;
//! paid buckets own their inline scene metadata and immutable axis buffers.
use crate::flatten::BitmapImage;
use crate::tree::owner::{self, PolicyValue, Result};
use crate::{FontVariation, GlyphId, Node, ProcessingContext, Tree};
use markdown_svg_budget::{
    BucketAllocator, BudgetVec, Error, FailureKind, FrozenList, LimitKind, RequestControl,
    ResourceKind, ResourcesRef, WorkControl,
};
use std::{
    collections::{hash_map::RandomState, HashMap},
    hash::{BuildHasher, Hash, Hasher},
};
fn map_error(p: &ProcessingContext, e: hashbrown::ControlError<Error>) -> Error {
    p.record_resource(match e {
        hashbrown::ControlError::Control(e) => e,
        hashbrown::ControlError::Reserve(hashbrown::TryReserveError::CapacityOverflow) => {
            Error::new(FailureKind::Quota, LimitKind::Capacity)
        }
        hashbrown::ControlError::Reserve(hashbrown::TryReserveError::AllocError { .. }) => {
            Error::new(FailureKind::Allocation, LimitKind::Capacity)
        }
    })
}

pub(super) enum SvgCache {
    Plain(HashMap<(fontdb::ID, GlyphId), Option<Node>>),
    Paid(hashbrown::HashMap<(fontdb::ID, GlyphId), Option<Node>, RandomState, BucketAllocator>),
}
impl SvgCache {
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Plain(m) => m.len(),
            Self::Paid(m) => m.len(),
        }
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub(super) fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            None => Self::Plain(HashMap::new()),
            Some(r) => Self::Paid(hashbrown::HashMap::with_hasher_in(
                RandomState::new(),
                BucketAllocator::new(r.clone(), ResourceKind::Scratch),
            )),
        }
    }
    pub(super) fn get<'a>(
        &'a self,
        key: &(fontdb::ID, GlyphId),
        p: &ProcessingContext,
    ) -> Result<Option<&'a Option<Node>>> {
        owner::check(p, 1)?;
        let value = match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.get(key)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                map.try_get_with_control(
                    map.hasher().hash_one(*key),
                    key,
                    &mut RequestControl(r),
                    |a, b, c| {
                        c.checkpoint(1)?;
                        Ok(a == b)
                    },
                )?
            }
        };
        if let Some(v) = value {
            v.validate_policy(p)?;
        }
        Ok(value)
    }
    pub(super) fn insert(
        &mut self,
        key: (fontdb::ID, GlyphId),
        value: Option<Node>,
        p: &ProcessingContext,
    ) -> Result<()> {
        owner::check(p, 1)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                let old = map.insert(key, value);
                let _ = old;
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let builder = map.hasher().clone();
                let hash = builder.hash_one(key);
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(builder.hash_one(*key))
                })
                .map_err(|e| map_error(p, e))?;
                let old = map
                    .try_insert_with_control(
                        hash,
                        key,
                        value,
                        &mut RequestControl(&r),
                        |a, b, c| {
                            c.checkpoint(1)?;
                            Ok(a == b)
                        },
                        |key, c| {
                            c.checkpoint(1)?;
                            Ok(builder.hash_one(*key))
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                let _ = old;
                owner::check(p, 0)
            }
        }
    }
}

pub(super) enum RasterCache {
    Plain(HashMap<(fontdb::ID, GlyphId), Option<BitmapImage>>),
    Paid(
        hashbrown::HashMap<
            (fontdb::ID, GlyphId),
            Option<BitmapImage>,
            RandomState,
            BucketAllocator,
        >,
    ),
}
impl RasterCache {
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Plain(m) => m.len(),
            Self::Paid(m) => m.len(),
        }
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub(super) fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            None => Self::Plain(HashMap::new()),
            Some(r) => Self::Paid(hashbrown::HashMap::with_hasher_in(
                RandomState::new(),
                BucketAllocator::new(r.clone(), ResourceKind::Scratch),
            )),
        }
    }
    pub(super) fn get<'a>(
        &'a self,
        key: &(fontdb::ID, GlyphId),
        p: &ProcessingContext,
    ) -> Result<Option<&'a Option<BitmapImage>>> {
        owner::check(p, 1)?;
        let value = match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.get(key)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                map.try_get_with_control(
                    map.hasher().hash_one(*key),
                    key,
                    &mut RequestControl(r),
                    |a, b, c| {
                        c.checkpoint(1)?;
                        Ok(a == b)
                    },
                )?
            }
        };
        if let Some(v) = value {
            v.validate_policy(p)?;
        }
        Ok(value)
    }
    pub(super) fn insert(
        &mut self,
        key: (fontdb::ID, GlyphId),
        value: Option<BitmapImage>,
        p: &ProcessingContext,
    ) -> Result<()> {
        owner::check(p, 1)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                let old = map.insert(key, value);
                let _ = old;
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let builder = map.hasher().clone();
                let hash = builder.hash_one(key);
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(builder.hash_one(*key))
                })
                .map_err(|e| map_error(p, e))?;
                let old = map
                    .try_insert_with_control(
                        hash,
                        key,
                        value,
                        &mut RequestControl(&r),
                        |a, b, c| {
                            c.checkpoint(1)?;
                            Ok(a == b)
                        },
                        |key, c| {
                            c.checkpoint(1)?;
                            Ok(builder.hash_one(*key))
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                let _ = old;
                owner::check(p, 0)
            }
        }
    }
}

pub(super) enum OpszCache {
    Plain(HashMap<fontdb::ID, bool>),
    Paid(hashbrown::HashMap<fontdb::ID, bool, RandomState, BucketAllocator>),
}
impl OpszCache {
    pub(super) fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            None => Self::Plain(HashMap::new()),
            Some(r) => Self::Paid(hashbrown::HashMap::with_hasher_in(
                RandomState::new(),
                BucketAllocator::new(r.clone(), ResourceKind::Scratch),
            )),
        }
    }
    pub(super) fn get<'a>(
        &'a self,
        key: &fontdb::ID,
        p: &ProcessingContext,
    ) -> Result<Option<&'a bool>> {
        owner::check(p, 1)?;
        let value = match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.get(key)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                map.try_get_with_control(
                    map.hasher().hash_one(*key),
                    key,
                    &mut RequestControl(r),
                    |a, b, c| {
                        c.checkpoint(1)?;
                        Ok(a == b)
                    },
                )?
            }
        };
        if let Some(v) = value {
            v.validate_policy(p)?;
        }
        Ok(value)
    }
    pub(super) fn insert(
        &mut self,
        key: fontdb::ID,
        value: bool,
        p: &ProcessingContext,
    ) -> Result<()> {
        owner::check(p, 1)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                let old = map.insert(key, value);
                let _ = old;
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let builder = map.hasher().clone();
                let hash = builder.hash_one(key);
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(builder.hash_one(*key))
                })
                .map_err(|e| map_error(p, e))?;
                let old = map
                    .try_insert_with_control(
                        hash,
                        key,
                        value,
                        &mut RequestControl(&r),
                        |a, b, c| {
                            c.checkpoint(1)?;
                            Ok(a == b)
                        },
                        |key, c| {
                            c.checkpoint(1)?;
                            Ok(builder.hash_one(*key))
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                let _ = old;
                owner::check(p, 0)
            }
        }
    }
}

type PlainColrKey = (fontdb::ID, GlyphId, Vec<FontVariation>, [u8; 4]);
pub(super) struct ColrKey {
    font: fontdb::ID,
    glyph: GlyphId,
    axes: FrozenList<FontVariation>,
    color: [u8; 4],
    hash: u64,
}
pub(super) enum ColrQuery<'a> {
    Plain(PlainColrKey),
    Paid {
        font: fontdb::ID,
        glyph: GlyphId,
        axes: &'a [FontVariation],
        color: [u8; 4],
        hash: u64,
    },
}
pub(super) enum ColrCache {
    Plain(HashMap<PlainColrKey, Option<Tree>>),
    Paid(hashbrown::HashMap<ColrKey, Option<Tree>, RandomState, BucketAllocator>),
}
fn axes_equal<C: WorkControl>(
    a: &[FontVariation],
    b: &[FontVariation],
    c: &mut C,
) -> std::result::Result<bool, C::Error> {
    c.checkpoint(1)?;
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.iter().zip(b) {
        c.checkpoint(1)?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}
fn colr_hash(
    font: fontdb::ID,
    glyph: GlyphId,
    axes: &[FontVariation],
    color: [u8; 4],
    builder: &RandomState,
    r: &ResourcesRef,
) -> Result<u64> {
    r.checkpoint(1)?;
    let mut hash = builder.build_hasher();
    font.hash(&mut hash);
    glyph.hash(&mut hash);
    hash.write_usize(axes.len());
    for axis in axes {
        r.checkpoint(1)?;
        axis.hash(&mut hash);
    }
    r.checkpoint(1)?;
    color.hash(&mut hash);
    Ok(hash.finish())
}
impl ColrCache {
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Plain(m) => m.len(),
            Self::Paid(m) => m.len(),
        }
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub(super) fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            None => Self::Plain(HashMap::new()),
            Some(r) => Self::Paid(hashbrown::HashMap::with_hasher_in(
                RandomState::new(),
                BucketAllocator::new(r.clone(), ResourceKind::Scratch),
            )),
        }
    }
    pub(super) fn get<'a, 'axes>(
        &'a self,
        font: fontdb::ID,
        glyph: GlyphId,
        axes: &'axes [FontVariation],
        color: [u8; 4],
        p: &ProcessingContext,
    ) -> Result<(ColrQuery<'axes>, Option<&'a Option<Tree>>)> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                let key = (font, glyph, axes.to_vec(), color);
                let value = map.get(&key);
                Ok((ColrQuery::Plain(key), value))
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                let hash = colr_hash(font, glyph, axes, color, map.hasher(), r)?;
                let value = map.try_get_with_control(
                    hash,
                    &(font, glyph, axes, color),
                    &mut RequestControl(r),
                    |q, k, c| {
                        k.axes.validate_policy(Some(r))?;
                        Ok(q.0 == k.font
                            && q.1 == k.glyph
                            && q.3 == k.color
                            && axes_equal(q.2, &k.axes, c)?)
                    },
                )?;
                if let Some(v) = value {
                    v.validate_policy(p)?;
                }
                Ok((
                    ColrQuery::Paid {
                        font,
                        glyph,
                        axes,
                        color,
                        hash,
                    },
                    value,
                ))
            }
        }
    }
    pub(super) fn insert(
        &mut self,
        key: ColrQuery<'_>,
        value: Option<Tree>,
        p: &ProcessingContext,
    ) -> Result<()> {
        owner::check(p, 0)?;
        value.validate_policy(p)?;
        match (self, key) {
            (Self::Plain(map), ColrQuery::Plain(key)) => {
                owner::validate_owner(p, None)?;
                drop(map.insert(key, value));
                owner::check(p, 0)
            }
            (
                Self::Paid(map),
                ColrQuery::Paid {
                    font,
                    glyph,
                    axes,
                    color,
                    hash,
                },
            ) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    key.axes.validate_policy(Some(&r))?;
                    c.checkpoint(1)?;
                    Ok(key.hash)
                })
                .map_err(|e| map_error(p, e))?;
                let mut copied = BudgetVec::new(r.clone(), ResourceKind::Scratch);
                copied.try_reserve_exact(axes.len())?;
                for chunk in axes.chunks(256) {
                    r.checkpoint(chunk.len() as u64)?;
                    copied.extend_reserved_copy(chunk);
                }
                let axes = FrozenList::try_freeze(&r, copied)?;
                let key = ColrKey {
                    font,
                    glyph,
                    axes,
                    color,
                    hash,
                };
                let old = map
                    .try_insert_with_control(
                        hash,
                        key,
                        value,
                        &mut RequestControl(&r),
                        |a, b, c| {
                            a.axes.validate_policy(Some(&r))?;
                            b.axes.validate_policy(Some(&r))?;
                            Ok(a.font == b.font
                                && a.glyph == b.glyph
                                && a.color == b.color
                                && axes_equal(&a.axes, &b.axes, c)?)
                        },
                        |key, c| {
                            key.axes.validate_policy(Some(&r))?;
                            c.checkpoint(1)?;
                            Ok(key.hash)
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                let _ = old;
                owner::check(p, 0)
            }
            (Self::Paid(map), _) => Err(owner::reject(p, Some(map.allocator().resources()))),
            (Self::Plain(_), _) => Err(owner::reject(p, None)),
        }
    }
}
