// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Append-only outline cache. Checked clones share the table until a write;
//! queries borrow axes, and every table/key/path allocation has its own owner.
use std::{
    collections::HashMap,
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash, Hasher},
};

use crate::{FontVariation, GlyphId, geometry::RequestControl};
use markdown_svg_budget::{
    BucketAllocator, BudgetVec, Error, FailureKind, FrozenList, LimitKind, PaidShared,
    ResourceKind, ResourcesRef, WorkControl,
};

type CompatKey = (fontdb::ID, GlyphId, Vec<FontVariation>);
type Outline = Option<tiny_skia_path::Path>;
type Table = hashbrown::HashMap<Key, Outline, RandomState, BucketAllocator>;

#[derive(Clone)]
pub(crate) struct Key {
    font: fontdb::ID,
    glyph: GlyphId,
    axes: FrozenList<FontVariation>,
    hash: u64,
}

pub(crate) enum Query<'a> {
    Compat(CompatKey),
    Paid {
        font: fontdb::ID,
        glyph: GlyphId,
        axes: &'a [FontVariation],
        hash: u64,
    },
}

#[derive(Clone)]
pub(crate) enum OutlineCache {
    Compat(HashMap<CompatKey, Outline>),
    Paid {
        resources: ResourcesRef,
        hasher: RandomState,
        table: Option<PaidShared<Table>>,
    },
}

impl OutlineCache {
    pub(crate) fn new(resources: Option<&ResourcesRef>) -> Self {
        match resources {
            None => Self::Compat(HashMap::new()),
            Some(resources) => Self::Paid {
                resources: resources.clone(),
                hasher: RandomState::new(),
                table: None,
            },
        }
    }

    /// Some(None) is a cached absent outline; None is a cache miss.
    pub(crate) fn lookup<'a>(
        &self,
        font: fontdb::ID,
        glyph: GlyphId,
        axes: &'a [FontVariation],
    ) -> Result<(Query<'a>, Option<Outline>), Error> {
        match self {
            Self::Compat(table) => {
                let key = (font, glyph, axes.to_vec());
                let value = table.get(&key).cloned();
                Ok((Query::Compat(key), value))
            }
            Self::Paid {
                resources,
                hasher,
                table,
            } => {
                resources.checkpoint(0)?;
                let hash = key_hash(resources, hasher, font, glyph, axes)?;
                let query = Query::Paid {
                    font,
                    glyph,
                    axes,
                    hash,
                };
                let value = match table {
                    None => None,
                    Some(table) => table
                        .try_get_with_control(
                            hash,
                            &(font, glyph, axes),
                            &mut RequestControl(resources),
                            |query, key, cx| {
                                Ok(query.0 == key.font
                                    && query.1 == key.glyph
                                    && axes_equal(query.2, &key.axes, cx)?)
                            },
                        )?
                        .cloned(),
                };
                resources.checkpoint(0)?;
                Ok((query, value))
            }
        }
    }

    pub(crate) fn insert(&mut self, query: Query<'_>, value: Outline) -> Result<(), Error> {
        match (self, query) {
            (Self::Compat(table), Query::Compat(key)) => {
                table.insert(key, value);
                Ok(())
            }
            (
                Self::Paid {
                    resources,
                    hasher,
                    table,
                },
                Query::Paid {
                    font,
                    glyph,
                    axes,
                    hash,
                },
            ) => {
                let resources = resources.clone();
                let result = (|| {
                    resources.checkpoint(0)?;
                    let axes = copy_axes(&resources, axes)?;
                    // A caller may import ordinary outline geometry, but never
                    // retain an unpaid vector under a paid table/header.
                    let value = value
                        .map(|path| {
                            let copied = path.try_copy_in(&resources, ResourceKind::Scratch);
                            drop(path);
                            resources.checkpoint(0)?;
                            copied
                        })
                        .transpose()?;
                    make_unique(table, &resources, hasher)?;
                    let map = table
                        .as_mut()
                        .expect("initialized paid table")
                        .get_mut_if_unique()
                        .expect("unique paid table");
                    let previous = map
                        .try_insert_with_control(
                            hash,
                            Key {
                                font,
                                glyph,
                                axes,
                                hash,
                            },
                            value,
                            &mut RequestControl(&resources),
                            keys_equal,
                            existing_hash,
                        )
                        .map_err(|error| table_error(&resources, error))?;
                    drop(previous);
                    resources.checkpoint(0)
                })();
                // A failed temporary/key/COW cleanup can be the first failure.
                resources.checkpoint(0)?;
                result
            }
            (Self::Paid { resources, .. }, _) => {
                Err(resources.record(Error::new(FailureKind::Invariant, LimitKind::None)))
            }
            (Self::Compat(_), _) => Err(Error::new(FailureKind::Invariant, LimitKind::None)),
        }
    }
}

fn table_error(resources: &ResourcesRef, error: hashbrown::ControlError<Error>) -> Error {
    if let Some(first) = resources.failure() {
        return first;
    }
    resources.record(match error {
        hashbrown::ControlError::Control(error) => error,
        hashbrown::ControlError::Reserve(hashbrown::TryReserveError::CapacityOverflow) => {
            Error::new(FailureKind::Quota, LimitKind::Capacity)
        }
        hashbrown::ControlError::Reserve(hashbrown::TryReserveError::AllocError { .. }) => {
            Error::new(FailureKind::Allocation, LimitKind::Capacity)
        }
    })
}

fn copy_axes(
    resources: &ResourcesRef,
    axes: &[FontVariation],
) -> Result<FrozenList<FontVariation>, Error> {
    let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    values.try_reserve_exact(axes.len())?;
    for chunk in axes.chunks(256) {
        resources.checkpoint(chunk.len() as u64)?;
        values.extend_reserved_copy(chunk);
    }
    FrozenList::try_freeze(resources, values)
}

fn axes_equal<C: WorkControl>(
    a: &[FontVariation],
    b: &[FontVariation],
    cx: &mut C,
) -> Result<bool, C::Error> {
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.iter().zip(b) {
        cx.checkpoint(1)?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}

fn keys_equal<C: WorkControl>(a: &Key, b: &Key, cx: &mut C) -> Result<bool, C::Error> {
    Ok(a.font == b.font && a.glyph == b.glyph && axes_equal(&a.axes, &b.axes, cx)?)
}

fn existing_hash<C: WorkControl>(key: &Key, cx: &mut C) -> Result<u64, C::Error> {
    cx.checkpoint(1)?;
    Ok(key.hash)
}

fn key_hash(
    resources: &ResourcesRef,
    builder: &RandomState,
    font: fontdb::ID,
    glyph: GlyphId,
    axes: &[FontVariation],
) -> Result<u64, Error> {
    resources.checkpoint(1)?;
    let mut state = builder.build_hasher();
    font.hash(&mut state);
    glyph.hash(&mut state);
    // Fixed Rust 1.99.1 slice Hash uses write_length_prefix, whose unchanged
    // SipHasher/default implementation is write_usize. FontVariation's default
    // hash_slice then hashes each tag/value bits in order. No second hashing
    // algorithm/seed or unchecked input-sized Hash call is introduced here.
    state.write_usize(axes.len());
    for value in axes {
        resources.checkpoint(1)?;
        value.hash(&mut state);
    }
    resources.checkpoint(0)?;
    Ok(state.finish())
}

fn make_unique(
    table: &mut Option<PaidShared<Table>>,
    resources: &ResourcesRef,
    hasher: &RandomState,
) -> Result<(), Error> {
    if let Some(table) = table.as_mut() {
        if table.get_mut_if_unique().is_some() {
            return resources.checkpoint(0);
        }
    }
    let next = PaidShared::try_new_with(resources, ResourceKind::Scratch, || {
        let mut next = Table::with_hasher_in(
            hasher.clone(),
            BucketAllocator::new(resources.clone(), ResourceKind::Scratch),
        );
        if let Some(source) = table.as_ref() {
            let mut cx = RequestControl(resources);
            next.try_reserve_with_control(source.len(), &mut cx, existing_hash)
                .map_err(|error| table_error(resources, error))?;
            source.try_for_each_with_control(&mut cx, |key, value, cx| {
                let previous = next
                    .try_insert_with_control(
                        key.hash,
                        key.clone(),
                        value.clone(),
                        cx,
                        keys_equal,
                        existing_hash,
                    )
                    .map_err(|error| table_error(resources, error))?;
                drop(previous);
                cx.checkpoint(0)
            })?;
        }
        resources.checkpoint(0)?;
        Ok(next)
    })?;
    let old = table.replace(next);
    drop(old);
    resources.checkpoint(0)
}

#[cfg(test)]
#[path = "outline_cache_tests.rs"]
mod tests;
