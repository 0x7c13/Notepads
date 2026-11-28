// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Test-only value observation from the producer's actual accepted backings.
//! The returned String is unmetered test observer storage, never an AST owner,
//! a production readonly materialization permission, or physical-free evidence.
//! Original literal expectations remain independent of these observed bytes.

use crate::native::{
    Context, Host, NativeViolation, WorkDelta,
    value::{self, GrammarControl, ValueControl, ValuePosition, ValueStorage, ValueText},
};

pub(super) fn text<H: Host>(
    ctx: &mut Context<'_, H>,
    value: &ValueText,
) -> Result<String, H::Error> {
    match &value.repr {
        ValueStorage::Owned(actual) => Ok(actual.to_string()),
        ValueStorage::Empty => Ok(String::new()),
        ValueStorage::Pending => Err(ctx.host.reject(NativeViolation::InvalidProvenance)),
        ValueStorage::Accepted(identity) => {
            let mut control = GrammarControl(&mut *ctx.host);
            let view = value::accepted_view(*identity, &ctx.mappings, &mut control)?;
            let input = match ctx.input_text {
                Some(input) => input,
                None => {
                    // Numeric definition fixtures have real proof-result
                    // owners but no P input. Admit only proof-backed atoms;
                    // next_backing must never enter its raw R/P branch here.
                    if view.terminal || !view.prefix.is_empty() {
                        return Err(control.reject(NativeViolation::InvalidProvenance));
                    }
                    for atom in &view.map.atoms {
                        control.work(WorkDelta::Events(1))?;
                        control.work(WorkDelta::Scan(
                            std::mem::size_of::<crate::native::mapped::Atom>() as u64,
                        ))?;
                        if atom.proof.is_none() || atom.hidden_syntax {
                            return Err(control.reject(NativeViolation::InvalidProvenance));
                        }
                    }
                    // Unused by the checked proof-only branch, never a
                    // replacement for absent raw source bytes.
                    ""
                }
            };
            let upper = view.len(&mut control)?;
            let mut cursor = ValuePosition::default();
            let mut observed = String::new();
            while let Some(chunk) = value::next_backing(
                &view,
                &ctx.mappings,
                input,
                &mut cursor,
                upper,
                65_536,
                &mut control,
            )? {
                observed.push_str(chunk);
            }
            Ok(observed)
        }
    }
}
