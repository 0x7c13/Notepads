// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Concrete parser resource caches. Queries borrow XML IDs; only insertion
//! creates a request-owned key. The map struct itself remains inline in Cache.
use super::reference::Checked;
use crate::tree::owner::{self, PolicyValue, Result, TreeString};
use crate::{ClipPath, Mask, Paint, ProcessingContext, SharedOwner, filter};
use markdown_svg_budget::{
    BucketAllocator, Error, FailureKind, LimitKind, RequestControl, ResourceKind, ResourcesRef,
    WorkControl,
};
use std::collections::{HashMap, HashSet, hash_map::RandomState};
use std::hash::{BuildHasher, Hasher};
#[derive(Debug)]
pub(super) struct ResourceKey {
    text: TreeString,
    hash: u64,
}
fn string_hash(text: &str, hasher: &RandomState, r: &ResourcesRef) -> Result<u64> {
    r.checkpoint(0)?;
    let mut h = hasher.build_hasher();
    // RandomState's fixed-toolchain SipHasher consumes the same byte stream as str::hash.
    for bytes in text.as_bytes().chunks(256) {
        r.checkpoint(bytes.len() as u64)?;
        h.write(bytes);
    }
    r.checkpoint(1)?;
    h.write_u8(0xff);
    Ok(h.finish())
}
fn equal<C: WorkControl>(a: &str, b: &str, c: &mut C) -> std::result::Result<bool, C::Error> {
    c.checkpoint(1)?;
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.as_bytes().chunks(256).zip(b.as_bytes().chunks(256)) {
        c.checkpoint(a.len() as u64)?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}
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

pub(super) enum ClipCache {
    Plain(HashMap<String, SharedOwner<ClipPath>>),
    Paid(
        hashbrown::HashMap<
            ResourceKey,
            Checked<SharedOwner<ClipPath>>,
            RandomState,
            BucketAllocator,
        >,
    ),
}
impl ClipCache {
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
        id: &str,
        p: &ProcessingContext,
    ) -> Result<Option<(&'a SharedOwner<ClipPath>, u8)>> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                Ok(map.get(id).map(|v| (v, 0)))
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                let hash = string_hash(id, map.hasher(), r)?;
                let result =
                    map.try_get_with_control(hash, id, &mut RequestControl(r), |query, key, c| {
                        equal(query, &key.text, c)
                    })?;
                if let Some(value) = result {
                    value.value.validate_policy(p)?;
                }
                Ok(result.map(|v| (&v.value, v.height)))
            }
        }
    }
    pub(super) fn contains_key(&self, id: &str, p: &ProcessingContext) -> Result<bool> {
        self.get(id, p).map(|v| v.is_some())
    }
    pub(super) fn insert(
        &mut self,
        id: &str,
        value: SharedOwner<ClipPath>,
        p: &ProcessingContext,
        height: u8,
    ) -> Result<()> {
        owner::check(p, 0)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                drop(map.insert(id.to_owned(), value));
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let hash = string_hash(id, map.hasher(), &r)?;
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(key.hash)
                })
                .map_err(|e| map_error(p, e))?;
                let key = ResourceKey {
                    text: TreeString::try_from_str(id, p)?,
                    hash,
                };
                let previous = map
                    .try_insert_with_control(
                        hash,
                        key,
                        Checked { value, height },
                        &mut RequestControl(&r),
                        |a, b, c| equal(&a.text, &b.text, c),
                        |key, c| {
                            c.checkpoint(1)?;
                            Ok(key.hash)
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                drop(previous);
                owner::check(p, 0)
            }
        }
    }
    pub(super) fn clear(&mut self, p: &ProcessingContext) -> Result<()> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.clear();
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                // Mandatory cleanup always retires every existing owner; never skip cleanup
                // on a sticky fault. No subsequent producer runs before the tail check.
                let old = std::mem::replace(
                    map,
                    hashbrown::HashMap::with_hasher_in(
                        map.hasher().clone(),
                        BucketAllocator::new(r, ResourceKind::Scratch),
                    ),
                );
                drop(old);
            }
        }
        owner::check(p, 0)
    }
}

pub(super) enum MaskCache {
    Plain(HashMap<String, SharedOwner<Mask>>),
    Paid(hashbrown::HashMap<ResourceKey, Checked<SharedOwner<Mask>>, RandomState, BucketAllocator>),
}
impl MaskCache {
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
        id: &str,
        p: &ProcessingContext,
    ) -> Result<Option<(&'a SharedOwner<Mask>, u8)>> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                Ok(map.get(id).map(|v| (v, 0)))
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                let hash = string_hash(id, map.hasher(), r)?;
                let result =
                    map.try_get_with_control(hash, id, &mut RequestControl(r), |query, key, c| {
                        equal(query, &key.text, c)
                    })?;
                if let Some(value) = result {
                    value.value.validate_policy(p)?;
                }
                Ok(result.map(|v| (&v.value, v.height)))
            }
        }
    }
    pub(super) fn contains_key(&self, id: &str, p: &ProcessingContext) -> Result<bool> {
        self.get(id, p).map(|v| v.is_some())
    }
    pub(super) fn insert(
        &mut self,
        id: &str,
        value: SharedOwner<Mask>,
        p: &ProcessingContext,
        height: u8,
    ) -> Result<()> {
        owner::check(p, 0)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                drop(map.insert(id.to_owned(), value));
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let hash = string_hash(id, map.hasher(), &r)?;
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(key.hash)
                })
                .map_err(|e| map_error(p, e))?;
                let key = ResourceKey {
                    text: TreeString::try_from_str(id, p)?,
                    hash,
                };
                let previous = map
                    .try_insert_with_control(
                        hash,
                        key,
                        Checked { value, height },
                        &mut RequestControl(&r),
                        |a, b, c| equal(&a.text, &b.text, c),
                        |key, c| {
                            c.checkpoint(1)?;
                            Ok(key.hash)
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                drop(previous);
                owner::check(p, 0)
            }
        }
    }
    pub(super) fn clear(&mut self, p: &ProcessingContext) -> Result<()> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.clear();
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                // Mandatory cleanup always retires every existing owner; never skip cleanup
                // on a sticky fault. No subsequent producer runs before the tail check.
                let old = std::mem::replace(
                    map,
                    hashbrown::HashMap::with_hasher_in(
                        map.hasher().clone(),
                        BucketAllocator::new(r, ResourceKind::Scratch),
                    ),
                );
                drop(old);
            }
        }
        owner::check(p, 0)
    }
}

pub(super) enum FilterCache {
    Plain(HashMap<String, SharedOwner<filter::Filter>>),
    Paid(
        hashbrown::HashMap<
            ResourceKey,
            Checked<SharedOwner<filter::Filter>>,
            RandomState,
            BucketAllocator,
        >,
    ),
}
impl FilterCache {
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
        id: &str,
        p: &ProcessingContext,
    ) -> Result<Option<(&'a SharedOwner<filter::Filter>, u8)>> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                Ok(map.get(id).map(|v| (v, 0)))
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                let hash = string_hash(id, map.hasher(), r)?;
                let result =
                    map.try_get_with_control(hash, id, &mut RequestControl(r), |query, key, c| {
                        equal(query, &key.text, c)
                    })?;
                if let Some(value) = result {
                    value.value.validate_policy(p)?;
                }
                Ok(result.map(|v| (&v.value, v.height)))
            }
        }
    }
    pub(super) fn contains_key(&self, id: &str, p: &ProcessingContext) -> Result<bool> {
        self.get(id, p).map(|v| v.is_some())
    }
    pub(super) fn insert(
        &mut self,
        id: &str,
        value: SharedOwner<filter::Filter>,
        p: &ProcessingContext,
        height: u8,
    ) -> Result<()> {
        owner::check(p, 0)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                drop(map.insert(id.to_owned(), value));
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let hash = string_hash(id, map.hasher(), &r)?;
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(key.hash)
                })
                .map_err(|e| map_error(p, e))?;
                let key = ResourceKey {
                    text: TreeString::try_from_str(id, p)?,
                    hash,
                };
                let previous = map
                    .try_insert_with_control(
                        hash,
                        key,
                        Checked { value, height },
                        &mut RequestControl(&r),
                        |a, b, c| equal(&a.text, &b.text, c),
                        |key, c| {
                            c.checkpoint(1)?;
                            Ok(key.hash)
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                drop(previous);
                owner::check(p, 0)
            }
        }
    }
    pub(super) fn clear(&mut self, p: &ProcessingContext) -> Result<()> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.clear();
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                // Mandatory cleanup always retires every existing owner; never skip cleanup
                // on a sticky fault. No subsequent producer runs before the tail check.
                let old = std::mem::replace(
                    map,
                    hashbrown::HashMap::with_hasher_in(
                        map.hasher().clone(),
                        BucketAllocator::new(r, ResourceKind::Scratch),
                    ),
                );
                drop(old);
            }
        }
        owner::check(p, 0)
    }
}

pub(super) enum PaintCache {
    Plain(HashMap<String, Paint>),
    Paid(hashbrown::HashMap<ResourceKey, Checked<Paint>, RandomState, BucketAllocator>),
}
impl PaintCache {
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
        id: &str,
        p: &ProcessingContext,
    ) -> Result<Option<(&'a Paint, u8)>> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                Ok(map.get(id).map(|v| (v, 0)))
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                let hash = string_hash(id, map.hasher(), r)?;
                let result =
                    map.try_get_with_control(hash, id, &mut RequestControl(r), |query, key, c| {
                        equal(query, &key.text, c)
                    })?;
                if let Some(value) = result {
                    value.value.validate_policy(p)?;
                }
                Ok(result.map(|v| (&v.value, v.height)))
            }
        }
    }
    pub(super) fn insert(
        &mut self,
        id: &str,
        value: Paint,
        p: &ProcessingContext,
        height: u8,
    ) -> Result<()> {
        owner::check(p, 0)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                drop(map.insert(id.to_owned(), value));
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let hash = string_hash(id, map.hasher(), &r)?;
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(key.hash)
                })
                .map_err(|e| map_error(p, e))?;
                let key = ResourceKey {
                    text: TreeString::try_from_str(id, p)?,
                    hash,
                };
                let previous = map
                    .try_insert_with_control(
                        hash,
                        key,
                        Checked { value, height },
                        &mut RequestControl(&r),
                        |a, b, c| equal(&a.text, &b.text, c),
                        |key, c| {
                            c.checkpoint(1)?;
                            Ok(key.hash)
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                drop(previous);
                owner::check(p, 0)
            }
        }
    }
    pub(super) fn clear(&mut self, p: &ProcessingContext) -> Result<()> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.clear();
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                // Mandatory cleanup always retires every existing owner; never skip cleanup
                // on a sticky fault. No subsequent producer runs before the tail check.
                let old = std::mem::replace(
                    map,
                    hashbrown::HashMap::with_hasher_in(
                        map.hasher().clone(),
                        BucketAllocator::new(r, ResourceKind::Scratch),
                    ),
                );
                drop(old);
            }
        }
        owner::check(p, 0)
    }
}
pub(super) enum FilterResultCache {
    Plain(HashMap<String, ()>),
    Paid(hashbrown::HashMap<ResourceKey, (), RandomState, BucketAllocator>),
}
impl FilterResultCache {
    pub(super) fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            None => Self::Plain(HashMap::new()),
            Some(r) => Self::Paid(hashbrown::HashMap::with_hasher_in(
                RandomState::new(),
                BucketAllocator::new(r.clone(), ResourceKind::Scratch),
            )),
        }
    }
    pub(super) fn get<'a>(&'a self, id: &str, p: &ProcessingContext) -> Result<Option<&'a ()>> {
        owner::check(p, 0)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                Ok(map.get(id))
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                let hash = string_hash(id, map.hasher(), r)?;
                let result =
                    map.try_get_with_control(hash, id, &mut RequestControl(r), |query, key, c| {
                        equal(query, &key.text, c)
                    })?;
                if let Some(value) = result {
                    value.validate_policy(p)?;
                }
                Ok(result)
            }
        }
    }
    pub(super) fn contains_key(&self, id: &str, p: &ProcessingContext) -> Result<bool> {
        self.get(id, p).map(|v| v.is_some())
    }
    pub(super) fn insert(&mut self, id: &str, value: (), p: &ProcessingContext) -> Result<()> {
        owner::check(p, 0)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                let _ = map.insert(id.to_owned(), value);
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let hash = string_hash(id, map.hasher(), &r)?;
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(key.hash)
                })
                .map_err(|e| map_error(p, e))?;
                let key = ResourceKey {
                    text: TreeString::try_from_str(id, p)?,
                    hash,
                };
                let previous = map
                    .try_insert_with_control(
                        hash,
                        key,
                        value,
                        &mut RequestControl(&r),
                        |a, b, c| equal(&a.text, &b.text, c),
                        |key, c| {
                            c.checkpoint(1)?;
                            Ok(key.hash)
                        },
                    )
                    .map_err(|e| map_error(p, e))?;
                let _ = previous;
                owner::check(p, 0)
            }
        }
    }
}

pub(super) enum IdSet {
    Plain(HashSet<u64>),
    Paid(hashbrown::HashMap<u64, (), RandomState, BucketAllocator>),
}
impl IdSet {
    pub(super) fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            None => Self::Plain(HashSet::new()),
            Some(r) => Self::Paid(hashbrown::HashMap::with_hasher_in(
                RandomState::new(),
                BucketAllocator::new(r.clone(), ResourceKind::Scratch),
            )),
        }
    }
    pub(super) fn contains(&self, key: u64, p: &ProcessingContext) -> Result<bool> {
        owner::check(p, 1)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                Ok(map.contains(&key))
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                let hash = map.hasher().hash_one(key);
                map.try_get_with_control(hash, &key, &mut RequestControl(r), |a, b, c| {
                    c.checkpoint(1)?;
                    Ok(a == b)
                })
                .map(|v| v.is_some())
            }
        }
    }
    pub(super) fn insert(&mut self, key: u64, p: &ProcessingContext) -> Result<()> {
        owner::check(p, 1)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.insert(key);
                Ok(())
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
                map.try_insert_with_control(
                    hash,
                    key,
                    (),
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
                owner::check(p, 0)
            }
        }
    }
}

#[cfg(feature = "text")]
pub(super) enum DecorationCache {
    Plain(HashMap<usize, SharedOwner<crate::tree::DecorationOwnerStyle>>),
    Paid(
        hashbrown::HashMap<
            usize,
            SharedOwner<crate::tree::DecorationOwnerStyle>,
            RandomState,
            BucketAllocator,
        >,
    ),
}
#[cfg(feature = "text")]
impl DecorationCache {
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
        key: usize,
        p: &ProcessingContext,
    ) -> Result<Option<&'a SharedOwner<crate::tree::DecorationOwnerStyle>>> {
        owner::check(p, 1)?;
        let value = match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                map.get(&key)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources();
                owner::validate_owner(p, Some(r))?;
                map.try_get_with_control(
                    map.hasher().hash_one(key),
                    &key,
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
        key: usize,
        value: SharedOwner<crate::tree::DecorationOwnerStyle>,
        p: &ProcessingContext,
    ) -> Result<()> {
        owner::check(p, 1)?;
        value.validate_policy(p)?;
        match self {
            Self::Plain(map) => {
                owner::validate_owner(p, None)?;
                drop(map.insert(key, value));
                owner::check(p, 0)
            }
            Self::Paid(map) => {
                let r = map.allocator().resources().clone();
                owner::validate_owner(p, Some(&r))?;
                let hash = map.hasher().hash_one(key);
                let builder = map.hasher().clone();
                map.try_reserve_with_control(1, &mut RequestControl(&r), |key, c| {
                    c.checkpoint(1)?;
                    Ok(builder.hash_one(*key))
                })
                .map_err(|e| map_error(p, e))?;
                let previous = map
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
                drop(previous);
                owner::check(p, 0)
            }
        }
    }
}
