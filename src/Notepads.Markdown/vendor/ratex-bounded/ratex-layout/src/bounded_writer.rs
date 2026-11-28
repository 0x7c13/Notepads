// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Bounded JSON key output; no unbounded String serializer or post-allocation quota check.
use ratex_types::{
    execution::Phase,
    storage::{StorageError, StorageFamily, StorageLedger},
};
use std::io::{self, Write};

pub(crate) struct BoundedWriter<'a> {
    storage: &'a StorageLedger<'a>,
    bytes: Vec<u8>,
    limit: usize,
}
impl<'a> BoundedWriter<'a> {
    pub(crate) fn new(storage: &'a StorageLedger<'a>, limit: usize) -> Self {
        Self {
            storage,
            bytes: Vec::new(),
            limit,
        }
    }
    pub(crate) fn finish(self) -> Result<String, StorageError> {
        self.storage.check(Phase::Layout)?;
        String::from_utf8(self.bytes).map_err(|_| self.storage.fail(StorageError::InvalidUtf8))
    }
}
impl Write for BoundedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // This writer is used only for the closed derive-Serialize AST tuple. Returning an
        // IO error would make serde_json allocate its boxed ErrorImpl during quota/OOM.
        // Retain the original typed failure and drain the finite serializer without writes.
        // The caller MUST check finish/the explicit ledger before publishing a key.
        if self.storage.check(Phase::Layout).is_err() {
            return Ok(bytes.len());
        }
        let required = self.bytes.len().checked_add(bytes.len());
        if required.is_none_or(|n| n > self.limit) {
            self.storage.fail(StorageError::FamilyBytes {
                family: StorageFamily::Strings,
                limit: self.limit as u64,
                requested: required.unwrap_or(usize::MAX) as u64,
            });
            return Ok(bytes.len());
        }
        self.storage
            .try_extend_copy(
                Phase::Layout,
                StorageFamily::Strings,
                &mut self.bytes,
                bytes,
            )
            .ok();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let _ = self.storage.check(Phase::Layout);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratex_types::execution::{ExecutionControl, Limits};
    use std::sync::atomic::AtomicBool;
    #[test]
    fn rejects_at_write_before_capacity_growth_and_retains_unicode() {
        let cancel = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancel, None, Limits::default());
        let storage = StorageLedger::new(&control);
        let mut writer = BoundedWriter::new(&storage, 8);
        writer.write_all("≠∉©".as_bytes()).unwrap();
        let charged = storage.used_bytes();
        assert!(writer.write_all("x".as_bytes()).is_ok());
        assert_eq!(storage.used_bytes(), charged);
        assert!(writer.finish().is_err());
    }
}
