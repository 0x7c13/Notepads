// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use crate::token::{SourceLocation, Token};
use ratex_types::{
    execution::Phase,
    storage::{BoundedClone, StorageError, StorageFamily, StorageLedger},
};

impl BoundedClone for SourceLocation {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        _family: StorageFamily,
    ) -> Result<Self, StorageError> {
        storage.check(phase)?;
        Ok(Self {
            start: self.start,
            end: self.end,
        })
    }
}
impl BoundedClone for Token {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        family: StorageFamily,
    ) -> Result<Self, StorageError> {
        Ok(Self {
            text: self.text.bounded_clone(storage, phase, family)?,
            is_builtin_literal: self.is_builtin_literal,
            loc: self.loc.bounded_clone(storage, phase, family)?,
            noexpand: self.noexpand,
            treat_as_relax: self.treat_as_relax,
        })
    }
}

const _: () = assert!(std::mem::size_of::<Token>() <= 64);
