// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One native input identity paired with its actual immutable S/P/N01 owner.

use crate::error::{CoreError, LimitKind};
#[cfg(test)]
use crate::identity::SourceKey;
use crate::identity::{ParseSpan, SourceSpan};
use crate::policy::{NativeLease, OwnedWork};
use crate::source::{InputPiece, InputPieceKind, ParseInput};
use comrak::native::{self as n, Host, PSpan, direct};
use std::marker::PhantomData;

/// Failure of this native input in this construction. The full first fault
/// remains in the original Work; this carrier owns no payload or resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct NativeFault<'native, 'id> {
    native: PhantomData<fn(&'native ()) -> &'native ()>,
    construction: crate::policy::ConstructionId<'id>,
}

pub(super) struct PreparedBinding<'native, 'source, 'work, 'id, 'op> {
    input: &'source ParseInput,
    projection: Option<&'source crate::html::HtmlProjection>,
    work: &'work mut OwnedWork<'id, 'op>,
    native: PhantomData<fn(&'native ()) -> &'native ()>,
    busy: bool,
}

/// This is the sole constructor. Neither a matching string nor a SourceKey can
/// recreate the pairing; the same owner supplies P, S and N01 before issuance.
pub(super) fn with_prepared<'source, 'id, 'op, T>(
    input: &'source ParseInput,
    work: &mut OwnedWork<'id, 'op>,
    body: impl for<'native, 'work> FnOnce(
        direct::NativeInput<'native>,
        PreparedBinding<'native, 'source, 'work, 'id, 'op>,
    ) -> Result<T, CoreError>,
) -> Result<T, CoreError> {
    with_prepared_projection(input, None, work, body)
}

pub(super) fn with_prepared_projection<'source, 'id, 'op, T>(
    input: &'source ParseInput,
    projection: Option<&'source crate::html::HtmlProjection>,
    work: &mut OwnedWork<'id, 'op>,
    body: impl for<'native, 'work> FnOnce(
        direct::NativeInput<'native>,
        PreparedBinding<'native, 'source, 'work, 'id, 'op>,
    ) -> Result<T, CoreError>,
) -> Result<T, CoreError> {
    let guard = PreparedFactory {
        work,
        closed: false,
    };
    guard.work.check()?;
    guard.work.ensure_budget(input.source().budget())?;
    let result = direct::with_input(input.text(), |native| {
        body(
            native,
            PreparedBinding {
                input,
                projection,
                work: &mut *guard.work,
                native: PhantomData,
                busy: false,
            },
        )
    });
    guard.finish(result)
}

/// The whole native session, including gaps between individual Host calls,
/// borrows this guard's one work authority. Catching its unwind cannot resume
/// the still-live construction scope. No native or input owner is duplicated.
struct PreparedFactory<'work, 'id, 'op> {
    work: &'work mut OwnedWork<'id, 'op>,
    closed: bool,
}

impl PreparedFactory<'_, '_, '_> {
    fn finish<T>(mut self, result: Result<T, CoreError>) -> Result<T, CoreError> {
        // A returned typed error precedes any later cancellation/clock check.
        let result = match result {
            Err(error) => Err(self.work.fail(error)),
            Ok(value) => self.work.check().map(|()| value),
        };
        self.closed = true;
        result
    }
}

impl Drop for PreparedFactory<'_, '_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            self.work.fail(CoreError::InternalInvariant);
        }
    }
}

struct PreparedCall<'call, 'native, 'source, 'work, 'id, 'op> {
    binding: &'call mut PreparedBinding<'native, 'source, 'work, 'id, 'op>,
    closed: bool,
}

impl PreparedCall<'_, '_, '_, '_, '_, '_> {
    fn finish<T>(mut self, result: Result<T, CoreError>) -> Result<T, CoreError> {
        let result = result.map_err(|error| self.binding.work.fail(error));
        let result = match self.binding.work.first_fault() {
            Some(error) => Err(error),
            None => result,
        };
        self.binding.busy = false;
        self.closed = true;
        result
    }
}

impl Drop for PreparedCall<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            self.binding.work.fail(CoreError::InternalInvariant);
            self.binding.busy = false;
        }
    }
}

impl<'id> PreparedBinding<'_, '_, '_, 'id, '_> {
    fn call<T>(
        &mut self,
        body: impl FnOnce(&mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.work.check()?;
        if self.busy {
            return Err(self.work.fail(CoreError::InternalInvariant));
        }
        self.busy = true;
        let guard = PreparedCall {
            binding: self,
            closed: false,
        };
        let result = body(guard.binding.work);
        guard.finish(result)
    }
}

impl<'native, 'id> PreparedBinding<'native, '_, '_, 'id, '_> {
    fn fail_native(&mut self, error: CoreError) -> NativeFault<'native, 'id> {
        self.work.fail(error);
        NativeFault {
            native: PhantomData,
            construction: PhantomData,
        }
    }

    /// Convert only after the original operation has latched the complete
    /// failure. No cancellation poll, clock read, fee or guard is added here.
    pub(super) fn carry<T>(
        &mut self,
        result: Result<T, CoreError>,
    ) -> Result<T, NativeFault<'native, 'id>> {
        match result {
            Err(error) => Err(self.fail_native(error)),
            Ok(value) => match self.work.first_fault() {
                Some(error) => Err(self.fail_native(error)),
                None => Ok(value),
            },
        }
    }

    /// Resolve within the same generative input/construction, before the
    /// original factory closes. Looking at the latch never discovers new work.
    pub(super) fn resolve<T>(
        &mut self,
        result: Result<T, NativeFault<'native, 'id>>,
    ) -> Result<T, CoreError> {
        match self.work.first_fault() {
            Some(error) => Err(error),
            None => match result {
                Ok(value) => Ok(value),
                Err(_) => Err(self.work.fail(CoreError::InternalInvariant)),
            },
        }
    }

    fn native_call<T>(
        &mut self,
        body: impl FnOnce(&mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
    ) -> Result<T, NativeFault<'native, 'id>> {
        let result = self.call(body);
        self.carry(result)
    }
}

impl<'native, 'id> Host for PreparedBinding<'native, '_, '_, 'id, '_> {
    type Error = NativeFault<'native, 'id>;
    type Lease = NativeLease<'id>;

    fn work(&mut self, delta: n::WorkDelta) -> Result<(), Self::Error> {
        self.native_call(|work| match delta {
            n::WorkDelta::Scan(n) => work.scan(n),
            n::WorkDelta::Copy(n) => work.copy(n),
            n::WorkDelta::Events(n) => work.events(n),
        })
    }

    fn admit(&mut self, request: n::NativeAdmission) -> Result<(), Self::Error> {
        self.native_call(|work| {
            let (kind, amount) = match request {
                n::NativeAdmission::Node(n) | n::NativeAdmission::Record(n) => {
                    (LimitKind::NativeRecords, n)
                }
                n::NativeAdmission::Depth(n) => {
                    let limits = work
                        .model_limits()
                        .ok_or(CoreError::UnsupportedConfiguration)?;
                    return work.check_limit(LimitKind::Depth, n, limits.depth);
                }
                n::NativeAdmission::DecodedBytes(n) => (LimitKind::DecodedBytes, n),
                n::NativeAdmission::ReferenceLookup(n) => (LimitKind::ReferenceLookups, n),
                n::NativeAdmission::ReferenceBytes(n) => (LimitKind::ReferenceBytes, n),
            };
            work.admit_model(kind, amount)
        })
    }

    fn reserve(&mut self, bytes: u64) -> Result<Self::Lease, Self::Error> {
        self.native_call(|work| work.reserve_native(bytes))
    }

    fn admit_actual(&mut self, lease: &mut Self::Lease, bytes: u64) -> Result<(), Self::Error> {
        self.native_call(|work| work.admit_native_actual(lease, bytes))
    }

    fn fact(&mut self, _: n::AcceptedFact<'_>) -> Result<(), Self::Error> {
        // The direct native path owns its mappings. Falling back to the old
        // transport callback would silently recreate a second representation.
        self.native_call(|_| Err(CoreError::InvalidProvenance))
    }

    fn email_autolink_allowed(&mut self, span: PSpan) -> Result<bool, Self::Error> {
        let input = self.input;
        let projection = self.projection;
        self.native_call(|work| {
            let Some(projection) = projection else {
                return Ok(true);
            };
            let mut allowed = true;
            PreparedSources { input, work }.p_sources(span, |source, work| {
                allowed &= projection.email_autolink_allowed(source, work)?;
                Ok(())
            })?;
            Ok(allowed)
        })
    }

    fn reject(&mut self, violation: n::NativeViolation) -> Self::Error {
        self.fail_native(match violation {
            n::NativeViolation::UnsupportedConfiguration => CoreError::UnsupportedConfiguration,
            n::NativeViolation::InvalidProvenance => CoreError::InvalidProvenance,
            n::NativeViolation::ArithmeticOverflow => CoreError::ArithmeticOverflow,
            n::NativeViolation::OutOfMemory => CoreError::OutOfMemory,
            n::NativeViolation::InternalInvariant => CoreError::InternalInvariant,
        })
    }
}

impl<'native, 'id> direct::DirectHost<'native> for PreparedBinding<'native, '_, '_, 'id, '_> {
    type ReadBrand = crate::policy::ConstructionId<'id>;

    fn finish<T>(&mut self, result: Result<T, Self::Error>) -> Result<T, Self::Error> {
        match result {
            Err(signal) => {
                if self.work.first_fault().is_none() {
                    self.work.fail(CoreError::InternalInvariant);
                }
                Err(signal)
            }
            Ok(value) => self.native_call(|_| Ok(value)),
        }
    }

    fn checkpoint_now(&mut self) -> Result<(), Self::Error> {
        self.native_call(|work| work.checkpoint_now())
    }
}

#[cfg(test)]
impl<'id> PreparedBinding<'_, '_, '_, 'id, '_> {
    pub(super) fn source_key(&self) -> SourceKey {
        self.input.source().key()
    }

    pub(super) fn p_anchor(&mut self, at: usize) -> Result<u64, CoreError> {
        PreparedSources {
            input: self.input,
            work: self.work,
        }
        .p_anchor(at)
    }
    pub(super) fn p_pieces(
        &mut self,
        span: PSpan,
        emit: impl FnMut(InputPiece, &mut OwnedWork<'id, '_>) -> Result<(), CoreError>,
    ) -> Result<(), CoreError> {
        PreparedSources {
            input: self.input,
            work: self.work,
        }
        .p_pieces(span, emit)
    }
    pub(super) fn p_sources(
        &mut self,
        span: PSpan,
        emit: impl FnMut(SourceSpan, &mut OwnedWork<'id, '_>) -> Result<(), CoreError>,
    ) -> Result<(), CoreError> {
        PreparedSources {
            input: self.input,
            work: self.work,
        }
        .p_sources(span, emit)
    }
}

// A borrowed composition cursor, never another Host, owner, or fault latch.
struct PreparedSources<'source, 'work, 'id, 'op> {
    input: &'source ParseInput,
    work: &'work mut OwnedWork<'id, 'op>,
}

/// Issued only while the original binding owns the unique work borrow. Both
/// invariant identities survive the split; neither P nor Work can be replaced.
pub(super) struct ReadWitness<'native, 'source, 'id> {
    input: &'source ParseInput,
    native: PhantomData<fn(&'native ()) -> &'native ()>,
    construction: crate::policy::ConstructionId<'id>,
}

pub(super) struct PreparedReads<'call, 'native, 'id, 'op> {
    work: &'call mut OwnedWork<'id, 'op>,
    native: PhantomData<fn(&'native ()) -> &'native ()>,
    closed: bool,
}

impl PreparedReads<'_, '_, '_, '_> {
    pub(super) fn text_chunk(
        &mut self,
        text: &str,
    ) -> Result<crate::memory::BuildTextChunk, CoreError> {
        crate::ir::direct::owned_text_chunk(text, 0, text.len(), self.work)
    }
}

impl<'native, 'source, 'id> PreparedBinding<'native, 'source, '_, 'id, '_> {
    pub(super) fn native_parts<T>(
        &mut self,
        body: impl FnOnce(
            ReadWitness<'native, 'source, 'id>,
            &mut OwnedWork<'id, '_>,
        ) -> Result<T, CoreError>,
    ) -> Result<T, NativeFault<'native, 'id>> {
        let result = self.with_parts(body);
        self.carry(result)
    }

    pub(super) fn with_parts<T>(
        &mut self,
        body: impl FnOnce(
            ReadWitness<'native, 'source, 'id>,
            &mut OwnedWork<'id, '_>,
        ) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let witness = ReadWitness {
            input: self.input,
            native: PhantomData,
            construction: PhantomData,
        };
        self.call(|work| body(witness, work))
    }
}

impl<'native, 'id> ReadWitness<'native, '_, 'id> {
    pub(super) fn input(&self) -> &ParseInput {
        self.input
    }

    pub(super) fn p_anchor(
        &self,
        at: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<u64, CoreError> {
        PreparedSources {
            input: self.input,
            work,
        }
        .p_anchor(at)
    }

    pub(super) fn piece_cursor(
        &self,
        span: PSpan,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<PieceCursor<'_, 'id>, CoreError> {
        work.check()?;
        if span.start > span.end || span.end > self.input.text().len() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let index = PreparedSources {
            input: self.input,
            work,
        }
        .p_index(span.start)?;
        Ok(PieceCursor {
            rows: self.input.pieces(),
            index,
            covered: span.start as u64,
            end: span.end as u64,
            brand: PhantomData,
        })
    }

    pub(super) fn p_pieces(
        &self,
        span: PSpan,
        work: &mut OwnedWork<'id, '_>,
        emit: impl FnMut(InputPiece, &mut OwnedWork<'id, '_>) -> Result<(), CoreError>,
    ) -> Result<(), CoreError> {
        PreparedSources {
            input: self.input,
            work,
        }
        .p_pieces(span, emit)
    }

    pub(super) fn with_reads<T>(
        &self,
        work: &mut OwnedWork<'id, '_>,
        body: impl FnOnce(&mut PreparedReads<'_, 'native, 'id, '_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        use direct::NativeReads;
        work.check()?;
        let mut reads = PreparedReads {
            work,
            native: PhantomData,
            closed: false,
        };
        let result = body(&mut reads);
        reads.read_finish(result)
    }

    pub(super) fn p_sources(
        &self,
        span: PSpan,
        work: &mut OwnedWork<'id, '_>,
        emit: impl FnMut(SourceSpan, &mut OwnedWork<'id, '_>) -> Result<(), CoreError>,
    ) -> Result<(), CoreError> {
        PreparedSources {
            input: self.input,
            work,
        }
        .p_sources(span, emit)
    }
}

/// A scalar cursor over this binding's immutable N01 table; it owns no rows.
pub(super) struct PieceCursor<'a, 'id> {
    rows: &'a [InputPiece],
    index: usize,
    covered: u64,
    end: u64,
    brand: crate::policy::ConstructionId<'id>,
}
impl<'id> PieceCursor<'_, 'id> {
    pub(super) fn next(
        &mut self,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<InputPiece>, CoreError> {
        work.check()?;
        if self.covered == self.end {
            return Ok(None);
        }
        work.events(1)?;
        let row = self
            .rows
            .get(self.index)
            .copied()
            .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
        let start = self.covered.max(row.parse.start());
        let end = self.end.min(row.parse.end());
        if start != self.covered || start >= end {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        let source = if row.kind == InputPieceKind::Copy {
            SourceSpan::new(
                row.source.start() + start - row.parse.start(),
                row.source.start() + end - row.parse.start(),
            )?
        } else {
            if start != row.parse.start() || end != row.parse.end() {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            row.source
        };
        self.index += 1;
        self.covered = end;
        Ok(Some(InputPiece {
            source,
            parse: ParseSpan::new(start, end)?,
            kind: row.kind,
        }))
    }
}

impl<'native, 'id> direct::NativeReads<'native> for PreparedReads<'_, 'native, 'id, '_> {
    type Error = CoreError;
    type Brand = crate::policy::ConstructionId<'id>;

    fn read_work(&mut self, delta: n::WorkDelta) -> Result<(), CoreError> {
        match delta {
            n::WorkDelta::Scan(n) => self.work.scan(n),
            n::WorkDelta::Copy(n) => self.work.copy(n),
            n::WorkDelta::Events(n) => self.work.events(n),
        }
    }

    fn read_reject(&mut self, violation: n::NativeViolation) -> CoreError {
        self.work.fail(match violation {
            n::NativeViolation::UnsupportedConfiguration => CoreError::UnsupportedConfiguration,
            n::NativeViolation::InvalidProvenance => CoreError::InvalidProvenance,
            n::NativeViolation::ArithmeticOverflow => CoreError::ArithmeticOverflow,
            n::NativeViolation::OutOfMemory => CoreError::OutOfMemory,
            n::NativeViolation::InternalInvariant => CoreError::InternalInvariant,
        })
    }

    fn read_finish<T>(&mut self, result: Result<T, CoreError>) -> Result<T, CoreError> {
        // A callback's error happened before a subsequent cancellation poll.
        let result = match result {
            Err(error) => Err(self.work.fail(error)),
            Ok(value) => self.work.check().map(|()| value),
        };
        self.closed = true;
        result
    }
}

impl Drop for PreparedReads<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            self.work.fail(CoreError::InternalInvariant);
        }
    }
}

impl<'id> PreparedSources<'_, '_, 'id, '_> {
    fn p_index(&mut self, at: usize) -> Result<usize, CoreError> {
        let mut a = 0;
        let mut b = self.input.pieces().len();
        while a < b {
            self.work.events(1)?;
            let middle = a + (b - a) / 2;
            if self.input.pieces()[middle].parse.end() <= at as u64 {
                a = middle + 1;
            } else {
                b = middle;
            }
        }
        Ok(a)
    }

    pub(super) fn p_anchor(&mut self, at: usize) -> Result<u64, CoreError> {
        self.work.check()?;
        if at > self.input.text().len() {
            return Err(self.work.fail(CoreError::InvalidProvenance));
        }
        if at == self.input.text().len() {
            return Ok(self.input.source().text().len() as u64);
        }
        let index = self.p_index(at)?;
        if let Some(piece) = self.input.pieces().get(index) {
            self.work.events(1)?;
            if piece.parse.start() <= at as u64 && (at as u64) < piece.parse.end() {
                if piece.kind == InputPieceKind::Copy {
                    return Ok(piece.source.start() + at as u64 - piece.parse.start());
                }
                if at as u64 == piece.parse.start() {
                    return Ok(piece.source.start());
                }
            }
        }
        Err(self.work.fail(CoreError::InvalidProvenance))
    }

    /// Visits actual N01 pieces in their original order. Copy intersections may
    /// shrink; a normalization primitive is always retained whole. The callback
    /// writes final storage with the same unique work borrow, without a recipe.
    pub(super) fn p_pieces(
        &mut self,
        span: PSpan,
        mut emit: impl FnMut(InputPiece, &mut OwnedWork<'id, '_>) -> Result<(), CoreError>,
    ) -> Result<(), CoreError> {
        self.work.check()?;
        let result = (|| {
            if span.start > span.end || span.end > self.input.text().len() {
                return Err(CoreError::InvalidProvenance);
            }
            if span.start == span.end {
                self.p_anchor(span.start)?;
                return Ok(());
            }
            let first = self.p_index(span.start)?;
            let mut covered = span.start as u64;
            for piece in &self.input.pieces()[first..] {
                self.work.events(1)?;
                if piece.parse.start() >= span.end as u64 {
                    break;
                }
                let start = (span.start as u64).max(piece.parse.start());
                let end = (span.end as u64).min(piece.parse.end());
                if start >= end {
                    continue;
                }
                if start != covered {
                    return Err(CoreError::InvalidProvenance);
                }
                let source = if piece.kind == InputPieceKind::Copy {
                    SourceSpan::new(
                        piece.source.start() + start - piece.parse.start(),
                        piece.source.start() + end - piece.parse.start(),
                    )?
                } else {
                    if start != piece.parse.start() || end != piece.parse.end() {
                        return Err(CoreError::InvalidProvenance);
                    }
                    piece.source
                };
                emit(
                    InputPiece {
                        source,
                        parse: ParseSpan::new(start, end)?,
                        kind: piece.kind,
                    },
                    self.work,
                )?;
                self.work.check()?;
                covered = end;
            }
            if covered != span.end as u64 {
                return Err(CoreError::InvalidProvenance);
            }
            Ok(())
        })();
        result.map_err(|error| self.work.fail(error))
    }

    /// SourceUnion's ordered adjacency merge, without sorting or deduplicating
    /// ancestry. Empty P has one real S anchor rather than an invented piece.
    pub(super) fn p_sources(
        &mut self,
        span: PSpan,
        mut emit: impl FnMut(SourceSpan, &mut OwnedWork<'id, '_>) -> Result<(), CoreError>,
    ) -> Result<(), CoreError> {
        let result = (|| {
            if span.start == span.end {
                let at = self.p_anchor(span.start)?;
                emit(SourceSpan::new(at, at)?, self.work)?;
            } else {
                let mut pending: Option<SourceSpan> = None;
                self.p_pieces(span, |piece, work| {
                    work.check()?;
                    if let Some(previous) = pending {
                        work.events(1)?;
                        if previous.end() == piece.source.start() {
                            pending = Some(SourceSpan::new(previous.start(), piece.source.end())?);
                            return Ok(());
                        }
                        if previous.end() > piece.source.start() {
                            return Err(CoreError::InvalidProvenance);
                        }
                        emit(previous, work)?;
                    }
                    pending = Some(piece.source);
                    Ok(())
                })?;
                emit(pending.ok_or(CoreError::InvalidProvenance)?, self.work)?;
            }
            self.work.check()
        })();
        result.map_err(|error| self.work.fail(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};
    use crate::source::SourceDocument;

    fn input(bytes: &[u8], revision: u64, budget: &SourceBudget) -> ParseInput {
        let mut op = budget.begin(CancellationToken::new());
        let source =
            SourceDocument::new(SourceKey::new([42; 16], revision).unwrap(), bytes, &mut op)
                .unwrap()
                .snapshot();
        ParseInput::prepare(source, &mut op).unwrap()
    }

    #[test]
    fn equal_p_keeps_the_selected_s_geometry_and_revision() {
        let budget = SourceBudget::source_fixture();
        let crlf = input(b"a\r\nb", 1, &budget);
        let lf = input(b"a\nb", 2, &budget);
        let nul = input(b"a\0b", 3, &budget);
        let replacement = input("a\u{fffd}b".as_bytes(), 4, &budget);
        assert_eq!(crlf.text(), lf.text());
        assert_eq!(nul.text(), replacement.text());
        for (prepared, revision, end, expected) in [
            (&crlf, 1, 2, 1..3),
            (&lf, 2, 2, 1..2),
            (&nul, 3, 4, 1..2),
            (&replacement, 4, 4, 1..4),
        ] {
            let mut op = budget.begin(CancellationToken::new());
            construction_scope(&mut op, |mut work| {
                with_prepared(prepared, &mut work, |native, mut binding| {
                    assert_eq!(native.len(), prepared.text().len());
                    assert_eq!(binding.source_key().revision(), revision);
                    let mut result = None;
                    binding.p_sources(PSpan { start: 1, end }, |source, _| {
                        assert!(result.is_none());
                        result = Some(source.start()..source.end());
                        Ok(())
                    })?;
                    assert_eq!(result, Some(expected));
                    assert_eq!(
                        binding.p_anchor(prepared.text().len())?,
                        prepared.source().text().len() as u64
                    );
                    Ok(())
                })?;
                work.finish()
            })
            .unwrap();
        }
    }

    #[test]
    fn n01_pieces_stay_whole_and_adjacent_sources_merge() {
        let budget = SourceBudget::source_fixture();
        let prepared = input(b"x\r\n\0z", 1, &budget);
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            with_prepared(&prepared, &mut work, |_, mut binding| {
                let mut pieces = Vec::new();
                binding.p_pieces(PSpan { start: 0, end: 6 }, |piece, _| {
                    pieces.push((
                        piece.kind,
                        piece.source.start(),
                        piece.source.end(),
                        piece.parse.start(),
                        piece.parse.end(),
                    ));
                    Ok(())
                })?;
                assert_eq!(
                    pieces,
                    [
                        (InputPieceKind::Copy, 0, 1, 0, 1),
                        (InputPieceKind::CrLf, 1, 3, 1, 2),
                        (InputPieceKind::Nul, 3, 4, 2, 5),
                        (InputPieceKind::Copy, 4, 5, 5, 6),
                    ]
                );
                let mut merged = Vec::new();
                binding.p_sources(PSpan { start: 0, end: 6 }, |source, _| {
                    merged.push(source.start()..source.end());
                    Ok(())
                })?;
                assert_eq!(merged.len(), 1);
                assert_eq!(merged[0], 0..5);
                Ok(())
            })?;
            work.finish()
        })
        .unwrap();
    }

    #[test]
    fn partial_n01_and_interior_anchor_latch_the_original_work() {
        let budget = SourceBudget::source_fixture();
        let prepared = input(b"\0", 1, &budget);
        for anchor in [false, true] {
            let mut op = budget.begin(CancellationToken::new());
            construction_scope(&mut op, |mut work| {
                let result = with_prepared(&prepared, &mut work, |_, mut binding| {
                    let result = if anchor {
                        binding.p_anchor(1).map(|_| ())
                    } else {
                        binding.p_pieces(PSpan { start: 0, end: 1 }, |_, _| {
                            panic!("partial transform escaped")
                        })
                    };
                    assert_eq!(result, Err(CoreError::InvalidProvenance));
                    assert_eq!(binding.p_anchor(0), Err(CoreError::InvalidProvenance));
                    Ok(()) // An ignored error cannot turn the factory successful.
                });
                assert_eq!(result, Err(CoreError::InvalidProvenance));
                assert_eq!(work.first_fault(), Some(CoreError::InvalidProvenance));
            });
        }
    }

    #[test]
    fn sink_and_body_errors_are_latched_without_a_second_operation() {
        let budget = SourceBudget::source_fixture();
        let prepared = input(b"abc", 1, &budget);
        for sink in [false, true] {
            let mut op = budget.begin(CancellationToken::new());
            construction_scope(&mut op, |mut work| {
                let result: Result<(), _> =
                    with_prepared(&prepared, &mut work, |_, mut binding| {
                        if sink {
                            binding.p_sources(PSpan { start: 0, end: 3 }, |_, _| {
                                Err(CoreError::ArithmeticOverflow)
                            })
                        } else {
                            Err(CoreError::ArithmeticOverflow)
                        }
                    });
                assert_eq!(result, Err(CoreError::ArithmeticOverflow));
                assert_eq!(work.first_fault(), Some(CoreError::ArithmeticOverflow));
            });
        }
    }

    #[test]
    fn typed_callback_fault_precedes_simultaneous_cancel_for_host_and_reads() {
        use direct::{DirectHost, NativeReads};
        let budget = SourceBudget::source_fixture();
        let prepared = input(b"abc", 1, &budget);
        for reads in [false, true] {
            for prior_fault in [false, true] {
                let cancel = CancellationToken::new();
                let mut op = budget.begin(cancel.clone());
                construction_scope(&mut op, |mut work| {
                    let expected = if prior_fault {
                        CoreError::InvalidProvenance
                    } else {
                        CoreError::InvalidBoundary
                    };
                    let result: Result<(), _> =
                        with_prepared(&prepared, &mut work, |_, mut binding| {
                            if reads {
                                binding.with_parts(|witness, work| {
                                    witness.with_reads(work, |access| {
                                        if prior_fault {
                                            access
                                                .read_reject(n::NativeViolation::InvalidProvenance);
                                        }
                                        cancel.cancel();
                                        access.read_finish(Err(CoreError::InvalidBoundary))
                                    })
                                })
                            } else {
                                if prior_fault {
                                    binding.reject(n::NativeViolation::InvalidProvenance);
                                }
                                cancel.cancel();
                                let result = binding.carry(Err(CoreError::InvalidBoundary));
                                let result = binding.finish(result);
                                binding.resolve(result)
                            }
                        });
                    assert_eq!(result, Err(expected));
                    assert_eq!(work.first_fault(), Some(expected));
                });
            }
        }
    }

    // Only the test host injects a panic: the real grammar calls work(), the
    // actual binding completes its local guard, and this wrapper then unwinds
    // while native arenas/mappings are live. No product panic hook is added.
    struct GrammarPanicHost<'budget, B> {
        binding: B,
        budget: &'budget SourceBudget,
        auxiliary_before_parse: u64,
        prior_fault: bool,
    }

    impl<B: Host> Host for GrammarPanicHost<'_, B> {
        type Error = B::Error;
        type Lease = B::Lease;

        fn work(&mut self, delta: n::WorkDelta) -> Result<(), Self::Error> {
            self.binding.work(delta)?;
            if matches!(delta, n::WorkDelta::Scan(bytes) if bytes != 0) {
                assert!(self.budget.usage().auxiliary_capacity > self.auxiliary_before_parse);
                if self.prior_fault {
                    self.binding.reject(n::NativeViolation::InvalidProvenance);
                }
                panic!("actual grammar scan returned from the inner binding guard");
            }
            Ok(())
        }
        fn admit(&mut self, request: n::NativeAdmission) -> Result<(), Self::Error> {
            self.binding.admit(request)
        }
        fn reserve(&mut self, bytes: u64) -> Result<Self::Lease, Self::Error> {
            self.binding.reserve(bytes)
        }
        fn admit_actual(&mut self, lease: &mut Self::Lease, bytes: u64) -> Result<(), Self::Error> {
            self.binding.admit_actual(lease, bytes)
        }
        fn fact(&mut self, fact: n::AcceptedFact<'_>) -> Result<(), Self::Error> {
            self.binding.fact(fact)
        }
        fn reject(&mut self, violation: n::NativeViolation) -> Self::Error {
            self.binding.reject(violation)
        }
    }

    impl<'input, B: direct::DirectHost<'input>> direct::DirectHost<'input> for GrammarPanicHost<'_, B> {
        type ReadBrand = B::ReadBrand;
        fn finish<T>(&mut self, result: Result<T, Self::Error>) -> Result<T, Self::Error> {
            self.binding.finish(result)
        }
        fn checkpoint_now(&mut self) -> Result<(), Self::Error> {
            self.binding.checkpoint_now()
        }
    }

    #[test]
    fn factory_unwind_before_inner_and_during_grammar_permanently_faults_work() {
        use crate::config::{ParseConfig, Profile};
        use crate::memory::BuildRows;
        use crate::model::AcceptedModel;
        use std::panic::{AssertUnwindSafe, catch_unwind};

        for grammar in [false, true] {
            for prior_fault in [false, true] {
                let budget = SourceBudget::model_fixture();
                let prepared = input(b"*x &amp; y*\n", 1, &budget);
                let mut old_op = budget.begin(CancellationToken::new());
                let old_model = AcceptedModel::parse(
                    prepared.source().clone(),
                    ParseConfig::new(Profile::C0),
                    1,
                    &mut old_op,
                )
                .unwrap();
                let old_identity = old_model.identity();
                let old_atoms = old_model.ir().atoms_len();
                let retained = budget.usage();
                let expected = if prior_fault {
                    CoreError::InvalidProvenance
                } else {
                    CoreError::InternalInvariant
                };
                let mut op = budget.begin(CancellationToken::new());
                construction_scope(&mut op, |mut work| {
                    let mut rows = BuildRows::new(4, LimitKind::ModelRecords, &mut work).unwrap();
                    rows.push(7_u32, &mut work).unwrap();
                    let before_parse = budget.usage().auxiliary_capacity;
                    let panic = catch_unwind(AssertUnwindSafe(|| {
                        let _: Result<(), CoreError> =
                            with_prepared(&prepared, &mut work, |native, mut binding| {
                                if grammar {
                                    let mut host = GrammarPanicHost {
                                        binding,
                                        budget: &budget,
                                        auxiliary_before_parse: before_parse,
                                        prior_fault,
                                    };
                                    let result = direct::parse(
                                        native,
                                        &super::super::options::c0(),
                                        &mut host,
                                        |_| {
                                            panic!(
                                                "grammar injection must occur before finalization"
                                            )
                                        },
                                    );
                                    host.binding.resolve(result)
                                } else {
                                    if prior_fault {
                                        binding.reject(n::NativeViolation::InvalidProvenance);
                                    }
                                    panic!("factory body before any inner guard");
                                }
                            });
                    }));
                    let payload = panic.expect_err("the selected unwind point must be reached");
                    assert_eq!(
                        payload.downcast_ref::<&str>().copied(),
                        Some(if grammar {
                            "actual grammar scan returned from the inner binding guard"
                        } else {
                            "factory body before any inner guard"
                        })
                    );
                    assert_eq!(work.first_fault(), Some(expected));
                    assert_eq!(rows.get(0, &mut work), Err(expected));
                    assert_eq!(rows.push(9, &mut work), Err(expected));
                    assert_eq!(rows.raw_value_for_test(0), Some(7));
                    assert_eq!(rows.raw_value_for_test(1), None);
                    assert_eq!(work.check(), Err(expected));
                    let retry: Result<(), _> = with_prepared(&prepared, &mut work, |_, _| {
                        panic!("faulted factory must not re-enter its body")
                    });
                    assert_eq!(retry, Err(expected));
                    assert_eq!(work.finish(), Err(expected));
                    drop(rows);
                });
                assert_eq!(op.first_fault(), Some(expected));
                let after = budget.usage();
                assert_eq!(after.source_capacity, retained.source_capacity);
                assert_eq!(after.source_owners, retained.source_owners);
                assert_eq!(after.auxiliary_capacity, retained.auxiliary_capacity);
                assert_eq!(old_model.identity(), old_identity);
                assert_eq!(old_model.ir().atoms_len(), old_atoms);
                assert_eq!(old_model.source().text(), "*x &amp; y*\n");
                drop(old_model);
                drop(prepared);
                let final_usage = budget.usage();
                assert_eq!(final_usage.source_capacity, 0);
                assert_eq!(final_usage.auxiliary_capacity, 0);
                assert_eq!(final_usage.source_owners, 0);
            }
        }
    }

    #[test]
    fn factory_typed_error_precedes_cancel_and_preserves_existing_fault() {
        let budget = SourceBudget::source_fixture();
        let prepared = input(b"x", 1, &budget);
        for prior_fault in [false, true] {
            let cancel = CancellationToken::new();
            let mut op = budget.begin(cancel.clone());
            construction_scope(&mut op, |mut work| {
                let expected = if prior_fault {
                    CoreError::InvalidProvenance
                } else {
                    CoreError::ArithmeticOverflow
                };
                let result: Result<(), _> =
                    with_prepared(&prepared, &mut work, |_, mut binding| {
                        if prior_fault {
                            binding.reject(n::NativeViolation::InvalidProvenance);
                        }
                        cancel.cancel();
                        Err(CoreError::ArithmeticOverflow)
                    });
                assert_eq!(result, Err(expected));
                assert_eq!(work.finish(), Err(expected));
            });
        }
    }

    #[test]
    fn caller_caught_stateless_panic_does_not_unwind_the_factory() {
        use std::panic::catch_unwind;
        let budget = SourceBudget::source_fixture();
        let prepared = input(b"x", 1, &budget);
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            with_prepared(&prepared, &mut work, |_, _| {
                let panic = catch_unwind(|| panic!("caller-local, no construction state touched"));
                assert!(panic.is_err());
                Ok(())
            })
            .unwrap();
            assert_eq!(work.first_fault(), None);
            assert_eq!(work.finish(), Ok(()));
        });
    }
}

#[cfg(test)]
#[path = "prepared_resource_tests.rs"]
mod resource_tests;

#[cfg(test)]
#[path = "prepared_fault_tests.rs"]
mod fault_tests;
