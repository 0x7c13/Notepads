// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private request-bound scene storage. No paid growable container escapes.

use crate::ProcessingContext;
use allocator_api2::boxed::Box as AllocBox;
use markdown_svg_budget::{
    BucketAllocator, BudgetVec, Error, FailureKind, LimitKind, PaidUtf8, ResourceKind, ResourcesRef,
};
use std::fmt;
use std::ops::{Deref, DerefMut};

pub(crate) type Result<T> = std::result::Result<T, Error>;

pub(crate) fn check(p: &ProcessingContext, work: u64) -> Result<()> {
    if let Some(error) = p.resource_failure() {
        return Err(error);
    }
    match p.resources() {
        Some(r) => r.checkpoint(work).map_err(|error| p.record_resource(error)),
        None => Ok(()),
    }
}

/// Preserve each real slot. No new account, error string or global ordering.
pub(crate) fn reject(p: &ProcessingContext, source: Option<&ResourcesRef>) -> Error {
    let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
    let existing = p
        .resource_failure()
        .or_else(|| source.and_then(ResourcesRef::failure));
    let result = p.record_resource(existing.unwrap_or(invariant));
    if let Some(source) = source {
        source.record(invariant);
    }
    result
}

pub(crate) fn finish<T>(p: &ProcessingContext, result: Result<T>) -> Result<T> {
    match result {
        Err(error) => Err(p.record_resource(error)),
        Ok(value) => {
            check(p, 0)?;
            Ok(value)
        }
    }
}

pub(crate) fn validate_owner(p: &ProcessingContext, source: Option<&ResourcesRef>) -> Result<()> {
    match (p.resources(), source) {
        (None, None) => check(p, 0),
        (Some(destination), Some(source)) if destination.is_same_request(source) => check(p, 0),
        _ => Err(reject(p, source)),
    }
}

/// Provenance for inline values, separately from every real child allocation.
#[derive(Debug)]
pub(crate) enum Origin {
    Plain,
    Request(ResourcesRef),
}

impl Origin {
    pub(crate) fn source_resources(&self) -> Option<&ResourcesRef> {
        match self {
            Self::Plain => None,
            Self::Request(r) => Some(r),
        }
    }
    pub(crate) fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            None => Self::Plain,
            Some(r) => Self::Request(r.clone()),
        }
    }
    pub(crate) fn validate(&self, p: &ProcessingContext) -> Result<()> {
        validate_owner(
            p,
            match self {
                Self::Plain => None,
                Self::Request(r) => Some(r),
            },
        )
    }
    pub(crate) fn checkpoint(&self, work: u64) -> Result<()> {
        match self {
            Self::Plain => Ok(()),
            Self::Request(r) => r.checkpoint(work),
        }
    }
    pub(crate) fn try_copy(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate(p)?;
        Ok(Self::new(p))
    }
}

/// A payload validates its actual owners, not just its outer provenance.
pub(crate) trait PolicyValue {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()>;
}

/// Unique scene allocation. Paid boxes never implement infallible Clone.
pub struct SceneBox<T> {
    inner: BoxStorage<T>,
}

enum BoxStorage<T> {
    Plain(Box<T>),
    Paid(AllocBox<T, BucketAllocator>),
}

impl<T> SceneBox<T> {
    pub(crate) fn source_resources(&self) -> Option<&ResourcesRef> {
        match &self.inner {
            BoxStorage::Plain(_) => None,
            BoxStorage::Paid(v) => Some(AllocBox::allocator(v).resources()),
        }
    }
    pub(crate) fn ensure_nonzero(p: &ProcessingContext) -> Result<()> {
        if p.resources().is_some() && std::mem::size_of::<T>() == 0 {
            return Err(reject(p, None));
        }
        Ok(())
    }
    /// The caller performs scene admission after this safety preflight.
    pub(crate) fn preflight(p: &ProcessingContext) -> Result<()> {
        Self::ensure_nonzero(p)?;
        check(p, 0)?;
        Ok(())
    }

    /// Reserve the real header/payload before evaluating the value factory.
    pub(crate) fn try_new_with(
        p: &ProcessingContext,
        make: impl FnOnce() -> Result<T>,
    ) -> Result<Self>
    where
        T: PolicyValue,
    {
        Self::preflight(p)?;
        match p.resources() {
            None => {
                let value = make().map_err(|error| p.record_resource(error))?;
                value.validate_policy(p)?;
                finish(
                    p,
                    Ok(Self {
                        inner: BoxStorage::Plain(Box::new(value)),
                    }),
                )
            }
            Some(r) => {
                let slot = AllocBox::<T, BucketAllocator>::try_new_uninit_in(BucketAllocator::new(
                    r.clone(),
                    ResourceKind::Scratch,
                ))
                .map_err(|_| {
                    p.record_resource(
                        r.failure()
                            .unwrap_or_else(|| Error::new(FailureKind::Invariant, LimitKind::None)),
                    )
                })?;
                let value = make().map_err(|error| p.record_resource(error))?;
                value.validate_policy(p)?;
                check(p, 0)?;
                let value = Self {
                    inner: BoxStorage::Paid(AllocBox::write(slot, value)),
                };
                finish(p, Ok(value))
            }
        }
    }

    pub(crate) fn validate_header(&self, p: &ProcessingContext) -> Result<()> {
        validate_owner(
            p,
            match &self.inner {
                BoxStorage::Plain(_) => None,
                BoxStorage::Paid(value) => Some(AllocBox::allocator(value).resources()),
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn try_take(self, p: &ProcessingContext) -> Result<T>
    where
        T: PolicyValue,
    {
        self.validate_policy(p)?;
        let value = match self.inner {
            BoxStorage::Plain(value) => *value,
            BoxStorage::Paid(value) => AllocBox::into_inner(value),
        };
        finish(p, Ok(value))
    }
}

impl<T: PolicyValue> PolicyValue for SceneBox<T> {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        self.validate_header(p)?;
        self.deref().validate_policy(p)
    }
}
impl<T> Deref for SceneBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match &self.inner {
            BoxStorage::Plain(v) => v,
            BoxStorage::Paid(v) => v,
        }
    }
}
impl<T> SceneBox<T> {
    // Construction and mutation stay in the U implementation. Public callers
    // can only borrow immutable payloads; owner replacement is never a raw exit.
    pub(crate) fn as_mut(&mut self) -> &mut T {
        match &mut self.inner {
            BoxStorage::Plain(v) => v,
            BoxStorage::Paid(v) => v,
        }
    }
}
impl<T> AsRef<T> for SceneBox<T> {
    fn as_ref(&self) -> &T {
        self
    }
}
impl<T: fmt::Debug> fmt::Debug for SceneBox<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

pub(crate) enum TreeVec<T> {
    Plain(Vec<T>),
    Paid(BudgetVec<T>),
}

impl<T> TreeVec<T> {
    pub(crate) fn source_resources(&self) -> Option<&ResourcesRef> {
        match self {
            Self::Plain(_) => None,
            Self::Paid(v) => Some(v.resources()),
        }
    }
    pub(crate) fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            None => Self::Plain(Vec::new()),
            Some(r) => Self::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch)),
        }
    }
    #[cfg(test)]
    pub(crate) fn plain(values: Vec<T>) -> Self {
        Self::Plain(values)
    }
    pub(crate) fn validate_container(&self, p: &ProcessingContext) -> Result<()> {
        validate_owner(
            p,
            match self {
                Self::Plain(_) => None,
                Self::Paid(v) => Some(v.resources()),
            },
        )
    }
    pub(crate) fn try_reserve(&mut self, additional: usize, p: &ProcessingContext) -> Result<()> {
        self.validate_container(p)?;
        if p.resources().is_some() && std::mem::size_of::<T>() > 65536 {
            return Err(p.record_resource(Error::new(FailureKind::Quota, LimitKind::Capacity)));
        }
        match self {
            Self::Plain(v) => {
                v.reserve(additional);
                Ok(())
            }
            Self::Paid(v) => v
                .try_reserve(additional)
                .map_err(|error| p.record_resource(error)),
        }
    }
    pub(crate) fn push_reserved(&mut self, value: T) {
        match self {
            Self::Plain(v) => v.push(value),
            Self::Paid(v) => v.push_reserved(value),
        }
    }
    pub(crate) fn pop(&mut self) -> Option<T> {
        match self {
            Self::Plain(v) => v.pop(),
            Self::Paid(v) => v.pop(),
        }
    }
    pub(crate) fn try_with_capacity(capacity: usize, p: &ProcessingContext) -> Result<Self> {
        check(p, 0)?;
        if p.resources().is_some() && std::mem::size_of::<T>() > 65536 {
            return Err(p.record_resource(Error::new(FailureKind::Quota, LimitKind::Capacity)));
        }
        match p.resources() {
            None => Ok(Self::Plain(Vec::with_capacity(capacity))),
            Some(r) => {
                let mut values = BudgetVec::new(r.clone(), ResourceKind::Scratch);
                values
                    .try_reserve_exact(capacity)
                    .map_err(|e| p.record_resource(e))?;
                finish(p, Ok(Self::Paid(values)))
            }
        }
    }
    pub(crate) fn try_copy_with(
        &self,
        p: &ProcessingContext,
        copy: &mut impl FnMut(&T) -> Result<T>,
    ) -> Result<Self> {
        self.validate_container(p)?;
        let mut target = Self::try_with_capacity(self.len(), p)?;
        let candidate = (|| {
            for value in self.iter() {
                check(p, std::mem::size_of::<T>().max(1) as u64)?;
                let value = copy(value).map_err(|error| p.record_resource(error))?;
                check(p, 0)?;
                target.push_reserved(value);
            }
            Ok(())
        })();
        match finish(p, candidate) {
            Ok(()) => finish(p, Ok(target)),
            Err(error) => {
                drop(target);
                finish(p, Err(error))
            }
        }
    }
}

impl<T: PolicyValue> TreeVec<T> {
    pub(crate) fn try_push(&mut self, value: T, p: &ProcessingContext) -> Result<()> {
        self.validate_container(p)?;
        value.validate_policy(p)?;
        self.try_reserve(1, p)?;
        check(p, std::mem::size_of::<T>().max(1) as u64)?;
        self.push_reserved(value);
        check(p, 0)
    }
    pub(crate) fn try_clear(&mut self, p: &ProcessingContext) -> Result<()> {
        self.validate_policy(p)?;
        match self {
            Self::Plain(v) => v.clear(),
            Self::Paid(v) => v.clear(),
        }
        check(p, 0)
    }
    pub(crate) fn try_append(&mut self, source: &mut Self, p: &ProcessingContext) -> Result<()> {
        self.validate_policy(p)?;
        source.validate_policy(p)?;
        self.try_reserve(source.len(), p)?;
        let mut values = match source {
            Self::Plain(v) => v.drain(..),
            Self::Paid(v) => v.drain(..),
        };
        let candidate = (|| {
            while !values.as_slice().is_empty() {
                check(p, std::mem::size_of::<T>().max(1) as u64)?;
                let value = values.next().expect("checked nonempty private drain");
                self.push_reserved(value);
            }
            Ok(())
        })();
        // On error, retire the remaining private source explicitly after the
        // operation error is canonical; never continue a producer with lost data.
        let candidate = finish(p, candidate);
        drop(values);
        finish(p, candidate)
    }
}
impl<T: PolicyValue> PolicyValue for TreeVec<T> {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        self.validate_container(p)?;
        for value in self.iter() {
            check(p, 1)?;
            value.validate_policy(p)?;
        }
        check(p, 0)
    }
}
impl<T> Deref for TreeVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v,
        }
    }
}
impl<T> DerefMut for TreeVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v,
        }
    }
}
impl<T: fmt::Debug> fmt::Debug for TreeVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

enum StringStorage {
    Plain(String),
    Paid(PaidUtf8),
}
/// Request-owned UTF8 metadata. Only borrowed text is public.
pub struct TreeString {
    inner: StringStorage,
}
impl TreeString {
    pub(crate) fn source_resources(&self) -> Option<&ResourcesRef> {
        match &self.inner {
            StringStorage::Plain(_) => None,
            StringStorage::Paid(v) => Some(v.resources()),
        }
    }
    pub(crate) fn plain(value: String) -> Self {
        Self {
            inner: StringStorage::Plain(value),
        }
    }
    pub(crate) fn new(p: &ProcessingContext) -> Self {
        Self {
            inner: match p.resources() {
                None => StringStorage::Plain(String::new()),
                Some(r) => StringStorage::Paid(PaidUtf8::new(r.clone(), ResourceKind::Scratch)),
            },
        }
    }
    pub(crate) fn try_from_fmt(args: fmt::Arguments<'_>, p: &ProcessingContext) -> Result<Self> {
        struct Sink<'a> {
            value: &'a mut TreeString,
            p: &'a ProcessingContext,
        }
        impl fmt::Write for Sink<'_> {
            fn write_str(&mut self, text: &str) -> fmt::Result {
                self.value
                    .try_push_str(text, self.p)
                    .map_err(|_| fmt::Error)
            }
        }
        check(p, 0)?;
        let mut target = Self::new(p);
        let result = fmt::write(
            &mut Sink {
                value: &mut target,
                p,
            },
            args,
        );
        if result.is_err() {
            let e = p.resource_failure().unwrap_or_else(|| reject(p, None));
            drop(target);
            return finish(p, Err(e));
        }
        finish(p, Ok(target))
    }
    pub(crate) fn try_clear(&mut self, p: &ProcessingContext) -> Result<()> {
        self.validate_policy(p)?;
        match &mut self.inner {
            StringStorage::Plain(v) => v.clear(),
            StringStorage::Paid(v) => v.clear(),
        };
        check(p, 0)
    }
    pub(crate) fn try_with_capacity(capacity: usize, p: &ProcessingContext) -> Result<Self> {
        check(p, 0)?;
        match p.resources() {
            None => Ok(Self::plain(String::with_capacity(capacity))),
            Some(r) => {
                let mut value = PaidUtf8::new(r.clone(), ResourceKind::Scratch);
                let result = value
                    .try_reserve_exact(capacity)
                    .map_err(|e| p.record_resource(e));
                match result {
                    Ok(()) => finish(
                        p,
                        Ok(Self {
                            inner: StringStorage::Paid(value),
                        }),
                    ),
                    Err(e) => {
                        drop(value);
                        finish(p, Err(e))
                    }
                }
            }
        }
    }
    pub(crate) fn try_from_str(value: &str, p: &ProcessingContext) -> Result<Self> {
        let mut target = Self::try_with_capacity(value.len(), p)?;
        let result = target
            .try_push_str(value, p)
            .map_err(|e| p.record_resource(e));
        match finish(p, result) {
            Ok(()) => finish(p, Ok(target)),
            Err(e) => {
                drop(target);
                finish(p, Err(e))
            }
        }
    }
    pub(crate) fn try_push_str(&mut self, value: &str, p: &ProcessingContext) -> Result<()> {
        self.validate_policy(p)?;
        match &mut self.inner {
            StringStorage::Plain(v) => {
                v.push_str(value);
                Ok(())
            }
            StringStorage::Paid(v) => {
                let mut text = value;
                while !text.is_empty() {
                    let mut end = text.len().min(256);
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    v.try_push_str(&text[..end])
                        .map_err(|e| p.record_resource(e))?;
                    text = &text[end..];
                }
                check(p, 0)
            }
        }
    }
    pub(crate) fn try_copy(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Self::try_from_str(self, p)
    }
    /// Borrow the retained UTF-8 text without transferring its storage.
    pub fn as_str(&self) -> &str {
        self
    }
}
impl PolicyValue for TreeString {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        validate_owner(
            p,
            match &self.inner {
                StringStorage::Plain(_) => None,
                StringStorage::Paid(v) => Some(v.resources()),
            },
        )
    }
}
impl Deref for TreeString {
    type Target = str;
    fn deref(&self) -> &str {
        match &self.inner {
            StringStorage::Plain(v) => v,
            StringStorage::Paid(v) => v.as_str(),
        }
    }
}
impl fmt::Debug for TreeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}
impl fmt::Display for TreeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}
impl PartialEq for TreeString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for TreeString {}

macro_rules! scalar_policy {
    ($($t:ty),* $(,)?) => { $(impl PolicyValue for $t {
        fn validate_policy(&self, p: &ProcessingContext) -> Result<()> { check(p, 0) }
    })* };
}
scalar_policy!((), u8, u16, u32, u64, usize, i16, i32, i64, f32, f64, bool);

/// Explicit fallible field copy. Implementations validate every dynamic owner
/// before allocating a destination and record failures before local cleanup.
pub(crate) trait TryCopy: PolicyValue + Sized {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self>;
}

macro_rules! scalar_copy {
    ($($t:ty),* $(,)?) => { $(impl TryCopy for $t {
        fn try_copy_in(&self, p:&ProcessingContext)->Result<Self>{check(p,0)?;Ok(*self)}
    })* };
}
scalar_copy!((), u8, u16, u32, u64, usize, i16, i32, i64, f32, f64, bool);
impl<T: PolicyValue> PolicyValue for Option<T> {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        check(p, 0)?;
        if let Some(v) = self {
            v.validate_policy(p)?;
        }
        Ok(())
    }
}
impl<T: TryCopy> TryCopy for Option<T> {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        self.as_ref().map(|v| v.try_copy_in(p)).transpose()
    }
}
impl<T: PolicyValue, const N: usize> PolicyValue for [T; N] {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        for v in self {
            check(p, 1)?;
            v.validate_policy(p)?;
        }
        check(p, 0)
    }
}
impl TryCopy for TreeString {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.try_copy(p)
    }
}
impl<T: TryCopy> TryCopy for TreeVec<T> {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        self.try_copy_with(p, &mut |v| v.try_copy_in(p))
    }
}
impl<T: TryCopy> TryCopy for SceneBox<T> {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        Self::ensure_nonzero(p)?;
        self.validate_header(p)?;
        Self::preflight(p)?;
        self.validate_policy(p)?;
        Self::try_new_with(p, || self.deref().try_copy_in(p))
    }
}

impl PolicyValue for Origin {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        self.validate(p)
    }
}
impl TryCopy for Origin {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.try_copy(p)
    }
}
impl<'a, T> IntoIterator for &'a TreeVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut TreeVec<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T: TryCopy> TryCopy for [T; 2] {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok([self[0].try_copy_in(p)?, self[1].try_copy_in(p)?])
    }
}
impl<T: TryCopy> TryCopy for [T; 3] {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok([
            self[0].try_copy_in(p)?,
            self[1].try_copy_in(p)?,
            self[2].try_copy_in(p)?,
        ])
    }
}
impl<T: TryCopy> TryCopy for [T; 4] {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok([
            self[0].try_copy_in(p)?,
            self[1].try_copy_in(p)?,
            self[2].try_copy_in(p)?,
            self[3].try_copy_in(p)?,
        ])
    }
}
impl<T: PartialEq> PartialEq for TreeVec<T> {
    fn eq(&self, v: &Self) -> bool {
        self.deref() == v.deref()
    }
}
impl<T: Eq> Eq for TreeVec<T> {}
impl<T: std::hash::Hash> std::hash::Hash for TreeVec<T> {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.deref().hash(h)
    }
}
impl std::hash::Hash for TreeString {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.as_str().hash(h)
    }
}

impl<A: PolicyValue, B: PolicyValue> PolicyValue for (A, B) {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        self.0.validate_policy(p)?;
        self.1.validate_policy(p)
    }
}
impl<A: TryCopy, B: TryCopy> TryCopy for (A, B) {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok((self.0.try_copy_in(p)?, self.1.try_copy_in(p)?))
    }
}

impl<T: Copy + PolicyValue> TreeVec<T> {
    pub(crate) fn try_remove(&mut self, index: usize, p: &ProcessingContext) -> Result<T> {
        self.validate_container(p)?;
        check(p, 1)?;
        if index >= self.len() {
            return Err(reject(p, None));
        }
        match self {
            Self::Plain(v) => Ok(v.remove(index)),
            Self::Paid(v) => {
                let result = v[index];
                let len = v.len();
                markdown_svg_budget::slice::copy_within_with_control(
                    v,
                    index + 1..len,
                    index,
                    &mut markdown_svg_budget::RequestControl(
                        p.resources().expect("validated paid owner"),
                    ),
                )
                .map_err(|e| p.record_resource(e))?;
                v.pop();
                finish(p, Ok(result))
            }
        }
    }
}

pub(crate) fn equal_utf8(a: &str, b: &str, p: &ProcessingContext) -> Result<bool> {
    check(p, 1)?;
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.as_bytes().chunks(256).zip(b.as_bytes().chunks(256)) {
        check(p, a.len() as u64)?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}

impl<T: PolicyValue> TreeVec<T> {
    pub(crate) fn try_from_iter<I: IntoIterator<Item = T>>(
        source: I,
        p: &ProcessingContext,
    ) -> Result<Self> {
        check(p, 0)?;
        let mut target = Self::new(p);
        let mut iter = source.into_iter();
        let result = (|| {
            loop {
                check(p, 1)?;
                let Some(value) = iter.next() else {
                    break;
                };
                target.try_push(value, p)?;
            }
            Ok(())
        })();
        let result = result.map_err(|e| p.record_resource(e));
        drop(iter);
        match finish(p, result) {
            Ok(()) => finish(p, Ok(target)),
            Err(e) => {
                drop(target);
                finish(p, Err(e))
            }
        }
    }
    pub(crate) fn try_reverse(&mut self, p: &ProcessingContext) -> Result<()> {
        self.validate_container(p)?;
        let len = self.len();
        for i in 0..len / 2 {
            check(p, (std::mem::size_of::<T>() as u64).max(1))?;
            self.swap(i, len - 1 - i);
        }
        check(p, 0)
    }
}
impl TreeString {
    pub(crate) fn try_push_char(&mut self, ch: char, p: &ProcessingContext) -> Result<()> {
        let mut bytes = [0; 4];
        self.try_push_str(ch.encode_utf8(&mut bytes), p)
    }
}
impl<T> TreeVec<T> {}

impl TreeString {
    pub(crate) fn capacity(&self) -> usize {
        match &self.inner {
            StringStorage::Plain(v) => v.capacity(),
            StringStorage::Paid(v) => v.capacity(),
        }
    }
}
impl<T> TreeVec<T> {
    pub(crate) fn capacity(&self) -> usize {
        match self {
            Self::Plain(v) => v.capacity(),
            Self::Paid(v) => v.capacity(),
        }
    }
}

impl<T: PolicyValue> TreeVec<T> {}

impl TreeString {
    pub(crate) fn try_write_fmt(
        &mut self,
        args: fmt::Arguments<'_>,
        p: &ProcessingContext,
    ) -> Result<()> {
        struct Sink<'a> {
            text: &'a mut TreeString,
            p: &'a ProcessingContext,
        }
        impl fmt::Write for Sink<'_> {
            fn write_str(&mut self, text: &str) -> fmt::Result {
                self.text.try_push_str(text, self.p).map_err(|_| fmt::Error)
            }
        }
        self.validate_policy(p)?;
        let result = fmt::write(&mut Sink { text: self, p }, args);
        if result.is_err() {
            return Err(p.resource_failure().unwrap_or_else(|| reject(p, None)));
        }
        check(p, 0)
    }
}

impl<T: PolicyValue> TreeVec<T> {
    pub(crate) fn try_truncate(&mut self, len: usize, p: &ProcessingContext) -> Result<()> {
        self.validate_policy(p)?;
        if len >= self.len() {
            return check(p, 0);
        }
        match self {
            Self::Plain(v) => {
                v.truncate(len);
                check(p, 0)
            }
            Self::Paid(v) => {
                let mut tail = v.drain_tail(len);
                let result = (|| {
                    while !tail.as_slice().is_empty() {
                        check(p, std::mem::size_of::<T>().max(1) as u64)?;
                        let value = tail.next().expect("validated nonempty forward tail");
                        drop(value);
                        check(p, 0)?;
                    }
                    Ok(())
                })();
                let result = result.map_err(|e| p.record_resource(e));
                drop(tail);
                finish(p, result)
            }
        }
    }
}

impl AsRef<str> for TreeString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
impl<T: PartialEq> PartialEq<Vec<T>> for TreeVec<T> {
    fn eq(&self, rhs: &Vec<T>) -> bool {
        self.deref() == rhs.as_slice()
    }
}
#[cfg(test)]
impl<T: PartialEq> PartialEq<TreeVec<T>> for Vec<T> {
    fn eq(&self, rhs: &TreeVec<T>) -> bool {
        self.as_slice() == rhs.deref()
    }
}
