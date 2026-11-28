// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One forward byte-classification pass over an already decoded D run.
//! This module grants no source/decoder/AST authority. Its sole growing owner
//! is NativeRows<EmailMatch>: unsuccessful candidates allocate no rows.

use super::{EmailMatch, EmailSelection, RunBrand};
use crate::native::{Context, Host, NativeAdmission, NativeViolation, WorkDelta, rows::NativeRows};
use std::marker::PhantomData;

#[derive(Clone, Copy)]
enum Class {
    Alnum,
    HyphenUnderscore,
    Dot,
    Plus,
    At,
    Barrier,
}
impl Class {
    fn of(byte: u8) -> Self {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => Self::Alnum,
            b'-' | b'_' => Self::HyphenUnderscore,
            b'.' => Self::Dot,
            b'+' => Self::Plus,
            b'@' => Self::At,
            _ => Self::Barrier,
        }
    }
    fn domain(self) -> bool {
        matches!(self, Self::Alnum | Self::HyphenUnderscore | Self::Dot)
    }
    fn local(self) -> bool {
        self.domain() || matches!(self, Self::Plus)
    }
}

fn add<H: Host>(value: usize, amount: usize, ctx: &mut Context<'_, H>) -> Result<usize, H::Error> {
    value
        .checked_add(amount)
        .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))
}

// Fixed scalar machine state, not a retained native record per @ or byte.
// `dots` is the three-state set {zero, one, multiple}; no unbounded counter.
// Trailing dots remain provisional until either EOF/barrier excludes them or
// another segment makes them internal and requires exactly one dot.
struct Pending {
    start: usize,
    at: usize,
    end: usize,
    segments: usize,
    dots: u8,
    invalid: bool,
    last_alnum: bool,
}
impl Pending {
    fn observe<H: Host>(
        &mut self,
        class: Class,
        next: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        ctx.event()?;
        match class {
            Class::Dot => {
                self.invalid |= self.segments == 0;
                self.dots = if self.dots == 0 { 1 } else { 2 };
            }
            Class::Alnum | Class::HyphenUnderscore => {
                self.invalid |= self.dots == 2;
                if self.segments == 0 || self.dots != 0 {
                    self.segments = add(self.segments, 1, ctx)?;
                }
                self.dots = 0;
                self.end = next;
                self.last_alnum = matches!(class, Class::Alnum);
            }
            _ => return Err(ctx.host.reject(NativeViolation::InternalInvariant)),
        }
        Ok(())
    }

    fn finish<'run, H: Host>(
        self,
        cursor: &mut usize,
        rows: &mut NativeRows<EmailMatch<'run>, H::Lease>,
        ctx: &mut Context<'_, H>,
    ) -> Result<(), H::Error> {
        ctx.event()?;
        if self.invalid || self.segments < 2 || !self.last_alnum {
            return Ok(());
        }
        if self.start < *cursor || self.start >= self.at || self.at >= self.end {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        ctx.host.admit(NativeAdmission::Record(1))?;
        ctx.host.work(WorkDelta::Copy(
            std::mem::size_of::<EmailMatch<'run>>() as u64
        ))?;
        rows.push(
            EmailMatch {
                start: self.start,
                at: self.at,
                end: self.end,
                invariant: PhantomData,
            },
            ctx.host,
        )?;
        *cursor = self.end;
        Ok(())
    }
}

pub(super) fn select<'run, H: Host>(
    decoded: &str,
    run: RunBrand<'run>,
    ctx: &mut Context<'_, H>,
) -> Result<EmailSelection<'run, H::Lease>, H::Error> {
    // Observe an already latched fault/cancellation before rejecting identity.
    ctx.event()?;
    if decoded.len() != run.bytes {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    let mut selected = NativeRows::default();
    let mut pending: Option<Pending> = None;
    let mut local_begin = 0usize;
    let mut cursor = 0usize;
    let mut position = 0usize;
    while position < decoded.len() {
        // Exactly one source-byte read/classification. Non-ASCII bytes are all
        // barriers; every selected endpoint borders ASCII and is therefore a
        // UTF8 boundary without an extra scalar/suffix scan.
        ctx.event()?;
        ctx.scan_step()?;
        let class = Class::of(decoded.as_bytes()[position]);
        let next = add(position, 1, ctx)?;
        if class.domain() {
            if let Some(candidate) = pending.as_mut() {
                candidate.observe(class, next, ctx)?;
            }
        } else if let Some(candidate) = pending.take() {
            // Complete the previous right-domain token BEFORE considering this
            // @. Its successful end now clips the next maximum-local run.
            candidate.finish(&mut cursor, &mut selected, ctx)?;
        }
        if matches!(class, Class::At) {
            ctx.event()?;
            let start = local_begin.max(cursor);
            if start < position {
                pending = Some(Pending {
                    start,
                    at: position,
                    end: next,
                    segments: 0,
                    dots: 0,
                    invalid: false,
                    last_alnum: false,
                });
            }
        }
        // '+' closes a domain but continues the local run. In particular the
        // suffix after an accepted email can become the next email's local.
        if !class.local() {
            local_begin = next;
        }
        position = next;
    }
    if let Some(candidate) = pending {
        candidate.finish(&mut cursor, &mut selected, ctx)?;
    }
    ctx.flush_scan()?;
    // Fixed state leaves scope. On every error above `selected` drops its own
    // page/directory owners without another work/admission call. On success
    // only these accepted rows survive, with their original independent leases.
    Ok(EmailSelection {
        run,
        selected,
        next: 0,
    })
}

#[cfg(test)]
#[path = "matcher_tests.rs"]
mod tests;
