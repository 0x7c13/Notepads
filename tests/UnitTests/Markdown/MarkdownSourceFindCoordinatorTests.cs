// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;

namespace Notepads.UnitTests;

public sealed class MarkdownSourceFindCoordinatorTests
{
    [Fact]
    public void AZeroWidthMatchInvalidatesAnOrdinaryCaretAtTheSameByte()
    {
        var state = Source();
        state.ObserveCaret(20, 0, true);
        Assert.True(state.TryBegin(out var caret));
        var match = Match(20, 20);
        state.ObserveCaret(20, 0, true, searchMatch: match);
        Assert.False(state.TryBegin(out _));
        Assert.False(state.CaretCurrent(caret));
        Assert.False(state.CompleteCaret(caret, out _));
        Assert.True(state.TryBegin(out var found));
        Assert.Equal(match, found.SearchMatch);
        Assert.Equal((20UL, 0UL), (found.Offset, found.Length));
        Assert.True(state.CompleteCaret(found, out bool reveal));
        Assert.True(reveal);
    }

    [Fact]
    public void SelectionRevisionInvalidatesTheSameMatchRangeWithoutASecondAdmission()
    {
        var state = Source();
        var first = Match(10, 15);
        state.ObserveCaret(10, 5, true, searchMatch: first);
        Assert.True(state.TryBegin(out var old));
        var current = first with { SelectionRevision = first.SelectionRevision + 1 };
        state.ObserveCaret(10, 5, true, searchMatch: current);
        Assert.False(state.TryBegin(out _));
        Assert.False(state.CompleteCaret(old, out _));
        Assert.True(state.TryBegin(out var latest));
        Assert.Equal(current, latest.SearchMatch);
        Assert.True(state.CompleteCaret(latest, out _));
        Assert.False(state.HasPending);
    }

    [Fact]
    public void RepeatingTheSameFoundRangeStillHasANewExplicitCommandIdentity()
    {
        var state = Source();
        var first = Match(4, 8);
        state.ObserveCaret(4, 4, true, searchMatch: first);
        Assert.True(state.TryBegin(out var old));
        state.ObserveCaret(4, 4, true, searchMatch: first with { Serial = 2 });
        Assert.False(state.CompleteCaret(old, out _));
        Assert.True(state.TryBegin(out var current));
        Assert.Equal(2L, current.SearchMatch.Value.Serial);
    }

    [Fact]
    public void ClearingTheMatchInvalidatesOldRangeWorkEvenWhenTheCaretSharesItsStart()
    {
        var state = Source();
        state.ObserveCaret(10, 5, true, searchMatch: Match(10, 15));
        Assert.True(state.TryBegin(out var old));
        state.ObserveCaret(10, 0, false);
        Assert.False(state.CompleteCaret(old, out _));
        Assert.True(state.TryBegin(out var caret));
        Assert.Null(caret.SearchMatch);
        Assert.Equal(0UL, caret.Length);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void AClearedMatchCannotRecreateCaretDebtFromADelayedNotification(bool projected)
    {
        var state = Source();
        state.ObserveCaret(0, 5, true, searchMatch: Match(0, 5));
        Assert.True(state.TryBegin(out var old));
        if (projected) Assert.True(state.CompleteCaret(old, out _));

        // The old preview collapse selected an external beta range. The source
        // still has alpha's unchanged native selection and ordinary end caret.
        Assert.True(state.DiscardClearedSourceCaret(5, 5));
        Assert.False(state.CanRevealCaret(old));
        for (int replay = 0; replay < 3; replay++)
        {
            state.ObserveCaret(5, 0, false, sourceSelectionRevision: 5);
            Assert.False(state.HasPending);
            Assert.False(state.TryBegin(out _));
        }
        Assert.False(state.CompleteCaret(old, out _));
        Assert.Null(state.Active);
        Assert.False(state.HasPending);

        // Right collapses the original alpha selection at its current byte 5.
        // That genuine native claim has a new revision even though its byte is unchanged.
        state.ObserveCaret(5, 0, false, sourceSelectionRevision: 6);
        Assert.True(state.TryBegin(out var collapsedCaret));
        Assert.Null(collapsedCaret.SearchMatch);
        Assert.Equal(5UL, collapsedCaret.Offset);
        Assert.True(state.CompleteCaret(collapsedCaret, out bool collapseReveal));
        Assert.False(collapseReveal);
        state.ObserveCaret(5, 0, false, sourceSelectionRevision: 6);
        Assert.False(state.HasPending);

        state.ObserveCaret(11, 0, false, sourceSelectionRevision: 7);
        Assert.True(state.TryBegin(out var laterCaret));
        Assert.Null(laterCaret.SearchMatch);
        Assert.Equal(11UL, laterCaret.Offset);
        Assert.True(state.CompleteCaret(laterCaret, out bool caretReveal));
        Assert.False(caretReveal);

        // An explicit Go To at the same caret still has its own reveal intent.
        state.ObserveCaret(11, 0, true, force: true, sourceSelectionRevision: 7);
        Assert.True(state.TryBegin(out var goTo));
        Assert.True(state.CompleteCaret(goTo, out bool goToReveal));
        Assert.True(goToReveal);

        var nextMatch = Match(6, 10) with { Serial = 2, SelectionRevision = 8 };
        state.ObserveCaret(6, 4, true, searchMatch: nextMatch);
        Assert.True(state.TryBegin(out var nextFind));
        Assert.Equal(nextMatch, nextFind.SearchMatch);
        Assert.True(state.CompleteCaret(nextFind, out bool findReveal));
        Assert.True(findReveal);
    }

    [Fact]
    public void FollowOffAdmitsOnlyTheSourceMatchAndNeverGrantsReveal()
    {
        var state = Source();
        state.ObserveSource(new(0, 100, 0, 0, 1, 100, 1));
        state.ObserveCaret(10, 5, true, searchMatch: Match(10, 15));
        Assert.True(state.TryBegin(out var found, follow: false));
        Assert.Equal(MarkdownFollowOperation.Caret, found.Operation);
        Assert.True(state.CompleteCaret(found, out bool reveal));
        Assert.False(reveal);
        Assert.False(state.TryBegin(out _, follow: false));
        Assert.False(state.HasPending);
        state.ObserveCaret(15, 0, true);
        Assert.False(state.TryBegin(out _, follow: false));
        Assert.False(state.HasPending);
    }

    [Fact]
    public void FollowOffCannotAdmitPreviewSelectionOrViewportDebt()
    {
        var state = new MarkdownFollowCoordinator();
        state.Reset(MarkdownFollowPane.Preview);
        state.ObserveCaret(10, 5, true);
        state.ObservePreview(10, 15, 0, 1);
        Assert.False(state.TryBegin(out _, follow: false));
        Assert.False(state.HasPending);
    }

    [Fact]
    public void ANewWheelKeepsTheMatchProjectionButRemovesItsRevealAuthority()
    {
        var state = Source();
        state.ObserveCaret(10, 5, true, searchMatch: Match(10, 15));
        Assert.True(state.TryBegin(out var found));
        state.BeginViewportIntent();
        state.ObserveSource(new(0, 100, 0, 0.5, 1, 100, 1));
        Assert.True(state.CompleteCaret(found, out bool reveal));
        Assert.False(reveal);
        Assert.False(state.CanRevealCaret(found));
        Assert.True(state.TryBegin(out var viewport));
        Assert.Equal(MarkdownFollowOperation.SourceViewport, viewport.Operation);
    }

    [Fact]
    public void SynchronousPublicationCanRecheckSerialAfterCompletionRetiresAdmission()
    {
        var state = Source();
        state.ObserveCaret(10, 5, true, searchMatch: Match(10, 15));
        Assert.True(state.TryBegin(out var publishing));
        Assert.True(state.CompleteCaret(publishing, out _));
        Assert.True(state.CaretCurrent(publishing));
        state.Reset(MarkdownFollowPane.Preview);
        Assert.False(state.CaretCurrent(publishing));
        Assert.False(state.CanRevealCaret(publishing));
    }

    private static MarkdownSearchMatch Match(long start, long end) => new(1, start, end, 2, 3, 4, 5);

    private static MarkdownFollowCoordinator Source()
    {
        var state = new MarkdownFollowCoordinator();
        state.Reset(MarkdownFollowPane.Source);
        return state;
    }
}
