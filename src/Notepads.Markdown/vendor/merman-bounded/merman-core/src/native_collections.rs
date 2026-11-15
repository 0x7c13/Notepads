// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! A small insertion-ordered map with explicit, deterministic backing layout.
//! Native mutation uses `try_insert` and the producer's existing receipt region.
//! Linear lookups are bounded by the native entry limit, rather than advertised
//! as hash-table O(1) operations. Upstream compatibility builders remain legacy.

use crate::OperationControl;
use markdown_memory::{CloneGroup, ClonePlan, Family, MemoryError, PlanClone, RegionHandle};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, Visitor},
    ser::SerializeMap,
};
use std::{borrow::Borrow, fmt, marker::PhantomData, ops::Index};

pub const NATIVE_ORDERED_MAP_ENTRY_LIMIT: usize = 4096;

pub type NativeHashMap<K, V> =
    hashbrown::HashMap<K, V, rustc_hash::FxBuildHasher, markdown_memory::ExactAllocator>;
pub type NativeHashSet<K> =
    hashbrown::HashSet<K, rustc_hash::FxBuildHasher, markdown_memory::ExactAllocator>;

pub(crate) fn native_allocator(control: &OperationControl) -> markdown_memory::ExactAllocator {
    match control.memory_scope() {
        Some(scope) => markdown_memory::ExactAllocator::new_in(scope.clone(), Family::Model),
        None => markdown_memory::ExactAllocator::default(),
    }
}
pub(crate) fn new_map<K, V>(control: &OperationControl) -> NativeHashMap<K, V> {
    NativeHashMap::with_hasher_in(rustc_hash::FxBuildHasher, native_allocator(control))
}
pub(crate) fn new_set<K>(control: &OperationControl) -> NativeHashSet<K> {
    NativeHashSet::with_hasher_in(rustc_hash::FxBuildHasher, native_allocator(control))
}
pub(crate) fn failure(control: &OperationControl, error: MemoryError) -> crate::OperationCancelled {
    if let Some(scope) = control.memory_scope() {
        scope.record_failure(error);
    }
    control.cancel();
    crate::OperationCancelled {
        phase: crate::OperationPhase::Semantic,
        reason: crate::CancelReason::Requested,
    }
}
pub(crate) fn result<T>(
    value: Result<T, MemoryError>,
    control: &OperationControl,
) -> crate::OperationControlResult<T> {
    value.map_err(|error| failure(control, error))
}
pub(crate) fn clone_owned<T: PlanClone + Clone>(
    value: &T,
    control: &OperationControl,
) -> crate::OperationControlResult<T> {
    match control.memory_region() {
        Some(region) => result(region.try_clone_recipe(value, Family::Model), control),
        None => Ok(value.clone()),
    }
}
pub(crate) fn owned_string(
    value: &str,
    control: &OperationControl,
) -> crate::OperationControlResult<String> {
    match control.memory_region() {
        Some(region) => result(region.try_string(value, Family::Model), control),
        None => Ok(value.to_owned()),
    }
}

pub(crate) fn unsupported_error(
    feature: &str,
    control: &OperationControl,
) -> crate::OperationControlResult<crate::Error> {
    Ok(crate::Error::diagram_parse_diagnostic(
        String::new(),
        crate::ParseDiagnostic::new(owned_string(feature, control)?)
            .with_code(owned_string("merman.native.unsupported", control)?),
    ))
}
pub(crate) fn push<T>(
    values: &mut Vec<T>,
    value: T,
    control: &OperationControl,
) -> crate::OperationControlResult<()> {
    match control.memory_region() {
        Some(region) => result(region.try_push(values, value, Family::Model), control),
        None => {
            values.push(value);
            Ok(())
        }
    }
}
pub(crate) fn reserve<T>(
    values: &mut Vec<T>,
    additional: usize,
    control: &OperationControl,
) -> crate::OperationControlResult<()> {
    match control.memory_region() {
        Some(region) => result(
            region.try_reserve_vec(values, additional, Family::Model),
            control,
        ),
        None => {
            values.reserve(additional);
            Ok(())
        }
    }
}
pub(crate) fn extend_cloned<T: PlanClone + Clone>(
    values: &mut Vec<T>,
    source: &[T],
    control: &OperationControl,
) -> crate::OperationControlResult<()> {
    if let Some(region) = control.memory_region() {
        // Include the exact outer scratch buffer and all nested cloned layouts.
        let mut copied = result(
            region.try_build_recipe(
                Family::Model,
                |plan| {
                    plan.array::<T>(source.len())?;
                    for value in source {
                        value.plan_clone(plan)?;
                    }
                    Ok(())
                },
                |group| {
                    let mut output = group.vector::<T>(source.len())?;
                    for value in source {
                        output.push(value.clone_prepared(group)?);
                    }
                    Ok(output)
                },
            ),
            control,
        )?;
        reserve(values, copied.len(), control)?;
        values.append(&mut copied);
    } else {
        values.extend_from_slice(source);
    }
    Ok(())
}
pub(crate) fn insert<K: Eq + std::hash::Hash, V>(
    map: &mut NativeHashMap<K, V>,
    key: K,
    value: V,
    control: &OperationControl,
) -> crate::OperationControlResult<Option<V>> {
    if let Some(scope) = control.memory_scope() {
        control.checkpoint()?;
        result(scope.charge_work(1), control)?;
    }
    if !map.contains_key(&key) {
        map.try_reserve(1).map_err(|_| {
            failure(
                control,
                control
                    .memory_scope()
                    .and_then(|scope| scope.failure())
                    .unwrap_or(MemoryError::OutOfMemory),
            )
        })?;
    }
    Ok(map.insert(key, value)) // No backing growth after successful reserve.
}
pub(crate) fn insert_set<K: Eq + std::hash::Hash>(
    set: &mut NativeHashSet<K>,
    key: K,
    control: &OperationControl,
) -> crate::OperationControlResult<bool> {
    if let Some(scope) = control.memory_scope() {
        control.checkpoint()?;
        result(scope.charge_work(1), control)?;
    }
    if set.contains(&key) {
        return Ok(false);
    }
    set.try_reserve(1).map_err(|_| {
        failure(
            control,
            control
                .memory_scope()
                .and_then(|scope| scope.failure())
                .unwrap_or(MemoryError::OutOfMemory),
        )
    })?;
    Ok(set.insert(key)) // No backing growth after successful reserve.
}

/// Concrete native callers use allocation-free Vec/style defaults. A key and
/// any nonempty default payload must already be constructed through the owner.
pub(crate) fn entry_default<'a, K: Eq + std::hash::Hash, V: Default>(
    map: &'a mut NativeHashMap<K, V>,
    key: K,
    control: &OperationControl,
) -> crate::OperationControlResult<&'a mut V> {
    if let Some(scope) = control.memory_scope() {
        control.checkpoint()?;
        result(scope.charge_work(1), control)?;
    }
    if !map.contains_key(&key) {
        map.try_reserve(1).map_err(|_| {
            failure(
                control,
                control
                    .memory_scope()
                    .and_then(|scope| scope.failure())
                    .unwrap_or(MemoryError::OutOfMemory),
            )
        })?;
    }
    Ok(map.entry(key).or_default())
}

pub(crate) fn format_owned(
    arguments: fmt::Arguments<'_>,
    control: &OperationControl,
) -> crate::OperationControlResult<String> {
    let Some(region) = control.memory_region() else {
        return Ok(fmt::format(arguments));
    };
    struct Count {
        length: usize,
    }
    impl fmt::Write for Count {
        fn write_str(&mut self, value: &str) -> fmt::Result {
            self.length = self.length.checked_add(value.len()).ok_or(fmt::Error)?;
            Ok(())
        }
    }
    let mut count = Count { length: 0 };
    fmt::write(&mut count, arguments)
        .map_err(|_| failure(control, MemoryError::ArithmeticOverflow))?;
    let work = count
        .length
        .checked_mul(2)
        .ok_or_else(|| failure(control, MemoryError::ArithmeticOverflow))?;
    result(region.scope().charge_work(work), control)?;
    let mut value = result(
        region.try_string_capacity(count.length, Family::Model),
        control,
    )?;
    struct Prepared<'a> {
        output: &'a mut String,
    }
    impl fmt::Write for Prepared<'_> {
        fn write_str(&mut self, value: &str) -> fmt::Result {
            if value.len() > self.output.capacity() - self.output.len() {
                return Err(fmt::Error);
            }
            self.output.push_str(value);
            Ok(())
        }
    }
    fmt::write(&mut Prepared { output: &mut value }, arguments)
        .map_err(|_| failure(control, MemoryError::InvalidState))?;
    Ok(value)
}

#[derive(Debug)]
pub struct OrderedMap<K, V> {
    entries: Vec<(K, V)>,
    scope: Option<markdown_memory::MemoryScope>,
    // The actual entries disappear before their joint producer guard.
    region: Option<RegionHandle>,
}

impl<K, V> Default for OrderedMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> OrderedMap<K, V> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            scope: None,
            region: None,
        }
    }
    pub fn new_in(region: RegionHandle) -> Self {
        Self {
            entries: Vec::new(),
            scope: Some(region.scope().clone()),
            region: Some(region),
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&K, &V)> + DoubleEndedIterator {
        self.entries.iter().map(|(key, value)| (key, value))
    }
    pub fn iter_mut(
        &mut self,
    ) -> impl ExactSizeIterator<Item = (&K, &mut V)> + DoubleEndedIterator {
        self.entries.iter_mut().map(|(key, value)| (&*key, value))
    }
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &K> + DoubleEndedIterator {
        self.entries.iter().map(|(key, _)| key)
    }
    pub fn values(&self) -> impl ExactSizeIterator<Item = &V> + DoubleEndedIterator {
        self.entries.iter().map(|(_, value)| value)
    }
    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut V> + DoubleEndedIterator {
        self.entries.iter_mut().map(|(_, value)| value)
    }
    pub fn get_index(&self, index: usize) -> Option<(&K, &V)> {
        self.entries.get(index).map(|(key, value)| (key, value))
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn get<Q: ?Sized + Eq>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.entries
            .iter()
            .find(|(stored, _)| stored.borrow() == key)
            .map(|(_, value)| value)
    }
    pub fn get_mut<Q: ?Sized + Eq>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
    {
        self.entries
            .iter_mut()
            .find(|(stored, _)| <K as Borrow<Q>>::borrow(stored) == key)
            .map(|(_, value)| value)
    }
    pub fn contains_key<Q: ?Sized + Eq>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.get(key).is_some()
    }
}

impl<K: Eq, V> OrderedMap<K, V> {
    pub fn try_entry_or_default(
        &mut self,
        key: K,
        control: &OperationControl,
    ) -> Result<&mut V, MemoryError>
    where
        V: Default,
    {
        let mut found = None;
        for (index, (stored, _)) in self.entries.iter().enumerate() {
            if self.scope.is_some() && index.is_multiple_of(128) {
                control.checkpoint().map_err(|_| MemoryError::Cancelled)?;
            }
            if let Some(scope) = &self.scope {
                scope.charge_work(1)?;
            }
            if *stored == key {
                found = Some(index);
                break;
            }
        }
        let index = match found {
            Some(index) => index,
            None => {
                if let Some(region) = &self.region {
                    if self.entries.len() >= NATIVE_ORDERED_MAP_ENTRY_LIMIT {
                        region.scope().record_failure(MemoryError::BudgetLimited);
                        return Err(MemoryError::BudgetLimited);
                    }
                    region.try_reserve_vec(&mut self.entries, 1, Family::Model)?;
                } else {
                    if let Some(scope) = &self.scope {
                        scope.record_failure(MemoryError::Unfunded);
                        return Err(MemoryError::Unfunded);
                    }
                    self.entries
                        .try_reserve(1)
                        .map_err(|_| MemoryError::OutOfMemory)?;
                }
                let index = self.entries.len();
                self.entries.push((key, V::default()));
                index
            }
        };
        Ok(&mut self.entries[index].1)
    }

    pub fn try_insert(
        &mut self,
        key: K,
        value: V,
        control: &OperationControl,
    ) -> Result<Option<V>, MemoryError> {
        for (index, (stored, previous)) in self.entries.iter_mut().enumerate() {
            if self.scope.is_some() && index.is_multiple_of(128) {
                control.checkpoint().map_err(|_| MemoryError::Cancelled)?;
            }
            if let Some(scope) = &self.scope {
                scope.charge_work(1)?;
            }
            if *stored == key {
                return Ok(Some(std::mem::replace(previous, value)));
            }
        }
        if let Some(region) = &self.region {
            if self.entries.len() >= NATIVE_ORDERED_MAP_ENTRY_LIMIT {
                region.scope().record_failure(MemoryError::BudgetLimited);
                return Err(MemoryError::BudgetLimited);
            }
            region.try_push(&mut self.entries, (key, value), Family::Model)?;
        } else {
            if let Some(scope) = &self.scope {
                scope.record_failure(MemoryError::Unfunded);
                return Err(MemoryError::Unfunded);
            }
            self.entries
                .try_reserve(1)
                .map_err(|_| MemoryError::OutOfMemory)?;
            self.entries.push((key, value));
        }
        Ok(None)
    }

    /// Compatibility-only mutation. Native producers call `try_insert`.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        assert!(
            self.scope.is_none(),
            "native map mutation requires try_insert"
        );
        if let Some(previous) = self
            .entries
            .iter_mut()
            .find(|(stored, _)| *stored == key)
            .map(|(_, value)| value)
        {
            return Some(std::mem::replace(previous, value));
        }
        self.entries.push((key, value));
        None
    }

    /// Compatibility-only entry construction, excluded from the native path.
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        assert!(
            self.scope.is_none(),
            "native map mutation requires try_insert"
        );
        let index = self.entries.iter().position(|(stored, _)| *stored == key);
        Entry {
            map: self,
            key,
            index,
        }
    }
}

impl<K, V> OrderedMap<K, V> {
    pub fn try_get<Q: ?Sized + Eq>(
        &self,
        key: &Q,
        control: &OperationControl,
    ) -> Result<Option<&V>, MemoryError>
    where
        K: Borrow<Q>,
    {
        for (index, (stored, value)) in self.entries.iter().enumerate() {
            if self.scope.is_some() && index.is_multiple_of(128) {
                control.checkpoint().map_err(|_| MemoryError::Cancelled)?;
            }
            if let Some(scope) = &self.scope {
                scope.charge_work(1)?;
            }
            if stored.borrow() == key {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    pub fn try_get_mut<Q: ?Sized + Eq>(
        &mut self,
        key: &Q,
        control: &OperationControl,
    ) -> Result<Option<&mut V>, MemoryError>
    where
        K: Borrow<Q>,
    {
        for (index, (stored, value)) in self.entries.iter_mut().enumerate() {
            if self.scope.is_some() && index.is_multiple_of(128) {
                control.checkpoint().map_err(|_| MemoryError::Cancelled)?;
            }
            if let Some(scope) = &self.scope {
                scope.charge_work(1)?;
            }
            if <K as Borrow<Q>>::borrow(stored) == key {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }
}

pub struct Entry<'a, K, V> {
    map: &'a mut OrderedMap<K, V>,
    key: K,
    index: Option<usize>,
}
impl<'a, K, V> Entry<'a, K, V> {
    pub fn or_insert_with(self, create: impl FnOnce() -> V) -> &'a mut V {
        let index = match self.index {
            Some(index) => index,
            None => {
                let index = self.map.entries.len();
                self.map.entries.push((self.key, create()));
                index
            }
        };
        &mut self.map.entries[index].1
    }
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}

// Infallible upstream cloning creates legacy values. Native deep cloning must
// use PlanClone and transfer every new nested receipt to its joint owner.
impl<K: Clone, V: Clone> Clone for OrderedMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            scope: None,
            region: None,
        }
    }
}
impl<K: PartialEq, V: PartialEq> PartialEq for OrderedMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}
impl<K: Eq, V: Eq> Eq for OrderedMap<K, V> {}

impl<K: PlanClone, V: PlanClone> PlanClone for OrderedMap<K, V> {
    fn plan_clone(&self, plan: &mut ClonePlan) -> Result<(), MemoryError> {
        self.entries.plan_clone(plan)
    }
    fn clone_prepared(&self, group: &mut CloneGroup) -> Result<Self, MemoryError> {
        Ok(Self {
            entries: self.entries.clone_prepared(group)?,
            scope: Some(group.scope().clone()),
            region: None,
        })
    }
}

impl<K: Eq, V> FromIterator<(K, V)> for OrderedMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(source: I) -> Self {
        let mut result = Self::new();
        for (key, value) in source {
            result.insert(key, value);
        }
        result
    }
}
impl<K, V> IntoIterator for OrderedMap<K, V> {
    type Item = (K, V);
    type IntoIter = OrderedIntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        // Also retain the enclosing NativePayload guard when moving yielded
        // plain values into another producer. The iterator itself retains its
        // backing until after that backing and every remaining entry are freed.
        OrderedIntoIter {
            values: self.entries.into_iter(),
            _region: self.region,
        }
    }
}
pub struct OrderedIntoIter<K, V> {
    values: std::vec::IntoIter<(K, V)>,
    _region: Option<RegionHandle>,
}
impl<K, V> Iterator for OrderedIntoIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        self.values.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.values.size_hint()
    }
}
impl<K, V> DoubleEndedIterator for OrderedIntoIter<K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.values.next_back()
    }
}
impl<K, V> ExactSizeIterator for OrderedIntoIter<K, V> {}
fn split_ref<K, V>(pair: &(K, V)) -> (&K, &V) {
    (&pair.0, &pair.1)
}
impl<'a, K, V> IntoIterator for &'a OrderedMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::iter::Map<std::slice::Iter<'a, (K, V)>, fn(&'a (K, V)) -> (&'a K, &'a V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().map(split_ref::<K, V>)
    }
}
impl<K, V, Q: ?Sized + Eq> Index<&Q> for OrderedMap<K, V>
where
    K: Borrow<Q>,
{
    type Output = V;
    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("ordered map key must exist")
    }
}

impl<K: Serialize, V: Serialize> Serialize for OrderedMap<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
impl<'de, K: Deserialize<'de> + Eq, V: Deserialize<'de>> Deserialize<'de> for OrderedMap<K, V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor<K, V>(PhantomData<(K, V)>);
        impl<'de, K: Deserialize<'de> + Eq, V: Deserialize<'de>> Visitor<'de> for MapVisitor<K, V> {
            type Value = OrderedMap<K, V>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an ordered map")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
                let mut result = OrderedMap::new();
                while let Some((key, value)) = input.next_entry()? {
                    result.insert(key, value);
                }
                Ok(result)
            }
        }
        deserializer.deserialize_map(MapVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_memory::{AllocationAuthority, MemoryScope, NativePayload, NativeRegion};
    use std::{
        alloc::Layout,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    struct Authority {
        used: AtomicUsize,
        work: AtomicUsize,
        allocations: AtomicUsize,
        fail_model: bool,
    }
    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, family: Family) -> Result<(), MemoryError> {
            if family == Family::Model && self.fail_model {
                return Err(MemoryError::BudgetLimited);
            }
            self.used.fetch_add(layout.size(), Ordering::SeqCst);
            Ok(())
        }
        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }
        fn failure(&self) -> Option<MemoryError> {
            None
        }
        fn before_allocate(&self, _: Layout, family: Family) -> Result<(), MemoryError> {
            if family == Family::Model {
                self.allocations.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
        fn charge_work(&self, steps: usize) -> Result<(), MemoryError> {
            self.work.fetch_add(steps, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn insertion_order_and_replacement_match_the_existing_json_baseline() {
        let mut map = OrderedMap::new();
        map.insert("second", vec![2]);
        map.insert("first", vec![1]);
        assert_eq!(map.insert("second", vec![22]), Some(vec![2]));
        assert_eq!(map.get_index(0), Some((&"second", &vec![22])));
        assert_eq!(
            serde_json::to_string(&map).unwrap(),
            r#"{"second":[22],"first":[1]}"#
        );
        let restored: OrderedMap<String, Vec<usize>> =
            serde_json::from_str(r#"{"second":[22],"first":[1]}"#).unwrap();
        assert_eq!(
            restored.keys().map(String::as_str).collect::<Vec<_>>(),
            ["second", "first"]
        );
    }

    #[test]
    fn native_backing_rejection_preserves_empty_state_before_payload_allocator() {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            work: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
            fail_model: true,
        });
        let scope = MemoryScope::new(authority.clone());
        let region = NativeRegion::try_new(&scope, 16).unwrap();
        let control = OperationControl::new().with_memory_region(region.clone());
        let mut map = OrderedMap::new_in(region.clone());
        assert_eq!(
            map.try_insert(1, 7, &control),
            Err(MemoryError::BudgetLimited)
        );
        assert!(map.is_empty());
        assert_eq!(authority.allocations.load(Ordering::SeqCst), 0);
        drop(map);
        drop(control);
        drop(region);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn native_search_charges_real_comparisons_and_joint_owner_outlives_producer() {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            work: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
            fail_model: false,
        });
        let scope = MemoryScope::new(authority.clone());
        let region = NativeRegion::try_new(&scope, 16).unwrap();
        let control = OperationControl::new().with_memory_region(region.clone());
        let mut map = OrderedMap::new_in(region.clone());
        for key in 0..4 {
            map.try_insert(key, key + 10, &control).unwrap();
        }
        assert_eq!(authority.work.load(Ordering::SeqCst), 6);
        assert_eq!(map.try_get(&3, &control).unwrap(), Some(&13));
        assert_eq!(authority.work.load(Ordering::SeqCst), 10);
        let payload = NativePayload::new(map, region.clone());
        drop(control);
        drop(region);
        assert_eq!(payload.get(&2), Some(&12));
        assert!(authority.used.load(Ordering::SeqCst) > 0);
        drop(payload);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn planned_ordered_clone_keeps_search_scope_and_refuses_unfunded_growth() {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            work: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
            fail_model: false,
        });
        let scope = MemoryScope::new(authority.clone());
        let region = NativeRegion::try_new(&scope, 16).unwrap();
        let control = OperationControl::new().with_memory_region(region.clone());
        let original: OrderedMap<u8, u8> = [(1, 11), (2, 12), (3, 13)].into_iter().collect();
        let mut copied = region.try_clone_recipe(&original, Family::Model).unwrap();
        let before = authority.work.load(Ordering::SeqCst);
        assert_eq!(copied.try_get(&3, &control).unwrap(), Some(&13));
        assert_eq!(authority.work.load(Ordering::SeqCst), before + 3);
        let allocations = authority.allocations.load(Ordering::SeqCst);
        assert_eq!(
            copied.try_insert(4, 14, &control),
            Err(MemoryError::Unfunded)
        );
        assert_eq!(copied.len(), 3);
        assert_eq!(authority.allocations.load(Ordering::SeqCst), allocations);
        let payload = NativePayload::new(copied, region.clone());
        drop(control);
        drop(region);
        drop(payload);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}
