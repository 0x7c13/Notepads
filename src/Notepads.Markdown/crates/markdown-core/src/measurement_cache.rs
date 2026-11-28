// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Optional pure measurement storage, independent of document operations and their first fault.
//! Exact backing allocation belongs to the FFI boundary; all cache policy uses safe slices.

use std::sync::Mutex;

pub const CACHE_TOTAL_BYTES: usize = 8 * 1024 * 1024;
pub const ENTRY_LIMIT: usize = 2048;
pub const KEY_LIMIT: usize = 64 * 1024;
pub const ENTRY_BYTES_LIMIT: usize = 512 * 1024;
const ROW_BYTES: usize = 32;
const DIRECTORY_BYTES: usize = ENTRY_LIMIT * ROW_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Disposition {
    Miss = 0,
    Hit = 1,
    NeedBuffer = 2,
    Stored = 3,
    StaleEpoch = 4,
    TooLarge = 5,
    Disabled = 6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheResult {
    pub epoch: u64,
    pub disposition: Disposition,
    pub required_bytes: u32,
    pub copied_bytes: u32,
}

impl CacheResult {
    pub fn disabled() -> Self {
        Self::new(0, Disposition::Disabled)
    }

    fn new(epoch: u64, disposition: Disposition) -> Self {
        Self {
            epoch,
            disposition,
            required_bytes: 0,
            copied_bytes: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheStats {
    pub epoch: u64,
    pub allocated_bytes: u64,
    pub occupied_bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub probes: u64,
    pub puts: u64,
    pub evictions: u64,
    pub rejected_puts: u64,
    pub entries: u32,
    pub enabled: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub enum CacheInitError {
    InvalidCapacity,
}

pub struct MeasurementCache {
    state: Mutex<State>,
}

struct State {
    backing: Box<[u8]>,
    owner_bytes: usize,
    allocate: Option<fn(usize) -> Option<Box<[u8]>>>,
    write_pos: usize,
    directory_cursor: usize,
    // Slots at or above this bound have never been written in this directory.
    // Eviction leaves holes, so stats.entries and the wrapping cursor are not bounds.
    directory_len: usize,
    stats: CacheStats,
}

#[derive(Clone, Copy)]
struct Entry {
    hash: u64,
    start: usize,
    key: usize,
    value: usize,
}

impl MeasurementCache {
    /// The caller accounts the complete containing owner allocation, including this mutex.
    /// The backing has its exact Box slice capacity and is exclusively owned from here.
    pub fn from_backing(
        mut backing: Box<[u8]>,
        owner_bytes: usize,
    ) -> Result<Self, CacheInitError> {
        if owner_bytes < size_of::<Self>()
            || owner_bytes.checked_add(backing.len()) != Some(CACHE_TOTAL_BYTES)
            || backing.len() < DIRECTORY_BYTES + ENTRY_BYTES_LIMIT
        {
            return Err(CacheInitError::InvalidCapacity);
        }
        backing[..DIRECTORY_BYTES].fill(0);
        Ok(Self {
            state: Mutex::new(State {
                backing,
                owner_bytes,
                allocate: None,
                write_pos: 0,
                directory_cursor: 0,
                directory_len: 0,
                stats: CacheStats {
                    epoch: 1,
                    allocated_bytes: CACHE_TOTAL_BYTES as u64,
                    enabled: true,
                    ..CacheStats::default()
                },
            }),
        })
    }

    /// Optional storage is allocated fallibly by its owning boundary on the first put.
    /// The owner plus all simultaneously live backing allocations stay within the cap.
    pub fn lazy(
        owner_bytes: usize,
        allocate: fn(usize) -> Option<Box<[u8]>>,
    ) -> Result<Self, CacheInitError> {
        if owner_bytes < size_of::<Self>()
            || owner_bytes > CACHE_TOTAL_BYTES - DIRECTORY_BYTES - ENTRY_BYTES_LIMIT
        {
            return Err(CacheInitError::InvalidCapacity);
        }
        Ok(Self {
            state: Mutex::new(State {
                backing: Box::default(),
                owner_bytes,
                allocate: Some(allocate),
                write_pos: 0,
                directory_cursor: 0,
                directory_len: 0,
                stats: CacheStats {
                    epoch: 1,
                    allocated_bytes: owner_bytes as u64,
                    enabled: true,
                    ..CacheStats::default()
                },
            }),
        })
    }

    pub fn stats(&self) -> CacheStats {
        match self.state.lock() {
            Ok(state) => state.stats,
            Err(poison) => poison.into_inner().disabled_stats(),
        }
    }

    pub fn clear(&self) -> CacheStats {
        match self.state.lock() {
            Ok(mut state) => {
                if state.allocate.is_some() {
                    state.backing = Box::default();
                    state.stats.allocated_bytes = state.owner_bytes as u64;
                } else {
                    state.backing[..DIRECTORY_BYTES].fill(0);
                }
                state.write_pos = 0;
                state.directory_cursor = 0;
                state.directory_len = 0;
                state.stats.entries = 0;
                state.stats.occupied_bytes = 0;
                if let Some(epoch) = state.stats.epoch.checked_add(1) {
                    state.stats.epoch = epoch;
                } else {
                    state.stats.enabled = false;
                }
                state.stats
            }
            Err(poison) => {
                // A poisoned optional cache has no reusable entries. Do not
                // inspect its directory, but release its independently owned
                // backing and keep it disabled for the rest of its lifetime.
                let mut state = poison.into_inner();
                state.backing = Box::default();
                state.stats.enabled = false;
                state.stats.entries = 0;
                state.stats.occupied_bytes = 0;
                state.stats.allocated_bytes = state.owner_bytes as u64;
                state.stats
            }
        }
    }

    pub fn lookup(&self, expected_epoch: u64, key: &[u8], output: &mut [u8]) -> CacheResult {
        self.lookup_hashed(expected_epoch, key, output, hash(key))
    }

    fn lookup_hashed(
        &self,
        expected_epoch: u64,
        key: &[u8],
        output: &mut [u8],
        key_hash: u64,
    ) -> CacheResult {
        let Ok(mut state) = self.state.lock() else {
            return CacheResult::disabled();
        };
        let epoch = state.stats.epoch;
        if !state.stats.enabled {
            state.stats.misses = state.stats.misses.saturating_add(1);
            return CacheResult::new(epoch, Disposition::Disabled);
        }
        if expected_epoch != epoch {
            state.stats.misses = state.stats.misses.saturating_add(1);
            return CacheResult::new(epoch, Disposition::StaleEpoch);
        }
        if key.is_empty() || key.len() > KEY_LIMIT {
            state.stats.misses = state.stats.misses.saturating_add(1);
            return CacheResult::new(epoch, Disposition::TooLarge);
        }
        for slot in 0..state.directory_len {
            if let Some(entry) = state.entry(slot)
                && state.matches(entry, key_hash, key)
            {
                if output.len() < entry.value {
                    state.stats.probes = state.stats.probes.saturating_add(1);
                    return CacheResult {
                        required_bytes: entry.value as u32,
                        ..CacheResult::new(epoch, Disposition::NeedBuffer)
                    };
                }
                let start = DIRECTORY_BYTES + entry.start + entry.key;
                output[..entry.value].copy_from_slice(&state.backing[start..start + entry.value]);
                state.stats.hits = state.stats.hits.saturating_add(1);
                return CacheResult {
                    required_bytes: entry.value as u32,
                    copied_bytes: entry.value as u32,
                    ..CacheResult::new(epoch, Disposition::Hit)
                };
            }
        }
        state.stats.misses = state.stats.misses.saturating_add(1);
        CacheResult::new(epoch, Disposition::Miss)
    }

    pub fn put(&self, expected_epoch: u64, key: &[u8], value: &[u8]) -> CacheResult {
        self.put_hashed(expected_epoch, key, value, hash(key))
    }

    fn put_hashed(
        &self,
        expected_epoch: u64,
        key: &[u8],
        value: &[u8],
        key_hash: u64,
    ) -> CacheResult {
        let Ok(mut state) = self.state.lock() else {
            return CacheResult::disabled();
        };
        let epoch = state.stats.epoch;
        let invalid = if !state.stats.enabled {
            Some(Disposition::Disabled)
        } else if expected_epoch != epoch {
            Some(Disposition::StaleEpoch)
        } else if key.is_empty()
            || key.len() > KEY_LIMIT
            || value.is_empty()
            || key
                .len()
                .checked_add(value.len())
                .is_none_or(|n| n > ENTRY_BYTES_LIMIT)
        {
            Some(Disposition::TooLarge)
        } else {
            None
        };
        if let Some(disposition) = invalid {
            state.stats.rejected_puts = state.stats.rejected_puts.saturating_add(1);
            return CacheResult::new(epoch, disposition);
        }
        let length = key.len() + value.len();
        if !state.ensure_capacity(length) {
            state.stats.rejected_puts = state.stats.rejected_puts.saturating_add(1);
            return CacheResult::new(epoch, Disposition::Disabled);
        }
        let data_len = state.backing.len() - DIRECTORY_BYTES;
        let start = if length <= data_len - state.write_pos {
            state.write_pos
        } else {
            0
        };
        let end = start + length;
        for slot in 0..state.directory_len {
            if let Some(entry) = state.entry(slot) {
                let same = state.matches(entry, key_hash, key);
                if same || (entry.start < end && start < entry.start + entry.key + entry.value) {
                    state.remove(slot, entry, !same);
                }
            }
        }
        let slot = if state.stats.entries as usize == ENTRY_LIMIT {
            state.directory_cursor
        } else {
            (0..ENTRY_LIMIT)
                .map(|n| (state.directory_cursor + n) % ENTRY_LIMIT)
                .find(|&slot| state.entry(slot).is_none())
                .unwrap_or(state.directory_cursor)
        };
        if let Some(entry) = state.entry(slot) {
            state.remove(slot, entry, true);
        }
        let data_start = DIRECTORY_BYTES + start;
        state.backing[data_start..data_start + key.len()].copy_from_slice(key);
        state.backing[data_start + key.len()..data_start + length].copy_from_slice(value);
        state.write_entry(
            slot,
            Entry {
                hash: key_hash,
                start,
                key: key.len(),
                value: value.len(),
            },
        );
        state.write_pos = end;
        state.directory_cursor = (slot + 1) % ENTRY_LIMIT;
        state.stats.entries += 1;
        state.stats.occupied_bytes += length as u64;
        state.stats.puts = state.stats.puts.saturating_add(1);
        CacheResult::new(epoch, Disposition::Stored)
    }
}

impl State {
    fn ensure_capacity(&mut self, length: usize) -> bool {
        let Some(allocate) = self.allocate else {
            return true;
        };
        let capacity = self.backing.len().saturating_sub(DIRECTORY_BYTES);
        if length <= capacity.saturating_sub(self.write_pos) {
            return true;
        }
        let maximum = CACHE_TOTAL_BYTES - self.owner_bytes;
        if self.backing.len() == maximum {
            return true; // The established bounded ring handles wrap and eviction.
        }
        let required = DIRECTORY_BYTES + self.write_pos + length;
        let mut next = self.backing.len().max(256 * 1024);
        while next < required && next < maximum {
            next = next.saturating_mul(2).min(maximum);
        }
        // Growing a full ring is optional. Before the last growth, retire its
        // contents if retaining old and new backing would exceed the same cap.
        let retain = self.owner_bytes + self.backing.len() + next <= CACHE_TOTAL_BYTES;
        if !retain {
            self.stats.evictions = self
                .stats
                .evictions
                .saturating_add(self.stats.entries as u64);
            self.stats.entries = 0;
            self.stats.occupied_bytes = 0;
            self.write_pos = 0;
            self.directory_cursor = 0;
            self.directory_len = 0;
            self.backing = Box::default();
            self.stats.allocated_bytes = self.owner_bytes as u64;
        }
        let Some(mut backing) = allocate(next) else {
            return false;
        };
        if backing.len() != next {
            return false;
        }
        if self.backing.is_empty() {
            backing[..DIRECTORY_BYTES].fill(0);
        } else {
            backing[..self.backing.len()].copy_from_slice(&self.backing);
        }
        self.backing = backing;
        self.stats.allocated_bytes = (self.owner_bytes + self.backing.len()) as u64;
        true
    }

    fn disabled_stats(&self) -> CacheStats {
        CacheStats {
            enabled: false,
            entries: 0,
            occupied_bytes: 0,
            ..self.stats
        }
    }

    fn entry(&self, slot: usize) -> Option<Entry> {
        if self.backing.is_empty() {
            return None;
        }
        let row = &self.backing[slot * ROW_BYTES..(slot + 1) * ROW_BYTES];
        if u32::from_le_bytes(row[20..24].try_into().unwrap()) == 0 {
            return None;
        }
        Some(Entry {
            hash: u64::from_le_bytes(row[..8].try_into().unwrap()),
            start: u32::from_le_bytes(row[8..12].try_into().unwrap()) as usize,
            key: u32::from_le_bytes(row[12..16].try_into().unwrap()) as usize,
            value: u32::from_le_bytes(row[16..20].try_into().unwrap()) as usize,
        })
    }

    fn matches(&self, entry: Entry, key_hash: u64, key: &[u8]) -> bool {
        entry.hash == key_hash
            && entry.key == key.len()
            && self.backing
                [DIRECTORY_BYTES + entry.start..DIRECTORY_BYTES + entry.start + entry.key]
                == *key
    }

    fn remove(&mut self, slot: usize, entry: Entry, eviction: bool) {
        self.backing[slot * ROW_BYTES + 20..slot * ROW_BYTES + 24].fill(0);
        self.stats.entries -= 1;
        self.stats.occupied_bytes -= (entry.key + entry.value) as u64;
        if eviction {
            self.stats.evictions = self.stats.evictions.saturating_add(1);
        }
    }

    fn write_entry(&mut self, slot: usize, entry: Entry) {
        let row = &mut self.backing[slot * ROW_BYTES..(slot + 1) * ROW_BYTES];
        row[..8].copy_from_slice(&entry.hash.to_le_bytes());
        row[8..12].copy_from_slice(&(entry.start as u32).to_le_bytes());
        row[12..16].copy_from_slice(&(entry.key as u32).to_le_bytes());
        row[16..20].copy_from_slice(&(entry.value as u32).to_le_bytes());
        row[24..32].fill(0);
        row[20..24].copy_from_slice(&1_u32.to_le_bytes());
        self.directory_len = self.directory_len.max(slot + 1);
    }
}

fn hash(key: &[u8]) -> u64 {
    // Reject oversize keys without scanning an unbounded caller-supplied slice.
    if key.len() > KEY_LIMIT {
        return 0;
    }
    key.iter().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests;
